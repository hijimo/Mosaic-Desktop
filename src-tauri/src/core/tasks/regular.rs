use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::{SessionTask, TaskContext, TaskKind};
use crate::protocol::types::UserInput;

/// Regular chat turn task — streams a model response for user input.
pub struct RegularTask;

impl Default for RegularTask {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl SessionTask for RegularTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Regular
    }

    async fn run(
        self: Arc<Self>,
        ctx: TaskContext,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        // Delegate to the session's existing turn execution logic.
        // The actual streaming is handled by Session::handle_user_turn.
        let _ = (ctx, input, cancellation_token);
        None
    }
}
