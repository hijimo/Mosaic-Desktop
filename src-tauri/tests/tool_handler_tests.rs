//! Tool handler integration tests.

use tauri_app_lib::config::ConfigLayerStack;
use tauri_app_lib::core::mcp_client::McpConnectionManager;
use tauri_app_lib::core::session::Session;
use tauri_app_lib::core::tools::handlers::*;
use tauri_app_lib::core::tools::router::{RouteResult, ToolRouter};
use tauri_app_lib::core::tools::ToolRegistry;

fn make_real_router() -> ToolRouter {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(ShellHandler));
    registry.register(Box::new(ApplyPatchHandler));
    registry.register(Box::new(ListDirHandler));
    registry.register(Box::new(ReadFileHandler));
    registry.register(Box::new(GrepFilesHandler));
    ToolRouter::new(registry, McpConnectionManager::new())
}

// ── T-01: Session registers 5 built-in tools ─────────────────────

#[tokio::test]
async fn t01_session_registers_builtin_tools() {
    let (tx, _rx) = async_channel::unbounded();
    let session = Session::new(
        std::env::current_dir().unwrap(),
        ConfigLayerStack::new(),
        tx,
    );
    let router = session.tool_router().await;
    let specs = router.collect_tool_specs();
    assert!(
        specs.len() >= 5,
        "expected >= 5 tool specs, got {}",
        specs.len()
    );
}

// ── T-02: Shell echo executes ────────────────────────────────────

#[tokio::test]
async fn t02_shell_echo() {
    let router = make_real_router();
    let result = router
        .route_tool_call("shell", serde_json::json!({"command": ["echo", "hello"]}))
        .await;
    match result {
        RouteResult::Handled(Ok(v)) => {
            assert_eq!(v["exit_code"], 0);
            assert!(v["stdout"].as_str().unwrap().contains("hello"));
        }
        other => panic!("expected Ok, got: {other:?}"),
    }
}

// ── T-03: list_dir lists current directory ───────────────────────

#[tokio::test]
async fn t03_list_dir_current() {
    let router = make_real_router();
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let result = router
        .route_tool_call("list_dir", serde_json::json!({"dir_path": cwd}))
        .await;
    match result {
        RouteResult::Handled(Ok(v)) => assert!(v.to_string().contains("Cargo.toml")),
        other => panic!("expected Ok, got: {other:?}"),
    }
}

// ── T-04: tool_specs include required tools ──────────────────────

#[tokio::test]
async fn t04_specs_include_required_tools() {
    let router = make_real_router();
    let specs = router.collect_tool_specs();
    let names: Vec<String> = specs
        .iter()
        .filter_map(|s| s.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect();
    for required in &[
        "shell",
        "apply_patch",
        "list_dir",
        "read_file",
        "grep_files",
    ] {
        assert!(
            names.iter().any(|n| n == required),
            "missing '{required}', got: {names:?}"
        );
    }
}

// ── T-05: apply_patch invalid patch ──────────────────────────────

#[tokio::test]
async fn t05_apply_patch_invalid() {
    let router = make_real_router();
    let result = router
        .route_tool_call("apply_patch", serde_json::json!({"input": "not a patch"}))
        .await;
    match result {
        RouteResult::Handled(Err(_)) => {}
        RouteResult::Handled(Ok(v)) => {
            let s = v.to_string().to_lowercase();
            assert!(s.contains("error") || s.contains("no changes") || s.contains("fail"));
        }
        other => panic!("expected error, got: {other:?}"),
    }
}

// ── T-06: grep_files missing pattern ─────────────────────────────

#[tokio::test]
async fn t06_grep_files_missing_pattern() {
    let router = make_real_router();
    let result = router
        .route_tool_call("grep_files", serde_json::json!({}))
        .await;
    assert!(matches!(result, RouteResult::Handled(Err(_))));
}
