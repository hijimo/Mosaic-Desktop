//! Persistence layer for the global, append-only message history file.
//!
//! The history is stored at `<codex_home>/history.jsonl` with one JSON object
//! per line. Each record:
//! ```text
//! {"session_id":"<uuid>","ts":<unix_seconds>,"text":"<message>"}
//! ```
//!
//! Writes use `O_APPEND` + advisory file locking for safe concurrent access.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Result, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::protocol::ThreadId;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const HISTORY_FILENAME: &str = "history.jsonl";
/// When history exceeds the hard cap, trim to this fraction of `max_bytes`.
const HISTORY_SOFT_CAP_RATIO: f64 = 0.8;
const MAX_RETRIES: usize = 10;
const RETRY_SLEEP: Duration = Duration::from_millis(100);

/// Persistence mode for history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryPersistence {
    #[default]
    SaveAll,
    None,
}

/// Resolved history configuration.
#[derive(Debug, Clone)]
pub struct HistoryConfig {
    pub persistence: HistoryPersistence,
    pub max_bytes: Option<usize>,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            persistence: HistoryPersistence::SaveAll,
            max_bytes: None,
        }
    }
}

impl HistoryConfig {
    /// Build from the TOML config section.
    pub fn from_toml(toml: &crate::config::toml_types::HistoryToml) -> Self {
        let persistence = match toml.persistence.as_deref() {
            Some("none") => HistoryPersistence::None,
            _ => HistoryPersistence::SaveAll,
        };
        Self {
            persistence,
            max_bytes: toml.max_entries.map(|n| n * 1024), // treat max_entries as KB
        }
    }
}

/// A single history entry.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub session_id: String,
    pub ts: u64,
    pub text: String,
}

fn history_filepath(codex_home: &Path) -> PathBuf {
    codex_home.join(HISTORY_FILENAME)
}

/// Append a text entry to the history file.
pub async fn append_entry(
    text: &str,
    conversation_id: &ThreadId,
    codex_home: &Path,
    config: &HistoryConfig,
) -> Result<()> {
    if config.persistence == HistoryPersistence::None {
        return Ok(());
    }

    let path = history_filepath(codex_home);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| std::io::Error::other(format!("clock error: {e}")))?
        .as_secs();

    let entry = HistoryEntry {
        session_id: conversation_id.to_string(),
        ts,
        text: text.to_string(),
    };
    let mut line = serde_json::to_string(&entry)
        .map_err(|e| std::io::Error::other(format!("serialize: {e}")))?;
    line.push('\n');

    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        options.append(true);
        options.mode(0o600);
    }

    let mut history_file = options.open(&path)?;
    ensure_owner_only_permissions(&history_file).await?;

    let max_bytes = config.max_bytes;

    tokio::task::spawn_blocking(move || -> Result<()> {
        for _ in 0..MAX_RETRIES {
            match history_file.try_lock() {
                Ok(()) => {
                    history_file.seek(SeekFrom::End(0))?;
                    history_file.write_all(line.as_bytes())?;
                    history_file.flush()?;
                    enforce_history_limit(&mut history_file, max_bytes)?;
                    return Ok(());
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    std::thread::sleep(RETRY_SLEEP);
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "could not acquire lock on history file",
        ))
    })
    .await??;

    Ok(())
}

/// Trim the history file to honor `max_bytes`, dropping oldest lines.
fn enforce_history_limit(file: &mut File, max_bytes: Option<usize>) -> Result<()> {
    let Some(max_bytes) = max_bytes else {
        return Ok(());
    };
    if max_bytes == 0 {
        return Ok(());
    }
    let max_bytes = max_bytes as u64;
    let mut current_len = file.metadata()?.len();
    if current_len <= max_bytes {
        return Ok(());
    }

    let mut reader_file = file.try_clone()?;
    reader_file.seek(SeekFrom::Start(0))?;
    let mut buf_reader = BufReader::new(reader_file);
    let mut line_lengths = Vec::new();
    let mut line_buf = String::new();
    loop {
        line_buf.clear();
        let bytes = buf_reader.read_line(&mut line_buf)?;
        if bytes == 0 {
            break;
        }
        line_lengths.push(bytes as u64);
    }
    if line_lengths.is_empty() {
        return Ok(());
    }

    let last_index = line_lengths.len() - 1;
    let soft_cap = ((max_bytes as f64) * HISTORY_SOFT_CAP_RATIO)
        .floor()
        .clamp(1.0, max_bytes as f64) as u64;
    let trim_target = soft_cap.max(line_lengths[last_index]);

    let mut drop_bytes = 0u64;
    let mut idx = 0usize;
    while current_len > trim_target && idx < last_index {
        current_len = current_len.saturating_sub(line_lengths[idx]);
        drop_bytes += line_lengths[idx];
        idx += 1;
    }
    if drop_bytes == 0 {
        return Ok(());
    }

    let mut reader = buf_reader.into_inner();
    reader.seek(SeekFrom::Start(drop_bytes))?;
    let mut tail = Vec::with_capacity(current_len as usize);
    reader.read_to_end(&mut tail)?;

    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&tail)?;
    file.flush()?;
    Ok(())
}

/// Get the history file's identifier (inode on Unix) and entry count.
pub async fn history_metadata(codex_home: &Path) -> (u64, usize) {
    history_metadata_for_file(&history_filepath(codex_home)).await
}

/// Look up a specific history entry by log_id and offset.
pub fn lookup(log_id: u64, offset: usize, codex_home: &Path) -> Option<HistoryEntry> {
    lookup_history_entry(&history_filepath(codex_home), log_id, offset)
}

