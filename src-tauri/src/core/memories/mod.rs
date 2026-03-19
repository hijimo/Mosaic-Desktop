//! Memory subsystem for startup extraction and consolidation.
//!
//! The memory pipeline persists per-rollout summaries and raw memories
//! to disk under `<codex_home>/memories/`. A consolidation step can later
//! merge them into a single `memory_summary.md` consumed at session start.

mod phase1;
mod phase2;
pub(crate) mod prompts;
mod start;
pub mod storage;
pub(crate) mod citations;
pub(crate) mod usage;

pub use start::start_memories_startup_task;

use std::path::{Path, PathBuf};

const ROLLOUT_SUMMARIES_SUBDIR: &str = "rollout_summaries";
const RAW_MEMORIES_FILENAME: &str = "raw_memories.md";

/// Default cap on how many raw memories are kept for consolidation.
pub const DEFAULT_MAX_RAW_MEMORIES: usize = 64;

/// Returns the memory root directory for a given codex home.
pub fn memory_root(codex_home: &Path) -> PathBuf {
    codex_home.join("memories")
}

fn rollout_summaries_dir(root: &Path) -> PathBuf {
    root.join(ROLLOUT_SUMMARIES_SUBDIR)
}

fn raw_memories_file(root: &Path) -> PathBuf {
    root.join(RAW_MEMORIES_FILENAME)
}

pub(crate) async fn ensure_layout(root: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(rollout_summaries_dir(root)).await
}

/// Read the consolidated memory summary for injection into developer
/// instructions. Returns `None` if the file is missing or empty.
pub async fn read_memory_summary(codex_home: &Path) -> Option<String> {
    let path = memory_root(codex_home).join("memory_summary.md");
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn memory_root_path() {
        let dir = tempdir().unwrap();
        let home = dir.path().join("codex");
        assert_eq!(memory_root(&home), home.join("memories"));
    }

    #[tokio::test]
    async fn read_memory_summary_returns_none_when_missing() {
        let dir = tempdir().unwrap();
        assert!(read_memory_summary(dir.path()).await.is_none());
    }

    #[tokio::test]
    async fn read_memory_summary_returns_none_when_empty() {
        let dir = tempdir().unwrap();
        let root = memory_root(dir.path());
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(root.join("memory_summary.md"), "  \n").await.unwrap();
        assert!(read_memory_summary(dir.path()).await.is_none());
    }

    #[tokio::test]
    async fn read_memory_summary_returns_content() {
        let dir = tempdir().unwrap();
        let root = memory_root(dir.path());
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(root.join("memory_summary.md"), "summary content").await.unwrap();
        assert_eq!(read_memory_summary(dir.path()).await.unwrap(), "summary content");
    }
}
