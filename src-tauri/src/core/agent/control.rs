/// Multi-agent management — spawning, coordinating, and lifecycle management of sub-agents.
///
/// Architecture: `AgentControl` holds a `Weak` reference to `ThreadManager`'s
/// internal thread registry. When spawning a sub-agent, it creates a real
/// `CodexThread` via the thread manager, making each agent a full-fledged
/// thread with its own Codex engine, session, and rollout.
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{watch, Mutex, RwLock};

use crate::config::ConfigLayerStack;
use crate::core::codex::Codex;
use crate::core::initial_history::InitialHistory;
use crate::core::rollout::policy::{SessionSource, SubAgentSource};
use crate::core::shell_snapshot::ShellSnapshot;
use crate::core::thread_manager::{CodexThread, ThreadManagerInner};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::submission::Op;
use crate::protocol::thread_id::ThreadId;
use crate::protocol::types::{AgentStatus, SandboxPolicy, TokenUsage, UserInput};

use super::guards::Guards;
use super::status::is_final;

const AGENT_NAMES: &str = include_str!("agent_names.txt");

fn agent_nickname_list() -> Vec<&'static str> {
    AGENT_NAMES
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect()
}

/// Notification message format for sub-agent completion.
fn format_subagent_notification_message(agent_id: &str, status: &AgentStatus) -> String {
    let payload = serde_json::json!({
        "agent_id": agent_id,
        "status": status,
    });
    format!("<subagent_notification>{}</subagent_notification>", payload)
}

/// Context line format for listing sub-agents.
fn format_subagent_context_line(agent_id: &str, agent_nickname: Option<&str>) -> String {
    match agent_nickname.filter(|n| !n.is_empty()) {
        Some(nickname) => format!("- {agent_id}: {nickname}"),
        None => format!("- {agent_id}"),
    }
}

// ── SpawnAgentOptions ────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct SpawnAgentOptions {
    pub fork_parent_spawn_call_id: Option<String>,
}

// ── AgentStatusEntry ─────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentStatusEntry {
    pub thread_id: String,
    pub nickname: String,
    pub depth: i32,
    pub status: AgentStatus,
    pub agent_role: Option<String>,
    pub parent_thread_id: Option<String>,
}

// ── AgentControl ─────────────────────────────────────────────────

/// Top-level controller for spawning and managing multiple agents.
///
/// Holds a `Weak` reference to the thread manager's inner state so that
/// spawning a sub-agent creates a real `CodexThread` with its own engine.
///
/// An `AgentControl` instance is shared per "user session" which means the same
/// `AgentControl` is used for every sub-agent. By doing so, we make sure the
/// guards are scoped to a user session.
#[derive(Clone)]
pub struct AgentControl {
    /// Weak handle to the global thread registry.
    manager: std::sync::Weak<ThreadManagerInner>,
    /// Spawn guards for concurrency limiting and nickname management.
    state: Arc<Guards>,
    /// Default config for spawned agents.
    default_config: Arc<Mutex<Option<ConfigLayerStack>>>,
    /// Default working directory.
    default_cwd: Arc<PathBuf>,
    /// Tracks the SessionSource for each spawned agent thread.
    agent_sources: Arc<tokio::sync::RwLock<std::collections::HashMap<ThreadId, SessionSource>>>,
}

impl Default for AgentControl {
    fn default() -> Self {
        Self {
            manager: std::sync::Weak::new(),
            state: Arc::new(Guards::default()),
            default_config: Arc::new(Mutex::new(None)),
            default_cwd: Arc::new(PathBuf::from(".")),
            agent_sources: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }
}

impl AgentControl {
    /// Construct a new `AgentControl` that can spawn/message agents via the given manager state.
    pub fn new(manager: std::sync::Weak<ThreadManagerInner>) -> Self {
        Self {
            manager,
            ..Default::default()
        }
    }

    /// Set the default config used when spawning sub-agents.
    pub async fn set_default_config(&self, config: ConfigLayerStack) {
        *self.default_config.lock().await = Some(config);
    }

    /// Set the default working directory.
    pub fn set_default_cwd(&mut self, cwd: PathBuf) {
        self.default_cwd = Arc::new(cwd);
    }

