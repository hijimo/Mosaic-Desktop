/// Event-driven hook system for the Codex engine.
///
/// Hooks fire **after** specific system events (AfterAgent, AfterToolUse).
/// Each registered hook returns a `HookResult`:
/// - `Success` — continue normally.
/// - `FailedContinue` — log the error, keep processing.
/// - `FailedAbort` — stop all subsequent hooks and send an `Error` event.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::protocol::event::{ErrorEvent, Event, EventMsg};

// ── Hook events ──────────────────────────────────────────────────

/// The two post-hoc event types that can trigger hooks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum HookEvent {
    AfterAgent {
        #[serde(rename = "agentId")]
        agent_id: String,
        result: serde_json::Value,
    },
    AfterToolUse {
        #[serde(rename = "toolName")]
        tool_name: String,
        result: serde_json::Value,
    },
}

impl HookEvent {
    /// Discriminant used for matching registrations to fired events.
    fn kind(&self) -> HookEventKind {
        match self {
            HookEvent::AfterAgent { .. } => HookEventKind::AfterAgent,
            HookEvent::AfterToolUse { .. } => HookEventKind::AfterToolUse,
        }
    }
}

/// Lightweight discriminant — no payload, just the variant tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEventKind {
    AfterAgent,
    AfterToolUse,
}

// ── Hook results ─────────────────────────────────────────────────

/// Outcome of a single hook execution.
#[derive(Debug, Clone, PartialEq)]
pub enum HookResult {
    Success,
    FailedContinue { error: String },
    FailedAbort { error: String },
}

// ── Handler trait ────────────────────────────────────────────────

/// Async trait implemented by each hook handler.
#[async_trait]
pub trait HookHandler: Send + Sync {
    async fn execute(&self, event: &HookEvent) -> HookResult;
}

// ── Definition & Registry ────────────────────────────────────────

/// A named hook bound to a specific event kind.
pub struct HookDefinition {
    pub name: String,
    pub event_kind: HookEventKind,
    pub handler: Box<dyn HookHandler>,
}

