//! Tracks which memory artifacts the model reads via tool calls.
//!
//! This module classifies file paths accessed through shell/exec tools
//! and emits tracing events so operators can observe memory usage patterns.

use tracing::debug;

const MEMORIES_USAGE_METRIC: &str = "mosaic.memories.usage";

/// The kind of memory artifact being accessed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoriesUsageKind {
    MemoryMd,
    MemorySummary,
    RawMemories,
    RolloutSummaries,
    Skills,
}

impl MemoriesUsageKind {
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::MemoryMd => "memory_md",
            Self::MemorySummary => "memory_summary",
            Self::RawMemories => "raw_memories",
            Self::RolloutSummaries => "rollout_summaries",
            Self::Skills => "skills",
        }
    }
}

/// Classify a file path as a memory artifact kind, if applicable.
pub fn get_memory_kind(path: &str) -> Option<MemoriesUsageKind> {
    if path.contains("memories/MEMORY.md") {
        Some(MemoriesUsageKind::MemoryMd)
    } else if path.contains("memories/memory_summary.md") {
        Some(MemoriesUsageKind::MemorySummary)
    } else if path.contains("memories/raw_memories.md") {
        Some(MemoriesUsageKind::RawMemories)
    } else if path.contains("memories/rollout_summaries/") {
        Some(MemoriesUsageKind::RolloutSummaries)
    } else if path.contains("memories/skills/") {
        Some(MemoriesUsageKind::Skills)
    } else {
        None
    }
}

/// Emit a tracing event for a memory artifact read.
///
/// In Codex this emits an OTel counter; Mosaic uses `tracing` until
/// a full telemetry stack is wired up.
pub fn emit_memory_read_metric(tool_name: &str, path: &str, success: bool) {
    if let Some(kind) = get_memory_kind(path) {
        debug!(
            metric = MEMORIES_USAGE_METRIC,
            kind = kind.as_tag(),
            tool = tool_name,
            success = success,
            "memory artifact read"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_memory_md() {
        assert_eq!(
            get_memory_kind("/home/user/.codex/memories/MEMORY.md"),
            Some(MemoriesUsageKind::MemoryMd)
        );
    }

    #[test]
    fn classify_memory_summary() {
        assert_eq!(
            get_memory_kind("/home/user/.codex/memories/memory_summary.md"),
            Some(MemoriesUsageKind::MemorySummary)
        );
    }

    #[test]
    fn classify_raw_memories() {
        assert_eq!(
            get_memory_kind("/home/user/.codex/memories/raw_memories.md"),
            Some(MemoriesUsageKind::RawMemories)
        );
    }

    #[test]
    fn classify_rollout_summaries() {
        assert_eq!(
            get_memory_kind("/home/user/.codex/memories/rollout_summaries/abc.md"),
            Some(MemoriesUsageKind::RolloutSummaries)
        );
    }

    #[test]
    fn classify_skills() {
        assert_eq!(
            get_memory_kind("/home/user/.codex/memories/skills/my_skill.md"),
            Some(MemoriesUsageKind::Skills)
        );
    }

    #[test]
    fn unrelated_path_returns_none() {
        assert_eq!(get_memory_kind("/home/user/project/src/main.rs"), None);
    }

    #[test]
    fn partial_match_not_confused() {
        // "memories/" alone without a known suffix should not match
        assert_eq!(get_memory_kind("/home/user/.codex/memories/unknown.txt"), None);
    }

    #[test]
    fn emit_does_not_panic() {
        // Smoke test: just ensure it doesn't panic
        emit_memory_read_metric("shell", "/home/.codex/memories/MEMORY.md", true);
        emit_memory_read_metric("shell", "/home/project/src/lib.rs", false);
    }
}