#[cfg(unix)]
async fn ensure_owner_only_permissions(file: &File) -> Result<()> {
    let metadata = file.metadata()?;
    let current_mode = metadata.permissions().mode() & 0o777;
    if current_mode != 0o600 {
        let mut perms = metadata.permissions();
        perms.set_mode(0o600);
        let perms_clone = perms.clone();
        let file_clone = file.try_clone()?;
        tokio::task::spawn_blocking(move || file_clone.set_permissions(perms_clone)).await??;
    }
    Ok(())
}

#[cfg(not(unix))]
async fn ensure_owner_only_permissions(_file: &File) -> Result<()> {
    Ok(())
}

async fn history_metadata_for_file(path: &Path) -> (u64, usize) {
    let log_id = match fs::metadata(path).await {
        Ok(m) => history_log_id(&m).unwrap_or(0),
        Err(_) => return (0, 0),
    };
    let mut file = match fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return (log_id, 0),
    };
    let mut buf = [0u8; 8192];
    let mut count = 0usize;
    loop {
        match file.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => count += buf[..n].iter().filter(|&&b| b == b'\n').count(),
            Err(_) => return (log_id, 0),
        }
    }
    (log_id, count)
}

fn lookup_history_entry(path: &Path, log_id: u64, offset: usize) -> Option<HistoryEntry> {
    let file = OpenOptions::new().read(true).open(path).ok()?;
    let metadata = file.metadata().ok()?;
    let current_log_id = history_log_id(&metadata)?;
    if log_id != 0 && current_log_id != log_id {
        return None;
    }

    for _ in 0..MAX_RETRIES {
        match file.try_lock_shared() {
            Ok(()) => {
                let reader = BufReader::new(&file);
                for (idx, line_res) in reader.lines().enumerate() {
                    if idx == offset {
                        return line_res.ok().and_then(|l| serde_json::from_str(&l).ok());
                    }
                }
                return None;
            }
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(RETRY_SLEEP),
            Err(_) => return None,
        }
    }
    None
}

#[cfg(unix)]
fn history_log_id(metadata: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.ino())
}

#[cfg(windows)]
fn history_log_id(metadata: &std::fs::Metadata) -> Option<u64> {
    use std::os::windows::fs::MetadataExt;
    Some(metadata.creation_time())
}

#[cfg(not(any(unix, windows)))]
fn history_log_id(_metadata: &std::fs::Metadata) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn lookup_reads_history_entries() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(HISTORY_FILENAME);

        let entries = vec![
            HistoryEntry {
                session_id: "first".into(),
                ts: 1,
                text: "hello".into(),
            },
            HistoryEntry {
                session_id: "second".into(),
                ts: 2,
                text: "world".into(),
            },
        ];

        let mut file = File::create(&path).unwrap();
        for e in &entries {
            writeln!(file, "{}", serde_json::to_string(e).unwrap()).unwrap();
        }

        let (log_id, count) = history_metadata_for_file(&path).await;
        assert_eq!(count, 2);

        let entry = lookup_history_entry(&path, log_id, 1).unwrap();
        assert_eq!(entry, entries[1]);
    }

    #[tokio::test]
    async fn lookup_stable_after_append() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(HISTORY_FILENAME);

        let e1 = HistoryEntry {
            session_id: "s1".into(),
            ts: 1,
            text: "first".into(),
        };
        let e2 = HistoryEntry {
            session_id: "s2".into(),
            ts: 2,
            text: "second".into(),
        };

        let mut file = File::create(&path).unwrap();
        writeln!(file, "{}", serde_json::to_string(&e1).unwrap()).unwrap();

        let (log_id, count) = history_metadata_for_file(&path).await;
        assert_eq!(count, 1);

        let mut append = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(append, "{}", serde_json::to_string(&e2).unwrap()).unwrap();

        let fetched = lookup_history_entry(&path, log_id, 1).unwrap();
        assert_eq!(fetched, e2);
    }

    #[tokio::test]
    async fn append_trims_when_beyond_max_bytes() {
        let dir = TempDir::new().unwrap();
        let codex_home = dir.path();
        let tid = ThreadId::new();

        let config = HistoryConfig {
            persistence: HistoryPersistence::SaveAll,
            max_bytes: None,
        };

        // Write first entry
        append_entry(&"a".repeat(200), &tid, codex_home, &config)
            .await
            .unwrap();

        let path = history_filepath(codex_home);
        let first_len = std::fs::metadata(&path).unwrap().len();

        // Set limit just above one entry
        let config = HistoryConfig {
            persistence: HistoryPersistence::SaveAll,
            max_bytes: Some((first_len + 10) as usize),
        };

        // Write second entry — should trigger trim
        append_entry(&"b".repeat(200), &tid, codex_home, &config)
            .await
            .unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let entries: Vec<HistoryEntry> = contents
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();

        assert_eq!(entries.len(), 1);
        assert!(entries[0].text.starts_with('b'));
    }

    #[tokio::test]
    async fn persistence_none_skips_write() {
        let dir = TempDir::new().unwrap();
        let codex_home = dir.path();
        let tid = ThreadId::new();

        let config = HistoryConfig {
            persistence: HistoryPersistence::None,
            max_bytes: None,
        };

        append_entry("hello", &tid, codex_home, &config)
            .await
            .unwrap();

        let path = history_filepath(codex_home);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn metadata_returns_zero_for_missing_file() {
        let dir = TempDir::new().unwrap();
        let (log_id, count) = history_metadata(dir.path()).await;
        assert_eq!(log_id, 0);
        assert_eq!(count, 0);
    }
}
