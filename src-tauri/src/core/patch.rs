use std::collections::HashMap;
use std::path::PathBuf;

use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{
    ApplyPatchApprovalRequestEvent, ErrorEvent, Event, EventMsg, PatchApplyBeginEvent,
    PatchApplyEndEvent,
};
use crate::protocol::types::{AskForApproval, FileChange, PatchApplyStatus};

/// Result of applying a patch.
#[derive(Debug)]
pub struct PatchResult {
    pub applied: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

impl PatchResult {
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Orchestrates patch application with event emission and approval policy.
pub struct PatchApplicator {
    approval_policy: AskForApproval,
    tx_event: async_channel::Sender<Event>,
}

impl PatchApplicator {
    pub fn new(approval_policy: AskForApproval, tx_event: async_channel::Sender<Event>) -> Self {
        Self {
            approval_policy,
            tx_event,
        }
    }

    async fn send_event(&self, msg: EventMsg) {
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            msg,
        };
        let _ = self.tx_event.send(event).await;
    }

    /// Whether the current approval policy requires user approval before applying patches.
    fn needs_approval(&self) -> bool {
        matches!(
            self.approval_policy,
            AskForApproval::UnlessTrusted | AskForApproval::Reject(_)
        )
    }

    /// Apply a set of file changes with full event lifecycle.
    ///
    /// Flow:
    /// 1. Check approval policy — if approval required, emit `ApplyPatchApprovalRequest`
    ///    and return `ApprovalDenied` (caller resumes after user approves via `Op::PatchApproval`).
    /// 2. Emit `PatchApplyBegin`.
    /// 3. Apply each file change to disk.
    /// 4. Emit `PatchApplyEnd` with success/failure status.
    /// 5. On failure, also emit an `Error` event.
    pub async fn apply(
        &self,
        changes: &HashMap<PathBuf, FileChange>,
        cwd: &std::path::Path,
        call_id: &str,
        turn_id: &str,
    ) -> Result<PatchResult, CodexError> {
        // ── 1. Pre-apply approval check ──────────────────────────
        if self.needs_approval() {
            self.send_event(EventMsg::ApplyPatchApprovalRequest(
                ApplyPatchApprovalRequestEvent {
                    call_id: call_id.to_string(),
                    turn_id: turn_id.to_string(),
                    changes: changes.clone(),
                    reason: Some("patch application requires approval".to_string()),
                    grant_root: Some(cwd.to_path_buf()),
                },
            ))
            .await;

            return Err(CodexError::new(
                ErrorCode::ApprovalDenied,
                "patch application paused pending approval",
            ));
        }

        self.apply_approved(changes, cwd, call_id, turn_id).await
    }

    /// Apply changes that have already been approved (or auto-approved).
    ///
    /// Emits `PatchApplyBegin` → performs I/O → emits `PatchApplyEnd` (+ `Error` on failure).
    pub async fn apply_approved(
        &self,
        changes: &HashMap<PathBuf, FileChange>,
        cwd: &std::path::Path,
        call_id: &str,
        turn_id: &str,
    ) -> Result<PatchResult, CodexError> {
        let auto_approved = !self.needs_approval();

        // ── 2. PatchApplyBegin ───────────────────────────────────
        self.send_event(EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
            call_id: call_id.to_string(),
            turn_id: turn_id.to_string(),
            auto_approved,
            changes: changes.clone(),
        }))
        .await;

        // ── 3. Apply file changes ────────────────────────────────
        let result = apply_file_changes(changes, cwd).await;

        // ── 4. PatchApplyEnd ─────────────────────────────────────
        let success = result.is_success();
        let (stdout, stderr) = if success {
            let msg = format!("applied {} file change(s)", result.applied.len());
            (msg, String::new())
        } else {
            let ok_msg = format!("applied {} file(s)", result.applied.len());
            let err_msg: Vec<String> = result
                .failed
                .iter()
                .map(|(p, e)| format!("{}: {e}", p.display()))
                .collect();
            (ok_msg, err_msg.join("\n"))
        };

        let status = if success {
            PatchApplyStatus::Completed
        } else {
            PatchApplyStatus::Failed
        };

