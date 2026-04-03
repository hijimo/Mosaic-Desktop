use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};

use crate::config::ConfigLayerStack;
use crate::core::agent::control::AgentControl;
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{Event, SessionConfiguredEvent};
use crate::protocol::submission::{Op, Submission};
use crate::protocol::thread_id::ThreadId;

use super::codex::{Codex, CodexHandle};

// ── NewThread ────────────────────────────────────────────────────

pub struct NewThread {
    pub thread_id: ThreadId,
    pub thread: Arc<CodexThread>,
    pub session_configured: SessionConfiguredEvent,
}

// ── CodexThread ──────────────────────────────────────────────────

pub struct CodexThread {
    handle: CodexHandle,
    thread_id: ThreadId,
    rollout_path: Option<PathBuf>,
}

impl CodexThread {
    pub fn new(handle: CodexHandle, thread_id: ThreadId) -> Self {
        Self {
            handle,
            thread_id,
            rollout_path: None,
        }
    }

    pub fn new_with_rollout_path(
        handle: CodexHandle,
        thread_id: ThreadId,
        rollout_path: Option<PathBuf>,
    ) -> Self {
        Self {
            handle,
            thread_id,
            rollout_path,
        }
    }

    pub fn id(&self) -> ThreadId {
        self.thread_id
    }

    pub async fn submit(&self, op: Op) -> Result<String, CodexError> {
        let id = uuid::Uuid::new_v4().to_string();
        self.handle
            .tx_sub
            .send(Submission { id: id.clone(), op })
            .await
            .map_err(|e| {
                CodexError::new(ErrorCode::InternalError, format!("submit failed: {e}"))
            })?;
        Ok(id)
    }

    /// Submit with a caller-provided submission ID.
    pub async fn submit_with_id(&self, sub: Submission) -> Result<(), CodexError> {
        self.handle
            .tx_sub
            .send(sub)
            .await
            .map_err(|e| {
                CodexError::new(ErrorCode::InternalError, format!("submit_with_id failed: {e}"))
            })
    }

    pub async fn next_event(&self) -> Result<Event, CodexError> {
        self.handle
            .rx_event
            .recv()
            .await
            .map_err(|e| CodexError::new(ErrorCode::InternalError, format!("recv failed: {e}")))
    }

    pub fn drain_events(&self, max: usize) -> Vec<Event> {
        let mut events = Vec::new();
        while events.len() < max {
            match self.handle.rx_event.try_recv() {
                Ok(ev) => events.push(ev),
                Err(_) => break,
            }
        }
        events
    }

    /// Inject user input into the thread, optionally targeting a specific turn.
    ///
    /// This matches codex-main's `steer_input(Vec<UserInput>, Option<&str>)`.
    /// In Mosaic, this is implemented via `Op::UserInput` since the Codex engine
    /// runs in a background task.
    pub async fn steer_input(
        &self,
        input: Vec<crate::protocol::types::UserInput>,
        _expected_turn_id: Option<&str>,
    ) -> Result<String, CodexError> {
        self.submit(Op::UserInput {
            items: input,
            final_output_json_schema: None,
        })
        .await
    }

    // ── Shared state accessors (matching codex-main's CodexThread) ──

    /// Get the current agent status without blocking.
    pub fn agent_status(&self) -> crate::protocol::types::AgentStatus {
        self.handle.agent_status.borrow().clone()
    }

