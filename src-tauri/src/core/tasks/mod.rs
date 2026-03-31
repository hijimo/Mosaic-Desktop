pub mod compact;
pub mod regular;
pub mod review;
pub mod undo;

use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::protocol::types::UserInput;

/// Describes the type of work a task performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Regular,
    Review,
    Compact,
    Undo,
}

/// Async task that drives a session turn.
///
/// Implementations encapsulate a specific workflow (regular chat, reviews,
/// compaction, undo). Each task is spawned on a background Tokio task and
/// communicates results via the session's event channel.
#[async_trait]
pub trait SessionTask: Send + Sync + 'static {
    /// Describes the type of work the task performs.
    fn kind(&self) -> TaskKind;

    /// Executes the task until completion or cancellation.
    ///
    /// Returns an optional final agent message.
    async fn run(
        self: Arc<Self>,
        ctx: TaskContext,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String>;

    /// Cleanup after an abort. Default is no-op.
    async fn abort(&self, _ctx: TaskContext) {}
}

/// Context passed to task runners, providing access to session services.
#[derive(Clone)]
pub struct TaskContext {
    pub eq_tx: async_channel::Sender<crate::protocol::event::Event>,
    /// Session config stack (cloned from session at spawn time).
    pub config: crate::config::ConfigLayerStack,
    /// Working directory.
    pub cwd: std::path::PathBuf,
}

impl TaskContext {
    pub fn new(
        eq_tx: async_channel::Sender<crate::protocol::event::Event>,
        config: crate::config::ConfigLayerStack,
        cwd: std::path::PathBuf,
    ) -> Self {
        Self { eq_tx, config, cwd }
    }

    pub async fn emit(&self, msg: crate::protocol::event::EventMsg) {
        let _ = self
            .eq_tx
            .send(crate::protocol::event::Event {
                id: uuid::Uuid::new_v4().to_string(),
                msg,
            })
            .await;
    }
}
