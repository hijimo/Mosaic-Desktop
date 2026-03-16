use chrono::{DateTime, Utc};

use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{
    Event, EventMsg, RealtimeConversationClosedEvent, RealtimeConversationRealtimeEvent,
    RealtimeConversationStartedEvent,
};
use crate::protocol::types::ConversationStartParams;

/// Active realtime voice conversation session.
#[derive(Debug, Clone)]
pub struct RealtimeSession {
    pub session_id: String,
    pub model: String,
    pub voice: Option<String>,
    pub started_at: DateTime<Utc>,
}

/// Manages the lifecycle of realtime voice conversations.
pub struct RealtimeConversationManager {
    active_session: Option<RealtimeSession>,
    tx_event: async_channel::Sender<Event>,
}

impl RealtimeConversationManager {
    pub fn new(tx_event: async_channel::Sender<Event>) -> Self {
        Self {
            active_session: None,
            tx_event,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active_session.is_some()
    }

    pub fn active_session(&self) -> Option<&RealtimeSession> {
        self.active_session.as_ref()
    }

    /// Start a new realtime conversation session.
    pub async fn start(&mut self, params: ConversationStartParams) -> Result<(), CodexError> {
        let session_id = params
            .session_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let session = RealtimeSession {
            session_id: session_id.clone(),
            model: params.prompt.clone(),
            voice: None,
            started_at: Utc::now(),
        };
        self.active_session = Some(session);

        self.send_event(EventMsg::RealtimeConversationStarted(
            RealtimeConversationStartedEvent {
                session_id: Some(session_id),
            },
        ))
        .await;

        Ok(())
    }

    /// Stop the active realtime conversation session.
    pub async fn stop(&mut self, reason: Option<String>) -> Result<(), CodexError> {
        self.active_session = None;

        self.send_event(EventMsg::RealtimeConversationClosed(
            RealtimeConversationClosedEvent { reason },
        ))
        .await;

        Ok(())
    }

    /// Send audio data to the active realtime session.
    pub async fn send_audio(&self, data: serde_json::Value) -> Result<(), CodexError> {
        if self.active_session.is_none() {
            return Err(CodexError::new(
                ErrorCode::SessionError,
                "No active realtime conversation session",
            ));
        }

        self.send_event(EventMsg::RealtimeConversationRealtime(
            RealtimeConversationRealtimeEvent { event: data },
        ))
        .await;

        Ok(())
    }

    /// Send text to the active realtime session.
    pub async fn send_text(&self, text: String) -> Result<(), CodexError> {
        if self.active_session.is_none() {
            return Err(CodexError::new(
                ErrorCode::SessionError,
                "No active realtime conversation session",
            ));
        }

        self.send_event(EventMsg::RealtimeConversationRealtime(
            RealtimeConversationRealtimeEvent {
                event: serde_json::json!({ "type": "text", "text": text }),
            },
        ))
        .await;

        Ok(())
    }

    async fn send_event(&self, msg: EventMsg) {
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            msg,
        };
        let _ = self.tx_event.send(event).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> (RealtimeConversationManager, async_channel::Receiver<Event>) {
        let (tx, rx) = async_channel::unbounded();
        (RealtimeConversationManager::new(tx), rx)
    }

    fn drain_events(rx: &async_channel::Receiver<Event>) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        events
    }

    #[tokio::test]
    async fn start_creates_active_session() {
        let (mut mgr, rx) = make_manager();
        assert!(!mgr.is_active());

        let params = ConversationStartParams {
            prompt: "gpt-4o-realtime".into(),
            session_id: Some("test-session-1".into()),
        };
        mgr.start(params).await.unwrap();

        assert!(mgr.is_active());
        let session = mgr.active_session().unwrap();
        assert_eq!(session.session_id, "test-session-1");
        assert_eq!(session.model, "gpt-4o-realtime");

        let events = drain_events(&rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0].msg,
            EventMsg::RealtimeConversationStarted(e) if e.session_id == Some("test-session-1".into())
        ));
    }

    #[tokio::test]
    async fn stop_clears_active_session() {
        let (mut mgr, rx) = make_manager();
        let params = ConversationStartParams {
            prompt: "gpt-4o-realtime".into(),
            session_id: None,
        };
        mgr.start(params).await.unwrap();
        assert!(mgr.is_active());

        mgr.stop(Some("user requested".into())).await.unwrap();
        assert!(!mgr.is_active());

        let events = drain_events(&rx);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[1].msg,
            EventMsg::RealtimeConversationClosed(e) if e.reason == Some("user requested".into())
        ));
    }

    #[tokio::test]
    async fn send_audio_without_session_returns_error() {
        let (mgr, _rx) = make_manager();
        let result = mgr.send_audio(serde_json::json!({"data": "base64"})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::SessionError);
    }

    #[tokio::test]
    async fn send_audio_with_active_session_succeeds() {
        let (mut mgr, rx) = make_manager();
        let params = ConversationStartParams {
            prompt: "gpt-4o-realtime".into(),
            session_id: Some("audio-test".into()),
        };
        mgr.start(params).await.unwrap();

        let audio_data = serde_json::json!({"type": "audio", "data": "base64encoded"});
        mgr.send_audio(audio_data.clone()).await.unwrap();

        let events = drain_events(&rx);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[1].msg,
            EventMsg::RealtimeConversationRealtime(e) if e.event == audio_data
        ));
    }

    #[tokio::test]
    async fn send_text_without_session_returns_error() {
        let (mgr, _rx) = make_manager();
        let result = mgr.send_text("hello".into()).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::SessionError);
    }

    #[tokio::test]
    async fn send_text_with_active_session_succeeds() {
        let (mut mgr, rx) = make_manager();
        let params = ConversationStartParams {
            prompt: "gpt-4o-realtime".into(),
            session_id: None,
        };
        mgr.start(params).await.unwrap();

        mgr.send_text("hello world".into()).await.unwrap();

        let events = drain_events(&rx);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[1].msg,
            EventMsg::RealtimeConversationRealtime(_)
        ));
    }

    #[tokio::test]
    async fn start_generates_session_id_when_none_provided() {
        let (mut mgr, _rx) = make_manager();
        let params = ConversationStartParams {
            prompt: "gpt-4o-realtime".into(),
            session_id: None,
        };
        mgr.start(params).await.unwrap();

        let session = mgr.active_session().unwrap();
        assert!(!session.session_id.is_empty());
    }

    #[tokio::test]
    async fn full_lifecycle_start_audio_stop() {
        let (mut mgr, rx) = make_manager();

        // Start
        mgr.start(ConversationStartParams {
            prompt: "gpt-4o-realtime".into(),
            session_id: Some("lifecycle-test".into()),
        })
        .await
        .unwrap();
        assert!(mgr.is_active());

        // Send audio
        mgr.send_audio(serde_json::json!({"frame": "data"}))
            .await
            .unwrap();

        // Stop
        mgr.stop(None).await.unwrap();
        assert!(!mgr.is_active());

        // Verify audio fails after stop
        let result = mgr.send_audio(serde_json::json!({})).await;
        assert!(result.is_err());

        let events = drain_events(&rx);
        assert_eq!(events.len(), 3); // started + realtime + closed
        assert!(matches!(
            &events[0].msg,
            EventMsg::RealtimeConversationStarted(_)
        ));
        assert!(matches!(
            &events[1].msg,
            EventMsg::RealtimeConversationRealtime(_)
        ));
        assert!(matches!(
            &events[2].msg,
            EventMsg::RealtimeConversationClosed(_)
        ));
    }
}