    /// Subscribe to agent status changes.
    pub fn subscribe_status(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::protocol::types::AgentStatus> {
        self.handle.agent_status.clone()
    }

    /// Get the working directory of this thread.
    pub fn cwd(&self) -> &std::path::Path {
        &self.handle.cwd
    }

    /// Get the config of this thread.
    pub fn config(&self) -> &crate::config::ConfigLayerStack {
        &self.handle.config
    }

    /// Get the conversation history.
    pub async fn history(
        &self,
    ) -> Vec<crate::protocol::types::ResponseInputItem> {
        let session_guard = self.handle.session.lock().await;
        match session_guard.as_ref() {
            Some(session) => session.history().await,
            None => Vec::new(),
        }
    }

    /// Inject a user message into the session history without creating a turn.
    pub async fn inject_user_message_without_turn(&self, message: String) {
        let session_guard = self.handle.session.lock().await;
        if let Some(session) = session_guard.as_ref() {
            let item = crate::protocol::types::ResponseInputItem::Message {
                role: "user".to_string(),
                content: vec![crate::protocol::types::ContentItem::InputText {
                    text: message,
                }],
            };
            session.add_to_history(vec![item]).await;
        }
    }

    /// Get the current model name.
    pub async fn model(&self) -> Option<String> {
        let session_guard = self.handle.session.lock().await;
        session_guard.as_ref().map(|s| s.model())
    }

    /// Get the session ID.
    pub async fn session_id(&self) -> Option<String> {
        let session_guard = self.handle.session.lock().await;
        session_guard.as_ref().map(|s| s.id().to_string())
    }

    /// Get the rollout path for this thread.
    pub fn rollout_path(&self) -> Option<PathBuf> {
        self.rollout_path.clone()
    }

    /// Get the state database handle for this thread.
    pub fn state_db(&self) -> Option<crate::core::state_db::StateDb> {
        self.handle.state_db.clone()
    }

    /// Check if a feature is enabled.
    pub fn enabled(&self, feature: crate::core::features::Feature) -> bool {
        self.handle.features.enabled(feature)
    }

    /// Get total token usage for this thread's session.
    pub async fn total_token_usage(&self) -> Option<crate::protocol::types::TokenUsage> {
        let session_guard = self.handle.session.lock().await;
        let session = session_guard.as_ref()?;
        let info = session.token_info().await?;
        Some(info.total_token_usage)
    }
}

// ── ThreadManagerInner ───────────────────────────────────────────

const THREAD_CREATED_CHANNEL_CAPACITY: usize = 64;

/// Shared inner state of the thread manager, accessible via `Arc`.
///
/// `AgentControl` holds a `Weak<ThreadManagerInner>` to register threads
/// it spawns without creating reference cycles.
pub struct ThreadManagerInner {
    threads: RwLock<HashMap<ThreadId, Arc<CodexThread>>>,
    thread_created_tx: broadcast::Sender<ThreadId>,
    max_threads: usize,
}

impl ThreadManagerInner {
    fn new(max_threads: usize) -> Self {
        let (tx, _) = broadcast::channel(THREAD_CREATED_CHANNEL_CAPACITY);
        Self {
            threads: RwLock::new(HashMap::new()),
            thread_created_tx: tx,
            max_threads,
        }
    }

    /// Register a thread (used by both ThreadManager and AgentControl).
    pub async fn register_thread(&self, thread_id: ThreadId, thread: Arc<CodexThread>) {
        self.threads.write().await.insert(thread_id, thread);
        let _ = self.thread_created_tx.send(thread_id);
    }

    pub async fn get_thread(&self, thread_id: ThreadId) -> Result<Arc<CodexThread>, CodexError> {
        self.threads
            .read()
            .await
            .get(&thread_id)
            .cloned()
            .ok_or_else(|| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("thread not found: {thread_id}"),
                )
            })
    }

    pub async fn list_thread_ids(&self) -> Vec<ThreadId> {
        self.threads.read().await.keys().copied().collect()
    }

    pub async fn thread_count(&self) -> usize {
        self.threads.read().await.len()
    }

    pub async fn remove_thread(&self, thread_id: &ThreadId) -> Option<Arc<CodexThread>> {
        self.threads.write().await.remove(thread_id)
    }

    fn is_at_limit(&self, current_count: usize) -> bool {
        current_count >= self.max_threads
    }

    pub fn subscribe_thread_created(&self) -> broadcast::Receiver<ThreadId> {
        self.thread_created_tx.subscribe()
    }

    pub async fn send_op(&self, thread_id: ThreadId, op: Op) -> Result<String, CodexError> {
        let thread = self.get_thread(thread_id).await?;
        thread.submit(op).await
    }

    pub fn notify_thread_created(&self, thread_id: ThreadId) {
        let _ = self.thread_created_tx.send(thread_id);
    }
}

