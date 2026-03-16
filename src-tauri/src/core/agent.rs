/// Multi-agent management — spawning, coordinating, and lifecycle management of sub-agents.
///
/// Uses weak references (`Weak<AgentInstance>`) to avoid circular references between
/// the control plane and individual agent instances.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use async_channel::{Receiver, Sender};
use tokio::sync::Mutex;

use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::Event;
use crate::protocol::types::{AgentStatus, SandboxPolicy, UserInput};

// ── AgentInstance ────────────────────────────────────────────────

/// A single running agent with its own submission/event channels and state.
pub struct AgentInstance {
    pub thread_id: String,
    pub nickname: String,
    pub depth: usize,
    pub forked: bool,
    pub cwd: PathBuf,
    pub sandbox_policy: SandboxPolicy,
    status: Mutex<AgentStatus>,
    tx_input: Sender<UserInput>,
    rx_input: Receiver<UserInput>,
    /// Signals the agent to resume after being paused.
    resume_tx: Sender<()>,
    resume_rx: Receiver<()>,
    /// Collects the final output when the agent completes.
    result: Mutex<Option<serde_json::Value>>,
}

impl std::fmt::Debug for AgentInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentInstance")
            .field("thread_id", &self.thread_id)
            .field("nickname", &self.nickname)
            .field("depth", &self.depth)
            .field("forked", &self.forked)
            .field("cwd", &self.cwd)
            .finish_non_exhaustive()
    }
}

impl AgentInstance {
    fn new(
        thread_id: String,
        nickname: String,
        depth: usize,
        forked: bool,
        cwd: PathBuf,
        sandbox_policy: SandboxPolicy,
    ) -> Self {
        let (tx_input, rx_input) = async_channel::bounded(16);
        let (resume_tx, resume_rx) = async_channel::bounded(1);
        Self {
            thread_id,
            nickname,
            depth,
            forked,
            cwd,
            sandbox_policy,
            status: Mutex::new(AgentStatus::PendingInit),
            tx_input,
            rx_input,
            resume_tx,
            resume_rx,
            result: Mutex::new(None),
        }
    }

    pub async fn status(&self) -> AgentStatus {
        self.status.lock().await.clone()
    }

    pub async fn set_status(&self, status: AgentStatus) {
        *self.status.lock().await = status;
    }

    pub async fn set_result(&self, value: serde_json::Value) {
        *self.result.lock().await = Some(value);
    }

    pub async fn take_result(&self) -> Option<serde_json::Value> {
        self.result.lock().await.take()
    }
}

// ── SpawnAgentOptions ────────────────────────────────────────────

/// Options for spawning a new agent.
#[derive(Debug, Clone)]
pub struct SpawnAgentOptions {
    pub model: Option<String>,
    pub sandbox_policy: Option<SandboxPolicy>,
    pub cwd: Option<PathBuf>,
    /// When true, the agent runs in an independent execution branch.
    pub fork: bool,
    /// Override the max recursion depth for this agent tree.
    pub max_depth: Option<usize>,
}

impl Default for SpawnAgentOptions {
    fn default() -> Self {
        Self {
            model: None,
            sandbox_policy: None,
            cwd: None,
            fork: false,
            max_depth: None,
        }
    }
}

// ── SpawnSlotGuard ───────────────────────────────────────────────

/// RAII guard that releases a spawn slot when dropped.
pub struct SpawnSlotGuard {
    active_count: Arc<std::sync::Mutex<usize>>,
}

impl SpawnSlotGuard {
    fn new(active_count: Arc<std::sync::Mutex<usize>>) -> Self {
        Self { active_count }
    }
}

impl Drop for SpawnSlotGuard {
    fn drop(&mut self) {
        if let Ok(mut count) = self.active_count.lock() {
            *count = count.saturating_sub(1);
        }
    }
}

/// Guards acquired during agent spawn: a spawn slot and a nickname.
pub struct Guards {
    pub spawn_slot: SpawnSlotGuard,
    pub nickname: String,
}

impl std::fmt::Debug for Guards {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guards")
            .field("nickname", &self.nickname)
            .finish_non_exhaustive()
    }
}

// ── ThreadManagerState ───────────────────────────────────────────

/// Internal state tracking all active agents via weak references.
pub struct ThreadManagerState {
    agents: HashMap<String, Weak<AgentInstance>>,
    next_nickname: u32,
}

