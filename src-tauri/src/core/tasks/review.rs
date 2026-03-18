use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::{SessionTask, TaskContext, TaskKind};
use crate::protocol::event::EventMsg;
use crate::protocol::types::UserInput;

/// Review task — runs a sub-agent to review code changes.
pub struct ReviewTask;

#[async_trait]
impl SessionTask for ReviewTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Review
    }

    async fn run(
        self: Arc<Self>,
        ctx: TaskContext,
        _input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        // Emit review mode entry. The actual review sub-agent conversation
        // will be implemented when codex_delegate (sub-codex thread) is available.
        ctx.emit(EventMsg::EnteredReviewMode(
            crate::protocol::types::ReviewRequest {
                target: crate::protocol::types::ReviewTarget::UncommittedChanges,
                user_facing_hint: None,
            },
        ))
        .await;

        if cancellation_token.is_cancelled() {
            return None;
        }

        // TODO: Start sub-codex conversation with review prompt,
        // process events, parse ReviewOutputEvent, emit ExitedReviewMode.
        warn!("review task: sub-agent conversation not yet implemented");

        ctx.emit(EventMsg::ExitedReviewMode(
            crate::protocol::event::ExitedReviewModeEvent {
                review_output: None,
            },
        ))
        .await;

        None
    }

    async fn abort(&self, ctx: TaskContext) {
        ctx.emit(EventMsg::ExitedReviewMode(
            crate::protocol::event::ExitedReviewModeEvent {
                review_output: None,
            },
        ))
        .await;
    }
}
