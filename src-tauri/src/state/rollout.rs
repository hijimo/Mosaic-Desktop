use serde::{Deserialize, Serialize};

use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::Event;

use super::db::StateDb;

/// A rollout is an ordered sequence of events for a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rollout {
    pub session_id: String,
    pub events: Vec<Event>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl StateDb {
    /// Save a rollout using a transaction for data integrity.
    pub fn save_rollout(&mut self, rollout: &Rollout) -> Result<(), CodexError> {
        let events_json = serde_json::to_string(&rollout.events).map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to serialize rollout events: {e}"),
            )
        })?;

        self.log_db
            .connection()
            .execute(
                "INSERT OR REPLACE INTO rollouts (session_id, events_json, created_at)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    rollout.session_id,
                    events_json,
                    rollout.created_at.to_rfc3339(),
                ],
            )
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to save rollout: {e}"),
                )
            })?;

        self.runtime.metrics.total_writes += 1;
        Ok(())
    }

    /// Load a rollout by session ID.
    pub fn load_rollout(&mut self, session_id: &str) -> Result<Option<Rollout>, CodexError> {
        self.runtime.metrics.total_reads += 1;

        let result = self.log_db.connection().query_row(
            "SELECT session_id, events_json, created_at FROM rollouts WHERE session_id = ?1",
            [session_id],
            |row| {
                let events_json: String = row.get(1)?;
                let created_str: String = row.get(2)?;
                Ok((row.get::<_, String>(0)?, events_json, created_str))
            },
        );

        match result {
            Ok((session_id, events_json, created_str)) => {
                let events: Vec<Event> = serde_json::from_str(&events_json).map_err(|e| {
                    CodexError::new(
                        ErrorCode::InternalError,
                        format!("failed to deserialize rollout events: {e}"),
                    )
                })?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_str)
                    .unwrap()
                    .with_timezone(&chrono::Utc);
                Ok(Some(Rollout {
                    session_id,
                    events,
                    created_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CodexError::new(
                ErrorCode::InternalError,
                format!("failed to load rollout: {e}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::event::{
        AgentMessageDeltaEvent, EventMsg, TurnCompleteEvent, TurnStartedEvent,
    };
    use crate::protocol::types::ModeKind;

    fn temp_db() -> (tempfile::TempDir, StateDb) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = StateDb::open(&path).unwrap();
        (dir, db)
    }

    fn turn_started_event(id: &str) -> Event {
        Event {
            id: id.to_string(),
            msg: EventMsg::TurnStarted(TurnStartedEvent {
                turn_id: "t1".to_string(),
                model_context_window: None,
                collaboration_mode_kind: ModeKind::default(),
            }),
        }
    }

    fn turn_complete_event(id: &str) -> Event {
        Event {
            id: id.to_string(),
            msg: EventMsg::TurnComplete(TurnCompleteEvent {
                turn_id: "t1".to_string(),
                last_agent_message: None,
            }),
        }
    }

    #[test]
    fn rollout_roundtrip() {
        let (_dir, mut db) = temp_db();
        let now = chrono::Utc::now();
        let rollout = Rollout {
            session_id: "sess-1".to_string(),
            events: vec![turn_started_event("e1"), turn_complete_event("e2")],
            created_at: now,
        };
        db.save_rollout(&rollout).unwrap();
        let loaded = db.load_rollout("sess-1").unwrap().unwrap();
        assert_eq!(loaded.session_id, rollout.session_id);
        assert_eq!(loaded.events.len(), 2);
        assert_eq!(loaded.events[0].id, "e1");
        assert_eq!(loaded.events[1].id, "e2");
    }

    #[test]
    fn load_nonexistent_rollout() {
        let (_dir, mut db) = temp_db();
        assert!(db.load_rollout("nope").unwrap().is_none());
    }

    #[test]
    fn rollout_preserves_event_order() {
        let (_dir, mut db) = temp_db();
        let events: Vec<Event> = (0..10)
            .map(|i| Event {
                id: format!("e-{i}"),
                msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
                    delta: format!("chunk-{i}"),
                }),
            })
            .collect();
        let rollout = Rollout {
            session_id: "ordered".to_string(),
            events,
            created_at: chrono::Utc::now(),
        };
        db.save_rollout(&rollout).unwrap();
        let loaded = db.load_rollout("ordered").unwrap().unwrap();
        for (i, event) in loaded.events.iter().enumerate() {
            assert_eq!(event.id, format!("e-{i}"));
        }
    }

    #[test]
    fn rollout_upsert() {
        let (_dir, mut db) = temp_db();
        let r1 = Rollout {
            session_id: "s1".to_string(),
            events: vec![turn_started_event("old")],
            created_at: chrono::Utc::now(),
        };
        db.save_rollout(&r1).unwrap();

        let r2 = Rollout {
            session_id: "s1".to_string(),
            events: vec![turn_started_event("new")],
            created_at: chrono::Utc::now(),
        };
        db.save_rollout(&r2).unwrap();

        let loaded = db.load_rollout("s1").unwrap().unwrap();
        assert_eq!(loaded.events.len(), 1);
        assert_eq!(loaded.events[0].id, "new");
    }
}
