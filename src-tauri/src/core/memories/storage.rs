//! Disk-level storage for raw memories and rollout summaries.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::{ensure_layout, raw_memories_file, rollout_summaries_dir};
use crate::protocol::ThreadId;
use crate::state::memories_db::Stage1Output;

/// Rebuild `raw_memories.md` from stage-1 outputs.
pub async fn rebuild_raw_memories_file(
    root: &Path,
    memories: &[Stage1Output],
    max_memories: usize,
) -> std::io::Result<()> {
    ensure_layout(root).await?;

    let retained = &memories[..memories.len().min(max_memories)];
    let mut body = String::from("# Raw Memories\n\n");

    if retained.is_empty() {
        body.push_str("No raw memories yet.\n");
        return tokio::fs::write(raw_memories_file(root), body).await;
    }

    body.push_str("Merged stage-1 raw memories (latest first):\n\n");
    for m in retained {
        writeln!(body, "## Thread `{}`", m.thread_id).map_err(fmt_err)?;
        writeln!(body, "updated_at: {}", m.source_updated_at.to_rfc3339()).map_err(fmt_err)?;
        writeln!(body, "cwd: {}", m.cwd.display()).map_err(fmt_err)?;
        writeln!(body, "rollout_path: {}", m.rollout_path.display()).map_err(fmt_err)?;
        let summary_file = format!("{}.md", rollout_summary_file_stem(m));
        writeln!(body, "rollout_summary_file: {summary_file}").map_err(fmt_err)?;
        writeln!(body).map_err(fmt_err)?;
        body.push_str(m.raw_memory.trim());
        body.push_str("\n\n");
    }

    tokio::fs::write(raw_memories_file(root), body).await
}

/// Sync rollout summary files from stage-1 outputs, pruning stale ones.
pub async fn sync_rollout_summaries(
    root: &Path,
    memories: &[Stage1Output],
    max_memories: usize,
) -> std::io::Result<()> {
    ensure_layout(root).await?;

    let retained = &memories[..memories.len().min(max_memories)];
    let keep: HashSet<String> = retained.iter().map(rollout_summary_file_stem).collect();

    // Prune stale files.
    let dir_path = rollout_summaries_dir(root);
    if let Ok(mut dir) = tokio::fs::read_dir(&dir_path).await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            if let Some(stem) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".md"))
            {
                if !keep.contains(stem) {
                    let _ = tokio::fs::remove_file(&path).await;
                }
            }
        }
    }

    // Write retained summaries.
    for m in retained {
        let stem = rollout_summary_file_stem(m);
        let path = rollout_summaries_dir(root).join(format!("{stem}.md"));

        let mut body = String::new();
        writeln!(body, "thread_id: {}", m.thread_id).map_err(fmt_err)?;
        writeln!(body, "updated_at: {}", m.source_updated_at.to_rfc3339()).map_err(fmt_err)?;
        writeln!(body, "rollout_path: {}", m.rollout_path.display()).map_err(fmt_err)?;
        writeln!(body, "cwd: {}", m.cwd.display()).map_err(fmt_err)?;
        if let Some(ref branch) = m.git_branch {
            writeln!(body, "git_branch: {branch}").map_err(fmt_err)?;
        }
        writeln!(body).map_err(fmt_err)?;
        body.push_str(&m.rollout_summary);
        body.push('\n');

        tokio::fs::write(path, body).await?;
    }

    // Clean up stale artifacts when no memories remain.
    if retained.is_empty() {
        for name in ["MEMORY.md", "memory_summary.md"] {
            let p = root.join(name);
            if let Err(e) = tokio::fs::remove_file(&p).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(e);
                }
            }
        }
        let skills_dir = root.join("skills");
        if let Err(e) = tokio::fs::remove_dir_all(&skills_dir).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e);
            }
        }
    }

    Ok(())
}

/// Build a deterministic file stem for a rollout summary.
///
/// Format: `<timestamp>-<4char_hash>[-<slug>]`
pub fn rollout_summary_file_stem(memory: &Stage1Output) -> String {
    rollout_summary_file_stem_from_parts(
        memory.thread_id,
        memory.source_updated_at,
        memory.rollout_slug.as_deref(),
    )
}

pub fn rollout_summary_file_stem_from_parts(
    thread_id: ThreadId,
    source_updated_at: DateTime<Utc>,
    rollout_slug: Option<&str>,
) -> String {
    const SLUG_MAX: usize = 60;
    const ALPHABET: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const SPACE: u32 = 14_776_336; // 62^4

    let uuid = thread_id.as_uuid();
    // Extract timestamp from UUID v7 if possible, else use source_updated_at.
    let (ts_str, hash_seed) = {
        let (secs, nanos) =
            uuid.get_timestamp()
                .map_or((source_updated_at.timestamp() as u64, 0u32), |ts| {
                    let (s, n) = ts.to_unix();
                    (s, n)
                });
        let dt = DateTime::<Utc>::from_timestamp(secs as i64, nanos).unwrap_or(source_updated_at);
        let seed = (uuid.as_u128() & 0xFFFF_FFFF) as u32;
        (dt.format("%Y-%m-%dT%H-%M-%S").to_string(), seed)
    };

    let mut hash_val = hash_seed % SPACE;
    let mut hash_chars = ['0'; 4];
    for i in (0..4).rev() {
        hash_chars[i] = ALPHABET[(hash_val % ALPHABET.len() as u32) as usize] as char;
        hash_val /= ALPHABET.len() as u32;
    }
    let short_hash: String = hash_chars.iter().collect();
    let prefix = format!("{ts_str}-{short_hash}");

    let Some(raw_slug) = rollout_slug else {
        return prefix;
    };

    let mut slug = String::with_capacity(SLUG_MAX);
    for ch in raw_slug.chars() {
        if slug.len() >= SLUG_MAX {
            break;
        }
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else {
            slug.push('_');
        }
    }
    while slug.ends_with('_') {
        slug.pop();
    }

    if slug.is_empty() {
        prefix
    } else {
        format!("{prefix}-{slug}")
    }
}

