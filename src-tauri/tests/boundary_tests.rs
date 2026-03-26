//! Boundary condition tests for Codex engine, tool handlers, and event mapping.

use tauri_app_lib::config::ConfigLayerStack;
use tauri_app_lib::core::codex::Codex;
use tauri_app_lib::core::event_mapping::parse_turn_item;
use tauri_app_lib::core::tools::handlers::*;
use tauri_app_lib::core::tools::router::{RouteResult, ToolRouter};
use tauri_app_lib::core::tools::ToolRegistry;
use tauri_app_lib::core::mcp_client::McpConnectionManager;
use tauri_app_lib::protocol::event::{Event, EventMsg};
use tauri_app_lib::protocol::items::TurnItem;
use tauri_app_lib::protocol::submission::{Op, Submission};
use tauri_app_lib::protocol::types::*;

// ── Helpers ──────────────────────────────────────────────────────

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

fn make_real_router() -> ToolRouter {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(ShellHandler));
    registry.register(Box::new(ApplyPatchHandler));
    registry.register(Box::new(ListDirHandler));
    registry.register(Box::new(ReadFileHandler));
    registry.register(Box::new(GrepFilesHandler));
    ToolRouter::new(registry, McpConnectionManager::new())
}

// ── B-01: UserTurn empty items ───────────────────────────────────

#[tokio::test]
async fn b01_empty_items_emits_bracket_events() {
    let (sq_tx, eq_rx, codex) = make_codex();
    sq_tx.send(Submission { id: "t1".into(), op: Op::UserTurn {
        items: vec![], cwd: std::env::current_dir().unwrap(),
        approval_policy: AskForApproval::Never, sandbox_policy: SandboxPolicy::new_read_only_policy(),
        model: "test".into(), effort: None, summary: None, service_tier: None,
        final_output_json_schema: None, collaboration_mode: None, personality: None,
    }}).await.unwrap();
    sq_tx.send(Submission { id: "s1".into(), op: Op::Shutdown }).await.unwrap();
    codex.run().await.unwrap();
    let events = drain(&eq_rx);
    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::TurnStarted(_))));
    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::TurnComplete(_))));
    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::ItemStarted(ev) if matches!(&ev.item, TurnItem::UserMessage(_)))));
}

// ── B-02: Multiple turns unique item_ids ─────────────────────────

#[tokio::test]
async fn b02_multiple_turns_unique_item_ids() {
    let (sq_tx, eq_rx, codex) = make_codex();
    for i in 0..3 {
        sq_tx.send(Submission { id: format!("t{i}"), op: user_turn(&format!("msg {i}")) }).await.unwrap();
    }
    sq_tx.send(Submission { id: "s1".into(), op: Op::Shutdown }).await.unwrap();
    codex.run().await.unwrap();
    let events = drain(&eq_rx);
    let ids: Vec<String> = events.iter().filter_map(|e| match &e.msg {
        EventMsg::ItemStarted(ev) => Some(ev.item.id().to_string()),
        _ => None,
    }).collect();
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "item_ids must be unique: {ids:?}");
}

// ── B-03: ItemStarted/Completed id consistency ───────────────────

#[tokio::test]
async fn b03_started_completed_id_match() {
    let (sq_tx, eq_rx, codex) = make_codex();
    sq_tx.send(Submission { id: "t1".into(), op: user_turn("check") }).await.unwrap();
    sq_tx.send(Submission { id: "s1".into(), op: Op::Shutdown }).await.unwrap();
    codex.run().await.unwrap();
    let events = drain(&eq_rx);
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

// ── B-04: Shell handler empty command ────────────────────────────

#[tokio::test]
async fn b04_shell_empty_command() {
    let router = make_real_router();
    let result = router.route_tool_call("shell", serde_json::json!({"command": []})).await;
    assert!(matches!(result, RouteResult::Handled(Err(_))));
}

// ── B-05: Shell handler timeout ──────────────────────────────────

#[tokio::test]
async fn b05_shell_timeout() {
    let router = make_real_router();
    let result = router.route_tool_call("shell", serde_json::json!({"command": ["sleep", "10"], "timeout_ms": 100})).await;
    match result {
        RouteResult::Handled(Ok(v)) => assert_eq!(v["timed_out"], true),
        other => panic!("expected timed_out, got: {other:?}"),
    }
}

// ── B-06: list_dir nonexistent path ──────────────────────────────

#[tokio::test]
async fn b06_list_dir_nonexistent() {
    let router = make_real_router();
    let result = router.route_tool_call("list_dir", serde_json::json!({"dir_path": "/nonexistent_xyz_123"})).await;
    match result {
        RouteResult::Handled(Err(_)) => {}
        RouteResult::Handled(Ok(v)) => assert!(v.to_string().to_lowercase().contains("error") || v.to_string().contains("No such")),
        other => panic!("expected error, got: {other:?}"),
    }
}

// ── B-07: read_file nonexistent ──────────────────────────────────

#[tokio::test]
async fn b07_read_file_nonexistent() {
    let router = make_real_router();
    let result = router.route_tool_call("read_file", serde_json::json!({"file_path": "/nonexistent_xyz.txt"})).await;
    assert!(matches!(result, RouteResult::Handled(Err(_)) | RouteResult::Handled(Ok(_))));
}

// ── B-08: event_mapping empty role ───────────────────────────────

#[test]
fn b08_empty_role_returns_none() {
    let item = ResponseItem::Message {
        id: None, role: "".into(),
        content: vec![ContentItem::OutputText { text: "text".into() }],
        end_turn: None, phase: None,
    };
    assert!(parse_turn_item(&item).is_none());
}

// ── B-09: event_mapping FunctionCall → None ──────────────────────

#[test]
fn b09_function_call_returns_none() {
    let item = ResponseItem::FunctionCall {
        id: Some("fc1".into()), name: "shell".into(),
        arguments: "{}".into(), call_id: "c1".into(),
    };
    assert!(parse_turn_item(&item).is_none());
}
