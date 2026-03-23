use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::config::{deserialize_toml, ConfigLayerStack};
use crate::core::rollout::{RolloutRecorder, RolloutRecorderParams};
use crate::core::rollout::policy::{EventPersistenceMode, RolloutItem};
use crate::protocol::event::{Event, EventMsg};
use crate::protocol::submission::{Op, Submission};

/// Per-thread (session) handle.
pub struct ThreadHandle {
    pub sq_tx: async_channel::Sender<Submission>,
    pub eq_rx: async_channel::Receiver<Event>,
}

/// Lightweight metadata for a thread, queryable without a running engine.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThreadMeta {
    pub thread_id: String,
    pub cwd: String,
    /// Populated after the first `session_configured` event is received.
    pub model: Option<String>,
    pub model_provider_id: Option<String>,
    pub name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Forked from this thread_id, if applicable.
    pub forked_from: Option<String>,
    /// Path to the rollout JSONL file on disk.
    pub rollout_path: Option<String>,
}

/// Shared application state managed by Tauri.
pub struct AppState {
    pub threads: Arc<Mutex<HashMap<String, ThreadHandle>>>,
    pub thread_meta: Arc<Mutex<HashMap<String, ThreadMeta>>>,
    pub config: Arc<Mutex<ConfigLayerStack>>,
}

// ── Event bridge ─────────────────────────────────────────────────

/// Spawn a background task that reads events from a thread's EQ, persists them
/// via RolloutRecorder, and emits them to the frontend via Tauri's event system.
pub fn spawn_event_bridge(
    app: AppHandle,
    thread_id: String,
    eq_rx: async_channel::Receiver<Event>,
    thread_meta: Arc<Mutex<HashMap<String, ThreadMeta>>>,
    recorder: RolloutRecorder,
) {
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = eq_rx.recv().await {
            // Persist event to rollout file
            let _ = recorder.record_items(&[RolloutItem::EventMsg(event.msg.clone())]).await;

            // Update thread metadata from session_configured event
            if let EventMsg::SessionConfigured(ref cfg) = event.msg {
                let _ = recorder.persist().await;
                let mut meta = thread_meta.lock().await;
                if let Some(m) = meta.get_mut(&thread_id) {
                    m.model = Some(cfg.model.clone());
                    m.model_provider_id = Some(cfg.model_provider_id.clone());
                    m.rollout_path = Some(recorder.rollout_path.to_string_lossy().into_owned());
                }
            }
            // Update thread name
            if let EventMsg::ThreadNameUpdated(ref upd) = event.msg {
                let mut meta = thread_meta.lock().await;
                if let Some(m) = meta.get_mut(&thread_id) {
                    m.name = upd.thread_name.clone();
                }
            }

            let payload = EventBridgePayload {
                thread_id: thread_id.clone(),
                event,
            };
            let _ = app.emit("codex-event", &payload);
        }
        // Session ended — flush and close the rollout file
        let _ = recorder.shutdown().await;
    });
}

#[derive(Clone, serde::Serialize)]
struct EventBridgePayload {
    thread_id: String,
    event: Event,
}

/// Returns the mosaic home directory (~/.mosaic).
fn mosaic_home() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".mosaic"))
        .unwrap_or_else(|| PathBuf::from(".mosaic"))
}

// ── Tauri commands ───────────────────────────────────────────────

/// Submit an operation to a specific thread.
#[tauri::command]
pub async fn submit_op(
    state: State<'_, AppState>,
    thread_id: String,
    id: String,
    op: serde_json::Value,
) -> Result<(), String> {
    let op: Op = serde_json::from_value(op).map_err(|e| format!("invalid op: {e}"))?;
    let threads = state.threads.lock().await;
    let handle = threads
        .get(&thread_id)
        .ok_or_else(|| format!("thread not found: {thread_id}"))?;
    handle
        .sq_tx
        .send(Submission { id, op })
        .await
        .map_err(|e| format!("send failed: {e}"))
}

/// Start a new thread (session). Returns the server-generated thread_id.
#[tauri::command]
pub async fn thread_start(
    app: AppHandle,
    state: State<'_, AppState>,
    cwd: Option<String>,
) -> Result<String, String> {
    let thread_id = uuid::Uuid::new_v4().to_string();
    let config = state.config.lock().await.clone();
    let work_dir = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let handle = crate::core::codex::Codex::spawn(config, work_dir.clone())
        .await
        .map_err(|e| format!("spawn failed: {e}"))?;

    let recorder = RolloutRecorder::new(
        &mosaic_home(),
        &work_dir,
        "mosaic",
        RolloutRecorderParams::Create {
            conversation_id: thread_id.clone(),
            source: crate::core::rollout::policy::SessionSource::Desktop,
            event_persistence_mode: EventPersistenceMode::Extended,
        },
    )
    .await
    .map_err(|e| format!("recorder init failed: {e}"))?;

    let rollout_path = recorder.rollout_path.to_string_lossy().into_owned();

    let meta = ThreadMeta {
        thread_id: thread_id.clone(),
        cwd: work_dir.to_string_lossy().into_owned(),
        model: None,
        model_provider_id: None,
        name: None,
        created_at: chrono::Utc::now(),
        forked_from: None,
        rollout_path: Some(rollout_path),
    };
    state.thread_meta.lock().await.insert(thread_id.clone(), meta);

    spawn_event_bridge(app, thread_id.clone(), handle.rx_event, state.thread_meta.clone(), recorder);

    let mut threads = state.threads.lock().await;
    threads.insert(
        thread_id.clone(),
        ThreadHandle {
            sq_tx: handle.tx_sub,
            eq_rx: async_channel::unbounded().1,
        },
    );
    Ok(thread_id)
}

