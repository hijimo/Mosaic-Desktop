//! Thread listing: discover and paginate rollout files on disk.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::SESSIONS_SUBDIR;
use super::ARCHIVED_SESSIONS_SUBDIR;
use super::policy::{RolloutItem, RolloutLine, SessionSource};

// ── Public types ─────────────────────────────────────────────────

/// A page of thread summaries.
#[derive(Debug, Default)]
pub struct ThreadsPage {
    pub items: Vec<ThreadItem>,
    pub next_cursor: Option<Cursor>,
    pub num_scanned_files: usize,
}

/// Summary for a single thread rollout file.
#[derive(Debug, Default)]
pub struct ThreadItem {
    pub path: PathBuf,
    pub thread_id: Option<String>,
    pub first_user_message: Option<String>,
    pub cwd: Option<PathBuf>,
    pub git_branch: Option<String>,
    pub source: Option<SessionSource>,
    pub model_provider: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Sort key for thread listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadSortKey {
    CreatedAt,
    UpdatedAt,
}

/// Opaque pagination cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub ts: String,
    pub id: String,
}

const MAX_SCAN_FILES: usize = 10_000;
const HEAD_RECORD_LIMIT: usize = 10;

// ── Core listing function ────────────────────────────────────────

/// List threads under `mosaic_home/sessions/` with pagination.
pub async fn get_threads(
    mosaic_home: &Path,
    page_size: usize,
    cursor: Option<&Cursor>,
    sort_key: ThreadSortKey,
    allowed_sources: &[SessionSource],
) -> io::Result<ThreadsPage> {
    let root = mosaic_home.join(SESSIONS_SUBDIR);
    get_threads_in_root(root, page_size, cursor, sort_key, allowed_sources).await
}

/// List threads in a specific root directory.
pub async fn get_threads_in_root(
    root: PathBuf,
    page_size: usize,
    cursor: Option<&Cursor>,
    _sort_key: ThreadSortKey,
    allowed_sources: &[SessionSource],
) -> io::Result<ThreadsPage> {
    if !root.exists() {
        return Ok(ThreadsPage::default());
    }

    let mut all_files = collect_rollout_paths_recursive(&root).await?;

    // Sort by filename timestamp descending.
    all_files.sort_by(|a, b| b.0.cmp(&a.0));

    // Apply cursor-based pagination.
    let start_idx = if let Some(c) = cursor {
        all_files
            .iter()
            .position(|(_, _, path)| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| {
                        let cursor_key = format!("{}|{}", c.ts, c.id);
                        let file_key = parse_timestamp_uuid_from_filename(n)
                            .map(|(ts, id)| format!("{}|{}", ts, id))
                            .unwrap_or_default();
                        file_key < cursor_key
                    })
                    .unwrap_or(false)
            })
            .unwrap_or(all_files.len())
    } else {
        0
    };

    let mut items = Vec::with_capacity(page_size);
    let mut scanned = 0usize;

    for (_ts_str, _id, path) in all_files.into_iter().skip(start_idx) {
        if items.len() >= page_size || scanned >= MAX_SCAN_FILES {
            break;
        }
        scanned += 1;

        if let Some(item) = build_thread_item(&path, allowed_sources).await {
            items.push(item);
        }
    }

    let next_cursor = if items.len() == page_size {
        items.last().and_then(|item| {
            let name = item.path.file_name()?.to_str()?;
            let (ts, id) = parse_timestamp_uuid_from_filename(name)?;
            Some(Cursor {
                ts,
                id: id.to_string(),
            })
        })
    } else {
        None
    };

    Ok(ThreadsPage {
        items,
        next_cursor,
        num_scanned_files: scanned,
    })
}

// ── File discovery ───────────────────────────────────────────────

/// Recursively collect rollout JSONL files under a root directory.
async fn collect_rollout_paths_recursive(
    root: &Path,
) -> io::Result<Vec<(String, Uuid, PathBuf)>> {
    let mut stack = vec![root.to_path_buf()];
    let mut paths = Vec::new();

    while let Some(dir) = stack.pop() {
        let mut read_dir = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            let ft = match entry.file_type().await {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            let name = match entry.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };
            if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                if let Some((ts, id)) = parse_timestamp_uuid_from_filename(&name) {
                    paths.push((ts, id, path));
                }
            }
        }
    }
    Ok(paths)
}

/// Build a [`ThreadItem`] by reading the head of a rollout file.
async fn build_thread_item(
    path: &Path,
    allowed_sources: &[SessionSource],
) -> Option<ThreadItem> {
    let summary = read_head_summary(path).await.ok()?;
    if !summary.saw_session_meta || !summary.saw_user_event {
        return None;
    }
    if !allowed_sources.is_empty() {
        if let Some(ref src) = summary.source {
            if !allowed_sources.contains(src) {
                return None;
            }
        }
    }
    let updated_at = file_modified_time_rfc3339(path).await;
    let created_at = summary.created_at;
    let updated_at_val = updated_at.or_else(|| created_at.clone());
    Some(ThreadItem {
        path: path.to_path_buf(),
        thread_id: summary.thread_id,
        first_user_message: summary.first_user_message,
        cwd: summary.cwd,
        git_branch: summary.git_branch,
        source: summary.source,
        model_provider: summary.model_provider,
        created_at,
        updated_at: updated_at_val,
    })
}

