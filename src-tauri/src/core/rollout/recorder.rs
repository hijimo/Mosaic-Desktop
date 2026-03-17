//! Persist session rollouts as JSONL so sessions can be replayed or inspected.
//!
//! Rollout files live under `~/.mosaic/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`.

use std::fs;
use std::io::Error as IoError;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::{info, trace, warn};

use super::SESSIONS_SUBDIR;
use crate::core::git_info::collect_git_info;
use super::policy::{
    EventPersistenceMode, RolloutItem, RolloutLine, SessionMeta, SessionMetaLine, is_persisted,
};

// ── Public types ─────────────────────────────────────────────────

/// Records all events for a session and flushes them to disk.
#[derive(Clone)]
pub struct RolloutRecorder {
    tx: mpsc::Sender<RolloutCmd>,
    pub rollout_path: PathBuf,
    event_persistence_mode: EventPersistenceMode,
}

/// Parameters for creating or resuming a recorder.
#[derive(Clone)]
pub enum RolloutRecorderParams {
    Create {
        conversation_id: String,
        source: super::policy::SessionSource,
        event_persistence_mode: EventPersistenceMode,
    },
    Resume {
        path: PathBuf,
        event_persistence_mode: EventPersistenceMode,
    },
}

// ── Internal command channel ─────────────────────────────────────

enum RolloutCmd {
    AddItems(Vec<RolloutItem>),
    Persist { ack: oneshot::Sender<()> },
    Flush { ack: oneshot::Sender<()> },
    Shutdown { ack: oneshot::Sender<()> },
}

// ── LogFileInfo ──────────────────────────────────────────────────

struct LogFileInfo {
    path: PathBuf,
    conversation_id: String,
    timestamp: DateTime<Utc>,
}

fn precompute_log_file_info(
    mosaic_home: &Path,
    conversation_id: &str,
) -> std::io::Result<LogFileInfo> {
    let now = Utc::now();
    let mut dir = mosaic_home.to_path_buf();
    dir.push(SESSIONS_SUBDIR);
    dir.push(now.format("%Y").to_string());
    dir.push(now.format("%m").to_string());
    dir.push(now.format("%d").to_string());

    let date_str = now.format("%Y-%m-%dT%H-%M-%S").to_string();
    let filename = format!("rollout-{date_str}-{conversation_id}.jsonl");
    let path = dir.join(filename);

    Ok(LogFileInfo {
        path,
        conversation_id: conversation_id.to_string(),
        timestamp: now,
    })
}

fn open_log_file(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
}

// ── JSONL writer ─────────────────────────────────────────────────

struct JsonlWriter {
    file: tokio::fs::File,
}

#[derive(serde::Serialize)]
struct RolloutLineRef<'a> {
    timestamp: String,
    #[serde(flatten)]
    item: &'a RolloutItem,
}

impl JsonlWriter {
    async fn write_rollout_item(&mut self, item: &RolloutItem) -> std::io::Result<()> {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let line = RolloutLineRef { timestamp, item };
        let mut json = serde_json::to_string(&line)?;
        json.push('\n');
        self.file.write_all(json.as_bytes()).await?;
        self.file.flush().await?;
        Ok(())
    }
}

// ── Background writer task ───────────────────────────────────────