impl ThreadManagerState {
    fn new() -> Self {
        Self {
            agents: HashMap::new(),
            next_nickname: 0,
        }
    }

    /// Allocate the next sequential nickname (e.g. "agent-0", "agent-1", …).
    fn allocate_nickname(&mut self) -> String {
        let nickname = format!("agent-{}", self.next_nickname);
        self.next_nickname += 1;
        nickname
    }

    /// Prune entries whose `Weak` has been dropped.
    fn prune_dead(&mut self) {
        self.agents.retain(|_, weak| weak.strong_count() > 0);
    }

    fn active_count(&self) -> usize {
        self.agents
            .values()
            .filter(|w| w.strong_count() > 0)
            .count()
    }
}

// ── AgentControl ─────────────────────────────────────────────────

/// Top-level controller for spawning and managing multiple agents.
///
/// Enforces a maximum recursion depth and tracks agents via weak references
/// so that dropped `Arc<AgentInstance>` values are automatically cleaned up.
pub struct AgentControl {
    state: Mutex<ThreadManagerState>,
    active_count: Arc<std::sync::Mutex<usize>>,
    max_recursion_depth: usize,
    default_cwd: PathBuf,
    default_sandbox_policy: SandboxPolicy,
    #[allow(dead_code)] // Used when emitting collab events in future integration
    tx_event: Sender<Event>,
}

impl AgentControl {
    pub fn new(
        max_recursion_depth: usize,
        default_cwd: PathBuf,
        default_sandbox_policy: SandboxPolicy,
        tx_event: Sender<Event>,
    ) -> Self {
        Self {
            state: Mutex::new(ThreadManagerState::new()),
            active_count: Arc::new(std::sync::Mutex::new(0)),
            max_recursion_depth,
            default_cwd,
            default_sandbox_policy,
            tx_event,
        }
    }

    /// Spawn a new agent, returning an `Arc<AgentInstance>` and associated [`Guards`].
    ///
    /// Fails if the requested depth exceeds `max_recursion_depth`.
    pub async fn spawn_agent(
        &self,
        options: SpawnAgentOptions,
        current_depth: usize,
    ) -> Result<(Arc<AgentInstance>, Guards), CodexError> {
        let effective_max = options.max_depth.unwrap_or(self.max_recursion_depth);
        if current_depth >= effective_max {
            return Err(CodexError::new(
                ErrorCode::SessionError,
                format!("Agent recursion depth {current_depth} exceeds maximum {effective_max}"),
            ));
        }

        let mut mgr = self.state.lock().await;
        mgr.prune_dead();

        let nickname = mgr.allocate_nickname();
        let thread_id = uuid::Uuid::new_v4().to_string();

        let cwd = options.cwd.unwrap_or_else(|| self.default_cwd.clone());
        let sandbox = options
            .sandbox_policy
            .unwrap_or_else(|| self.default_sandbox_policy.clone());

        let instance = Arc::new(AgentInstance::new(
            thread_id.clone(),
            nickname.clone(),
            current_depth + 1,
            options.fork,
            cwd,
            sandbox,
        ));

        mgr.agents.insert(thread_id, Arc::downgrade(&instance));

        // Increment active count and create the slot guard.
        {
            let mut count = self.active_count.lock().unwrap();
            *count += 1;
        }
        let slot_guard = SpawnSlotGuard::new(Arc::clone(&self.active_count));

        let guards = Guards {
            spawn_slot: slot_guard,
            nickname,
        };

        Ok((instance, guards))
    }

    /// Send user input to a running agent.
    pub async fn send_input(&self, agent_id: &str, input: UserInput) -> Result<(), CodexError> {
        let instance = self.get_instance(agent_id).await?;
        instance.tx_input.send(input).await.map_err(|_| {
            CodexError::new(
                ErrorCode::SessionError,
                format!("Agent {agent_id} input channel closed"),
            )
        })
    }

    /// Signal a paused agent to resume execution.
    pub async fn resume_agent(&self, agent_id: &str) -> Result<(), CodexError> {
        let instance = self.get_instance(agent_id).await?;
        instance.resume_tx.send(()).await.map_err(|_| {
            CodexError::new(
                ErrorCode::SessionError,
                format!("Agent {agent_id} resume channel closed"),
            )
        })
    }

