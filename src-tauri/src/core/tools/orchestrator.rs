//! Tool orchestrator — approval + sandbox + execution.
//!
//! Drives the sequence: check approval requirement → check cache →
//! request user approval (if needed) → handle side effects → execute.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::core::session::Session;
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::types::{AskForApproval, ExecPolicyAmendment, ReviewDecision};

/// Re-export from exec_policy for convenience.
pub use crate::core::exec_policy::ExecApprovalRequirement;

/// Determine the approval requirement based on the current policy.
pub fn default_exec_approval_requirement(policy: &AskForApproval) -> ExecApprovalRequirement {
    match policy {
        AskForApproval::Never | AskForApproval::OnFailure => ExecApprovalRequirement::Skip {
            bypass_sandbox: false,
            proposed_execpolicy_amendment: None,
        },
        AskForApproval::UnlessTrusted | AskForApproval::OnRequest => {
            ExecApprovalRequirement::NeedsApproval {
                reason: None,
                proposed_execpolicy_amendment: None,
            }
        }
        AskForApproval::Reject(_) => ExecApprovalRequirement::Forbidden {
            reason: "approval policy rejects all tool executions".into(),
        },
    }
}

/// Cache key for per-command approval decisions.
#[derive(Clone, Debug, Serialize)]
struct ApprovalKey {
    command: Vec<String>,
    cwd: String,
}

/// Central orchestrator for tool execution with approval support.
pub struct ToolOrchestrator;

impl ToolOrchestrator {
    pub fn new() -> Self {
        Self
    }

    /// Run a tool with approval checks, caching, and side effects.
    pub async fn run(
        &self,
        session: &Session,
        call_id: &str,
        turn_id: &str,
        _tool_name: &str,
        command_for_approval: Vec<String>,
        cwd: PathBuf,
        requirement: ExecApprovalRequirement,
        execute: impl std::future::Future<Output = Result<serde_json::Value, CodexError>>,
    ) -> Result<serde_json::Value, CodexError> {
        match requirement {
            ExecApprovalRequirement::Forbidden { reason } => {
                return Err(CodexError::new(ErrorCode::ToolExecutionFailed, reason));
            }
            ExecApprovalRequirement::Skip { .. } => {}
            ExecApprovalRequirement::NeedsApproval {
                reason,
                proposed_execpolicy_amendment,
            } => {
                let approval_key = ApprovalKey {
                    command: command_for_approval.clone(),
                    cwd: cwd.to_string_lossy().into_owned(),
                };

                let decision = session
                    .with_cached_approval(
                        &[approval_key],
                        session.request_exec_approval(
                            call_id.to_string(),
                            turn_id.to_string(),
                            command_for_approval.clone(),
                            cwd,
                            reason,
                            proposed_execpolicy_amendment,
                        ),
                    )
                    .await;

                match &decision {
                    ReviewDecision::Approved => {}
                    ReviewDecision::ApprovedForSession => {
                        session
                            .add_to_exec_allow_list(command_for_approval)
                            .await;
                    }
                    ReviewDecision::ApprovedExecpolicyAmendment {
                        proposed_execpolicy_amendment,
                    } => {
                        session
                            .add_to_exec_allow_list(
                                proposed_execpolicy_amendment.command.clone(),
                            )
                            .await;
                    }
                    ReviewDecision::Denied => {
                        return Err(CodexError::new(
                            ErrorCode::ApprovalDenied,
                            "tool execution rejected by user",
                        ));
                    }
                    ReviewDecision::Abort => {
                        session.interrupt().await;
                        return Err(CodexError::new(
                            ErrorCode::ApprovalDenied,
                            "tool execution aborted by user",
                        ));
                    }
                    _ => {
                        return Err(CodexError::new(
                            ErrorCode::ApprovalDenied,
                            "tool execution rejected by user",
                        ));
                    }
                }
            }
        }

        execute.await
    }
}

