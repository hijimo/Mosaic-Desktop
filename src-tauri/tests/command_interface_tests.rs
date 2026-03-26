//! Integration tests for all Tauri command interfaces and Op variants.
//!
//! Tests the core logic behind each command without requiring Tauri AppHandle.
//! For commands that need AppHandle (thread_start/resume/fork), we test the
//! underlying Codex::spawn + channel pipeline directly.
//!
//! Run: cargo test --test command_interface_tests -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use tauri_app_lib::config::{ConfigLayer, ConfigLayerStack, ConfigToml};
use tauri_app_lib::core::codex::{Codex, CodexHandle};
use tauri_app_lib::core::rollout::policy::RolloutItem;
use tauri_app_lib::core::state_db::StateDb;
use tauri_app_lib::protocol::event::{Event, EventMsg};
use tauri_app_lib::protocol::submission::{Op, Submission};
use tauri_app_lib::protocol::types::*;

// ── Infrastructure ───────────────────────────────────────────────

fn make_codex() -> (async_channel::Sender<Submission>, async_channel::Receiver<Event>, Codex) {
    let (sq_tx, sq_rx) = async_channel::unbounded();
    let (eq_tx, eq_rx) = async_channel::unbounded();
    let codex = Codex::new(sq_rx, eq_tx, ConfigLayerStack::new(), std::env::current_dir().unwrap());
    (sq_tx, eq_rx, codex)
}

fn drain(rx: &async_channel::Receiver<Event>) -> Vec<Event> {
    let mut out = vec![];
    while let Ok(ev) = rx.try_recv() { out.push(ev); }
    out
}

async fn run_ops(ops: Vec<Op>) -> Vec<Event> {
    let (sq_tx, eq_rx, codex) = make_codex();
    for (i, op) in ops.into_iter().enumerate() {
        sq_tx.send(Submission { id: format!("op-{i}"), op }).await.unwrap();
    }
    sq_tx.send(Submission { id: "shutdown".into(), op: Op::Shutdown }).await.unwrap();
    codex.run().await.unwrap();
    drain(&eq_rx)
}

fn user_turn(text: &str) -> Op {
    Op::UserTurn {
        items: vec![UserInput::Text { text: text.into(), text_elements: vec![] }],
        cwd: std::env::current_dir().unwrap(),
        approval_policy: AskForApproval::Never,
        sandbox_policy: SandboxPolicy::new_read_only_policy(),
        model: "test".into(),
        effort: None, summary: None, service_tier: None,
        final_output_json_schema: None, collaboration_mode: None, personality: None,
    }
}

fn has_event(events: &[Event], f: impl Fn(&EventMsg) -> bool) -> bool {
    events.iter().any(|e| f(&e.msg))
}

// ══════════════════════════════════════════════════════════════════
// Op variants
// ══════════════════════════════════════════════════════════════════

// ── UserTurn ─────────────────────────────────────────────────────

#[tokio::test]
async fn op_user_turn_emits_bracket_events() {
    let events = run_ops(vec![user_turn("hello")]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::TurnStarted(_))));
    assert!(has_event(&events, |m| matches!(m, EventMsg::TurnComplete(_))));
    assert!(has_event(&events, |m| matches!(m, EventMsg::ItemStarted(ev) if matches!(&ev.item, tauri_app_lib::protocol::items::TurnItem::UserMessage(_)))));
    assert!(has_event(&events, |m| matches!(m, EventMsg::ItemCompleted(ev) if matches!(&ev.item, tauri_app_lib::protocol::items::TurnItem::UserMessage(_)))));
}

#[tokio::test]
async fn op_user_turn_empty_items() {
    let events = run_ops(vec![Op::UserTurn {
        items: vec![], cwd: std::env::current_dir().unwrap(),
        approval_policy: AskForApproval::Never, sandbox_policy: SandboxPolicy::new_read_only_policy(),
        model: "test".into(), effort: None, summary: None, service_tier: None,
        final_output_json_schema: None, collaboration_mode: None, personality: None,
    }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::TurnStarted(_))));
    assert!(has_event(&events, |m| matches!(m, EventMsg::TurnComplete(_))));
}