    /// Wait for an agent to complete and return its result.
    pub async fn wait(&self, agent_id: &str) -> Result<serde_json::Value, CodexError> {
        let instance = self.get_instance(agent_id).await?;

        // Poll until the agent reaches a terminal status.
        loop {
            let status = instance.status().await;
            match status {
                AgentStatus::Completed(_) => {
                    return Ok(instance
                        .take_result()
                        .await
                        .unwrap_or(serde_json::Value::Null));
                }
                AgentStatus::Errored(msg) => {
                    return Err(CodexError::new(ErrorCode::SessionError, msg));
                }
                AgentStatus::Shutdown | AgentStatus::NotFound => {
                    return Err(CodexError::new(
                        ErrorCode::SessionError,
                        format!("Agent {agent_id} is no longer available"),
                    ));
                }
                AgentStatus::PendingInit | AgentStatus::Running => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    }

    /// Gracefully close an agent, releasing its resources.
    pub async fn close_agent(&self, agent_id: &str) -> Result<(), CodexError> {
        let instance = self.get_instance(agent_id).await?;

        // Mark as shutdown so any waiting loops exit.
        instance.set_status(AgentStatus::Shutdown).await;

        // Close channels to unblock any pending sends/receives.
        instance.tx_input.close();
        instance.resume_tx.close();

        // Remove from the registry.
        let mut mgr = self.state.lock().await;
        mgr.agents.remove(agent_id);

        Ok(())
    }

    /// Return the number of currently alive agents.
    pub async fn active_count(&self) -> usize {
        let mgr = self.state.lock().await;
        mgr.active_count()
    }

    /// Retrieve a strong reference to an agent, or error if not found / dropped.
    async fn get_instance(&self, agent_id: &str) -> Result<Arc<AgentInstance>, CodexError> {
        let mgr = self.state.lock().await;
        mgr.agents
            .get(agent_id)
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                CodexError::new(
                    ErrorCode::SessionError,
                    format!("Agent {agent_id} not found or already closed"),
                )
            })
    }

    /// Receive the next input for an agent (called by the agent itself).
    pub async fn recv_input(instance: &AgentInstance) -> Result<UserInput, CodexError> {
        instance.rx_input.recv().await.map_err(|_| {
            CodexError::new(
                ErrorCode::SessionError,
                "Agent input channel closed".to_string(),
            )
        })
    }

    /// Wait for a resume signal (called by the agent itself).
    pub async fn recv_resume(instance: &AgentInstance) -> Result<(), CodexError> {
        instance.resume_rx.recv().await.map_err(|_| {
            CodexError::new(
                ErrorCode::SessionError,
                "Agent resume channel closed".to_string(),
            )
        })
    }
}

// ── Batch Job System ─────────────────────────────────────────────

/// Configuration for running batch jobs from a CSV file.
#[derive(Debug, Clone)]
pub struct BatchJobConfig {
    pub csv_path: PathBuf,
    pub concurrency: usize,
}

/// Result of a single batch job row execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub row_index: usize,
    pub success: bool,
    pub output: String,
}