// ── ThreadManager ────────────────────────────────────────────────

/// Manages the lifecycle of multiple Codex threads.
///
/// Owns the shared `ThreadManagerInner` and provides an `AgentControl`
/// that is bound to the same inner state.
pub struct ThreadManager {
    inner: Arc<ThreadManagerInner>,
    agent_control: AgentControl,
}

impl ThreadManager {
    pub fn new(max_threads: usize, default_cwd: PathBuf) -> Self {
        let inner = Arc::new(ThreadManagerInner::new(max_threads));
        let mut agent_control = AgentControl::new(Arc::downgrade(&inner));
        agent_control.set_default_cwd(default_cwd);
        Self {
            inner,
            agent_control,
        }
    }

    /// Get a reference to the agent control plane.
    pub fn agent_control(&self) -> &AgentControl {
        &self.agent_control
    }

    /// Get a cloneable Arc to the inner state (for passing to AgentControl externally).
    pub fn inner(&self) -> &Arc<ThreadManagerInner> {
        &self.inner
    }

    pub async fn start_thread(
        &self,
        config: ConfigLayerStack,
        cwd: PathBuf,
    ) -> Result<NewThread, CodexError> {
        {
            let count = self.inner.thread_count().await;
            if self.inner.is_at_limit(count) {
                return Err(CodexError::new(
                    ErrorCode::InternalError,
                    format!("thread limit reached ({}/{})", count, self.inner.max_threads),
                ));
            }
        }

        let thread_id = ThreadId::new();
        let handle = Codex::spawn(config, cwd).await?;
        let session_configured = wait_for_session_configured(&handle).await?;

        let thread = Arc::new(CodexThread::new(handle, thread_id));
        self.inner
            .register_thread(thread_id, Arc::clone(&thread))
            .await;

        Ok(NewThread {
            thread_id,
            thread,
            session_configured,
        })
    }

    pub async fn get_thread(&self, thread_id: ThreadId) -> Result<Arc<CodexThread>, CodexError> {
        self.inner.get_thread(thread_id).await
    }

    pub async fn list_thread_ids(&self) -> Vec<ThreadId> {
        self.inner.list_thread_ids().await
    }

    pub async fn thread_count(&self) -> usize {
        self.inner.thread_count().await
    }

    pub async fn remove_thread(&self, thread_id: &ThreadId) -> Option<Arc<CodexThread>> {
        self.inner.remove_thread(thread_id).await
    }

    pub async fn shutdown_thread(&self, thread_id: &ThreadId) -> Result<(), CodexError> {
        if let Some(thread) = self.remove_thread(thread_id).await {
            thread.submit(Op::Shutdown).await?;
        }
        Ok(())
    }

    pub async fn shutdown_all(&self) -> Result<(), CodexError> {
        let threads: Vec<Arc<CodexThread>> = {
            let map = self.inner.threads.read().await;
            map.values().cloned().collect()
        };
        for thread in &threads {
            let _ = thread.submit(Op::Shutdown).await;
        }
        self.inner.threads.write().await.clear();
        Ok(())
    }

    pub fn subscribe_thread_created(&self) -> broadcast::Receiver<ThreadId> {
        self.inner.subscribe_thread_created()
    }

    pub async fn send_op(&self, thread_id: ThreadId, op: Op) -> Result<String, CodexError> {
        let thread = self.inner.get_thread(thread_id).await?;
        thread.submit(op).await
    }