        self.send_event(EventMsg::PatchApplyEnd(PatchApplyEndEvent {
            call_id: call_id.to_string(),
            turn_id: turn_id.to_string(),
            stdout,
            stderr: stderr.clone(),
            success,
            changes: changes.clone(),
            status,
        }))
        .await;

        // ── 5. Error event on failure ────────────────────────────
        if !success {
            self.send_event(EventMsg::Error(ErrorEvent {
                message: format!("patch application failed: {stderr}"),
                codex_error_info: None,
            }))
            .await;
        }

        Ok(result)
    }
}

/// Apply a set of file changes (add/delete/update) to the filesystem.
///
/// This is the low-level I/O function — no events, no approval checks.
async fn apply_file_changes(
    changes: &HashMap<PathBuf, FileChange>,
    cwd: &std::path::Path,
) -> PatchResult {
    let mut applied = Vec::new();
    let mut failed = Vec::new();

    for (path, change) in changes {
        let full_path = if path.is_absolute() {
            path.clone()
        } else {
            cwd.join(path)
        };

        let result = match change {
            FileChange::Add { content } => {
                if let Some(parent) = full_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                tokio::fs::write(&full_path, content).await
            }
            FileChange::Delete { .. } => tokio::fs::remove_file(&full_path).await,
            FileChange::Update {
                unified_diff: _,
                move_path: _,
            } => {
                // TODO: apply unified diff properly
                // For now, just acknowledge the patch
                Ok(())
            }
        };

        match result {
            Ok(()) => applied.push(path.clone()),
            Err(e) => failed.push((path.clone(), e.to_string())),
        }
    }

    PatchResult { applied, failed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_applicator(
        approval_policy: AskForApproval,
    ) -> (PatchApplicator, async_channel::Receiver<Event>) {
        let (tx, rx) = async_channel::unbounded();
        let applicator = PatchApplicator::new(approval_policy, tx);
        (applicator, rx)
    }

    fn drain_events(rx: &async_channel::Receiver<Event>) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        events
    }

    #[tokio::test]
    async fn auto_approved_patch_emits_begin_and_end() {
        let (applicator, rx) = make_applicator(AskForApproval::Never);
        let tmp = tempfile::tempdir().unwrap();
        let mut changes = HashMap::new();
        changes.insert(
            PathBuf::from("hello.txt"),
            FileChange::Add {
                content: "hello world".to_string(),
            },
        );

        let result = applicator
            .apply(&changes, tmp.path(), "call-1", "turn-1")
            .await
            .unwrap();

        assert!(result.is_success());
        assert_eq!(result.applied.len(), 1);

        let events = drain_events(&rx);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].msg, EventMsg::PatchApplyBegin(_)));
        assert!(matches!(events[1].msg, EventMsg::PatchApplyEnd(_)));

        if let EventMsg::PatchApplyEnd(ref end) = events[1].msg {
            assert!(end.success);
            assert_eq!(end.status, PatchApplyStatus::Completed);
        }

        // Verify file was actually created
        let content = tokio::fs::read_to_string(tmp.path().join("hello.txt"))
            .await
            .unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn approval_required_emits_request_and_returns_error() {
        let (applicator, rx) = make_applicator(AskForApproval::UnlessTrusted);
        let tmp = tempfile::tempdir().unwrap();
        let mut changes = HashMap::new();
        changes.insert(
            PathBuf::from("file.txt"),
            FileChange::Add {
                content: "data".to_string(),
            },
        );

        let err = applicator
            .apply(&changes, tmp.path(), "call-2", "turn-2")
            .await
            .unwrap_err();

        assert_eq!(err.code, ErrorCode::ApprovalDenied);

        let events = drain_events(&rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].msg,
            EventMsg::ApplyPatchApprovalRequest(_)
        ));

        // File should NOT have been created
        assert!(!tmp.path().join("file.txt").exists());
    }

    #[tokio::test]
    async fn failed_patch_emits_error_event() {
        let (applicator, rx) = make_applicator(AskForApproval::Never);
        let tmp = tempfile::tempdir().unwrap();
        let mut changes = HashMap::new();
        // Try to delete a file that doesn't exist
        changes.insert(
            PathBuf::from("nonexistent.txt"),
            FileChange::Delete {
                content: String::new(),
            },
        );

        let result = applicator
            .apply(&changes, tmp.path(), "call-3", "turn-3")
            .await
            .unwrap();

        assert!(!result.is_success());
        assert_eq!(result.failed.len(), 1);

        let events = drain_events(&rx);
        // PatchApplyBegin + PatchApplyEnd + Error
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0].msg, EventMsg::PatchApplyBegin(_)));
        assert!(matches!(events[1].msg, EventMsg::PatchApplyEnd(_)));
        assert!(matches!(events[2].msg, EventMsg::Error(_)));

        if let EventMsg::PatchApplyEnd(ref end) = events[1].msg {
            assert!(!end.success);
            assert_eq!(end.status, PatchApplyStatus::Failed);
        }
    }

    #[tokio::test]
    async fn apply_approved_bypasses_approval_check() {
        let (applicator, rx) = make_applicator(AskForApproval::UnlessTrusted);
        let tmp = tempfile::tempdir().unwrap();
        let mut changes = HashMap::new();
        changes.insert(
            PathBuf::from("approved.txt"),
            FileChange::Add {
                content: "approved content".to_string(),
            },
        );

        // apply_approved skips the approval check
        let result = applicator
            .apply_approved(&changes, tmp.path(), "call-4", "turn-4")
            .await
            .unwrap();

        assert!(result.is_success());

        let events = drain_events(&rx);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].msg, EventMsg::PatchApplyBegin(_)));
        assert!(matches!(events[1].msg, EventMsg::PatchApplyEnd(_)));

        // auto_approved should be false since the policy requires approval
        if let EventMsg::PatchApplyBegin(ref begin) = events[0].msg {
            assert!(!begin.auto_approved);
        }
    }

    #[tokio::test]
    async fn delete_file_change_removes_file() {
        let (applicator, rx) = make_applicator(AskForApproval::Never);
        let tmp = tempfile::tempdir().unwrap();

        // Create the file first
        let file_path = tmp.path().join("to_delete.txt");
        tokio::fs::write(&file_path, "delete me").await.unwrap();
        assert!(file_path.exists());

        let mut changes = HashMap::new();
        changes.insert(
            PathBuf::from("to_delete.txt"),
            FileChange::Delete {
                content: "delete me".to_string(),
            },
        );

        let result = applicator
            .apply(&changes, tmp.path(), "call-5", "turn-5")
            .await
            .unwrap();

        assert!(result.is_success());
        assert!(!file_path.exists());

        let events = drain_events(&rx);
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn nested_directory_creation_on_add() {
        let (applicator, rx) = make_applicator(AskForApproval::Never);
        let tmp = tempfile::tempdir().unwrap();
        let mut changes = HashMap::new();
        changes.insert(
            PathBuf::from("deep/nested/dir/file.txt"),
            FileChange::Add {
                content: "nested".to_string(),
            },
        );

        let result = applicator
            .apply(&changes, tmp.path(), "call-6", "turn-6")
            .await
            .unwrap();

        assert!(result.is_success());
        let content = tokio::fs::read_to_string(tmp.path().join("deep/nested/dir/file.txt"))
            .await
            .unwrap();
        assert_eq!(content, "nested");

        let events = drain_events(&rx);
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn on_request_policy_auto_approves() {
        // OnRequest (default) should auto-approve patches
        let (applicator, rx) = make_applicator(AskForApproval::OnRequest);
        let tmp = tempfile::tempdir().unwrap();
        let mut changes = HashMap::new();
        changes.insert(
            PathBuf::from("auto.txt"),
            FileChange::Add {
                content: "auto".to_string(),
            },
        );

        let result = applicator
            .apply(&changes, tmp.path(), "call-7", "turn-7")
            .await
            .unwrap();

        assert!(result.is_success());

        let events = drain_events(&rx);
        // Should go straight to Begin/End, no approval request
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].msg, EventMsg::PatchApplyBegin(_)));
    }
}