async fn rollout_writer(
    file: Option<tokio::fs::File>,
    mut deferred_log_file_info: Option<LogFileInfo>,
    mut rx: mpsc::Receiver<RolloutCmd>,
    mut meta: Option<SessionMeta>,
    cwd: PathBuf,
    _rollout_path: PathBuf,
    _model_provider: String,
) -> std::io::Result<()> {
    let mut writer = file.map(|f| JsonlWriter { file: f });
    let mut buffered_items = Vec::<RolloutItem>::new();

    // For resumed sessions, write meta immediately if present.
    if writer.is_some() {
        if let Some(session_meta) = meta.take() {
            let git = collect_git_info(&cwd).await;
            let meta_line = SessionMetaLine {
                meta: session_meta,
                git,
            };
            let item = RolloutItem::SessionMeta(meta_line);
            if let Some(w) = writer.as_mut() {
                w.write_rollout_item(&item).await?;
            }
        }
    }

    while let Some(cmd) = rx.recv().await {
        match cmd {
            RolloutCmd::AddItems(items) => {
                if items.is_empty() {
                    continue;
                }
                if writer.is_none() {
                    buffered_items.extend(items);
                    continue;
                }
                if let Some(w) = writer.as_mut() {
                    for item in &items {
                        w.write_rollout_item(item).await?;
                    }
                }
            }
            RolloutCmd::Persist { ack } => {
                if writer.is_none() {
                    let result = async {
                        let Some(log_file_info) = deferred_log_file_info.take() else {
                            return Err(IoError::other("missing deferred log file metadata"));
                        };
                        let file = open_log_file(&log_file_info.path)?;
                        writer = Some(JsonlWriter {
                            file: tokio::fs::File::from_std(file),
                        });

                        if let Some(session_meta) = meta.take() {
                            let git = collect_git_info(&cwd).await;
                            let meta_line = SessionMetaLine {
                                meta: session_meta,
                                git,
                            };
                            let item = RolloutItem::SessionMeta(meta_line);
                            if let Some(w) = writer.as_mut() {
                                w.write_rollout_item(&item).await?;
                            }
                        }

                        if !buffered_items.is_empty() {
                            if let Some(w) = writer.as_mut() {
                                for item in &buffered_items {
                                    w.write_rollout_item(item).await?;
                                }
                            }
                            buffered_items.clear();
                        }
                        Ok(())
                    }
                    .await;

                    if let Err(err) = result {
                        let _ = ack.send(());
                        return Err(err);
                    }
                }
                let _ = ack.send(());
            }
            RolloutCmd::Flush { ack } => {
                if let Some(w) = writer.as_mut() {
                    let _ = w.file.flush().await;
                }
                let _ = ack.send(());
            }
            RolloutCmd::Shutdown { ack } => {
                if let Some(w) = writer.as_mut() {
                    let _ = w.file.flush().await;
                }
                let _ = ack.send(());
                return Ok(());
            }
        }
    }
    Ok(())
}

// ── RolloutRecorder impl ─────────────────────────────────────────

impl RolloutRecorder {
    /// Create a new recorder. For new sessions the file is deferred until `persist()`.
    pub async fn new(
        mosaic_home: &Path,
        cwd: &Path,
        model_provider: &str,
        params: RolloutRecorderParams,
    ) -> std::io::Result<Self> {
        let (file, deferred, rollout_path, meta, mode) = match params {
            RolloutRecorderParams::Create {
                conversation_id,
                source,
                event_persistence_mode,
            } => {
                let log_file_info = precompute_log_file_info(mosaic_home, &conversation_id)?;
                let path = log_file_info.path.clone();
                let ts = log_file_info.timestamp.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                let session_meta = SessionMeta {
                    id: conversation_id,
                    forked_from_id: None,
                    timestamp: ts,
                    cwd: cwd.to_path_buf(),
                    cli_version: env!("CARGO_PKG_VERSION").to_string(),
                    source,
                    model_provider: Some(model_provider.to_string()),
                    agent_nickname: None,
                    agent_role: None,
                    memory_mode: None,
                };
                (None, Some(log_file_info), path, Some(session_meta), event_persistence_mode)
            }
            RolloutRecorderParams::Resume {
                path,
                event_persistence_mode,
            } => {
                let file = tokio::fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .await?;
                (Some(file), None, path, None, event_persistence_mode)
            }
        };

        let cwd = cwd.to_path_buf();
        let model_provider = model_provider.to_string();
        let (tx, rx) = mpsc::channel::<RolloutCmd>(256);
        let rp = rollout_path.clone();

        tokio::task::spawn(rollout_writer(
            file, deferred, rx, meta, cwd, rp, model_provider,
        ));

        Ok(Self {
            tx,
            rollout_path,
            event_persistence_mode: mode,
        })
    }