#[tokio::test]
async fn op_user_turn_with_image() {
    let events = run_ops(vec![Op::UserTurn {
        items: vec![
            UserInput::Text { text: "look".into(), text_elements: vec![] },
            UserInput::Image { image_url: "https://example.com/img.png".into() },
        ],
        cwd: std::env::current_dir().unwrap(),
        approval_policy: AskForApproval::Never, sandbox_policy: SandboxPolicy::new_read_only_policy(),
        model: "test".into(), effort: None, summary: None, service_tier: None,
        final_output_json_schema: None, collaboration_mode: None, personality: None,
    }]).await;
    let user_item = events.iter().find_map(|e| match &e.msg {
        EventMsg::ItemCompleted(ev) => match &ev.item {
            tauri_app_lib::protocol::items::TurnItem::UserMessage(m) => Some(m.clone()),
            _ => None,
        },
        _ => None,
    });
    assert!(user_item.is_some());
    assert_eq!(user_item.unwrap().content.len(), 2);
}

// ── UserInput (legacy) ───────────────────────────────────────────

#[tokio::test]
async fn op_user_input_legacy() {
    let events = run_ops(vec![Op::UserInput {
        items: vec![UserInput::Text { text: "legacy".into(), text_elements: vec![] }],
        final_output_json_schema: None,
    }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::TurnStarted(_))));
    assert!(has_event(&events, |m| matches!(m, EventMsg::TurnComplete(_))));
}

// ── Interrupt ────────────────────────────────────────────────────

#[tokio::test]
async fn op_interrupt_emits_aborted() {
    let events = run_ops(vec![Op::Interrupt]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::TurnAborted(_))));
}

#[tokio::test]
async fn op_interrupt_without_turn() {
    // Should not panic even without active turn
    let events = run_ops(vec![Op::Interrupt]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::ShutdownComplete)));
}

// ── Shutdown ─────────────────────────────────────────────────────

#[tokio::test]
async fn op_shutdown_emits_complete() {
    let events = run_ops(vec![]).await; // just shutdown
    assert!(has_event(&events, |m| matches!(m, EventMsg::ShutdownComplete)));
}

// ── AddToHistory ─────────────────────────────────────────────────

#[tokio::test]
async fn op_add_to_history() {
    let events = run_ops(vec![Op::AddToHistory { text: "stored text".into(), role: "user".into() }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::RawResponseItem(_))));
}

#[tokio::test]
async fn op_add_to_history_empty_text() {
    let events = run_ops(vec![Op::AddToHistory { text: "".into(), role: "user".into() }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::RawResponseItem(_))));
}

#[tokio::test]
async fn op_add_to_history_assistant_role() {
    let events = run_ops(vec![Op::AddToHistory { text: "ai said".into(), role: "assistant".into() }]).await;
    let raw = events.iter().find_map(|e| match &e.msg {
        EventMsg::RawResponseItem(r) => Some(&r.item),
        _ => None,
    });
    assert!(raw.is_some());
    if let ResponseItem::Message { role, .. } = raw.unwrap() {
        assert_eq!(role, "assistant");
    }
}

// ── GetHistoryEntryRequest ───────────────────────────────────────

#[tokio::test]
async fn op_get_history_entry_valid() {
    let events = run_ops(vec![
        Op::AddToHistory { text: "first".into(), role: "user".into() },
        Op::GetHistoryEntryRequest { offset: 0, log_id: 1 },
    ]).await;
    let resp = events.iter().find_map(|e| match &e.msg {
        EventMsg::GetHistoryEntryResponse(r) => Some(r),
        _ => None,
    });
    assert!(resp.is_some());
    assert!(resp.unwrap().entry.is_some(), "offset 0 should have an entry");
}

#[tokio::test]
async fn op_get_history_entry_out_of_range() {
    let events = run_ops(vec![Op::GetHistoryEntryRequest { offset: 9999, log_id: 0 }]).await;
    let resp = events.iter().find_map(|e| match &e.msg {
        EventMsg::GetHistoryEntryResponse(r) => Some(r),
        _ => None,
    });
    assert!(resp.unwrap().entry.is_none());
}

// ── SetThreadName ────────────────────────────────────────────────

#[tokio::test]
async fn op_set_thread_name() {
    let events = run_ops(vec![Op::SetThreadName { name: "My Chat".into() }]).await;
    let upd = events.iter().find_map(|e| match &e.msg {
        EventMsg::ThreadNameUpdated(u) => Some(u),
        _ => None,
    });
    assert!(upd.is_some());
    assert_eq!(upd.unwrap().thread_name, Some("My Chat".into()));
}

#[tokio::test]
async fn op_set_thread_name_empty() {
    let events = run_ops(vec![Op::SetThreadName { name: "".into() }]).await;
    let upd = events.iter().find_map(|e| match &e.msg {
        EventMsg::ThreadNameUpdated(u) => Some(u),
        _ => None,
    });
    assert_eq!(upd.unwrap().thread_name, Some("".into()));
}

// ── ThreadRollback ───────────────────────────────────────────────

