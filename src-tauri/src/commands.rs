use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::config::{deserialize_toml, ConfigLayerStack};
use crate::protocol::event::{Event, EventMsg};
use crate::protocol::submission::{Op, Submission};

/// Per-thread (session) handle.
pub struct ThreadHandle {
    pub sq_tx: async_channel::Sender<Submission>,
    pub eq_rx: async_channel::Receiver<Event>,
}

/// Shared application state managed by Tauri.
pub struct AppState {
    pub threads: Arc<Mutex<HashMap<String, ThreadHandle>>>,
    pub config: Arc<Mutex<ConfigLayerStack>>,
}

// ── Event bridge ─────────────────────────────────────────────────

/// Spawn a background task that reads events from a thread's EQ and emits
/// them to the frontend via Tauri's event system.
pub fn spawn_event_bridge(app: AppHandle, thread_id: String, eq_rx: async_channel::Receiver<Event>) {
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = eq_rx.recv().await {
            let payload = EventBridgePayload {
                thread_id: thread_id.clone(),
                event,
            };
            let _ = app.emit("codex-event", &payload);
        }
    });
}

#[derive(Clone, serde::Serialize)]
struct EventBridgePayload {
    thread_id: String,
    event: Event,
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

/// Start a new thread (session). Returns the thread_id.
#[tauri::command]
pub async fn thread_start(
    app: AppHandle,
    state: State<'_, AppState>,
    thread_id: String,
    cwd: Option<String>,
) -> Result<String, String> {
    let config = state.config.lock().await.clone();
    let work_dir = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let handle = crate::core::codex::Codex::spawn(config, work_dir)
        .await
        .map_err(|e| format!("spawn failed: {e}"))?;

    spawn_event_bridge(app, thread_id.clone(), handle.rx_event);

    let mut threads = state.threads.lock().await;
    threads.insert(
        thread_id.clone(),
        ThreadHandle {
            sq_tx: handle.tx_sub,
            eq_rx: async_channel::unbounded().1, // bridge owns the real rx
        },
    );
    Ok(thread_id)
}

/// List active thread IDs.
#[tauri::command]
pub async fn thread_list(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let threads = state.threads.lock().await;
    Ok(threads.keys().cloned().collect())
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
    Ok(())
}

/// Fork a thread: create a new thread from an existing one's config.
#[tauri::command]
pub async fn thread_fork(
    app: AppHandle,
    state: State<'_, AppState>,
    source_thread_id: String,
    new_thread_id: String,
    cwd: Option<String>,
) -> Result<String, String> {
    // Verify source exists
    {
        let threads = state.threads.lock().await;
        if !threads.contains_key(&source_thread_id) {
            return Err(format!("source thread not found: {source_thread_id}"));
        }
    }

    // Create a new thread with the same config
    let config = state.config.lock().await.clone();
    let work_dir = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let handle = crate::core::codex::Codex::spawn(config, work_dir)
        .await
        .map_err(|e| format!("spawn failed: {e}"))?;

    spawn_event_bridge(app, new_thread_id.clone(), handle.rx_event);

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
