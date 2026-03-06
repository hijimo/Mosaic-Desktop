use std::path::Path;
use std::time::Instant;

use crate::execpolicy::{Decision, Policy};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{
    ErrorEvent, Event, EventMsg, ExecApprovalRequestEvent, ExecCommandBeginEvent,
    ExecCommandEndEvent,
};
use crate::protocol::types::{AskForApproval, ExecCommandSource, ExecCommandStatus, SandboxPolicy};
use crate::shell_command;

/// Result of a completed command execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Sandbox-aware command executor.
///
/// Enforces `SandboxPolicy` restrictions, applies `AskForApproval` logic,
/// and emits bracket events (`ExecCommandBegin` / `ExecCommandEnd`) around
/// every execution attempt.
pub struct CommandExecutor {
    pub sandbox_policy: SandboxPolicy,
    pub approval_policy: AskForApproval,
    pub exec_policy: Policy,
    pub allow_list: Vec<Vec<String>>,
    pub tx_event: async_channel::Sender<Event>,
}

impl CommandExecutor {
    /// Create a new executor with the given policies and event channel.
    pub fn new(
        sandbox_policy: SandboxPolicy,
        approval_policy: AskForApproval,
        exec_policy: Policy,
        tx_event: async_channel::Sender<Event>,
    ) -> Self {
        Self {
            sandbox_policy,
            approval_policy,
            exec_policy,
            allow_list: Vec::new(),
            tx_event,
        }
    }

    /// Add a command prefix to the session allow-list.
    pub fn add_to_allow_list(&mut self, prefix: Vec<String>) {
        if !self.allow_list.contains(&prefix) {
            self.allow_list.push(prefix);
        }
    }

    /// Check whether `command` is on the session allow-list.
    fn is_allow_listed(&self, command: &[String]) -> bool {
        self.allow_list
            .iter()
            .any(|prefix| command.len() >= prefix.len() && command[..prefix.len()] == prefix[..])
    }

    /// Validate that the command is permitted under the active `SandboxPolicy`.
    ///
    /// Returns `Ok(())` when the command may proceed, or a `CodexError` with
    /// `ErrorCode::SandboxViolation` when it must be blocked.
    fn check_sandbox_policy(&self, command: &[String], cwd: &Path) -> Result<(), CodexError> {
        match &self.sandbox_policy {
            SandboxPolicy::ReadOnly { .. } => {
                // Under ReadOnly, only known-safe (read-only) commands are allowed.
                if !shell_command::is_safe_command(command) {
                    return Err(CodexError::new(
                        ErrorCode::SandboxViolation,
                        format!(
                            "command '{}' is not allowed under ReadOnly sandbox policy",
                            command.join(" ")
                        ),
                    ));
                }
            }
            SandboxPolicy::WorkspaceWrite { .. } => {
                // Dangerous commands are always blocked in workspace-write mode.
                if shell_command::is_dangerous_command(command) {
                    return Err(CodexError::new(
                        ErrorCode::SandboxViolation,
                        format!(
                            "dangerous command '{}' is not allowed under WorkspaceWrite sandbox policy",
                            command.join(" ")
                        ),
                    ));
                }
                // If the command references paths outside writable roots, block it.
                let writable = self.sandbox_policy.get_writable_roots_with_cwd(cwd);
                for token in command.iter().skip(1) {
                    let path = Path::new(token);
                    if path.is_absolute() {
                        let in_writable = writable.iter().any(|wr| wr.is_path_writable(path));
                        if !in_writable && !shell_command::is_safe_command(command) {
                            return Err(CodexError::new(
                                ErrorCode::SandboxViolation,
                                format!(
                                    "path '{}' is outside writable roots under WorkspaceWrite sandbox policy",
                                    token
                                ),
                            ));
                        }
                    }
                }
            }
            SandboxPolicy::DangerFullAccess => {
                // No restrictions.
            }
            SandboxPolicy::ExternalSandbox { .. } => {
                // External sandbox — the OS-level sandbox handles restrictions.
            }
        }
        Ok(())
    }

