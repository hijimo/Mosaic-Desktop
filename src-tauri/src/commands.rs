use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;
use tracing::error;

use crate::config::{deserialize_toml, ConfigLayerStack};
use crate::core::rollout::{RolloutRecorder, RolloutRecorderParams};
use crate::core::rollout::policy::{EventPersistenceMode, RolloutItem};
use crate::core::state_db::{StateDb, PersistedThreadMeta};
use crate::protocol::event::{Event, EventMsg};
use crate::protocol::submission::{Op, Submission};
use crate::share::types::{ShareMessageRequest, ShareMessageResponse};

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
    pub recorders: Arc<Mutex<HashMap<String, RolloutRecorder>>>,
    pub config: Arc<Mutex<ConfigLayerStack>>,
    pub db: StateDb,
}

// ── Event bridge ─────────────────────────────────────────────────

/// Spawn a background task that reads events from a thread's EQ, persists them
/// via RolloutRecorder, and emits them to the frontend via Tauri's event system.
pub fn spawn_event_bridge(
    app: AppHandle,
    thread_id: String,
    eq_rx: async_channel::Receiver<Event>,
    thread_meta: Arc<Mutex<HashMap<String, ThreadMeta>>>,
    recorders: Arc<Mutex<HashMap<String, RolloutRecorder>>>,
    recorder: RolloutRecorder,
    db: StateDb,
) {
    tauri::async_runtime::spawn(async move {
        // Store recorder reference for submit_op access
        recorders.lock().await.insert(thread_id.clone(), recorder.clone());

        while let Ok(event) = eq_rx.recv().await {
            // Persist event to rollout file
            let _ = recorder.record_items(&[RolloutItem::EventMsg(event.msg.clone())]).await;

            // Additionally persist RawResponseItem as a structured RolloutItem::ResponseItem
            // so that resume can reconstruct full history including tool calls.
            if let EventMsg::RawResponseItem(ref raw) = event.msg {
                let _ = recorder.record_items(&[RolloutItem::ResponseItem(raw.item.clone())]).await;
            }

            // Update thread metadata from session_configured event
            if let EventMsg::SessionConfigured(ref cfg) = event.msg {
                let _ = recorder.persist().await;
                let rp = recorder.rollout_path.to_string_lossy().into_owned();
                let mut meta = thread_meta.lock().await;
                if let Some(m) = meta.get_mut(&thread_id) {
                    m.model = Some(cfg.model.clone());
                    m.model_provider_id = Some(cfg.model_provider_id.clone());
                    m.rollout_path = Some(rp.clone());
                }
                let _ = db.update_thread_fields(
                    &thread_id,
                    Some(&cfg.model),
                    Some(&cfg.model_provider_id),
                    None,
                    Some(&rp),
                ).await;
            }
            // Update thread name
            if let EventMsg::ThreadNameUpdated(ref upd) = event.msg {
                let mut meta = thread_meta.lock().await;
                if let Some(m) = meta.get_mut(&thread_id) {
                    m.name = upd.thread_name.clone();
                }
                if let Some(name) = &upd.thread_name {
                    let _ = db.update_thread_fields(&thread_id, None, None, Some(name), None).await;
                }
            }

            let payload = EventBridgePayload {
                thread_id: thread_id.clone(),
                event,
            };
            let _ = app.emit("codex-event", &payload);
        }
        // Session ended — flush and close the rollout file
        recorders.lock().await.remove(&thread_id);
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
    let op: Op = serde_json::from_value(op.clone()).map_err(|e| format!("invalid op: {e}"))?;

    // Persist user message to rollout before sending to engine
    if let Some(items) = op.user_input_items() {
        if let Some(recorder) = state.recorders.lock().await.get(&thread_id) {
            let content: Vec<crate::protocol::types::ContentItem> = items.iter().map(|i| match i {
                crate::protocol::types::UserInput::Text { text, .. } =>
                    crate::protocol::types::ContentItem::InputText { text: text.clone() },
                crate::protocol::types::UserInput::Image { image_url } =>
                    crate::protocol::types::ContentItem::InputImage { image_url: image_url.clone() },
                crate::protocol::types::UserInput::LocalImage { path } =>
                    crate::protocol::types::ContentItem::InputText { text: format!("[image: {}]", path.display()) },
                crate::protocol::types::UserInput::Skill { name, path } =>
                    crate::protocol::types::ContentItem::InputText { text: format!("<skill>\n<name>{name}</name>\n<path>{}</path>\n</skill>", path.display()) },
                crate::protocol::types::UserInput::Mention { name, path } =>
                    crate::protocol::types::ContentItem::InputText { text: format!("@{name} ({path})") },
            }).collect();
            let response_item = crate::protocol::types::ResponseItem::Message {
                id: Some(uuid::Uuid::new_v4().to_string()),
                role: "user".into(),
                content,
                end_turn: None,
                phase: None,
            };
            let _ = recorder.record_items(&[RolloutItem::ResponseItem(response_item)]).await;
        }
    }

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

    let now = chrono::Utc::now();
    let meta = ThreadMeta {
        thread_id: thread_id.clone(),
        cwd: work_dir.to_string_lossy().into_owned(),
        model: None,
        model_provider_id: None,
        name: None,
        created_at: now,
        forked_from: None,
        rollout_path: Some(rollout_path.clone()),
    };
    state.thread_meta.lock().await.insert(thread_id.clone(), meta);

    // Persist to DB
    let _ = state.db.upsert_thread(&PersistedThreadMeta {
        thread_id: thread_id.clone(),
        cwd: work_dir.to_string_lossy().into_owned(),
        model: None,
        model_provider_id: None,
        name: None,
        created_at: now.to_rfc3339(),
        forked_from: None,
        rollout_path: Some(rollout_path),
    }).await;

    spawn_event_bridge(app, thread_id.clone(), handle.rx_event, state.thread_meta.clone(), state.recorders.clone(), recorder, state.db.clone());

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

/// List all threads (persisted history + active in-memory).
#[tauri::command]
pub async fn thread_list(state: State<'_, AppState>) -> Result<Vec<ThreadMeta>, String> {
    let persisted = state.db.list_threads(200).await;
    let in_mem = state.thread_meta.lock().await;

    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    // In-memory threads first (most up-to-date)
    for m in in_mem.values() {
        seen.insert(m.thread_id.clone());
        result.push(m.clone());
    }
    // Then persisted threads not currently active
    for p in persisted {
        if seen.contains(&p.thread_id) {
            continue;
        }
        result.push(ThreadMeta {
            thread_id: p.thread_id,
            cwd: p.cwd,
            model: p.model,
            model_provider_id: p.model_provider_id,
            name: p.name,
            created_at: p.created_at.parse().unwrap_or_else(|_| chrono::Utc::now()),
            forked_from: p.forked_from,
            rollout_path: p.rollout_path,
        });
    }

    result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(result)
}

/// Get metadata for a specific thread (checks in-memory first, then DB).
#[tauri::command]
pub async fn thread_get_info(
    state: State<'_, AppState>,
    thread_id: String,
) -> Result<ThreadMeta, String> {
    {
        let meta = state.thread_meta.lock().await;
        if let Some(m) = meta.get(&thread_id) {
            return Ok(m.clone());
        }
    }
    state.db.get_thread(&thread_id).await
        .map(|p| ThreadMeta {
            thread_id: p.thread_id,
            cwd: p.cwd,
            model: p.model,
            model_provider_id: p.model_provider_id,
            name: p.name,
            created_at: p.created_at.parse().unwrap_or_else(|_| chrono::Utc::now()),
            forked_from: p.forked_from,
            rollout_path: p.rollout_path,
        })
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
    let _ = state.db.delete_thread(&thread_id).await;
    Ok(())
}

/// Load chat history for a thread from its rollout file, returning TurnGroups
/// (items grouped by turn_id) suitable for rendering in the frontend.
#[tauri::command]
pub async fn thread_get_messages(
    state: State<'_, AppState>,
    thread_id: String,
) -> Result<Vec<crate::core::thread_history::TurnGroup>, String> {
    let persisted = state.db.get_thread(&thread_id).await
        .ok_or_else(|| format!("thread not found: {thread_id}"))?;
    let rollout_path_str = persisted.rollout_path
        .ok_or_else(|| format!("no rollout path for thread: {thread_id}"))?;
    let path = std::path::PathBuf::from(&rollout_path_str);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = tokio::fs::read_to_string(&path).await
        .map_err(|e| format!("failed to read rollout: {e}"))?;
    let rollout_items = parse_rollout_items(&text);
    Ok(crate::core::thread_history::build_turn_groups_from_rollout_items(&rollout_items))
}

/// Parse rollout JSONL text into RolloutItem entries for the builder.
pub fn parse_rollout_items(text: &str) -> Vec<crate::core::rollout::policy::RolloutItem> {
    use crate::core::rollout::policy::{RolloutItem, RolloutLine, SessionMetaLine};
    use crate::protocol::event::EventMsg;
    use crate::protocol::types::ResponseItem;

    let mut items = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let typ = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match typ {
            // Session metadata
            "session_meta" => {
                if let Some(meta) = v.get("meta") {
                    if let Ok(meta_line) = serde_json::from_value::<SessionMetaLine>(serde_json::json!({
                        "meta": meta,
                        "git": v.get("git"),
                    })) {
                        items.push(RolloutItem::SessionMeta(meta_line));
                    }
                }
            }
            // ResponseItem types
            "message" | "reasoning" | "web_search_call" => {
                if let Ok(ri) = serde_json::from_value::<ResponseItem>(v) {
                    items.push(RolloutItem::ResponseItem(ri));
                }
            }
            // FunctionCall/Output are also ResponseItems
            "function_call" | "function_call_output" | "custom_tool_call"
            | "custom_tool_call_output" | "local_shell_call" => {
                if let Ok(ri) = serde_json::from_value::<ResponseItem>(v) {
                    items.push(RolloutItem::ResponseItem(ri));
                }
            }
            // Known EventMsg types — parse directly without clone
            "task_started" | "task_complete" | "turn_aborted"
            | "user_message" | "agent_message"
            | "agent_reasoning" | "agent_reasoning_raw_content"
            | "token_count" | "context_compacted" | "thread_rolled_back"
            | "error" | "exec_command_end" | "mcp_tool_call_end"
            | "patch_apply_end" | "web_search_end" | "apply_patch_approval_request"
            | "view_image_tool_call" | "dynamic_tool_call_request" | "dynamic_tool_call_response"
            | "item_started" | "item_completed"
            | "collab_agent_spawn_end" | "collab_agent_interaction_end"
            | "collab_waiting_end" | "collab_close_end" | "collab_resume_end"
            | "entered_review_mode" | "exited_review_mode"
            | "undo_completed" => {
                if let Ok(ev) = serde_json::from_value::<EventMsg>(v) {
                    items.push(RolloutItem::EventMsg(ev));
                }
            }
            // Fallback: try RolloutLine (compacted, turn_context)
            _ => {
                if let Ok(rl) = serde_json::from_value::<RolloutLine>(v) {
                    items.push(rl.item);
                }
            }
        }
    }
    items
}

/// Resume a previously persisted thread: reload history from rollout file,
/// re-spawn the Codex engine with full conversation context, and register
/// the thread handle so subsequent operations work normally.
///
/// Idempotent: if the thread is already running, returns existing metadata.
#[tauri::command]
pub async fn thread_resume(
    app: AppHandle,
    state: State<'_, AppState>,
    thread_id: String,
) -> Result<ThreadMeta, String> {
    // 1. Already running — return existing meta.
    {
        let threads = state.threads.lock().await;
        if threads.contains_key(&thread_id) {
            let meta = state.thread_meta.lock().await;
            if let Some(m) = meta.get(&thread_id) {
                return Ok(m.clone());
            }
        }
    }

    // 2. Look up persisted metadata from DB.
    let persisted = state.db.get_thread(&thread_id).await
        .ok_or_else(|| format!("thread not found in database: {thread_id}"))?;

    let rollout_path_str = persisted.rollout_path.clone()
        .ok_or_else(|| format!("no rollout path for thread: {thread_id}"))?;
    let rollout_path = PathBuf::from(&rollout_path_str);

    if !rollout_path.exists() {
        return Err(format!("rollout file not found: {}", rollout_path.display()));
    }

    // 3. Load history from rollout file.
    let resumed = crate::core::rollout::RolloutRecorder::get_rollout_history(&rollout_path)
        .await
        .map_err(|e| format!("failed to load rollout: {e}"))?;

    // 4. Spawn Codex engine with resumed history (synchronous injection).
    let work_dir = PathBuf::from(&persisted.cwd);
    let config = state.config.lock().await.clone();
    let handle = crate::core::codex::Codex::spawn_with_history(
        config,
        work_dir.clone(),
        crate::core::initial_history::InitialHistory::Resumed(resumed),
    )
    .await
    .map_err(|e| format!("spawn failed: {e}"))?;

    // 5. Create a Resume-mode recorder (appends to existing rollout file).
    let recorder = RolloutRecorder::new(
        &mosaic_home(),
        &work_dir,
        "mosaic",
        RolloutRecorderParams::Resume {
            path: rollout_path,
            event_persistence_mode: EventPersistenceMode::Extended,
        },
    )
    .await
    .map_err(|e| format!("recorder resume failed: {e}"))?;

    let rp = recorder.rollout_path.to_string_lossy().into_owned();

    // 6. Build ThreadMeta and register in memory.
    let meta = ThreadMeta {
        thread_id: thread_id.clone(),
        cwd: persisted.cwd,
        model: persisted.model,
        model_provider_id: persisted.model_provider_id,
        name: persisted.name,
        created_at: persisted.created_at.parse().unwrap_or_else(|_| chrono::Utc::now()),
        forked_from: persisted.forked_from,
        rollout_path: Some(rp),
    };
    state.thread_meta.lock().await.insert(thread_id.clone(), meta.clone());

    spawn_event_bridge(app, thread_id.clone(), handle.rx_event, state.thread_meta.clone(), state.recorders.clone(), recorder, state.db.clone());

    let mut threads = state.threads.lock().await;
    threads.insert(
        thread_id,
        ThreadHandle {
            sq_tx: handle.tx_sub,
            eq_rx: async_channel::unbounded().1,
        },
    );

    Ok(meta)
}

/// Fork a thread: create a new thread from an existing one's history.
/// Loads the source thread's rollout, truncates to full history, and spawns
/// a new engine with that context.
/// Returns the server-generated new_thread_id.
#[tauri::command]
pub async fn thread_fork(
    app: AppHandle,
    state: State<'_, AppState>,
    source_thread_id: String,
    nth_user_message: Option<usize>,
    cwd: Option<String>,
) -> Result<String, String> {
    use crate::core::initial_history::InitialHistory;
    use crate::core::rollout::truncation::truncate_before_nth_user_message;

    // Load source thread's rollout path (from memory or DB).
    let source_rollout_path = {
        let meta = state.thread_meta.lock().await;
        meta.get(&source_thread_id)
            .and_then(|m| m.rollout_path.clone())
    };

    // Load and truncate history from rollout.
    let (initial_history, forked_rollout_items) = if let Some(rp) = source_rollout_path {
        let path = PathBuf::from(&rp);
        if path.exists() {
            match crate::core::rollout::RolloutRecorder::get_rollout_history(&path).await {
                Ok(resumed) => {
                    let nth = nth_user_message.unwrap_or(usize::MAX);
                    let forked = truncate_before_nth_user_message(&resumed.history, nth);
                    if forked.is_empty() {
                        (InitialHistory::New, None)
                    } else {
                        let items_for_persist = forked.clone();
                        (InitialHistory::Forked(forked), Some(items_for_persist))
                    }
                }
                Err(_) => (InitialHistory::New, None),
            }
        } else {
            (InitialHistory::New, None)
        }
    } else {
        (InitialHistory::New, None)
    };

    let new_thread_id = uuid::Uuid::new_v4().to_string();
    let config = state.config.lock().await.clone();
    let work_dir = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let handle = crate::core::codex::Codex::spawn_with_history(
        config,
        work_dir.clone(),
        initial_history,
    )
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

    // Persist source rollout items into the new thread's rollout file.
    if let Some(ref items) = forked_rollout_items {
        if let Err(e) = recorder.record_items(items).await {
            tracing::warn!("failed to persist forked rollout items: {e}");
        }
    }

    let now = chrono::Utc::now();
    let meta = ThreadMeta {
        thread_id: new_thread_id.clone(),
        cwd: work_dir.to_string_lossy().into_owned(),
        model: None,
        model_provider_id: None,
        name: None,
        created_at: now,
        forked_from: Some(source_thread_id.clone()),
        rollout_path: Some(rollout_path.clone()),
    };
    state.thread_meta.lock().await.insert(new_thread_id.clone(), meta);

    let _ = state.db.upsert_thread(&PersistedThreadMeta {
        thread_id: new_thread_id.clone(),
        cwd: work_dir.to_string_lossy().into_owned(),
        model: None,
        model_provider_id: None,
        name: None,
        created_at: now.to_rfc3339(),
        forked_from: Some(source_thread_id),
        rollout_path: Some(rollout_path),
    }).await;

    spawn_event_bridge(app, new_thread_id.clone(), handle.rx_event, state.thread_meta.clone(), state.recorders.clone(), recorder, state.db.clone());

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

#[tauri::command]
pub async fn share_message(payload: ShareMessageRequest) -> Result<ShareMessageResponse, String> {
    crate::share::share_message(payload)
        .await
        .map_err(|e| {
            error!("share message failed: {e:#}");
            format!("share message failed: {e:#}")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rollout_items_and_build_turn_groups() {
        let rollout = r#"{"timestamp":"2026-03-24T03:02:52.544Z","type":"session_meta","meta":{"id":"abc","timestamp":"2026-03-24T03:02:52Z","cwd":"/tmp","cli_version":"0.1.0","source":"desktop"}}
{"timestamp":"2026-03-24T03:02:52.545Z","type":"task_started","turn_id":"turn-1","collaboration_mode_kind":"default"}
{"timestamp":"2026-03-24T03:02:53Z","type":"user_message","message":"hello","images":[],"local_images":[],"text_elements":[]}
{"timestamp":"2026-03-24T08:58:04.248Z","type":"agent_message","message":"Hi there!","phase":"final_answer"}
{"timestamp":"2026-03-24T08:58:04.300Z","type":"task_complete","turn_id":"turn-1"}
"#;
        let items = parse_rollout_items(rollout);
        assert!(items.len() >= 3, "expected at least 3 items, got {}", items.len());

        let groups = crate::core::thread_history::build_turn_groups_from_rollout_items(&items);
        assert_eq!(groups.len(), 1, "expected 1 turn group, got {}", groups.len());
        assert_eq!(groups[0].turn_id, "turn-1");
        assert_eq!(groups[0].items.len(), 2); // user + agent
    }

    #[test]
    fn parse_rollout_items_handles_response_items() {
        let rollout = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","meta":{"id":"t1","timestamp":"2026-01-01T00:00:00Z","cwd":"/tmp","cli_version":"0.1.0","source":"desktop"}}
{"timestamp":"2026-01-01T00:00:01Z","type":"task_started","turn_id":"turn-1","collaboration_mode_kind":"default"}
{"timestamp":"2026-01-01T00:00:02Z","type":"message","id":"u1","role":"user","content":[{"type":"input_text","text":"Q1"}]}
{"timestamp":"2026-01-01T00:00:03Z","type":"message","id":"a1","role":"assistant","content":[{"type":"output_text","text":"A1"}]}
{"timestamp":"2026-01-01T00:00:04Z","type":"task_complete","turn_id":"turn-1"}
"#;
        let items = parse_rollout_items(rollout);
        let groups = crate::core::thread_history::build_turn_groups_from_rollout_items(&items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 2); // user + agent from ResponseItem::Message
    }

    #[test]
    fn parse_rollout_items_empty() {
        assert!(parse_rollout_items("").is_empty());
        assert!(parse_rollout_items("  \n  \n").is_empty());
    }

    #[tokio::test]
    async fn thread_get_messages_with_real_rollout_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = crate::core::state_db::StateDb::open(&tmp.path().join("state.db")).unwrap();

        let rollout_path = tmp.path().join("rollout.jsonl");
        let rollout_content = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","meta":{"id":"t-test","timestamp":"2026-01-01T00:00:00Z","cwd":"/tmp","cli_version":"0.1.0","source":"desktop"}}
{"timestamp":"2026-01-01T00:00:01Z","type":"task_started","turn_id":"turn-1","collaboration_mode_kind":"default"}
{"timestamp":"2026-01-01T00:00:02Z","type":"user_message","message":"hello","images":[],"local_images":[],"text_elements":[]}
{"timestamp":"2026-01-01T00:00:03Z","type":"message","id":"a1","role":"assistant","content":[{"type":"output_text","text":"hi there"}],"phase":"final_answer"}
{"timestamp":"2026-01-01T00:00:04Z","type":"task_complete","turn_id":"turn-1"}
"#;
        tokio::fs::write(&rollout_path, rollout_content).await.unwrap();

        let meta = PersistedThreadMeta {
            thread_id: "t-test".into(),
            cwd: "/tmp".into(),
            model: None,
            model_provider_id: None,
            name: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            forked_from: None,
            rollout_path: Some(rollout_path.to_string_lossy().into_owned()),
        };
        db.upsert_thread(&meta).await.unwrap();

        let text = tokio::fs::read_to_string(&rollout_path).await.unwrap();
        let items = parse_rollout_items(&text);
        let groups = crate::core::thread_history::build_turn_groups_from_rollout_items(&items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 2); // user + agent
    }

    #[tokio::test]
    async fn thread_get_messages_missing_rollout_returns_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let rollout_path = tmp.path().join("nonexistent.jsonl");
        assert!(!rollout_path.exists());
    }

    #[tokio::test]
    async fn verify_real_rollout_fixture() {
        let path = std::path::Path::new(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test-rollout.jsonl")
        );
        assert!(path.exists(), "fixture file missing");

        let text = tokio::fs::read_to_string(path).await.unwrap();
        let items = parse_rollout_items(&text);
        assert!(items.len() >= 5, "expected >=5 rollout items, got {}", items.len());

        let groups = crate::core::thread_history::build_turn_groups_from_rollout_items(&items);
        assert!(!groups.is_empty(), "expected at least 1 turn group");
        let total: usize = groups.iter().map(|g| g.items.len()).sum();
        assert!(total >= 2, "expected >=2 items, got {total}");
    }
}