    /// Spawn a new agent thread and submit the initial prompt.
    pub async fn spawn_agent(
        &self,
        config: ConfigLayerStack,
        items: Vec<UserInput>,
        session_source: Option<SessionSource>,
    ) -> Result<ThreadId, CodexError> {
        self.spawn_agent_with_options(config, items, session_source, SpawnAgentOptions::default())
            .await
    }

    pub async fn spawn_agent_with_options(
        &self,
        config: ConfigLayerStack,
        items: Vec<UserInput>,
        session_source: Option<SessionSource>,
        _options: SpawnAgentOptions,
    ) -> Result<ThreadId, CodexError> {
        let inner = self.upgrade()?;
        let mut reservation = self.state.reserve_spawn_slot(None)?;

        let inherited_shell_snapshot = self
            .inherited_shell_snapshot_for_source(&inner, session_source.as_ref())
            .await;

        // Assign nickname for ThreadSpawn sub-agents.
        let session_source = match session_source {
            Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth,
                agent_role,
                ..
            })) => {
                let agent_nickname =
                    reservation.reserve_agent_nickname(&agent_nickname_list())?;
                Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id,
                    depth,
                    agent_nickname: Some(agent_nickname),
                    agent_role,
                }))
            }
            other => other,
        };
        let notification_source = session_source.clone();

        let cwd = self.default_cwd.as_ref().clone();
        let thread_id = ThreadId::new();
        let handle = Codex::spawn_with_history(config, cwd, InitialHistory::New).await?;
        let _session_configured =
            crate::core::thread_manager::wait_for_session_configured(&handle).await?;

        let thread = Arc::new(CodexThread::new(handle, thread_id));
        inner.register_thread(thread_id, thread).await;

        reservation.commit(&thread_id.to_string());

        // Record the session source for this agent.
        if let Some(ref src) = session_source {
            self.agent_sources.write().await.insert(thread_id, src.clone());
        }

        inner.notify_thread_created(thread_id);

        self.send_input(thread_id, items).await?;
        self.maybe_start_completion_watcher(thread_id, notification_source);

        let _ = inherited_shell_snapshot; // reserved for future use
        Ok(thread_id)
    }

    /// Resume an existing agent thread from a recorded rollout file.
    pub async fn resume_agent_from_rollout(
        &self,
        config: ConfigLayerStack,
        thread_id: ThreadId,
        session_source: SessionSource,
    ) -> Result<ThreadId, CodexError> {
        let inner = self.upgrade()?;
        let mut reservation = self.state.reserve_spawn_slot(None)?;

        // Rehydrate nickname/role for ThreadSpawn sources.
        let session_source = match session_source {
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth,
                agent_role,
                ..
            }) => {
                // Try to reserve the original nickname if available.
                let reserved_nickname = reservation
                    .reserve_agent_nickname(&agent_nickname_list())
                    .ok();
                SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id,
                    depth,
                    agent_nickname: reserved_nickname,
                    agent_role,
                })
            }
            other => other,
        };
        let notification_source = session_source.clone();

        let inherited_shell_snapshot = self
            .inherited_shell_snapshot_for_source(&inner, Some(&session_source))
            .await;

        // Find the rollout path for this thread.
        let mosaic_home = dirs::home_dir()
            .map(|h| h.join(".mosaic"))
            .unwrap_or_else(|| self.default_cwd.as_ref().join(".mosaic"));
        let rollout_path =
            crate::core::rollout::find_thread_path_by_id_str(&mosaic_home, &thread_id.to_string())
                .await
                .ok()
                .flatten()
                .ok_or_else(|| {
                    CodexError::new(
                        ErrorCode::SessionError,
                        format!("Thread {thread_id} rollout not found"),
                    )
                })?;

        let resumed_history =
            crate::core::rollout::RolloutRecorder::get_rollout_history(&rollout_path)
                .await
                .map_err(|e| {
                    CodexError::new(
                        ErrorCode::InternalError,
                        format!("failed to load rollout: {e}"),
                    )
                })?;

        let cwd = self.default_cwd.as_ref().clone();
        let new_thread_id = ThreadId::new();
        let handle = Codex::spawn_with_history(
            config,
            cwd,
            InitialHistory::Resumed(resumed_history),
        )
        .await?;
        let _session_configured =
            crate::core::thread_manager::wait_for_session_configured(&handle).await?;

        let thread = Arc::new(CodexThread::new(handle, new_thread_id));
        inner.register_thread(new_thread_id, thread).await;

        reservation.commit(&new_thread_id.to_string());

        // Record the session source for this resumed agent.
        self.agent_sources
            .write()
            .await
            .insert(new_thread_id, notification_source.clone());

        inner.notify_thread_created(new_thread_id);
        self.maybe_start_completion_watcher(new_thread_id, Some(notification_source));

        let _ = inherited_shell_snapshot;
        Ok(new_thread_id)
    }

    /// Send rich user input items to an existing agent thread.
    pub async fn send_input(
        &self,
        agent_id: ThreadId,
        items: Vec<UserInput>,
    ) -> Result<String, CodexError> {
        let inner = self.upgrade()?;
        inner
            .send_op(
                agent_id,
                Op::UserInput {
                    items,
                    final_output_json_schema: None,
                },
            )
            .await
    }

    /// Interrupt the current task for an existing agent thread.
    pub async fn interrupt_agent(&self, agent_id: ThreadId) -> Result<String, CodexError> {
        let inner = self.upgrade()?;
        inner.send_op(agent_id, Op::Interrupt).await
    }

    /// Submit a shutdown request to an existing agent thread.
    pub async fn shutdown_agent(&self, agent_id: ThreadId) -> Result<String, CodexError> {
        let inner = self.upgrade()?;
        let result = inner.send_op(agent_id, Op::Shutdown).await;
        let _ = inner.remove_thread(&agent_id).await;
        self.state.release_spawned_thread(&agent_id.to_string());
        self.agent_sources.write().await.remove(&agent_id);
        result
    }

    /// Fetch the last known status for `agent_id`, returning `NotFound` when unavailable.
    pub async fn get_status(&self, agent_id: ThreadId) -> AgentStatus {
        let Ok(inner) = self.upgrade() else {
            return AgentStatus::NotFound;
        };
        let Ok(thread) = inner.get_thread(agent_id).await else {
            return AgentStatus::NotFound;
        };
        thread.agent_status()
    }

    pub async fn get_agent_nickname_and_role(
        &self,
        agent_id: ThreadId,
    ) -> Option<(Option<String>, Option<String>)> {
        let sources = self.agent_sources.read().await;
        if let Some(session_source) = sources.get(&agent_id) {
            return Some((
                session_source.get_nickname(),
                session_source.get_agent_role(),
            ));
        }
        // Fallback: agent exists but has no recorded session source.
        let Ok(inner) = self.upgrade() else {
            return None;
        };
        inner.get_thread(agent_id).await.ok()?;
        Some((None, None))
    }

    /// Subscribe to status updates for `agent_id`, yielding the latest value and changes.
    pub async fn subscribe_status(
        &self,
        agent_id: ThreadId,
    ) -> Result<watch::Receiver<AgentStatus>, CodexError> {
        let inner = self.upgrade()?;
        let thread = inner.get_thread(agent_id).await?;
        Ok(thread.subscribe_status())
    }

    pub async fn get_total_token_usage(&self, agent_id: ThreadId) -> Option<TokenUsage> {
        let Ok(inner) = self.upgrade() else {
            return None;
        };
        let Ok(thread) = inner.get_thread(agent_id).await else {
            return None;
        };
        thread.total_token_usage().await
    }

    pub async fn format_environment_context_subagents(
        &self,
        parent_thread_id: ThreadId,
    ) -> String {
        let Ok(inner) = self.upgrade() else {
            return String::new();
        };

        let sources = self.agent_sources.read().await;
        let mut agents = Vec::new();
        for tid in inner.list_thread_ids().await {
            let Some(session_source) = sources.get(&tid) else {
                continue;
            };
            let SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: agent_parent,
                agent_nickname,
                ..
            }) = session_source
            else {
                continue;
            };
            if *agent_parent != parent_thread_id {
                continue;
            }
            agents.push(format_subagent_context_line(
                &tid.to_string(),
                agent_nickname.as_deref(),
            ));
        }
        agents.sort();
        agents.join("\n")
    }

    /// Wait for an agent to reach a terminal status.
    pub async fn wait(&self, agent_id: ThreadId) -> Result<AgentStatus, CodexError> {
        let mut rx = self.subscribe_status(agent_id).await?;
        loop {
            let status = rx.borrow_and_update().clone();
            if is_final(&status) {
                return Ok(status);
            }
            if rx.changed().await.is_err() {
                return Ok(self.get_status(agent_id).await);
            }
        }
    }

    /// Return the number of currently tracked agents.
    pub async fn active_count(&self) -> usize {
        let Ok(inner) = self.upgrade() else {
            return 0;
        };
        inner.thread_count().await
    }

    // ── Private helpers ──────────────────────────────────────────

    fn upgrade(&self) -> Result<Arc<ThreadManagerInner>, CodexError> {
        self.manager.upgrade().ok_or_else(|| {
            CodexError::new(
                ErrorCode::SessionError,
                "thread manager dropped".to_string(),
            )
        })
    }

    /// Starts a detached watcher for sub-agents spawned from another thread.
    ///
    /// This is only enabled for `SubAgentSource::ThreadSpawn`, where a parent thread exists and
    /// can receive completion notifications.
    fn maybe_start_completion_watcher(
        &self,
        child_thread_id: ThreadId,
        session_source: Option<SessionSource>,
    ) {
        let Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        })) = session_source
        else {
            return;
        };
        let control = self.clone();
        tokio::spawn(async move {
            let status = match control.subscribe_status(child_thread_id).await {
                Ok(mut status_rx) => {
                    let mut status = status_rx.borrow().clone();
                    while !is_final(&status) {
                        if status_rx.changed().await.is_err() {
                            status = control.get_status(child_thread_id).await;
                            break;
                        }
                        status = status_rx.borrow().clone();
                    }
                    status
                }
                Err(_) => control.get_status(child_thread_id).await,
            };
            if !is_final(&status) {
                return;
            }

            let Ok(inner) = control.upgrade() else {
                return;
            };
            let Ok(parent_thread) = inner.get_thread(parent_thread_id).await else {
                return;
            };
            parent_thread
                .inject_user_message_without_turn(format_subagent_notification_message(
                    &child_thread_id.to_string(),
                    &status,
                ))
                .await;

            // Clean up the agent source tracking entry.
            control.agent_sources.write().await.remove(&child_thread_id);
        });
    }

    async fn inherited_shell_snapshot_for_source(
        &self,
        _inner: &Arc<ThreadManagerInner>,
        _session_source: Option<&SessionSource>,
    ) -> Option<Arc<ShellSnapshot>> {
        // Shell snapshot inheritance requires the parent thread to expose its
        // shell state. This is a placeholder matching codex-main's signature.
        None
    }
}