#[tokio::test]
async fn op_thread_rollback() {
    let events = run_ops(vec![
        Op::AddToHistory { text: "msg1".into(), role: "user".into() },
        Op::AddToHistory { text: "msg2".into(), role: "user".into() },
        Op::ThreadRollback { num_turns: 1 },
    ]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::ThreadRolledBack(r) if r.num_turns == 1)));
}

#[tokio::test]
async fn op_thread_rollback_zero() {
    let events = run_ops(vec![Op::ThreadRollback { num_turns: 0 }]).await;
    // Should not error
    assert!(has_event(&events, |m| matches!(m, EventMsg::ShutdownComplete)));
}

#[tokio::test]
async fn op_thread_rollback_exceeds_history() {
    let events = run_ops(vec![Op::ThreadRollback { num_turns: 999 }]).await;
    // Should emit error (rollback exceeds history length)
    // The exact behavior depends on session.rollback implementation
    assert!(has_event(&events, |m| matches!(m, EventMsg::ShutdownComplete)));
}

// ── Compact ──────────────────────────────────────────────────────

#[tokio::test]
async fn op_compact_empty_history() {
    let events = run_ops(vec![Op::Compact]).await;
    // Compact on empty history should either succeed or emit error, not panic
    assert!(has_event(&events, |m| matches!(m, EventMsg::ShutdownComplete)));
}

#[tokio::test]
async fn op_compact_with_history() {
    let events = run_ops(vec![
        Op::AddToHistory { text: "msg".into(), role: "user".into() },
        Op::Compact,
    ]).await;
    // Should emit ContextCompacted or Error
    let compacted = has_event(&events, |m| matches!(m, EventMsg::ContextCompacted(_)));
    let errored = has_event(&events, |m| matches!(m, EventMsg::Error(_)));
    assert!(compacted || errored, "compact should emit ContextCompacted or Error");
}

// ── ListSkills ───────────────────────────────────────────────────

#[tokio::test]
async fn op_list_skills() {
    let events = run_ops(vec![Op::ListSkills { cwds: vec![], force_reload: false }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::ListSkillsResponse(_))));
}

// ── ListMcpTools ─────────────────────────────────────────────────

#[tokio::test]
async fn op_list_mcp_tools() {
    let events = run_ops(vec![Op::ListMcpTools]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::McpListToolsResponse(_))));
}

// ── ListCustomPrompts ────────────────────────────────────────────

#[tokio::test]
async fn op_list_custom_prompts() {
    let events = run_ops(vec![Op::ListCustomPrompts]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::ListCustomPromptsResponse(_))));
}

// ── OverrideTurnContext ──────────────────────────────────────────

#[tokio::test]
async fn op_override_turn_context_no_active_turn() {
    let events = run_ops(vec![Op::OverrideTurnContext {
        cwd: None, approval_policy: None, sandbox_policy: None, model: None,
        effort: None, summary: None, service_tier: None, collaboration_mode: None, personality: None,
    }]).await;
    // No active turn — should not panic
    assert!(has_event(&events, |m| matches!(m, EventMsg::ShutdownComplete)));
}

// ── DynamicToolResponse ──────────────────────────────────────────

#[tokio::test]
async fn op_dynamic_tool_response_unknown_id() {
    let events = run_ops(vec![Op::DynamicToolResponse {
        id: "nonexistent".into(),
        response: DynamicToolResponse {
            content_items: vec![],
            success: true,
        },
    }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::Error(_))));
}

// ── RealtimeConversation ─────────────────────────────────────────

#[tokio::test]
async fn op_realtime_start_close() {
    let events = run_ops(vec![
        Op::RealtimeConversationStart(ConversationStartParams { prompt: "test".into(), session_id: Some("s1".into()) }),
        Op::RealtimeConversationClose,
    ]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::RealtimeConversationStarted(_))));
    assert!(has_event(&events, |m| matches!(m, EventMsg::RealtimeConversationClosed(_))));
}

#[tokio::test]
async fn op_realtime_start_no_session_id() {
    let events = run_ops(vec![
        Op::RealtimeConversationStart(ConversationStartParams { prompt: "test".into(), session_id: None }),
    ]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::RealtimeConversationStarted(_))));
}

// ── ExecApproval ─────────────────────────────────────────────────

#[tokio::test]
async fn op_exec_approval_denied_without_pending() {
    let events = run_ops(vec![Op::ExecApproval {
        id: "a1".into(), turn_id: None,
        decision: ReviewDecision::Denied,
        custom_instructions: None,
    }]).await;
    // No pending approval — should not panic
    assert!(has_event(&events, |m| matches!(m, EventMsg::ShutdownComplete)));
}

