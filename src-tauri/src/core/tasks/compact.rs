use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::{SessionTask, TaskContext, TaskKind};
use crate::protocol::event::EventMsg;
use crate::protocol::types::UserInput;

/// Compact task — requests conversation history compression.
///
/// The actual compaction is performed by the Codex engine when it receives
/// the compact event. This task simply signals the intent.
pub struct CompactTask;

#[async_trait]
impl SessionTask for CompactTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Compact
    }

    async fn run(
        self: Arc<Self>,
        ctx: TaskContext,
        _input: Vec<UserInput>,
        _cancellation_token: CancellationToken,
    ) -> Option<String> {
        // Signal that compaction should happen. The Codex engine handles
        // the actual session.compact_history call in its main loop.
        ctx.emit(EventMsg::ContextCompacted(
            crate::protocol::event::ContextCompactedEvent,
        ))
        .await;
        None
    }
}