// ── Head summary reader ──────────────────────────────────────────

#[derive(Default)]
struct HeadTailSummary {
    saw_session_meta: bool,
    saw_user_event: bool,
    thread_id: Option<String>,
    first_user_message: Option<String>,
    cwd: Option<PathBuf>,
    git_branch: Option<String>,
    source: Option<SessionSource>,
    model_provider: Option<String>,
    created_at: Option<String>,
}

async fn read_head_summary(path: &Path) -> io::Result<HeadTailSummary> {
    use tokio::io::AsyncBufReadExt;

    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();
    let mut summary = HeadTailSummary::default();
    let mut lines_scanned = 0usize;

    while lines_scanned < HEAD_RECORD_LIMIT + 200 {
        let Some(line) = lines.next_line().await? else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        lines_scanned += 1;

        let Ok(rollout_line) = serde_json::from_str::<RolloutLine>(trimmed) else {
            continue;
        };

        match rollout_line.item {
            RolloutItem::SessionMeta(meta_line) => {
                if !summary.saw_session_meta {
                    summary.source = Some(meta_line.meta.source);
                    summary.model_provider = meta_line.meta.model_provider;
                    summary.thread_id = Some(meta_line.meta.id);
                    summary.cwd = Some(meta_line.meta.cwd);
                    summary.git_branch = meta_line.git.as_ref().and_then(|g| g.branch.clone());
                    summary.created_at = Some(meta_line.meta.timestamp);
                    summary.saw_session_meta = true;
                }
            }
            RolloutItem::EventMsg(crate::protocol::event::EventMsg::UserMessage(user)) => {
                summary.saw_user_event = true;
                if summary.first_user_message.is_none() {
                    let msg = user.message.trim().to_string();
                    if !msg.is_empty() {
                        summary.first_user_message = Some(msg);
                    }
                }
            }
            _ => {}
        }

        if summary.saw_session_meta && summary.saw_user_event {
            break;
        }
    }
    Ok(summary)
}

// ── Filename parsing ─────────────────────────────────────────────

/// Parse `rollout-YYYY-MM-DDThh-mm-ss-<uuid>.jsonl` into (timestamp_str, uuid).
pub fn parse_timestamp_uuid_from_filename(name: &str) -> Option<(String, Uuid)> {
    let core = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    // Scan from right for a '-' where the suffix is a valid UUID.
    let (sep_idx, uuid) = core
        .match_indices('-')
        .rev()
        .find_map(|(i, _)| Uuid::parse_str(&core[i + 1..]).ok().map(|u| (i, u)))?;
    let ts_str = &core[..sep_idx];
    Some((ts_str.to_string(), uuid))
}

/// Extract `YYYY/MM/DD` directory components from a rollout filename.
pub fn rollout_date_parts(file_name: &OsStr) -> Option<(String, String, String)> {
    let name = file_name.to_string_lossy();
    let date = name.strip_prefix("rollout-")?.get(..10)?;
    let year = date.get(..4)?.to_string();
    let month = date.get(5..7)?.to_string();
    let day = date.get(8..10)?.to_string();
    Some((year, month, day))
}

// ── Path lookup by ID ────────────────────────────────────────────

/// Find a thread rollout file by its UUID string.
pub async fn find_thread_path_by_id_str(
    mosaic_home: &Path,
    id_str: &str,
) -> io::Result<Option<PathBuf>> {
    find_in_subdir(mosaic_home, SESSIONS_SUBDIR, id_str).await
}

/// Find an archived thread rollout file by its UUID string.
pub async fn find_archived_thread_path_by_id_str(
    mosaic_home: &Path,
    id_str: &str,
) -> io::Result<Option<PathBuf>> {
    find_in_subdir(mosaic_home, ARCHIVED_SESSIONS_SUBDIR, id_str).await
}

async fn find_in_subdir(
    mosaic_home: &Path,
    subdir: &str,
    id_str: &str,
) -> io::Result<Option<PathBuf>> {
    if Uuid::parse_str(id_str).is_err() {
        return Ok(None);
    }
    let root = mosaic_home.join(subdir);
    if !root.exists() {
        return Ok(None);
    }
    let paths = collect_rollout_paths_recursive(&root).await?;
    Ok(paths
        .into_iter()
        .find(|(_, id, _)| id.to_string() == id_str)
        .map(|(_, _, path)| path))
}

// ── Session meta reader ──────────────────────────────────────────

/// Read the [`SessionMetaLine`] from the head of a rollout file.
pub async fn read_session_meta_line(
    path: &Path,
) -> io::Result<super::policy::SessionMetaLine> {
    use tokio::io::AsyncBufReadExt;

    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(rollout_line) = serde_json::from_str::<RolloutLine>(trimmed) {
            if let RolloutItem::SessionMeta(meta_line) = rollout_line.item {
                return Ok(meta_line);
            }
        }
    }
    Err(io::Error::other(format!(
        "rollout at {} does not contain session metadata",
        path.display()
    )))
}

// ── Helpers ──────────────────────────────────────────────────────

async fn file_modified_time_rfc3339(path: &Path) -> Option<String> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    let modified = meta.modified().ok()?;
    let dt: DateTime<Utc> = modified.into();
    Some(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}
