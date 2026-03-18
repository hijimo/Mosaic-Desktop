//! Handler for exec_command and write_stdin tool calls.
//!
//! Dispatches to the shared `UnifiedExecProcessManager` for PTY-backed
//! interactive sessions with process reuse.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::core::unified_exec::{
    ExecCommandRequest, UnifiedExecProcessManager, UnifiedExecResponse, WriteStdinRequest,
};
use crate::protocol::error::{CodexError, ErrorCode};

/// Shared handler for `exec_command` and `write_stdin`.
pub struct UnifiedExecHandler {
    manager: Arc<UnifiedExecProcessManager>,
}

impl UnifiedExecHandler {
    pub fn new(manager: Arc<UnifiedExecProcessManager>) -> Self {
        Self { manager }
    }
}

#[derive(Debug, Deserialize)]
struct ExecCommandArgs {
    cmd: String,
    #[serde(default)]
    workdir: Option<String>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default)]
    login: Option<bool>,
    #[serde(default)]
    tty: bool,
    #[serde(default = "default_exec_yield_time_ms")]
    yield_time_ms: u64,
    #[serde(default)]
    max_output_tokens: Option<usize>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    sandbox_permissions: Option<String>,
    #[serde(default)]
    additional_permissions: Option<serde_json::Value>,
    #[serde(default)]
    justification: Option<String>,
    #[serde(default)]
    prefix_rule: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct WriteStdinArgs {
    session_id: i32,
    #[serde(default)]
    chars: String,
    #[serde(default = "default_write_stdin_yield_time_ms")]
    yield_time_ms: u64,
    #[serde(default)]
    max_output_tokens: Option<usize>,
}

fn default_exec_yield_time_ms() -> u64 {
    10_000
}
fn default_write_stdin_yield_time_ms() -> u64 {
    250
}

#[async_trait]
impl ToolHandler for UnifiedExecHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "exec_command" || n == "write_stdin")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("exec_command".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        if args.get("session_id").is_some() {
            return self.handle_write_stdin(args).await;
        }
        self.handle_exec_command(args).await
    }
}

impl UnifiedExecHandler {
    async fn handle_exec_command(
        &self,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        let params: ExecCommandArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid exec_command args: {e}"),
            )
        })?;

        // Resolve shell
        let default_shell =
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let shell_path = params.shell.as_deref().unwrap_or(&default_shell);

        // Login shell check
        let allow_login_shell = true; // TODO: wire from config
        let use_login = match params.login {
            Some(true) if !allow_login_shell => {
                return Err(CodexError::new(
                    ErrorCode::InvalidInput,
                    "login shell is disabled by config; omit `login` or set it to false.",
                ));
            }
            Some(v) => v,
            None => allow_login_shell,
        };

        let mut shell_args = vec![shell_path.to_string()];
        if use_login {
            shell_args.push("-l".to_string());
        }
        shell_args.push("-c".to_string());
        shell_args.push(params.cmd.clone());

        // Validate additional permissions
        let uses_additional =
            params.sandbox_permissions.as_deref() == Some("with_additional_permissions");
        if uses_additional {
            super::normalize_and_validate_additional_permissions(
                true,
                None,
                params.sandbox_permissions.as_deref(),
                params.additional_permissions.as_ref(),
            )
            .map_err(|e| CodexError::new(ErrorCode::InvalidInput, e))?;
        }

        // Intercept apply_patch
        let cwd = std::env::current_dir().unwrap_or_default();
        let timeout_ms = params.timeout_ms.unwrap_or(params.yield_time_ms.max(120_000));
        if let Some(result) = super::apply_patch::intercept_apply_patch(
            &shell_args,
            &cwd,
            Some(timeout_ms),
        )
        .await?
        {
            return Ok(result);
        }

        // Allocate process ID and build request
        let process_id = self.manager.allocate_process_id().await;
        let call_id = uuid::Uuid::new_v4().to_string();

        let workdir = params
            .workdir
            .filter(|wd| !wd.is_empty())
            .map(std::path::PathBuf::from);

        let request = ExecCommandRequest {
            command: shell_args,
            process_id,
            yield_time_ms: params.yield_time_ms,
            max_output_tokens: params.max_output_tokens,
            workdir,
            tty: params.tty,
            justification: params.justification,
            prefix_rule: params.prefix_rule,
        };

        let response = self
            .manager
            .exec_command(request, &call_id)
            .await
            .map_err(|err| {
                CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    format!("exec_command failed: {err}"),
                )
            })?;

        Ok(serde_json::to_value(format_response(&response)).unwrap_or_default())
    }

    async fn handle_write_stdin(
        &self,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        let params: WriteStdinArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid write_stdin args: {e}"),
            )
        })?;

        let response = self
            .manager
            .write_stdin(WriteStdinRequest {
                process_id: &params.session_id.to_string(),
                input: &params.chars,
                yield_time_ms: params.yield_time_ms,
                max_output_tokens: params.max_output_tokens,
            })
            .await
            .map_err(|err| {
                CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    format!("write_stdin failed: {err}"),
                )
            })?;

        Ok(serde_json::to_value(format_response(&response)).unwrap_or_default())
    }
}

/// Format a UnifiedExecResponse into a human-readable string (matches codex-main).
fn format_response(response: &UnifiedExecResponse) -> String {
    let mut sections = Vec::new();

    if !response.chunk_id.is_empty() {
        sections.push(format!("Chunk ID: {}", response.chunk_id));
    }

    sections.push(format!(
        "Wall time: {:.4} seconds",
        response.wall_time.as_secs_f64()
    ));

    if let Some(exit_code) = response.exit_code {
        sections.push(format!("Process exited with code {exit_code}"));
    }

    if let Some(process_id) = &response.process_id {
        sections.push(format!("Process running with session ID {process_id}"));
    }

    if let Some(original_token_count) = response.original_token_count {
        sections.push(format!("Original token count: {original_token_count}"));
    }

    sections.push("Output:".to_string());
    sections.push(response.output.clone());

    sections.join("\n")
}