    /// Determine whether the command requires user approval based on the
    /// active `AskForApproval` policy.
    ///
    /// Returns `true` when approval must be requested before execution.
    fn needs_approval(&self, command: &[String]) -> bool {
        match &self.approval_policy {
            AskForApproval::Never => false,
            AskForApproval::UnlessTrusted => !self.is_allow_listed(command),
            AskForApproval::OnFailure => false, // approval requested *after* failure
            AskForApproval::OnRequest => {
                // Check exec policy; if no explicit allow, prompt.
                let eval = self.exec_policy.check(command, &|_| Decision::Prompt);
                eval.decision != Decision::Allow && !self.is_allow_listed(command)
            }
            AskForApproval::Reject(_) => false,
        }
    }

    /// Determine whether approval is needed after a failed execution
    /// (non-zero exit code) under the `OnFailure` policy.
    fn needs_post_failure_approval(&self) -> bool {
        matches!(self.approval_policy, AskForApproval::OnFailure)
    }

    /// Send an event to the event queue, ignoring channel-closed errors.
    async fn send_event(&self, msg: EventMsg) {
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            msg,
        };
        let _ = self.tx_event.send(event).await;
    }

    /// Execute a shell command inside the sandbox.
    ///
    /// 1. Validates the command against the active `SandboxPolicy`.
    /// 2. Checks the `AskForApproval` policy (pre-execution).
    /// 3. Emits `ExecCommandBegin`.
    /// 4. Spawns the child process.
    /// 5. Emits `ExecCommandEnd`.
    /// 6. Optionally requests post-failure approval (`OnFailure` policy).
    ///
    /// Returns `ExecResult` on success, or `CodexError` on sandbox violation
    /// or process spawn failure.
    pub async fn execute(
        &self,
        command: Vec<String>,
        cwd: &Path,
        call_id: &str,
        turn_id: &str,
    ) -> Result<ExecResult, CodexError> {
        // ── 1. Sandbox policy check ──────────────────────────────
        if let Err(violation) = self.check_sandbox_policy(&command, cwd) {
            self.send_event(EventMsg::Error(ErrorEvent {
                message: violation.message.clone(),
                codex_error_info: None,
            }))
            .await;
            return Err(violation);
        }

        // ── 2. Pre-execution approval ────────────────────────────
        if self.needs_approval(&command) {
            let parsed_cmd = vec![shell_command::classify_command(&command)];
            self.send_event(EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                call_id: call_id.to_string(),
                approval_id: None,
                turn_id: turn_id.to_string(),
                command: command.clone(),
                cwd: cwd.to_path_buf(),
                reason: Some("command requires approval".to_string()),
                network_approval_context: None,
                proposed_execpolicy_amendment: None,
                proposed_network_policy_amendments: None,
                additional_permissions: None,
                available_decisions: None,
                parsed_cmd,
            }))
            .await;

            return Err(CodexError::new(
                ErrorCode::ApprovalDenied,
                "command execution paused pending approval",
            ));
        }

        let parsed_cmd = vec![shell_command::classify_command(&command)];

        // ── 3. ExecCommandBegin ──────────────────────────────────
        self.send_event(EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.to_string(),
            process_id: None,
            turn_id: turn_id.to_string(),
            command: command.clone(),
            cwd: cwd.to_path_buf(),
            parsed_cmd: parsed_cmd.clone(),
            source: ExecCommandSource::Agent,
            interaction_input: None,
        }))
        .await;

        // ── 4. Spawn process ─────────────────────────────────────
        let start = Instant::now();
        let result = spawn_process(&command, cwd).await;
        let duration = start.elapsed();

        match result {
            Ok(exec_result) => {
                let status = if exec_result.exit_code == 0 {
                    ExecCommandStatus::Completed
                } else {
                    ExecCommandStatus::Failed
                };

                // ── 5. ExecCommandEnd ────────────────────────────
                self.send_event(EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                    call_id: call_id.to_string(),
                    process_id: None,
                    turn_id: turn_id.to_string(),
                    command: command.clone(),
                    cwd: cwd.to_path_buf(),
                    parsed_cmd,
                    source: ExecCommandSource::Agent,
                    interaction_input: None,
                    stdout: exec_result.stdout.clone(),
                    stderr: exec_result.stderr.clone(),
                    aggregated_output: format!("{}{}", exec_result.stdout, exec_result.stderr),
                    exit_code: exec_result.exit_code,
                    duration,
                    formatted_output: format_output(&exec_result),
                    status,
                }))
                .await;

                // ── 6. Post-failure approval (OnFailure) ─────────
                if exec_result.exit_code != 0 && self.needs_post_failure_approval() {
                    let parsed_cmd_post = vec![shell_command::classify_command(&command)];
                    self.send_event(EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                        call_id: call_id.to_string(),
                        approval_id: None,
                        turn_id: turn_id.to_string(),
                        command: command.clone(),
                        cwd: cwd.to_path_buf(),
                        reason: Some(format!(
                            "command exited with code {}",
                            exec_result.exit_code
                        )),
                        network_approval_context: None,
                        proposed_execpolicy_amendment: None,
                        proposed_network_policy_amendments: None,
                        additional_permissions: None,
                        available_decisions: None,
                        parsed_cmd: parsed_cmd_post,
                    }))
                    .await;
                }

                Ok(exec_result)
            }
            Err(spawn_err) => {
                // Emit error + end event on spawn failure.
                self.send_event(EventMsg::Error(ErrorEvent {
                    message: spawn_err.message.clone(),
                    codex_error_info: None,
                }))
                .await;

                self.send_event(EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                    call_id: call_id.to_string(),
                    process_id: None,
                    turn_id: turn_id.to_string(),
                    command,
                    cwd: cwd.to_path_buf(),
                    parsed_cmd,
                    source: ExecCommandSource::Agent,
                    interaction_input: None,
                    stdout: String::new(),
                    stderr: spawn_err.message.clone(),
                    aggregated_output: spawn_err.message.clone(),
                    exit_code: -1,
                    duration,
                    formatted_output: spawn_err.message.clone(),
                    status: ExecCommandStatus::Failed,
                }))
                .await;

                Err(spawn_err)
            }
        }
    }
}

