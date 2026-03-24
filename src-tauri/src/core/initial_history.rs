use crate::core::rollout::policy::RolloutItem;
use crate::core::rollout::recorder::ResumedHistory;

/// Session initial history source.
#[derive(Debug, Clone)]
pub enum InitialHistory {
    /// Brand new session.
    New,
    /// Resumed from existing rollout (keeps original thread_id).
    Resumed(ResumedHistory),
    /// Forked from existing rollout (new thread_id, possibly truncated).
    Forked(Vec<RolloutItem>),
}