/// Execute batch jobs from a CSV file with bounded concurrency.
///
/// Each row in the CSV produces exactly one [`BatchResult`]. At most
/// `config.concurrency` jobs run simultaneously. The caller provides a
/// `job_fn` closure that receives `(row_index, columns)` and returns
/// `Ok(output)` on success or `Err(output)` on failure.
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

        let handle = tokio::spawn(async move {
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
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(total);
    for handle in handles {
        let result = handle.await.map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("Batch job task panicked: {e}"),
            )
        })?;
        results.push(result);
    }

    // Sort by row_index to guarantee deterministic output order.
    results.sort_by_key(|r| r.row_index);

    Ok(results)
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_control(max_depth: usize) -> AgentControl {
        let (tx, _rx) = async_channel::unbounded();
        AgentControl::new(
            max_depth,
            PathBuf::from("/tmp"),
            SandboxPolicy::new_read_only_policy(),
            tx,
        )
    }

    #[tokio::test]
    async fn spawn_agent_returns_instance_and_guards() {
        let ctrl = make_control(3);
        let (instance, guards) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();

        assert_eq!(instance.depth, 1);
        assert_eq!(guards.nickname, "agent-0");
        assert!(!instance.thread_id.is_empty());
        assert_eq!(ctrl.active_count().await, 1);
    }

    #[tokio::test]
    async fn spawn_increments_nicknames() {
        let ctrl = make_control(5);
        let (_a, g1) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();
        let (_b, g2) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();
        assert_eq!(g1.nickname, "agent-0");
        assert_eq!(g2.nickname, "agent-1");
    }

    #[tokio::test]
    async fn spawn_at_max_depth_is_rejected() {
        let ctrl = make_control(2);
        let result = ctrl.spawn_agent(SpawnAgentOptions::default(), 2).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::SessionError);
        assert!(err.message.contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn spawn_with_custom_max_depth_override() {
        let ctrl = make_control(10);
        let opts = SpawnAgentOptions {
            max_depth: Some(1),
            ..Default::default()
        };
        // depth 0 < max_depth 1 → ok
        let result = ctrl.spawn_agent(opts.clone(), 0).await;
        assert!(result.is_ok());

        // depth 1 >= max_depth 1 → rejected
        let result = ctrl.spawn_agent(opts, 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn close_agent_removes_from_registry() {
        let ctrl = make_control(5);
        let (instance, _guards) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();
        let id = instance.thread_id.clone();

        ctrl.close_agent(&id).await.unwrap();

        let result = ctrl
            .send_input(
                &id,
                UserInput::Text {
                    text: "hello".into(),
                    text_elements: vec![],
                },
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn close_agent_sets_shutdown_status() {
        let ctrl = make_control(5);
        let (instance, _guards) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();
        let id = instance.thread_id.clone();

        ctrl.close_agent(&id).await.unwrap();
        assert_eq!(instance.status().await, AgentStatus::Shutdown);
    }

    #[tokio::test]
    async fn send_input_and_receive() {
        let ctrl = make_control(5);
        let (instance, _guards) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();
        let id = instance.thread_id.clone();

        let input = UserInput::Text {
            text: "test input".into(),
            text_elements: vec![],
        };
        ctrl.send_input(&id, input.clone()).await.unwrap();

        let received = AgentControl::recv_input(&instance).await.unwrap();
        assert_eq!(received, input);
    }

    #[tokio::test]
    async fn resume_agent_signal() {
        let ctrl = make_control(5);
        let (instance, _guards) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();
        let id = instance.thread_id.clone();

        ctrl.resume_agent(&id).await.unwrap();
        AgentControl::recv_resume(&instance).await.unwrap();
    }

    #[tokio::test]
    async fn wait_returns_result_on_completion() {
        let ctrl = make_control(5);
        let (instance, _guards) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();
        let id = instance.thread_id.clone();

        // Simulate agent completing in background.
        let inst = Arc::clone(&instance);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            inst.set_result(serde_json::json!({"done": true})).await;
            inst.set_status(AgentStatus::Completed(None)).await;
        });

        let result = ctrl.wait(&id).await.unwrap();
        assert_eq!(result, serde_json::json!({"done": true}));
    }

    #[tokio::test]
    async fn wait_returns_error_on_agent_error() {
        let ctrl = make_control(5);
        let (instance, _guards) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();
        let id = instance.thread_id.clone();

        let inst = Arc::clone(&instance);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            inst.set_status(AgentStatus::Errored("something broke".into()))
                .await;
        });

        let err = ctrl.wait(&id).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::SessionError);
        assert!(err.message.contains("something broke"));
    }

    #[tokio::test]
    async fn weak_reference_cleanup_on_drop() {
        let ctrl = make_control(5);
        let (instance, _guards) = ctrl
            .spawn_agent(SpawnAgentOptions::default(), 0)
            .await
            .unwrap();
        let id = instance.thread_id.clone();

        assert_eq!(ctrl.active_count().await, 1);

        // Drop the strong reference — the weak ref in the registry becomes stale.
        drop(instance);
        drop(_guards);

        // After prune, the agent should be gone.
        let result = ctrl
            .send_input(
                &id,
                UserInput::Text {
                    text: "hello".into(),
                    text_elements: vec![],
                },
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fork_mode_sets_flag() {
        let ctrl = make_control(5);
        let opts = SpawnAgentOptions {
            fork: true,
            ..Default::default()
        };
        let (instance, _guards) = ctrl.spawn_agent(opts, 0).await.unwrap();
        assert!(instance.forked);
    }

    #[tokio::test]
    async fn custom_cwd_and_sandbox() {
        let ctrl = make_control(5);
        let opts = SpawnAgentOptions {
            cwd: Some(PathBuf::from("/custom/dir")),
            sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
            ..Default::default()
        };
        let (instance, _guards) = ctrl.spawn_agent(opts, 0).await.unwrap();
        assert_eq!(instance.cwd, PathBuf::from("/custom/dir"));
        assert!(instance.sandbox_policy.has_full_disk_write_access());
    }

    #[tokio::test]
    async fn close_nonexistent_agent_errors() {
        let ctrl = make_control(5);
        let result = ctrl.close_agent("nonexistent-id").await;
        assert!(result.is_err());
    }

    // ── Batch Job Tests ──────────────────────────────────────────

    #[tokio::test]
    async fn batch_jobs_returns_one_result_per_row() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("input.csv");
        tokio::fs::write(&csv_path, "a,b\nc,d\ne,f\n")
            .await
            .unwrap();

        let results = run_batch_jobs(
            BatchJobConfig {
                csv_path,
                concurrency: 2,
            },
            |row_index, cols| async move { Ok(format!("row {row_index}: {}", cols.join("+"))) },
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 3);
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.row_index, i);
            assert!(r.success);
        }
    }

    #[tokio::test]
    async fn batch_jobs_respects_concurrency_limit() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("input.csv");
        // 10 rows
        let csv_content = (0..10)
            .map(|i| format!("row{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        tokio::fs::write(&csv_path, &csv_content).await.unwrap();

        let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let active_c = Arc::clone(&active);
        let max_c = Arc::clone(&max_seen);

        let results = run_batch_jobs(
            BatchJobConfig {
                csv_path,
                concurrency: 3,
            },
            move |row_index, _cols| {
                let a = Arc::clone(&active_c);
                let m = Arc::clone(&max_c);
                async move {
                    let current = a.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    m.fetch_max(current, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    a.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(format!("done-{row_index}"))
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 10);
        assert!(max_seen.load(std::sync::atomic::Ordering::SeqCst) <= 3);
    }

    #[tokio::test]
    async fn batch_jobs_captures_failures() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("input.csv");
        tokio::fs::write(&csv_path, "ok\nfail\nok\n").await.unwrap();

        let results = run_batch_jobs(
            BatchJobConfig {
                csv_path,
                concurrency: 4,
            },
            |row_index, _cols| async move {
                if row_index == 1 {
                    Err("something went wrong".to_string())
                } else {
                    Ok("success".to_string())
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 3);
        assert!(results[0].success);
        assert!(!results[1].success);
        assert_eq!(results[1].output, "something went wrong");
        assert!(results[2].success);
    }

    #[tokio::test]
    async fn batch_jobs_zero_concurrency_errors() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("input.csv");
        tokio::fs::write(&csv_path, "a\n").await.unwrap();

        let result = run_batch_jobs(
            BatchJobConfig {
                csv_path,
                concurrency: 0,
            },
            |_, _| async { Ok("x".to_string()) },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn batch_jobs_missing_csv_errors() {
        let result = run_batch_jobs(
            BatchJobConfig {
                csv_path: PathBuf::from("/nonexistent/file.csv"),
                concurrency: 1,
            },
            |_, _| async { Ok("x".to_string()) },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn batch_jobs_empty_csv_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("empty.csv");
        tokio::fs::write(&csv_path, "").await.unwrap();

        let results = run_batch_jobs(
            BatchJobConfig {
                csv_path,
                concurrency: 2,
            },
            |_, _| async { Ok("x".to_string()) },
        )
        .await
        .unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn batch_results_sorted_by_row_index() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("input.csv");
        let csv_content = (0..20)
            .map(|i| format!("row{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        tokio::fs::write(&csv_path, &csv_content).await.unwrap();

        let results = run_batch_jobs(
            BatchJobConfig {
                csv_path,
                concurrency: 5,
            },
            |row_index, _| async move {
                // Variable sleep to encourage out-of-order completion.
                let delay = if row_index % 2 == 0 { 5 } else { 1 };
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                Ok(format!("{row_index}"))
            },
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 20);
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.row_index, i);
        }
    }
}