impl Default for ToolOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_requirement_never_skips() {
        let req = default_exec_approval_requirement(&AskForApproval::Never);
        assert!(matches!(req, ExecApprovalRequirement::Skip { .. }));
    }

    #[test]
    fn default_requirement_on_failure_skips() {
        let req = default_exec_approval_requirement(&AskForApproval::OnFailure);
        assert!(matches!(req, ExecApprovalRequirement::Skip { .. }));
    }

    #[test]
    fn default_requirement_unless_trusted_needs_approval() {
        let req = default_exec_approval_requirement(&AskForApproval::UnlessTrusted);
        assert!(matches!(req, ExecApprovalRequirement::NeedsApproval { .. }));
    }

    #[test]
    fn default_requirement_on_request_needs_approval() {
        let req = default_exec_approval_requirement(&AskForApproval::OnRequest);
        assert!(matches!(req, ExecApprovalRequirement::NeedsApproval { .. }));
    }

    #[tokio::test]
    async fn orchestrator_skip_executes_directly() {
        let orch = ToolOrchestrator::new();
        let (tx, _rx) = async_channel::unbounded();
        let session = Session::new(
            std::path::PathBuf::from("/tmp"),
            crate::config::ConfigLayerStack::default(),
            tx,
        );
        let result = orch
            .run(
                &session, "call-1", "turn-1", "echo",
                vec!["echo".into(), "hello".into()],
                PathBuf::from("/tmp"),
                ExecApprovalRequirement::Skip { bypass_sandbox: false, proposed_execpolicy_amendment: None },
                async { Ok(serde_json::json!({"ok": true})) },
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!({"ok": true}));
    }

    #[tokio::test]
    async fn orchestrator_forbidden_returns_error() {
        let orch = ToolOrchestrator::new();
        let (tx, _rx) = async_channel::unbounded();
        let session = Session::new(
            std::path::PathBuf::from("/tmp"),
            crate::config::ConfigLayerStack::default(),
            tx,
        );
        let result = orch
            .run(
                &session, "call-1", "turn-1", "rm",
                vec!["rm".into(), "-rf".into()],
                PathBuf::from("/tmp"),
                ExecApprovalRequirement::Forbidden { reason: "dangerous".into() },
                async { Ok(serde_json::json!({})) },
            )
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ToolExecutionFailed);
    }

    #[tokio::test]
    async fn orchestrator_needs_approval_approved_executes() {
        let orch = ToolOrchestrator::new();
        let (tx, _rx) = async_channel::unbounded();
        let session = Arc::new(Session::new(
            std::path::PathBuf::from("/tmp"),
            crate::config::ConfigLayerStack::default(),
            tx,
        ));

        let session_clone = Arc::clone(&session);
        let approver = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            session_clone.notify_approval("call-1", ReviewDecision::Approved).await;
        });

        let result = orch
            .run(
                &session, "call-1", "turn-1", "ls",
                vec!["ls".into()],
                PathBuf::from("/tmp"),
                ExecApprovalRequirement::NeedsApproval {
                    reason: None,
                    proposed_execpolicy_amendment: None,
                },
                async { Ok(serde_json::json!({"files": ["a.txt"]})) },
            )
            .await;

        approver.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!({"files": ["a.txt"]}));
    }

    #[tokio::test]
    async fn orchestrator_needs_approval_denied_returns_error() {
        let orch = ToolOrchestrator::new();
        let (tx, _rx) = async_channel::unbounded();
        let session = Arc::new(Session::new(
            std::path::PathBuf::from("/tmp"),
            crate::config::ConfigLayerStack::default(),
            tx,
        ));

        let session_clone = Arc::clone(&session);
        let denier = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            session_clone.notify_approval("call-2", ReviewDecision::Denied).await;
        });

        let result = orch
            .run(
                &session, "call-2", "turn-1", "rm",
                vec!["rm".into(), "-rf".into(), "/".into()],
                PathBuf::from("/tmp"),
                ExecApprovalRequirement::NeedsApproval {
                    reason: Some("dangerous command".into()),
                    proposed_execpolicy_amendment: None,
                },
                async { Ok(serde_json::json!({})) },
            )
            .await;

        denier.await.unwrap();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ApprovalDenied);
    }

    #[tokio::test]
    async fn orchestrator_needs_approval_abort_returns_error() {
        let orch = ToolOrchestrator::new();
        let (tx, _rx) = async_channel::unbounded();
        let session = Arc::new(Session::new(
            std::path::PathBuf::from("/tmp"),
            crate::config::ConfigLayerStack::default(),
            tx,
        ));

        let session_clone = Arc::clone(&session);
        let aborter = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            session_clone.notify_approval("call-3", ReviewDecision::Abort).await;
        });

        let result = orch
            .run(
                &session, "call-3", "turn-1", "cmd",
                vec!["cmd".into()],
                PathBuf::from("/tmp"),
                ExecApprovalRequirement::NeedsApproval {
                    reason: None,
                    proposed_execpolicy_amendment: None,
                },
                async { Ok(serde_json::json!({})) },
            )
            .await;

        aborter.await.unwrap();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ApprovalDenied);
    }

    #[tokio::test]
    async fn approval_event_is_emitted() {
        let (tx, rx) = async_channel::unbounded();
        let session = Arc::new(Session::new(
            std::path::PathBuf::from("/tmp"),
            crate::config::ConfigLayerStack::default(),
            tx,
        ));

        let session_clone = Arc::clone(&session);
        let approver = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            session_clone.notify_approval("call-ev", ReviewDecision::Approved).await;
        });

        let orch = ToolOrchestrator::new();
        let _ = orch
            .run(
                &session, "call-ev", "turn-1", "ls",
                vec!["ls".into()],
                PathBuf::from("/tmp"),
                ExecApprovalRequirement::NeedsApproval {
                    reason: None,
                    proposed_execpolicy_amendment: None,
                },
                async { Ok(serde_json::json!({})) },
            )
            .await;

        approver.await.unwrap();

        let mut found = false;
        while let Ok(event) = rx.try_recv() {
            if matches!(event.msg, crate::protocol::event::EventMsg::ExecApprovalRequest(_)) {
                found = true;
                break;
            }
        }
        assert!(found, "ExecApprovalRequest event should have been emitted");
    }

    #[tokio::test]
    async fn orchestrator_approved_for_session_adds_to_allow_list() {
        let orch = ToolOrchestrator::new();
        let (tx, _rx) = async_channel::unbounded();
        let session = Arc::new(Session::new(
            std::path::PathBuf::from("/tmp"),
            crate::config::ConfigLayerStack::default(),
            tx,
        ));

        let session_clone = Arc::clone(&session);
        let approver = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            session_clone.notify_approval("call-s", ReviewDecision::ApprovedForSession).await;
        });

        let _ = orch
            .run(
                &session, "call-s", "turn-1", "cargo",
                vec!["cargo".into(), "test".into()],
                PathBuf::from("/tmp"),
                ExecApprovalRequirement::NeedsApproval {
                    reason: None,
                    proposed_execpolicy_amendment: None,
                },
                async { Ok(serde_json::json!({})) },
            )
            .await;

        approver.await.unwrap();
        assert!(
            session.is_exec_allow_listed(&["cargo".into(), "test".into()]).await,
            "ApprovedForSession should add command to allow list"
        );
    }

    #[tokio::test]
    async fn cached_approval_skips_second_prompt() {
        let (tx, _rx) = async_channel::unbounded();
        let session = Arc::new(Session::new(
            std::path::PathBuf::from("/tmp"),
            crate::config::ConfigLayerStack::default(),
            tx,
        ));

        // First call: approve for session
        let session_clone = Arc::clone(&session);
        let approver = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            session_clone.notify_approval("call-c1", ReviewDecision::ApprovedForSession).await;
        });

        let orch = ToolOrchestrator::new();
        let _ = orch
            .run(
                &session, "call-c1", "turn-1", "ls",
                vec!["ls".into()],
                PathBuf::from("/tmp"),
                ExecApprovalRequirement::NeedsApproval {
                    reason: None,
                    proposed_execpolicy_amendment: None,
                },
                async { Ok(serde_json::json!({})) },
            )
            .await;
        approver.await.unwrap();

        // Second call: same command, should be auto-approved (no oneshot needed)
        let result = orch
            .run(
                &session, "call-c2", "turn-1", "ls",
                vec!["ls".into()],
                PathBuf::from("/tmp"),
                ExecApprovalRequirement::NeedsApproval {
                    reason: None,
                    proposed_execpolicy_amendment: None,
                },
                async { Ok(serde_json::json!({"cached": true})) },
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!({"cached": true}));
    }
}
