//! Heuristic decision logic for commands not matched by any policy rule.

use crate::execpolicy::prefix_rule::Decision;
use crate::protocol::types::{AskForApproval, SandboxPolicy};
use crate::shell_command::is_dangerous_command;

/// Derive a [`Decision`] for a command not matched by any execpolicy rule.
pub fn render_decision_for_unmatched_command(
    approval_policy: AskForApproval,
    sandbox_policy: &SandboxPolicy,
    command: &[String],
    used_complex_parsing: bool,
) -> Decision {
    if is_known_safe_command(command) && !used_complex_parsing {
        return Decision::Allow;
    }

    if is_dangerous_command(command) {
        return if matches!(approval_policy, AskForApproval::Never) {
            Decision::Forbidden
        } else {
            Decision::Prompt
        };
    }

    match approval_policy {
        AskForApproval::Never | AskForApproval::OnFailure => Decision::Allow,
        AskForApproval::UnlessTrusted => Decision::Prompt,
        AskForApproval::OnRequest | AskForApproval::Reject(_) => match sandbox_policy {
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. } => {
                Decision::Allow
            }
            SandboxPolicy::ReadOnly { .. } | SandboxPolicy::WorkspaceWrite { .. } => {
                Decision::Allow
            }
        },
    }
}

/// A minimal set of commands considered safe without sandbox.
fn is_known_safe_command(tokens: &[String]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    let cmd = tokens[0].as_str();
    // Extract basename for absolute paths
    let basename = std::path::Path::new(cmd)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(cmd);
    matches!(
        basename,
        "ls" | "cat"
            | "head"
            | "tail"
            | "wc"
            | "echo"
            | "pwd"
            | "date"
            | "whoami"
            | "hostname"
            | "uname"
            | "which"
            | "type"
            | "file"
            | "stat"
            | "du"
            | "df"
            | "env"
            | "printenv"
            | "true"
            | "false"
            | "test"
            | "["
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn safe_commands_are_allowed() {
        assert!(is_known_safe_command(&cmd(&["ls", "-la"])));
        assert!(is_known_safe_command(&cmd(&["cat", "file.txt"])));
        assert!(is_known_safe_command(&cmd(&["echo", "hello"])));
    }

    #[test]
    fn unknown_commands_are_not_safe() {
        assert!(!is_known_safe_command(&cmd(&[
            "curl",
            "https://example.com"
        ])));
        assert!(!is_known_safe_command(&cmd(&["python3", "-c", "code"])));
    }

    #[test]
    fn dangerous_command_prompts_or_forbids() {
        let d = render_decision_for_unmatched_command(
            AskForApproval::OnRequest,
            &SandboxPolicy::DangerFullAccess,
            &cmd(&["rm", "-rf", "/"]),
            false,
        );
        assert_eq!(d, Decision::Prompt);

        let d = render_decision_for_unmatched_command(
            AskForApproval::Never,
            &SandboxPolicy::DangerFullAccess,
            &cmd(&["rm", "-rf", "/"]),
            false,
        );
        assert_eq!(d, Decision::Forbidden);
    }

    #[test]
    fn safe_command_allowed_regardless() {
        let d = render_decision_for_unmatched_command(
            AskForApproval::UnlessTrusted,
            &SandboxPolicy::new_read_only_policy(),
            &cmd(&["ls"]),
            false,
        );
        assert_eq!(d, Decision::Allow);
    }
}