// ── Batch Job System ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BatchJobConfig {
    pub csv_path: PathBuf,
    pub concurrency: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub row_index: usize,
    pub success: bool,
    pub output: String,
}

pub async fn run_batch_jobs<F, Fut>(
    config: BatchJobConfig,
    job_fn: F,
) -> Result<Vec<BatchResult>, CodexError>
where
    F: Fn(usize, Vec<String>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send,
{
    if config.concurrency == 0 {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "Batch concurrency must be at least 1".to_string(),
        ));
    }

    let content = tokio::fs::read_to_string(&config.csv_path)
        .await
        .map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("Failed to read CSV file {}: {e}", config.csv_path.display()),
            )
        })?;

    let rows: Vec<Vec<String>> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split(',')
                .map(|cell| cell.trim().to_string())
                .collect()
        })
        .collect();

    let total = rows.len();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.concurrency));
    let job_fn = Arc::new(job_fn);
    let mut handles = Vec::with_capacity(total);

    for (row_index, row) in rows.into_iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let f = Arc::clone(&job_fn);
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed unexpectedly");
            match f(row_index, row).await {
                Ok(output) => BatchResult {
                    row_index,
                    success: true,
                    output,
                },
                Err(output) => BatchResult {
                    row_index,
                    success: false,
                    output,
                },
            }
        }));
    }

    let mut results = Vec::with_capacity(total);
    for handle in handles {
        results.push(handle.await.map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("Batch job task panicked: {e}"),
            )
        })?);
    }
    results.sort_by_key(|r| r.row_index);
    Ok(results)
}
