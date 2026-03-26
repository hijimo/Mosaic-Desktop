//! Rollout persistence policy tests.

use tauri_app_lib::core::rollout::policy::{is_persisted, EventPersistenceMode, RolloutItem};
use tauri_app_lib::protocol::event::*;
use tauri_app_lib::protocol::types;

// ── P-01: Streaming deltas not persisted ─────────────────────────

#[test]
fn p01_streaming_deltas_not_persisted() {
    let delta = EventMsg::AgentMessageContentDelta(AgentMessageContentDeltaEvent {
        thread_id: "t1".into(), turn_id: "turn1".into(), item_id: "i1".into(), delta: "text".into(),
    });
    assert!(!is_persisted(&RolloutItem::EventMsg(delta.clone()), EventPersistenceMode::Limited));
    assert!(!is_persisted(&RolloutItem::EventMsg(delta), EventPersistenceMode::Extended));

    let reasoning = EventMsg::ReasoningContentDelta(ReasoningContentDeltaEvent {
        thread_id: "t1".into(), turn_id: "turn1".into(), item_id: "i1".into(), delta: "r".into(), summary_index: 0,
    });
    assert!(!is_persisted(&RolloutItem::EventMsg(reasoning), EventPersistenceMode::Extended));
}

// ── P-02: ItemStarted/Completed not persisted ────────────────────

#[test]
fn p02_item_events_not_persisted() {
    let started = EventMsg::ItemStarted(ItemStartedEvent {
        thread_id: "t1".into(), turn_id: "turn1".into(),
        item: tauri_app_lib::protocol::items::TurnItem::ContextCompaction(
            tauri_app_lib::protocol::items::ContextCompactionItem::default(),
        ),
    });
    assert!(!is_persisted(&RolloutItem::EventMsg(started), EventPersistenceMode::Extended));

    let completed = EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id: "t1".into(), turn_id: "turn1".into(),
        item: tauri_app_lib::protocol::items::TurnItem::ContextCompaction(
            tauri_app_lib::protocol::items::ContextCompactionItem::default(),
        ),
    });
    assert!(!is_persisted(&RolloutItem::EventMsg(completed), EventPersistenceMode::Extended));
}

// ── P-03: ResponseItem always persisted ──────────────────────────

#[test]
fn p03_response_item_always_persisted() {
    assert!(is_persisted(&RolloutItem::ResponseItem(types::ResponseItem::Other), EventPersistenceMode::Limited));
    assert!(is_persisted(&RolloutItem::ResponseItem(types::ResponseItem::Other), EventPersistenceMode::Extended));
}

// ── P-04: Core events persisted in Limited ───────────────────────

#[test]
fn p04_core_events_persisted_limited() {
    let cases = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "t1".into(), model_context_window: None,
            collaboration_mode_kind: types::ModeKind::Default,
        }),
        EventMsg::TurnComplete(TurnCompleteEvent { turn_id: "t1".into(), last_agent_message: None }),
        EventMsg::AgentMessage(AgentMessageEvent { message: "hi".into(), phase: None }),
        EventMsg::UserMessage(UserMessageEvent { message: "hello".into(), images: None, local_images: vec![], text_elements: vec![] }),
        EventMsg::TokenCount(TokenCountEvent { info: None, rate_limits: None }),
        EventMsg::ContextCompacted(ContextCompactedEvent),
    ];
    for ev in cases {
        assert!(is_persisted(&RolloutItem::EventMsg(ev), EventPersistenceMode::Limited));
    }
}

// ── P-05: Extended events only in Extended mode ──────────────────

#[test]
fn p05_extended_events_only_extended() {
    let ev = EventMsg::ExecCommandEnd(ExecCommandEndEvent {
        call_id: "c1".into(), process_id: None, turn_id: "t1".into(),
        command: vec!["echo".into()], cwd: "/tmp".into(),
        parsed_cmd: vec![], source: types::ExecCommandSource::Agent,
        interaction_input: None,
        stdout: "".into(), stderr: "".into(), aggregated_output: "".into(),
        exit_code: 0, duration: std::time::Duration::from_secs(0),
        formatted_output: "".into(), status: types::ExecCommandStatus::Completed,
    });
    assert!(!is_persisted(&RolloutItem::EventMsg(ev.clone()), EventPersistenceMode::Limited));
    assert!(is_persisted(&RolloutItem::EventMsg(ev), EventPersistenceMode::Extended));
}