/// List active thread IDs.
#[tauri::command]
pub async fn thread_list(state: State<'_, AppState>) -> Result<Vec<ThreadMeta>, String> {
    let meta = state.thread_meta.lock().await;
    Ok(meta.values().cloned().collect())
}

/// Get metadata for a specific thread.
#[tauri::command]
pub async fn thread_get_info(
    state: State<'_, AppState>,
    thread_id: String,
) -> Result<ThreadMeta, String> {
    let meta = state.thread_meta.lock().await;
    meta.get(&thread_id)
        .cloned()
        .ok_or_else(|| format!("thread not found: {thread_id}"))
}

/// Archive (remove) a thread.
#[tauri::command]
pub async fn thread_archive(
    state: State<'_, AppState>,
    thread_id: String,
) -> Result<(), String> {
    let mut threads = state.threads.lock().await;
    let handle = threads
        .remove(&thread_id)
        .ok_or_else(|| format!("thread not found: {thread_id}"))?;
    // Send shutdown to gracefully stop the engine
    let _ = handle.sq_tx.send(Submission {
        id: uuid::Uuid::new_v4().to_string(),
        op: Op::Shutdown,
    }).await;
    state.thread_meta.lock().await.remove(&thread_id);
    Ok(())
}

/// Fork a thread: create a new thread from an existing one's config.
/// Returns the server-generated new_thread_id.
#[tauri::command]
pub async fn thread_fork(
    app: AppHandle,
    state: State<'_, AppState>,
    source_thread_id: String,
    cwd: Option<String>,
) -> Result<String, String> {
    {
        let threads = state.threads.lock().await;
        if !threads.contains_key(&source_thread_id) {
            return Err(format!("source thread not found: {source_thread_id}"));
        }
    }

    let new_thread_id = uuid::Uuid::new_v4().to_string();
    let config = state.config.lock().await.clone();
    let work_dir = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let handle = crate::core::codex::Codex::spawn(config, work_dir.clone())
        .await
        .map_err(|e| format!("spawn failed: {e}"))?;

    let recorder = RolloutRecorder::new(
        &mosaic_home(),
        &work_dir,
        "mosaic",
        RolloutRecorderParams::Create {
            conversation_id: new_thread_id.clone(),
            source: crate::core::rollout::policy::SessionSource::Desktop,
            event_persistence_mode: EventPersistenceMode::Extended,
        },
    )
    .await
    .map_err(|e| format!("recorder init failed: {e}"))?;

    let rollout_path = recorder.rollout_path.to_string_lossy().into_owned();

    let meta = ThreadMeta {
        thread_id: new_thread_id.clone(),
        cwd: work_dir.to_string_lossy().into_owned(),
        model: None,
        model_provider_id: None,
        name: None,
        created_at: chrono::Utc::now(),
        forked_from: Some(source_thread_id),
        rollout_path: Some(rollout_path),
    };
    state.thread_meta.lock().await.insert(new_thread_id.clone(), meta);

    spawn_event_bridge(app, new_thread_id.clone(), handle.rx_event, state.thread_meta.clone(), recorder);

    let mut threads = state.threads.lock().await;
    threads.insert(
        new_thread_id.clone(),
        ThreadHandle {
            sq_tx: handle.tx_sub,
            eq_rx: async_channel::unbounded().1,
        },
    );
    Ok(new_thread_id)
}

/// One-shot fuzzy file search.
#[tauri::command]
pub async fn fuzzy_file_search(
    query: String,
    roots: Vec<String>,
) -> Result<Vec<crate::file_search::FileMatch>, String> {
    let roots: Vec<PathBuf> = roots.into_iter().map(PathBuf::from).collect();
    if roots.is_empty() {
        return Err("at least one root directory is required".into());
    }
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let result = tokio::task::spawn_blocking(move || {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let threads = std::num::NonZero::new(cpus.min(12)).unwrap();
        let limit = std::num::NonZero::new(50).unwrap();
        crate::file_search::run(
            &query,
            roots,
            crate::file_search::FileSearchOptions {
                limit,
                threads,
                compute_indices: true,
                ..Default::default()
            },
            None,
        )
    })
    .await
    .map_err(|e| format!("search task failed: {e}"))?
    .map_err(|e| format!("search failed: {e}"))?;

    Ok(result.matches)
}

/// Get the current merged configuration.
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let config = state.config.lock().await;
    let merged = config.merge();
    serde_json::to_value(&merged).map_err(|e| format!("serialize failed: {e}"))
}

/// Update configuration from a TOML string.
#[tauri::command]
pub async fn update_config(
    state: State<'_, AppState>,
    toml_content: String,
) -> Result<(), String> {
    let parsed = deserialize_toml(&toml_content).map_err(|e| e.message)?;
    let mut config = state.config.lock().await;
    config.add_layer(crate::config::ConfigLayer::Session, parsed);
    Ok(())
}

/// Get the current working directory.
#[tauri::command]
pub fn get_cwd() -> Result<String, String> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("failed to get cwd: {e}"))
}