/// Central registry that stores hook definitions and fires them.
pub struct HookRegistry {
    hooks: Vec<HookDefinition>,
    tx_event: Option<async_channel::Sender<Event>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: Vec::new(),
            tx_event: None,
        }
    }

    /// Create a registry wired to the event queue so it can emit `Error`
    /// events when a hook aborts.
    pub fn with_event_sender(tx_event: async_channel::Sender<Event>) -> Self {
        Self {
            hooks: Vec::new(),
            tx_event: Some(tx_event),
        }
    }

    /// Attach (or replace) the event sender after construction.
    pub fn set_event_sender(&mut self, tx_event: async_channel::Sender<Event>) {
        self.tx_event = Some(tx_event);
    }

    /// Register a new hook definition.
    pub fn register(&mut self, definition: HookDefinition) {
        self.hooks.push(definition);
    }

    /// Fire all hooks whose `event_kind` matches the given event.
    ///
    /// Returns the collected `HookResult` list.  Processing stops early
    /// when any hook returns `FailedAbort`; an `EventMsg::Error` is sent
    /// on the EQ in that case.  `FailedContinue` is recorded but does
    /// not interrupt the remaining hooks.
    pub async fn fire(&self, event: &HookEvent) -> Vec<HookResult> {
        let target_kind = event.kind();
        let mut results = Vec::new();

        for hook in &self.hooks {
            if hook.event_kind != target_kind {
                continue;
            }

            let result = hook.handler.execute(event).await;

            match &result {
                HookResult::Success => {}
                HookResult::FailedContinue { error } => {
                    // Log but keep going.
                    eprintln!("[hooks] hook '{}' failed (continue): {error}", hook.name);
                }
                HookResult::FailedAbort { error } => {
                    eprintln!("[hooks] hook '{}' aborted processing: {error}", hook.name);

                    // Send Error event on the EQ.
                    if let Some(tx) = &self.tx_event {
                        let err_event = Event {
                            id: uuid::Uuid::new_v4().to_string(),
                            msg: EventMsg::Error(ErrorEvent {
                                message: format!("Hook '{}' aborted: {error}", hook.name),
                                codex_error_info: None,
                            }),
                        };
                        // Best-effort send; if the channel is closed we
                        // still honour the abort semantics.
                        let _ = tx.send(err_event).await;
                    }

                    results.push(result);
                    return results; // stop processing
                }
            }

            results.push(result);
        }

        results
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Legacy compatibility shims ───────────────────────────────────
//
// The old Hook trait exposed synchronous lifecycle callbacks
// (notify_session_start, notify_turn_start, etc.) that codex.rs
// still calls.  These no-ops keep the build green until codex.rs
// migrates to the event-driven `fire` API.

impl HookRegistry {
    pub fn notify_session_start(&self, _session_id: &str) {}
    pub fn notify_session_end(&self, _session_id: &str) {}
    pub fn notify_turn_start(&self, _turn_id: &str) {}
    pub fn notify_turn_complete(&self, _turn_id: &str) {}
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple handler that always returns a fixed result.
    struct FixedHandler(HookResult);

    #[async_trait]
    impl HookHandler for FixedHandler {
        async fn execute(&self, _event: &HookEvent) -> HookResult {
            self.0.clone()
        }
    }

    fn after_agent_event() -> HookEvent {
        HookEvent::AfterAgent {
            agent_id: "a1".into(),
            result: serde_json::json!({"ok": true}),
        }
    }

    fn after_tool_event() -> HookEvent {
        HookEvent::AfterToolUse {
            tool_name: "read_file".into(),
            result: serde_json::json!("done"),
        }
    }

    #[tokio::test]
    async fn fire_returns_empty_when_no_hooks() {
        let registry = HookRegistry::new();
        let results = registry.fire(&after_agent_event()).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn fire_collects_success_results() {
        let mut registry = HookRegistry::new();
        registry.register(HookDefinition {
            name: "h1".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::Success)),
        });
        registry.register(HookDefinition {
            name: "h2".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::Success)),
        });

        let results = registry.fire(&after_agent_event()).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], HookResult::Success);
        assert_eq!(results[1], HookResult::Success);
    }

    #[tokio::test]
    async fn fire_skips_non_matching_hooks() {
        let mut registry = HookRegistry::new();
        // Register for AfterToolUse only.
        registry.register(HookDefinition {
            name: "tool_hook".into(),
            event_kind: HookEventKind::AfterToolUse,
            handler: Box::new(FixedHandler(HookResult::Success)),
        });

        // Fire AfterAgent — should not match.
        let results = registry.fire(&after_agent_event()).await;
        assert!(results.is_empty());

        // Fire AfterToolUse — should match.
        let results = registry.fire(&after_tool_event()).await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn failed_continue_does_not_stop_processing() {
        let mut registry = HookRegistry::new();
        registry.register(HookDefinition {
            name: "warn".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::FailedContinue {
                error: "non-fatal".into(),
            })),
        });
        registry.register(HookDefinition {
            name: "ok".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::Success)),
        });

        let results = registry.fire(&after_agent_event()).await;
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0],
            HookResult::FailedContinue {
                error: "non-fatal".into()
            }
        );
        assert_eq!(results[1], HookResult::Success);
    }

    #[tokio::test]
    async fn failed_abort_stops_processing() {
        let (tx, rx) = async_channel::unbounded();
        let mut registry = HookRegistry::with_event_sender(tx);

        registry.register(HookDefinition {
            name: "abort_hook".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::FailedAbort {
                error: "fatal".into(),
            })),
        });
        // This hook should never execute.
        registry.register(HookDefinition {
            name: "unreachable".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::Success)),
        });

        let results = registry.fire(&after_agent_event()).await;

        // Only the aborting hook's result is collected.
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0],
            HookResult::FailedAbort {
                error: "fatal".into()
            }
        );

        // An Error event was sent on the EQ.
        let event = rx.try_recv().expect("expected an error event");
        match &event.msg {
            EventMsg::Error(e) => {
                assert!(
                    e.message.contains("abort_hook"),
                    "error should mention hook name"
                );
                assert!(
                    e.message.contains("fatal"),
                    "error should contain the abort reason"
                );
            }
            other => panic!("expected Error event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn failed_abort_without_event_sender_still_stops() {
        let mut registry = HookRegistry::new(); // no tx_event
        registry.register(HookDefinition {
            name: "abort".into(),
            event_kind: HookEventKind::AfterToolUse,
            handler: Box::new(FixedHandler(HookResult::FailedAbort {
                error: "boom".into(),
            })),
        });
        registry.register(HookDefinition {
            name: "never".into(),
            event_kind: HookEventKind::AfterToolUse,
            handler: Box::new(FixedHandler(HookResult::Success)),
        });

        let results = registry.fire(&after_tool_event()).await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn mixed_results_before_abort() {
        let (tx, rx) = async_channel::unbounded();
        let mut registry = HookRegistry::with_event_sender(tx);

        registry.register(HookDefinition {
            name: "ok1".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::Success)),
        });
        registry.register(HookDefinition {
            name: "warn1".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::FailedContinue {
                error: "meh".into(),
            })),
        });
        registry.register(HookDefinition {
            name: "abort1".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::FailedAbort {
                error: "stop".into(),
            })),
        });
        registry.register(HookDefinition {
            name: "never_reached".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::Success)),
        });

        let results = registry.fire(&after_agent_event()).await;
        assert_eq!(results.len(), 3); // Success, FailedContinue, FailedAbort
        assert_eq!(results[0], HookResult::Success);
        assert_eq!(
            results[1],
            HookResult::FailedContinue {
                error: "meh".into()
            }
        );
        assert_eq!(
            results[2],
            HookResult::FailedAbort {
                error: "stop".into()
            }
        );

        // Exactly one Error event.
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn hook_event_json_roundtrip() {
        let events = vec![after_agent_event(), after_tool_event()];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let decoded: HookEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, decoded);
        }
    }

    #[tokio::test]
    async fn hook_event_uses_camel_case() {
        let event = after_agent_event();
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("agentId"), "expected camelCase agentId");
        assert!(!json.contains("agent_id"), "should not contain snake_case");

        let event = after_tool_event();
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("toolName"), "expected camelCase toolName");
        assert!(!json.contains("tool_name"), "should not contain snake_case");
    }

    #[tokio::test]
    async fn set_event_sender_after_construction() {
        let (tx, rx) = async_channel::unbounded();
        let mut registry = HookRegistry::new();
        registry.set_event_sender(tx);

        registry.register(HookDefinition {
            name: "late_abort".into(),
            event_kind: HookEventKind::AfterAgent,
            handler: Box::new(FixedHandler(HookResult::FailedAbort {
                error: "late".into(),
            })),
        });

        registry.fire(&after_agent_event()).await;
        assert!(rx.try_recv().is_ok(), "error event should be sent");
    }
}