    /// Resume a thread from a recorded rollout file.
    pub async fn resume_thread_from_rollout(
        &self,
        config: ConfigLayerStack,
        cwd: PathBuf,
        rollout_path: PathBuf,
    ) -> Result<NewThread, CodexError> {
        use super::initial_history::InitialHistory;

        let resumed = crate::core::rollout::RolloutRecorder::get_rollout_history(&rollout_path)
            .await
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to load rollout: {e}"),
                )
            })?;
        self.resume_thread_with_history(config, cwd, InitialHistory::Resumed(resumed))
            .await
    }

    /// Resume a thread with pre-loaded history.
    pub async fn resume_thread_with_history(
        &self,
        config: ConfigLayerStack,
        cwd: PathBuf,
        initial_history: super::initial_history::InitialHistory,
    ) -> Result<NewThread, CodexError> {
        {
            let count = self.inner.thread_count().await;
            if self.inner.is_at_limit(count) {
                return Err(CodexError::new(
                    ErrorCode::InternalError,
                    format!("thread limit reached ({}/{})", count, self.inner.max_threads),
                ));
            }
        }

        let thread_id = ThreadId::new();
        let handle = Codex::spawn_with_history(config, cwd, initial_history).await?;
        let session_configured = wait_for_session_configured(&handle).await?;

        let thread = Arc::new(CodexThread::new(handle, thread_id));
        self.inner
            .register_thread(thread_id, Arc::clone(&thread))
            .await;

        Ok(NewThread {
            thread_id,
            thread,
            session_configured,
        })
    }

    pub async fn fork_thread(
        &self,
        rollout_path: &std::path::Path,
        nth_user_message: usize,
        config: ConfigLayerStack,
        cwd: PathBuf,
    ) -> Result<NewThread, CodexError> {
        use super::initial_history::InitialHistory;
        use super::rollout::truncation::truncate_before_nth_user_message;

        {
            let count = self.inner.thread_count().await;
            if self.inner.is_at_limit(count) {
                return Err(CodexError::new(
                    ErrorCode::InternalError,
                    format!("thread limit reached ({}/{})", count, self.inner.max_threads),
                ));
            }
        }

        let text = tokio::fs::read_to_string(rollout_path).await.map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to read rollout: {e}"),
            )
        })?;
        let items = crate::commands::parse_rollout_items(&text);
        let truncated = truncate_before_nth_user_message(&items, nth_user_message);

        let thread_id = ThreadId::new();
        let handle =
            Codex::spawn_with_history(config, cwd, InitialHistory::Forked(truncated)).await?;
        let session_configured = wait_for_session_configured(&handle).await?;

        let thread = Arc::new(CodexThread::new(handle, thread_id));
        self.inner
            .register_thread(thread_id, Arc::clone(&thread))
            .await;

        Ok(NewThread {
            thread_id,
            thread,
            session_configured,
        })
    }
}

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new(6, PathBuf::from("."))
    }
}

// ── helpers ──────────────────────────────────────────────────────

pub async fn wait_for_session_configured(
    handle: &CodexHandle,
) -> Result<SessionConfiguredEvent, CodexError> {
    let event = handle.rx_event.recv().await.map_err(|e| {
        CodexError::new(
            ErrorCode::InternalError,
            format!("failed to receive initial event: {e}"),
        )
    })?;
    match event.msg {
        crate::protocol::event::EventMsg::SessionConfigured(sc) => Ok(sc),
        other => Err(CodexError::new(
            ErrorCode::InternalError,
            format!(
                "expected SessionConfigured as first event, got: {:?}",
                std::mem::discriminant(&other)
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_id_uniqueness() {
        let a = ThreadId::new();
        let b = ThreadId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn default_max_threads() {
        let mgr = ThreadManager::default();
        assert_eq!(mgr.inner.max_threads, 6);
    }

    #[tokio::test]
    async fn empty_manager_has_no_threads() {
        let mgr = ThreadManager::new(4, PathBuf::from("/tmp"));
        assert_eq!(mgr.thread_count().await, 0);
        assert!(mgr.list_thread_ids().await.is_empty());
    }

    #[tokio::test]
    async fn get_nonexistent_thread_returns_error() {
        let mgr = ThreadManager::new(4, PathBuf::from("/tmp"));
        let id = ThreadId::new();
        let result = mgr.get_thread(id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn subscribe_returns_receiver() {
        let mgr = ThreadManager::new(4, PathBuf::from("/tmp"));
        let _rx = mgr.subscribe_thread_created();
    }

    #[tokio::test]
    async fn agent_control_is_bound() {
        let mgr = ThreadManager::new(4, PathBuf::from("/tmp"));
        // Agent control should be able to upgrade the weak ref.
        let ctrl = mgr.agent_control();
        assert_eq!(ctrl.active_count().await, 0);
    }
}