fn fmt_err(e: std::fmt::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ThreadId;
    use chrono::TimeZone;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn make_memory(thread_id: ThreadId, slug: Option<&str>, ts: i64) -> Stage1Output {
        Stage1Output {
            thread_id,
            source_updated_at: Utc.timestamp_opt(ts, 0).single().unwrap(),
            raw_memory: "raw memory".to_string(),
            rollout_summary: "summary".to_string(),
            rollout_slug: slug.map(String::from),
            rollout_path: PathBuf::from("/tmp/rollout.jsonl"),
            cwd: PathBuf::from("/tmp/workspace"),
            git_branch: None,
            generated_at: Utc.timestamp_opt(ts + 1, 0).single().unwrap(),
        }
    }

    #[test]
    fn file_stem_without_slug() {
        let id = ThreadId::try_from("0194f5a6-89ab-7cde-8123-456789abcdef").unwrap();
        let m = make_memory(id, None, 100);
        let stem = rollout_summary_file_stem(&m);
        assert!(stem.contains('-'));
        let parts: Vec<&str> = stem.splitn(5, '-').collect();
        assert!(parts.len() >= 4);
    }

    #[test]
    fn file_stem_with_slug_sanitized() {
        let id = ThreadId::try_from("0194f5a6-89ab-7cde-8123-456789abcdef").unwrap();
        let m = make_memory(id, Some("Unsafe Slug/With Spaces"), 100);
        let stem = rollout_summary_file_stem(&m);
        assert!(stem.contains("unsafe_slug_with_spaces"));
    }

    #[test]
    fn file_stem_slug_truncated_at_60() {
        let id = ThreadId::try_from("0194f5a6-89ab-7cde-8123-456789abcdef").unwrap();
        let long_slug = "a".repeat(100);
        let m = make_memory(id, Some(&long_slug), 100);
        let stem = rollout_summary_file_stem(&m);
        let prefix = rollout_summary_file_stem_from_parts(id, m.source_updated_at, None);
        let slug_part = stem.strip_prefix(&format!("{prefix}-")).unwrap();
        assert!(slug_part.len() <= 60);
    }

    #[tokio::test]
    async fn rebuild_and_sync_roundtrip() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("memory");

        let id = ThreadId::new();
        let memories = vec![make_memory(id, None, 100)];

        sync_rollout_summaries(&root, &memories, 64).await.unwrap();
        rebuild_raw_memories_file(&root, &memories, 64)
            .await
            .unwrap();

        let raw = tokio::fs::read_to_string(raw_memories_file(&root))
            .await
            .unwrap();
        assert!(raw.contains("raw memory"));
        assert!(raw.contains(&id.to_string()));
        assert!(raw.contains("cwd: /tmp/workspace"));

        let mut entries = tokio::fs::read_dir(rollout_summaries_dir(&root))
            .await
            .unwrap();
        let mut count = 0;
        while let Ok(Some(_)) = entries.next_entry().await {
            count += 1;
        }
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn sync_prunes_stale_files() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("memory");
        super::ensure_layout(&root).await.unwrap();

        let stale = rollout_summaries_dir(&root).join("stale-id.md");
        tokio::fs::write(&stale, "stale").await.unwrap();

        sync_rollout_summaries(&root, &[], 64).await.unwrap();

        assert!(!tokio::fs::try_exists(&stale).await.unwrap());
    }

    #[tokio::test]
    async fn empty_sync_removes_artifacts() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("memory");
        super::ensure_layout(&root).await.unwrap();

        let memory_md = root.join("MEMORY.md");
        let summary_md = root.join("memory_summary.md");
        let skill_file = root.join("skills/demo/SKILL.md");
        tokio::fs::write(&memory_md, "old").await.unwrap();
        tokio::fs::write(&summary_md, "old").await.unwrap();
        tokio::fs::create_dir_all(skill_file.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&skill_file, "old").await.unwrap();

        sync_rollout_summaries(&root, &[], 64).await.unwrap();

        assert!(!tokio::fs::try_exists(&memory_md).await.unwrap());
        assert!(!tokio::fs::try_exists(&summary_md).await.unwrap());
        assert!(!tokio::fs::try_exists(root.join("skills")).await.unwrap());
    }

    #[tokio::test]
    async fn rebuild_empty_memories() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("memory");

        rebuild_raw_memories_file(&root, &[], 64).await.unwrap();

        let raw = tokio::fs::read_to_string(raw_memories_file(&root))
            .await
            .unwrap();
        assert_eq!(raw, "# Raw Memories\n\nNo raw memories yet.\n");
    }

    #[tokio::test]
    async fn rollout_summary_includes_git_branch() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("memory");

        let mut m = make_memory(ThreadId::new(), None, 200);
        m.git_branch = Some("feature/test".to_string());

        sync_rollout_summaries(&root, &[m], 64).await.unwrap();

        let mut entries = tokio::fs::read_dir(rollout_summaries_dir(&root))
            .await
            .unwrap();
        let entry = entries.next_entry().await.unwrap().unwrap();
        let content = tokio::fs::read_to_string(entry.path()).await.unwrap();
        assert!(content.contains("git_branch: feature/test"));
    }
}
