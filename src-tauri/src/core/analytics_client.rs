use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::mpsc;
use tracing::warn;

/// Context for tracking analytics events within a turn.
#[derive(Clone, Debug)]
pub struct TrackEventsContext {
    pub model_slug: String,
    pub thread_id: String,
    pub turn_id: String,
}

pub fn build_track_events_context(
    model_slug: String,
    thread_id: String,
    turn_id: String,
) -> TrackEventsContext {
    TrackEventsContext {
        model_slug,
        thread_id,
        turn_id,
    }
}

/// A skill invocation event for analytics.
#[derive(Clone, Debug)]
pub struct SkillInvocation {
    pub skill_name: String,
    pub skill_path: PathBuf,
    pub invocation_type: InvocationType,
}

/// How a skill was invoked.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InvocationType {
    Explicit,
    Implicit,
}

/// An app invocation event for analytics.
pub struct AppInvocation {
    pub connector_id: Option<String>,
    pub app_name: Option<String>,
    pub invocation_type: Option<InvocationType>,
}

const QUEUE_SIZE: usize = 256;

enum AnalyticsJob {
    SkillInvocations {
        context: TrackEventsContext,
        invocations: Vec<SkillInvocation>,
    },
    AppUsed {
        context: TrackEventsContext,
        app: AppInvocation,
    },
}

/// Async analytics event client.
///
/// Enqueues events onto a background task for batched delivery.
/// Currently logs events; wire protocol delivery is a future extension.
pub struct AnalyticsEventsClient {
    tx: mpsc::Sender<AnalyticsJob>,
}

impl AnalyticsEventsClient {
    pub fn new() -> Self {
        let (tx, mut rx) = mpsc::channel::<AnalyticsJob>(QUEUE_SIZE);
        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                match job {
                    AnalyticsJob::SkillInvocations {
                        context,
                        invocations,
                    } => {
                        for inv in &invocations {
                            tracing::debug!(
                                model = context.model_slug,
                                turn = context.turn_id,
                                skill = inv.skill_name,
                                "analytics: skill invocation"
                            );
                        }
                    }
                    AnalyticsJob::AppUsed { context, app } => {
                        tracing::debug!(
                            model = context.model_slug,
                            turn = context.turn_id,
                            app = app.app_name.as_deref().unwrap_or("unknown"),
                            "analytics: app used"
                        );
                    }
                }
            }
        });
        Self { tx }
    }

    pub fn track_skill_invocations(
        &self,
        context: TrackEventsContext,
        invocations: Vec<SkillInvocation>,
    ) {
        if self
            .tx
            .try_send(AnalyticsJob::SkillInvocations {
                context,
                invocations,
            })
            .is_err()
        {
            warn!("analytics queue full, dropping skill invocations");
        }
    }

    pub fn track_app_used(&self, context: TrackEventsContext, app: AppInvocation) {
        if self
            .tx
            .try_send(AnalyticsJob::AppUsed { context, app })
            .is_err()
        {
            warn!("analytics queue full, dropping app used event");
        }
    }
}

impl Default for AnalyticsEventsClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn track_skill_invocations_does_not_panic() {
        let client = AnalyticsEventsClient::new();
        let ctx = build_track_events_context("gpt-4".into(), "thread-1".into(), "turn-1".into());
        client.track_skill_invocations(
            ctx,
            vec![SkillInvocation {
                skill_name: "test".into(),
                skill_path: PathBuf::from("/tmp/SKILL.md"),
                invocation_type: InvocationType::Explicit,
            }],
        );
        // Give the background task a moment to process.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn track_app_used_does_not_panic() {
        let client = AnalyticsEventsClient::new();
        let ctx = build_track_events_context("gpt-4".into(), "thread-1".into(), "turn-1".into());
        client.track_app_used(
            ctx,
            AppInvocation {
                connector_id: Some("conn-1".into()),
                app_name: Some("my-app".into()),
                invocation_type: Some(InvocationType::Explicit),
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