// ── Free functions ───────────────────────────────────────────────

/// Spawn a child process and collect its output.
async fn spawn_process(command: &[String], cwd: &Path) -> Result<ExecResult, CodexError> {
    if command.is_empty() {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "cannot execute empty command",
        ));
    }

    let output = tokio::process::Command::new(&command[0])
        .args(&command[1..])
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to spawn process '{}': {e}", command[0]),
            )
        })?;

    Ok(ExecResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Format stdout + stderr into a single display string.
fn format_output(result: &ExecResult) -> String {
    let mut out = String::new();
    if !result.stdout.is_empty() {
        out.push_str(&result.stdout);
    }
    if !result.stderr.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&result.stderr);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execpolicy::Policy;

    /// Helper: build a CommandExecutor with the given policies and a fresh channel.
    fn make_executor(
        sandbox: SandboxPolicy,
        approval: AskForApproval,
    ) -> (CommandExecutor, async_channel::Receiver<Event>) {
        let (tx, rx) = async_channel::unbounded();
        let executor = CommandExecutor::new(sandbox, approval, Policy::empty(), tx);
        (executor, rx)
    }

    // ── Sandbox policy unit tests ────────────────────────────────

    #[test]
    fn readonly_allows_safe_commands() {
        let (exec, _rx) =
            make_executor(SandboxPolicy::new_read_only_policy(), AskForApproval::Never);
        let cmd = vec!["ls".to_string(), "-la".to_string()];
        assert!(exec.check_sandbox_policy(&cmd, Path::new("/tmp")).is_ok());
    }

    #[test]
    fn readonly_blocks_unsafe_commands() {
        let (exec, _rx) =
            make_executor(SandboxPolicy::new_read_only_policy(), AskForApproval::Never);
        let cmd = vec!["rm".to_string(), "file.txt".to_string()];
        let err = exec
            .check_sandbox_policy(&cmd, Path::new("/tmp"))
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::SandboxViolation);
    }

    #[test]
    fn workspace_write_blocks_dangerous_commands() {
        let (exec, _rx) = make_executor(
            SandboxPolicy::new_workspace_write_policy(),
            AskForApproval::Never,
        );
        let cmd = vec!["rm".to_string(), "-rf".to_string(), "/".to_string()];
        let err = exec
            .check_sandbox_policy(&cmd, Path::new("/workspace"))
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::SandboxViolation);
    }

    #[test]
    fn danger_full_access_allows_everything() {
        let (exec, _rx) = make_executor(SandboxPolicy::DangerFullAccess, AskForApproval::Never);
        let cmd = vec!["rm".to_string(), "-rf".to_string(), "/".to_string()];
        assert!(exec.check_sandbox_policy(&cmd, Path::new("/")).is_ok());
    }

    // ── Approval policy unit tests ───────────────────────────────

    #[test]
    fn never_approval_does_not_need_approval() {
        let (exec, _rx) = make_executor(SandboxPolicy::DangerFullAccess, AskForApproval::Never);
        assert!(!exec.needs_approval(&["git".to_string(), "status".to_string()]));
    }

    #[test]
    fn unless_trusted_needs_approval_for_unknown() {
        let (exec, _rx) = make_executor(
            SandboxPolicy::DangerFullAccess,
            AskForApproval::UnlessTrusted,
        );
        assert!(exec.needs_approval(&["curl".to_string(), "https://example.com".to_string()]));
    }

    #[test]
    fn unless_trusted_skips_approval_for_allow_listed() {
        let (mut exec, _rx) = make_executor(
            SandboxPolicy::DangerFullAccess,
            AskForApproval::UnlessTrusted,
        );
        exec.add_to_allow_list(vec!["git".to_string()]);
        assert!(!exec.needs_approval(&["git".to_string(), "status".to_string()]));
    }

    #[test]
    fn on_failure_does_not_need_pre_approval() {
        let (exec, _rx) = make_executor(SandboxPolicy::DangerFullAccess, AskForApproval::OnFailure);
        assert!(!exec.needs_approval(&["npm".to_string(), "test".to_string()]));
        assert!(exec.needs_post_failure_approval());
    }

    // ── Execute integration tests ────────────────────────────────

    #[tokio::test]
    async fn execute_echo_succeeds() {
        let (exec, rx) = make_executor(SandboxPolicy::DangerFullAccess, AskForApproval::Never);
        let cwd = std::env::current_dir().unwrap();
        let result = exec
            .execute(
                vec!["echo".to_string(), "hello".to_string()],
                &cwd,
                "call-1",
                "turn-1",
            )
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));

        // Should have received ExecCommandBegin + ExecCommandEnd
        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        assert!(events.len() >= 2);
        assert!(matches!(events[0].msg, EventMsg::ExecCommandBegin(_)));
        assert!(matches!(events[1].msg, EventMsg::ExecCommandEnd(_)));
    }

    #[tokio::test]
    async fn execute_blocked_by_readonly_sandbox() {
        let (exec, rx) =
            make_executor(SandboxPolicy::new_read_only_policy(), AskForApproval::Never);
        let cwd = std::env::current_dir().unwrap();
        let err = exec
            .execute(
                vec!["mkdir".to_string(), "/tmp/test_dir".to_string()],
                &cwd,
                "call-2",
                "turn-2",
            )
            .await
            .unwrap_err();

        assert_eq!(err.code, ErrorCode::SandboxViolation);

        // Should have received an Error event
        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::Error(_))));
    }

    #[tokio::test]
    async fn execute_paused_for_approval() {
        let (exec, rx) = make_executor(
            SandboxPolicy::DangerFullAccess,
            AskForApproval::UnlessTrusted,
        );
        let cwd = std::env::current_dir().unwrap();
        let err = exec
            .execute(
                vec!["curl".to_string(), "https://example.com".to_string()],
                &cwd,
                "call-3",
                "turn-3",
            )
            .await
            .unwrap_err();

        assert_eq!(err.code, ErrorCode::ApprovalDenied);

        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::ExecApprovalRequest(_))));
    }

    #[tokio::test]
    async fn execute_empty_command_returns_error() {
        let (exec, _rx) = make_executor(SandboxPolicy::DangerFullAccess, AskForApproval::Never);
        let cwd = std::env::current_dir().unwrap();
        let err = exec
            .execute(vec![], &cwd, "call-4", "turn-4")
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn on_failure_emits_approval_request_on_nonzero_exit() {
        let (exec, rx) = make_executor(SandboxPolicy::DangerFullAccess, AskForApproval::OnFailure);
        let cwd = std::env::current_dir().unwrap();
        let result = exec
            .execute(vec!["false".to_string()], &cwd, "call-5", "turn-5")
            .await
            .unwrap();

        assert_ne!(result.exit_code, 0);

        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        // Begin + End + ApprovalRequest
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::ExecApprovalRequest(_))));
    }
}
