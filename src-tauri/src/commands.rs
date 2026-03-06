use std::sync::Arc;

use tauri::State;
use tokio::sync::Mutex;

use crate::config::{deserialize_toml, ConfigLayerStack};
use crate::protocol::event::Event;
use crate::protocol::submission::{Op, Submission};

/// Shared application state managed by Tauri.
pub struct AppState {
    pub sq_tx: async_channel::Sender<Submission>,
    pub eq_rx: Arc<Mutex<async_channel::Receiver<Event>>>,
    pub config: Arc<Mutex<ConfigLayerStack>>,
}

// ── Tauri commands ───────────────────────────────────────────────

/// Submit an operation to the core engine.
#[tauri::command]
pub async fn submit_op(
    state: State<'_, AppState>,
    id: String,
    op: serde_json::Value,
) -> Result<(), String> {
    let op: Op = serde_json::from_value(op).map_err(|e| format!("invalid op: {e}"))?;
    state
        .sq_tx
        .send(Submission { id, op })
        .await
        .map_err(|e| format!("send failed: {e}"))
}

/// Poll for pending events from the core engine.
#[tauri::command]
pub async fn poll_events(
    state: State<'_, AppState>,
    max_count: Option<usize>,
) -> Result<Vec<Event>, String> {
    let rx = state.eq_rx.lock().await;
    let limit = max_count.unwrap_or(100);
    let mut events = Vec::new();
    while events.len() < limit {
        match rx.try_recv() {
            Ok(event) => events.push(event),
            Err(_) => break,
        }
    }
    Ok(events)
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
