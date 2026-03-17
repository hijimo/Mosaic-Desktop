use crate::protocol::types::{
    AskForApproval, Personality, ResponseInputItem, SandboxPolicy,
};

use std::path::Path;

/// Snapshot of turn-level settings used for diffing between turns.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnSettingsSnapshot {
    pub cwd: String,
    pub sandbox_policy: SandboxPolicy,
    pub approval_policy: AskForApproval,
    pub personality: Option<Personality>,
}

impl TurnSettingsSnapshot {
    pub fn new(
        cwd: &Path,
        sandbox_policy: SandboxPolicy,
        approval_policy: AskForApproval,
        personality: Option<Personality>,
    ) -> Self {
        Self {
            cwd: cwd.display().to_string(),
            sandbox_policy,
            approval_policy,
            personality,
        }
    }
}

/// Build a developer-instructions update item when settings change between turns.
///
/// Returns `None` if the settings are identical (no update needed).
pub fn build_settings_update(
    previous: Option<&TurnSettingsSnapshot>,
    current: &TurnSettingsSnapshot,
) -> Option<ResponseInputItem> {
    let Some(prev) = previous else {
        // First turn — emit full context.
        return Some(settings_to_message(current));
    };

    if prev == current {
        return None;
    }

    // Settings changed — emit an update message.
    let mut parts = Vec::new();

    if prev.cwd != current.cwd {
        parts.push(format!("Working directory changed to: {}", current.cwd));
    }
    if prev.sandbox_policy != current.sandbox_policy {
        parts.push(format!(
            "Sandbox policy changed to: {:?}",
            current.sandbox_policy
        ));
    }
    if prev.approval_policy != current.approval_policy {
        parts.push(format!(
            "Approval policy changed to: {:?}",
            current.approval_policy
        ));
    }
    if prev.personality != current.personality {
        parts.push(format!(
            "Personality changed to: {:?}",
            current.personality
        ));
    }

    if parts.is_empty() {
        return None;
    }

    Some(ResponseInputItem::Message {
        role: "developer".into(),
        content: parts.join("\n"),
    })
}

fn settings_to_message(snapshot: &TurnSettingsSnapshot) -> ResponseInputItem {
    let content = format!(
        "Current working directory: {}\nSandbox policy: {:?}\nApproval policy: {:?}",
        snapshot.cwd, snapshot.sandbox_policy, snapshot.approval_policy,
    );
    ResponseInputItem::Message {
        role: "developer".into(),
        content,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn default_snapshot() -> TurnSettingsSnapshot {
        TurnSettingsSnapshot::new(
            &PathBuf::from("/project"),
            SandboxPolicy::new_read_only_policy(),
            AskForApproval::default(),
            None,
        )
    }

    #[test]
    fn first_turn_emits_full_context() {
        let current = default_snapshot();
        let update = build_settings_update(None, &current);
        assert!(update.is_some());
        if let Some(ResponseInputItem::Message { role, content }) = &update {
            assert_eq!(role, "developer");
            assert!(content.contains("/project"));
        }
    }

    #[test]
    fn identical_settings_no_update() {
        let snap = default_snapshot();
        let update = build_settings_update(Some(&snap), &snap);
        assert!(update.is_none());
    }

    #[test]
    fn cwd_change_emits_update() {
        let prev = default_snapshot();
        let mut current = default_snapshot();
        current.cwd = "/other".into();
        let update = build_settings_update(Some(&prev), &current);
        assert!(update.is_some());
        if let Some(ResponseInputItem::Message { content, .. }) = &update {
            assert!(content.contains("/other"));
        }
    }

    #[test]
    fn policy_change_emits_update() {
        let prev = default_snapshot();
        let mut current = default_snapshot();
        current.sandbox_policy = SandboxPolicy::DangerFullAccess;
        let update = build_settings_update(Some(&prev), &current);
        assert!(update.is_some());
        if let Some(ResponseInputItem::Message { content, .. }) = &update {
            assert!(content.contains("Sandbox policy changed"));
        }
    }
}