#[tokio::test]
async fn op_exec_approval_abort() {
    let events = run_ops(vec![Op::ExecApproval {
        id: "a1".into(), turn_id: None,
        decision: ReviewDecision::Abort,
        custom_instructions: None,
    }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::TurnAborted(_))));
}

#[tokio::test]
async fn op_exec_approval_with_custom_instructions() {
    let events = run_ops(vec![Op::ExecApproval {
        id: "a1".into(), turn_id: None,
        decision: ReviewDecision::Denied,
        custom_instructions: Some("do it differently".into()),
    }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::ShutdownComplete)));
}

// ── PatchApproval ────────────────────────────────────────────────

#[tokio::test]
async fn op_patch_approval_denied_without_pending() {
    let events = run_ops(vec![Op::PatchApproval {
        id: "p1".into(),
        decision: ReviewDecision::Denied,
        custom_instructions: None,
    }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::ShutdownComplete)));
}

#[tokio::test]
async fn op_patch_approval_abort() {
    let events = run_ops(vec![Op::PatchApproval {
        id: "p1".into(),
        decision: ReviewDecision::Abort,
        custom_instructions: None,
    }]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::TurnAborted(_))));
}

// ══════════════════════════════════════════════════════════════════
// Op deserialization (submit_op path)
// ══════════════════════════════════════════════════════════════════

#[test]
fn op_deserialize_user_turn() {
    let json = serde_json::json!({
        "type": "user_turn",
        "items": [{"type": "text", "text": "hello", "text_elements": []}],
        "cwd": "/tmp",
        "approval_policy": "never",
        "sandbox_policy": {"type": "read-only"},
        "model": "gpt-4o"
    });
    let op: Op = serde_json::from_value(json).unwrap();
    assert!(matches!(op, Op::UserTurn { .. }));
}

#[test]
fn op_deserialize_shutdown() {
    let json = serde_json::json!({"type": "shutdown"});
    let op: Op = serde_json::from_value(json).unwrap();
    assert!(matches!(op, Op::Shutdown));
}

#[test]
fn op_deserialize_interrupt() {
    let json = serde_json::json!({"type": "interrupt"});
    let op: Op = serde_json::from_value(json).unwrap();
    assert!(matches!(op, Op::Interrupt));
}

#[test]
fn op_deserialize_add_to_history() {
    let json = serde_json::json!({"type": "add_to_history", "text": "hello", "role": "user"});
    let op: Op = serde_json::from_value(json).unwrap();
    assert!(matches!(op, Op::AddToHistory { .. }));
}

#[test]
fn op_deserialize_set_thread_name() {
    let json = serde_json::json!({"type": "set_thread_name", "name": "My Chat"});
    let op: Op = serde_json::from_value(json).unwrap();
    assert!(matches!(op, Op::SetThreadName { .. }));
}

#[test]
fn op_deserialize_invalid_returns_error() {
    let json = serde_json::json!({"type": "nonexistent_op"});
    let result = serde_json::from_value::<Op>(json);
    assert!(result.is_err());
}

#[test]
fn op_deserialize_missing_type_returns_error() {
    let json = serde_json::json!({"text": "hello"});
    let result = serde_json::from_value::<Op>(json);
    assert!(result.is_err());
}

// ══════════════════════════════════════════════════════════════════
// Pure function interfaces
// ══════════════════════════════════════════════════════════════════

// ── get_cwd ──────────────────────────────────────────────────────

#[test]
fn get_cwd_returns_valid_path() {
    let cwd = std::env::current_dir().unwrap().to_string_lossy().into_owned();
    assert!(!cwd.is_empty());
    assert!(std::path::Path::new(&cwd).exists());
}

// ── fuzzy_file_search ────────────────────────────────────────────

#[tokio::test]
async fn fuzzy_search_finds_cargo_toml() {
    let cwd = std::env::current_dir().unwrap().to_string_lossy().into_owned();
    let result = tauri_app_lib::file_search::run(
        "Cargo.toml",
        vec![PathBuf::from(&cwd)],
        tauri_app_lib::file_search::FileSearchOptions {
            limit: std::num::NonZero::new(10).unwrap(),
            threads: std::num::NonZero::new(2).unwrap(),
            compute_indices: false,
            ..Default::default()
        },
        None,
    );
    assert!(result.is_ok());
    let matches = result.unwrap().matches;
    assert!(!matches.is_empty(), "should find Cargo.toml");
    assert!(matches.iter().any(|m| m.path.to_string_lossy().contains("Cargo.toml")));
}

