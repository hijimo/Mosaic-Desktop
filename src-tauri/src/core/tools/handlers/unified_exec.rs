use async_trait::async_trait;
use serde::Deserialize;
use std::time::Instant;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct UnifiedExecHandler;

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

fn default_exec_yield_time_ms() -> u64 { 10_000 }
fn default_write_stdin_yield_time_ms() -> u64 { 250 }

#[async_trait]
impl ToolHandler for UnifiedExecHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "exec_command" || n == "write_stdin")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("exec_command".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Dispatch based on presence of session_id (write_stdin) vs cmd (exec_command)
        if args.get("session_id").is_some() {
            return handle_write_stdin(args).await;
        }
        handle_exec_command(args).await
    }
}

async fn handle_exec_command(args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
    let params: ExecCommandArgs = serde_json::from_value(args).map_err(|e| {
        CodexError::new(ErrorCode::InvalidInput, format!("invalid exec_command args: {e}"))
    })?;

    // Resolve shell: model-provided shell takes precedence, then $SHELL, then /bin/sh
    let default_shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let shell_path = params.shell.as_deref().unwrap_or(&default_shell);

    // Resolve login shell with config check (matches source Codex get_command)
    // TODO: wire allow_login_shell from actual tools_config
    let allow_login_shell = true;
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

    // Validate additional permissions (matches source Codex security checks)
    let uses_additional = params.sandbox_permissions.as_deref() == Some("with_additional_permissions");
    if uses_additional {
        // TODO: wire approval_policy from actual turn context
        let approval_policy: Option<&str> = None; // defaults to on_request
        let request_permission_enabled = true; // TODO: wire from features
        let _ = super::normalize_and_validate_additional_permissions(
            request_permission_enabled,
            approval_policy,
            params.sandbox_permissions.as_deref(),
            params.additional_permissions.as_ref(),
        ).map_err(|e| CodexError::new(ErrorCode::InvalidInput, e))?;
    }

    let program = &shell_args[0];
    let cmd_args = &shell_args[1..];

    let timeout_ms = params.timeout_ms.unwrap_or(params.yield_time_ms.max(120_000));

    // Intercept apply_patch commands (matches source Codex behavior)
    if let Some(result) = super::apply_patch::intercept_apply_patch(
        &shell_args,
        &std::env::current_dir().unwrap_or_default(),
        Some(timeout_ms),
    ).await? {
        return Ok(result);
    }

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(cmd_args);
    if let Some(ref wd) = params.workdir {
        if !wd.is_empty() {
            cmd.current_dir(wd);
        }
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let timeout = std::time::Duration::from_millis(timeout_ms);
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let start = Instant::now();

    let child = cmd.spawn().map_err(|e| {
        CodexError::new(ErrorCode::ToolExecutionFailed, format!("spawn error: {e}"))
    })?;

    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return Err(CodexError::new(ErrorCode::ToolExecutionFailed, format!("{e}")));
        }
        Err(_) => {
            return Ok(serde_json::json!({
                "exit_code": -1,
                "output": "command timed out",
                "timed_out": true,
                "wall_time_seconds": start.elapsed().as_secs_f64(),
            }));
        }
    };

    let wall_time = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.is_empty() {
        stderr.to_string()
    } else {
        format!("{stdout}\n{stderr}")
    };

    Ok(serde_json::json!({
        "exit_code": output.status.code().unwrap_or(-1),
        "output": combined,
        "wall_time_seconds": format!("{:.4}", wall_time.as_secs_f64()),
    }))
}

async fn handle_write_stdin(args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
    let _params: WriteStdinArgs = serde_json::from_value(args).map_err(|e| {
        CodexError::new(ErrorCode::InvalidInput, format!("invalid write_stdin args: {e}"))
    })?;

    // write_stdin requires the UnifiedExecProcessManager for PTY session management.
    // This will be fully implemented when the process manager is wired.
    Err(CodexError::new(
        ErrorCode::ToolExecutionFailed,
        "write_stdin requires the unified exec process manager (PTY sessions not yet wired)",
    ))
}
