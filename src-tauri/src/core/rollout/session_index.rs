//! Session index: append-only JSONL index mapping thread IDs to human-readable names.
//!
//! The index file `session_index.jsonl` lives at `~/.mosaic/session_index.jsonl`.
//! Each line is a [`SessionIndexEntry`]. The most recent entry wins when resolving.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

const SESSION_INDEX_FILE: &str = "session_index.jsonl";

/// A single entry in the session index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionIndexEntry {
    pub id: String,
    pub thread_name: String,
    pub updated_at: String,
}

/// Append a thread name to the session index.
pub async fn append_thread_name(
    mosaic_home: &Path,
    thread_id: &str,
    name: &str,
) -> std::io::Result<()> {
    let updated_at = chrono::Utc::now().to_rfc3339();
    let entry = SessionIndexEntry {
        id: thread_id.to_string(),
        thread_name: name.to_string(),
        updated_at,
    };
    let path = mosaic_home.join(SESSION_INDEX_FILE);
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;
    let mut line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
    line.push('\n');
    file.write_all(line.as_bytes()).await?;
    file.flush().await?;
    Ok(())
}

/// Find the latest thread name for a thread ID.
pub async fn find_thread_name_by_id(
    mosaic_home: &Path,
    thread_id: &str,
) -> std::io::Result<Option<String>> {
    let path = mosaic_home.join(SESSION_INDEX_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let file = tokio::fs::File::open(&path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();
    let mut latest_name: Option<String> = None;

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<SessionIndexEntry>(trimmed) {
            if entry.id == thread_id && !entry.thread_name.trim().is_empty() {
                latest_name = Some(entry.thread_name.trim().to_string());
            }
        }
    }
    Ok(latest_name)
}

/// Find the most recently updated thread ID for a thread name.
pub async fn find_thread_id_by_name(
    mosaic_home: &Path,
    name: &str,
) -> std::io::Result<Option<String>> {
    if name.trim().is_empty() {
        return Ok(None);
    }
    let path = mosaic_home.join(SESSION_INDEX_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let file = tokio::fs::File::open(&path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();
    let mut latest_id: Option<String> = None;

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<SessionIndexEntry>(trimmed) {
            if entry.thread_name == name {
                latest_id = Some(entry.id);
            }
        }
    }
    Ok(latest_id)
}

/// Find a thread rollout file path by thread name.
pub async fn find_thread_path_by_name_str(
    mosaic_home: &Path,
    name: &str,
) -> std::io::Result<Option<PathBuf>> {
    let Some(thread_id) = find_thread_id_by_name(mosaic_home, name).await? else {
        return Ok(None);
    };
    super::list::find_thread_path_by_id_str(mosaic_home, &thread_id).await
}
