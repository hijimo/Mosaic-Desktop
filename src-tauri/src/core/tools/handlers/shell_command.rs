use async_trait::async_trait;
use serde::Deserialize;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

/// Backend variant for shell_command execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellCommandBackend {
    Classic,
    ZshFork,
}

/// Handler for the `shell_command` tool variant.
///
/// Unlike `shell` (which takes a command array), `shell_command` takes a single
/// string that is passed to the user's default shell. Supports Classic and ZshFork backends.
pub struct ShellCommandHandler {
    pub backend: ShellCommandBackend,
}

impl ShellCommandHandler {
    pub fn classic() -> Self {
        Self {
            backend: ShellCommandBackend::Classic,
        }
    }

    pub fn zsh_fork() -> Self {
        Self {
            backend: ShellCommandBackend::ZshFork,
        }
    }

    /// Resolve whether to use a login shell, matching source Codex `resolve_use_login_shell`.
    /// Returns error if login=true but config disallows it.
    fn resolve_use_login_shell(
        login: Option<bool>,
        allow_login_shell: bool,
    ) -> Result<bool, CodexError> {
        if !allow_login_shell && login == Some(true) {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "login shell is disabled by config; omit `login` or set it to false.",
            ));
        }
        Ok(login.unwrap_or(allow_login_shell))
    }

    /// Check if a shell_command invocation is safe (read-only).
    /// Derives the full command array and delegates to `is_known_safe_command`.
    pub fn is_safe_command(
        command: &str,
        login: Option<bool>,
        allow_login_shell: bool,
        backend: ShellCommandBackend,
    ) -> bool {
        let use_login = match Self::resolve_use_login_shell(login, allow_login_shell) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let args = derive_shell_args(&shell, command, use_login, backend);
        super::shell::is_known_safe_command(&args)
    }
}

impl Default for ShellCommandHandler {
    fn default() -> Self {
        Self::classic()
    }
}

#[derive(Deserialize)]
struct ShellCommandArgs {
    command: String,
    #[serde(default)]
    workdir: Option<String>,
    #[serde(default)]
    login: Option<bool>,
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

#[async_trait]
impl ToolHandler for ShellCommandHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "shell_command")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("shell_command".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: ShellCommandArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid shell_command args: {e}"),
            )
        })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

        // Resolve login shell with config check (matches source Codex behavior)
        // TODO: wire allow_login_shell from actual tools_config
        let allow_login_shell = true;
        let use_login = Self::resolve_use_login_shell(params.login, allow_login_shell)?;
        let shell_args = derive_shell_args(&shell, &params.command, use_login, self.backend);

        // Intercept apply_patch commands (matches source Codex behavior)
        if let Some(result) = super::apply_patch::intercept_apply_patch(
            &shell_args,
            &std::env::current_dir().unwrap_or_default(),
            Some(params.timeout_ms.unwrap_or(120_000)),
        )
        .await?
        {
            return Ok(result);
        }

        let program = &shell_args[0];
        let cmd_args = &shell_args[1..];

        let mut cmd = tokio::process::Command::new(program);
        cmd.args(cmd_args);
        if let Some(ref wd) = params.workdir {
            cmd.current_dir(wd);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let timeout = std::time::Duration::from_millis(params.timeout_ms.unwrap_or(120_000));

        let child = cmd.spawn().map_err(|e| {
            CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("failed to spawn: {e}"),
            )
        })?;

        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                return Err(CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    format!("{e}"),
                ));
            }
            Err(_) => {
                return Ok(serde_json::json!({
                    "exit_code": -1, "stdout": "", "stderr": "command timed out", "timed_out": true,
                }));
            }
        };

        Ok(serde_json::json!({
            "exit_code": output.status.code().unwrap_or(-1),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        }))
    }
}

/// Derive shell execution arguments based on shell path, command, login flag, and backend.
/// Also used by is_known_safe_command checks.
pub fn derive_shell_args(
    shell_path: &str,
    command: &str,
    use_login: bool,
    backend: ShellCommandBackend,
) -> Vec<String> {
    let mut args = vec![shell_path.to_string()];

    match backend {
        ShellCommandBackend::Classic => {
            if use_login {
                args.push("-l".to_string());
            }
            args.push("-c".to_string());
            args.push(command.to_string());
        }
        ShellCommandBackend::ZshFork => {
            // ZshFork uses -i for interactive-like behavior
            if use_login {
                args.push("-l".to_string());
            }
            args.push("-c".to_string());
            args.push(command.to_string());
        }
    }

    args
}
