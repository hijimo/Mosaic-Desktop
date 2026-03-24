use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::config::{deserialize_toml, ConfigLayerStack};
use crate::core::rollout::{RolloutRecorder, RolloutRecorderParams};
use crate::core::rollout::policy::{EventPersistenceMode, RolloutItem};
use crate::core::state_db::{StateDb, PersistedThreadMeta};
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
    recorder: RolloutRecorder,
    db: StateDb,
) {
    tauri::async_runtime::spawn(async move {
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

    spawn_event_bridge(app, thread_id.clone(), handle.rx_event, state.thread_meta.clone(), recorder, state.db.clone());

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

    spawn_event_bridge(app, thread_id.clone(), handle.rx_event, state.thread_meta.clone(), recorder, state.db.clone());

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

    spawn_event_bridge(app, new_thread_id.clone(), handle.rx_event, state.thread_meta.clone(), recorder, state.db.clone());

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