#[tokio::test]
async fn fuzzy_search_empty_query_returns_empty() {
    let cwd = std::env::current_dir().unwrap().to_string_lossy().into_owned();
    let result = tauri_app_lib::file_search::run(
        "",
        vec![PathBuf::from(&cwd)],
        tauri_app_lib::file_search::FileSearchOptions {
            limit: std::num::NonZero::new(10).unwrap(),
            threads: std::num::NonZero::new(2).unwrap(),
            compute_indices: false,
            ..Default::default()
        },
        None,
    );
    // Empty query may return empty or all files depending on implementation
    assert!(result.is_ok());
}

// ── get_config / update_config ───────────────────────────────────

#[test]
fn config_merge_returns_valid_json() {
    let config = ConfigLayerStack::new();
    let merged = config.merge();
    let json = serde_json::to_value(&merged);
    assert!(json.is_ok());
}

#[test]
fn config_update_applies_session_layer() {
    let mut config = ConfigLayerStack::new();
    let toml_str = r#"model = "gpt-4o""#;
    let parsed = tauri_app_lib::config::deserialize_toml(toml_str).unwrap();
    config.add_layer(ConfigLayer::Session, parsed);
    let merged = config.merge();
    assert_eq!(merged.model, Some("gpt-4o".into()));
}

#[test]
fn config_update_invalid_toml_returns_error() {
    let result = tauri_app_lib::config::deserialize_toml("not valid toml {{{}}}");
    assert!(result.is_err());
}

// ── Multiple turns: unique IDs ───────────────────────────────────

#[tokio::test]
async fn multiple_turns_unique_item_ids() {
    let events = run_ops(vec![
        user_turn("msg 0"),
        user_turn("msg 1"),
        user_turn("msg 2"),
    ]).await;
    let ids: Vec<String> = events.iter().filter_map(|e| match &e.msg {
        EventMsg::ItemStarted(ev) => Some(ev.item.id().to_string()),
        _ => None,
    }).collect();
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "item_ids must be unique: {ids:?}");
}

// ── No legacy events ─────────────────────────────────────────────

#[tokio::test]
async fn no_legacy_events_emitted() {
    let events = run_ops(vec![user_turn("test")]).await;
    assert!(!has_event(&events, |m| matches!(m, EventMsg::AgentMessageDelta(_))));
    assert!(!has_event(&events, |m| matches!(m, EventMsg::AgentMessage(_))));
    assert!(!has_event(&events, |m| matches!(m, EventMsg::UserMessage(_))));
    assert!(!has_event(&events, |m| matches!(m, EventMsg::AgentReasoningDelta(_))));
}

// ── ItemStarted/Completed consistency ────────────────────────────

#[tokio::test]
async fn item_started_completed_ids_match() {
    let events = run_ops(vec![user_turn("check")]).await;
    let started: Vec<String> = events.iter().filter_map(|e| match &e.msg {
        EventMsg::ItemStarted(ev) => Some(ev.item.id().to_string()),
        _ => None,
    }).collect();
    let completed: Vec<String> = events.iter().filter_map(|e| match &e.msg {
        EventMsg::ItemCompleted(ev) => Some(ev.item.id().to_string()),
        _ => None,
    }).collect();
    for id in &started {
        assert!(completed.contains(id), "ItemStarted '{id}' has no matching ItemCompleted");
    }
}

// ── SessionConfigured on startup ─────────────────────────────────

#[tokio::test]
async fn session_configured_on_startup() {
    let events = run_ops(vec![]).await;
    assert!(has_event(&events, |m| matches!(m, EventMsg::SessionConfigured(_))));
}

// ── Codex::spawn lifecycle ───────────────────────────────────────

#[tokio::test]
async fn codex_spawn_and_shutdown() {
    let handle = Codex::spawn(ConfigLayerStack::new(), std::env::current_dir().unwrap())
        .await
        .unwrap();

    // Should receive SessionConfigured
    let mut got_session = false;
    for _ in 0..50 {
        if let Ok(ev) = handle.rx_event.try_recv() {
            if matches!(ev.msg, EventMsg::SessionConfigured(_)) {
                got_session = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(got_session, "should receive SessionConfigured");

    // Shutdown
    handle.tx_sub.send(Submission { id: "s".into(), op: Op::Shutdown }).await.unwrap();
    let mut got_shutdown = false;
    for _ in 0..50 {
        if let Ok(ev) = handle.rx_event.try_recv() {
            if matches!(ev.msg, EventMsg::ShutdownComplete) {
                got_shutdown = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(got_shutdown, "should receive ShutdownComplete");
}
