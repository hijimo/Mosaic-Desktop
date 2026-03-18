use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::{SessionTask, TaskContext, TaskKind};
use crate::protocol::event::EventMsg;
use crate::protocol::types::UserInput;

/// Undo task — restores the working tree to a previous ghost snapshot.
pub struct UndoTask;

#[async_trait]
impl SessionTask for UndoTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Undo
    }

    async fn run(
        self: Arc<Self>,
        ctx: TaskContext,
        _input: Vec<UserInput>,
        _cancellation_token: CancellationToken,
    ) -> Option<String> {
        ctx.emit(EventMsg::UndoStarted(
            crate::protocol::event::UndoStartedEvent {
                message: Some("Undo in progress...".to_string()),
            },
        ))
        .await;

        // TODO: Implement actual ghost snapshot restore using codex_git.
        // For now, emit a placeholder completion.
        ctx.emit(EventMsg::UndoCompleted(
            crate::protocol::event::UndoCompletedEvent {
                success: false,
                message: Some(
                    "Undo not yet implemented: requires ghost snapshot integration.".to_string(),
                ),
            },
        ))
        .await;

        None
    }
}
