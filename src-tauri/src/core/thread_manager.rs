use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock, broadcast};

use crate::config::ConfigLayerStack;
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{Event, SessionConfiguredEvent};
use crate::protocol::submission::{Op, Submission};
use crate::protocol::thread_id::ThreadId;

use super::codex::{Codex, CodexHandle};

// ── NewThread ────────────────────────────────────────────────────

/// Represents a newly created thread, including its first event
/// (`SessionConfigured`).
pub struct NewThread {
    pub thread_id: ThreadId,
    pub thread: Arc<CodexThread>,
    pub session_configured: SessionConfiguredEvent,
}

// ── CodexThread ──────────────────────────────────────────────────

/// A running Codex thread with its communication channels.
pub struct CodexThread {
    handle: CodexHandle,
    thread_id: ThreadId,
}

impl CodexThread {
    fn new(handle: CodexHandle, thread_id: ThreadId) -> Self {
        Self { handle, thread_id }
    }

    pub fn id(&self) -> ThreadId {
        self.thread_id
    }

    /// Submit an operation to this thread.
    pub async fn submit(&self, op: Op) -> Result<String, CodexError> {
        let id = uuid::Uuid::new_v4().to_string();
        self.handle
            .tx_sub
            .send(Submission {
                id: id.clone(),
                op,
            })
            .await
            .map_err(|e| {
                CodexError::new(ErrorCode::InternalError, format!("submit failed: {e}"))
            })?;
        Ok(id)
    }

    /// Receive the next event from this thread.
    pub async fn next_event(&self) -> Result<Event, CodexError> {
        self.handle.rx_event.recv().await.map_err(|e| {
            CodexError::new(ErrorCode::InternalError, format!("recv failed: {e}"))
        })
    }

    /// Drain all currently available events without blocking.
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
}

// ── ThreadManager ────────────────────────────────────────────────

const THREAD_CREATED_CHANNEL_CAPACITY: usize = 64;

/// Manages the lifecycle of multiple Codex threads.
///
/// Each thread runs its own `Codex` engine on a background task.
/// The manager tracks them by [`ThreadId`] and provides methods to
/// create, list, access, and remove threads.
pub struct ThreadManager {
    threads: Arc<RwLock<HashMap<ThreadId, Arc<CodexThread>>>>,
    thread_created_tx: broadcast::Sender<ThreadId>,
    max_threads: usize,
}

impl ThreadManager {
    pub fn new(max_threads: usize) -> Self {
        let (tx, _) = broadcast::channel(THREAD_CREATED_CHANNEL_CAPACITY);
        Self {
            threads: Arc::new(RwLock::new(HashMap::new())),
            thread_created_tx: tx,
            max_threads,
        }
    }

    /// Start a new thread with the given config and working directory.
    ///
    /// Spawns a `Codex` engine on a background task and waits for the
    /// initial `SessionConfigured` event before returning.
    pub async fn start_thread(
        &self,
        config: ConfigLayerStack,
        cwd: PathBuf,
    ) -> Result<NewThread, CodexError> {
        // Enforce limit
        {
            let threads = self.threads.read().await;
            if threads.len() >= self.max_threads {
                return Err(CodexError::new(
                    ErrorCode::InternalError,
                    format!(
                        "thread limit reached ({}/{})",
                        threads.len(),
                        self.max_threads
                    ),
                ));
            }
        }

        let thread_id = ThreadId::new();
        let handle = Codex::spawn(config, cwd).await?;

        // Wait for the first SessionConfigured event.
        let session_configured = wait_for_session_configured(&handle).await?;

        let thread = Arc::new(CodexThread::new(handle, thread_id));
        self.threads.write().await.insert(thread_id, Arc::clone(&thread));
        let _ = self.thread_created_tx.send(thread_id);

        Ok(NewThread {
            thread_id,
            thread,
            session_configured,
        })
    }

    /// Get a thread by ID.
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

    /// List all active thread IDs.
    pub async fn list_thread_ids(&self) -> Vec<ThreadId> {
        self.threads.read().await.keys().copied().collect()
    }

    /// Number of active threads.
    pub async fn thread_count(&self) -> usize {
        self.threads.read().await.len()
    }

    /// Remove a thread from the manager. Returns the thread if found.
    ///
    /// Note: this does NOT send a shutdown op. Call `shutdown_thread`
    /// if you want a graceful shutdown.
    pub async fn remove_thread(&self, thread_id: &ThreadId) -> Option<Arc<CodexThread>> {
        self.threads.write().await.remove(thread_id)
    }

    /// Gracefully shut down a single thread and remove it.
    pub async fn shutdown_thread(&self, thread_id: &ThreadId) -> Result<(), CodexError> {
        if let Some(thread) = self.remove_thread(thread_id).await {
            thread.submit(Op::Shutdown).await?;
        }
        Ok(())
    }

    /// Shut down all threads and clear the map.
    pub async fn shutdown_all(&self) -> Result<(), CodexError> {
        let threads: Vec<Arc<CodexThread>> = {
            let map = self.threads.read().await;
            map.values().cloned().collect()
        };
        for thread in &threads {
            let _ = thread.submit(Op::Shutdown).await;
        }
        self.threads.write().await.clear();
        Ok(())
    }

    /// Subscribe to notifications when new threads are created.
    pub fn subscribe_thread_created(&self) -> broadcast::Receiver<ThreadId> {
        self.thread_created_tx.subscribe()
    }

    /// Send an operation to a specific thread by ID.
    pub async fn send_op(&self, thread_id: ThreadId, op: Op) -> Result<String, CodexError> {
        let thread = self.get_thread(thread_id).await?;
        thread.submit(op).await
    }
}

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new(6)
    }
}

// ── helpers ──────────────────────────────────────────────────────

/// Wait for the first `SessionConfigured` event from a freshly spawned handle.
async fn wait_for_session_configured(
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
        assert_eq!(mgr.max_threads, 6);
    }

    #[tokio::test]
    async fn empty_manager_has_no_threads() {
        let mgr = ThreadManager::new(4);
        assert_eq!(mgr.thread_count().await, 0);
        assert!(mgr.list_thread_ids().await.is_empty());
    }

    #[tokio::test]
    async fn get_nonexistent_thread_returns_error() {
        let mgr = ThreadManager::new(4);
        let id = ThreadId::new();
        let result = mgr.get_thread(id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn subscribe_returns_receiver() {
        let mgr = ThreadManager::new(4);
        let _rx = mgr.subscribe_thread_created();
        // Just verify it doesn't panic
    }
}
