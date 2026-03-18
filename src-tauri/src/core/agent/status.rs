//! Agent status derivation from events.

use crate::protocol::event::EventMsg;
use crate::protocol::types::AgentStatus;

/// Derive the next agent status from a single emitted event.
/// Returns `None` when the event does not affect status tracking.
pub fn agent_status_from_event(msg: &EventMsg) -> Option<AgentStatus> {
    match msg {
        EventMsg::TurnStarted(_) => Some(AgentStatus::Running),
        EventMsg::TurnComplete(ev) => Some(AgentStatus::Completed(ev.last_agent_message.clone())),
        EventMsg::TurnAborted(ev) => Some(AgentStatus::Errored(format!("{:?}", ev.reason))),
        EventMsg::Error(ev) => Some(AgentStatus::Errored(ev.message.clone())),
        EventMsg::ShutdownComplete => Some(AgentStatus::Shutdown),
        _ => None,
    }
}

/// Whether the status represents a terminal state.
pub fn is_final(status: &AgentStatus) -> bool {
    !matches!(status, AgentStatus::PendingInit | AgentStatus::Running)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::event::*;

    #[test]
    fn turn_started_maps_to_running() {
        let msg = EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "t1".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        });
        assert_eq!(agent_status_from_event(&msg), Some(AgentStatus::Running));
    }

    #[test]
    fn turn_complete_maps_to_completed() {
        let msg = EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "t1".into(),
            last_agent_message: Some("done".into()),
        });
        assert_eq!(
            agent_status_from_event(&msg),
            Some(AgentStatus::Completed(Some("done".into())))
        );
    }

    #[test]
    fn error_maps_to_errored() {
        let msg = EventMsg::Error(ErrorEvent {
            message: "boom".into(),
            codex_error_info: None,
        });
        assert_eq!(
            agent_status_from_event(&msg),
            Some(AgentStatus::Errored("boom".into()))
        );
    }

    #[test]
    fn shutdown_maps_to_shutdown() {
        let msg = EventMsg::ShutdownComplete;
        assert_eq!(agent_status_from_event(&msg), Some(AgentStatus::Shutdown));
    }

    #[test]
    fn is_final_checks() {
        assert!(!is_final(&AgentStatus::PendingInit));
        assert!(!is_final(&AgentStatus::Running));
        assert!(is_final(&AgentStatus::Completed(None)));
        assert!(is_final(&AgentStatus::Errored("err".into())));
        assert!(is_final(&AgentStatus::Shutdown));
        assert!(is_final(&AgentStatus::NotFound));
    }
}