    /// Record items, filtering by persistence policy.
    pub async fn record_items(&self, items: &[RolloutItem]) -> std::io::Result<()> {
        let filtered: Vec<RolloutItem> = items
            .iter()
            .filter(|item| is_persisted(item, self.event_persistence_mode))
            .cloned()
            .collect();
        if filtered.is_empty() {
            return Ok(());
        }
        self.tx
            .send(RolloutCmd::AddItems(filtered))
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout items: {e}")))
    }

    /// Materialize the rollout file on disk (idempotent after first call).
    pub async fn persist(&self) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(RolloutCmd::Persist { ack: tx })
            .await
            .map_err(|e| IoError::other(format!("failed to queue persist: {e}")))?;
        rx.await
            .map_err(|e| IoError::other(format!("persist ack failed: {e}")))
    }

    /// Flush all queued writes.
    pub async fn flush(&self) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(RolloutCmd::Flush { ack: tx })
            .await
            .map_err(|e| IoError::other(format!("failed to queue flush: {e}")))?;
        rx.await
            .map_err(|e| IoError::other(format!("flush ack failed: {e}")))
    }

    /// Gracefully shut down the writer task.
    pub async fn shutdown(&self) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        match self.tx.send(RolloutCmd::Shutdown { ack: tx }).await {
            Ok(_) => rx
                .await
                .map_err(|e| IoError::other(format!("shutdown ack failed: {e}"))),
            Err(e) => {
                warn!("failed to send rollout shutdown: {e}");
                Err(IoError::other(format!("shutdown send failed: {e}")))
            }
        }
    }

    /// Load all rollout items from a JSONL file on disk.
    pub async fn load_rollout_items(
        path: &Path,
    ) -> std::io::Result<(Vec<RolloutItem>, Option<String>, usize)> {
        trace!("Loading rollout from {path:?}");
        let text = tokio::fs::read_to_string(path).await?;
        if text.trim().is_empty() {
            return Err(IoError::other("empty session file"));
        }

        let mut items = Vec::new();
        let mut thread_id: Option<String> = None;
        let mut parse_errors = 0usize;

        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    warn!("failed to parse rollout line: {e}");
                    parse_errors += 1;
                    continue;
                }
            };
            match serde_json::from_value::<RolloutLine>(v) {
                Ok(rollout_line) => {
                    if let RolloutItem::SessionMeta(ref meta_line) = rollout_line.item {
                        if thread_id.is_none() {
                            thread_id = Some(meta_line.meta.id.clone());
                        }
                    }
                    items.push(rollout_line.item);
                }
                Err(e) => {
                    trace!("failed to parse rollout line structure: {e}");
                    parse_errors += 1;
                }
            }
        }

        info!(
            "Loaded rollout: {} items, thread_id={:?}, parse_errors={}",
            items.len(),
            thread_id,
            parse_errors
        );
        Ok((items, thread_id, parse_errors))
    }

    /// Get the rollout history for session resumption.
    pub async fn get_rollout_history(
        path: &Path,
    ) -> std::io::Result<ResumedHistory> {
        let (items, thread_id, _) = Self::load_rollout_items(path).await?;
        let conversation_id = thread_id
            .ok_or_else(|| IoError::other("missing thread ID in rollout file"))?;
        if items.is_empty() {
            return Err(IoError::other("empty rollout history"));
        }
        info!("Resumed rollout from {path:?}");
        Ok(ResumedHistory {
            conversation_id,
            history: items,
            rollout_path: path.to_path_buf(),
        })
    }

    pub fn rollout_path(&self) -> &Path {
        &self.rollout_path
    }
}

/// History loaded from a rollout file for session resumption.
#[derive(Debug, Clone)]
pub struct ResumedHistory {
    pub conversation_id: String,
    pub history: Vec<RolloutItem>,
    pub rollout_path: PathBuf,
}
