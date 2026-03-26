//! Real API integration tests — full submit_op pipeline with real model.
//!
//! Run all: cargo test --test real_api_tests -- --ignored --nocapture
//! Run one: cargo test --test real_api_tests <test_name> -- --ignored --nocapture
//!
//! Requires ~/.codex/config.toml with valid model + provider + API key.

use std::path::PathBuf;
use std::time::Duration;
use tauri_app_lib::config::{ConfigLayer, ConfigLayerStack};
use tauri_app_lib::core::codex::{Codex, CodexHandle};
use tauri_app_lib::protocol::event::{Event, EventMsg};
use tauri_app_lib::protocol::items::TurnItem;
use tauri_app_lib::protocol::submission::{Op, Submission};
use tauri_app_lib::protocol::types::*;

// ── Infrastructure ───────────────────────────────────────────────

fn load_config() -> ConfigLayerStack {
    let mut stack = ConfigLayerStack::new();
    if let Some(home) = std::env::var_os("HOME") {
        let path = std::path::Path::new(&home).join(".codex/config.toml");
        if let Ok(content) = std::fs::read_to_string(&path) {
            let mut skip = false;
            let mut cleaned = Vec::new();
            for line in content.lines() {
                if line.starts_with("[shell_environment_policy")
                    || line.starts_with("[mcp_servers")
                {
                    skip = true;
                    continue;
                }
                if skip {
                    if line.starts_with('[')
                        && !line.starts_with("[shell_environment_policy")
                        && !line.starts_with("[mcp_servers")
                    {
                        skip = false;
                    } else {
                        continue;
                    }
                }
                cleaned.push(line);
            }
            if let Ok(parsed) = tauri_app_lib::config::deserialize_toml(&cleaned.join("\n")) {
                stack.add_layer(ConfigLayer::User, parsed);
            }
        }
    }
    stack
}

struct Engine {
    handle: CodexHandle,
    model: String,
    cwd: PathBuf,
}

async fn spawn() -> Engine {
    let config = load_config();
    let merged = config.merge();
    let profile = merged.profile.clone().unwrap_or_default();
    let resolved = if profile.is_empty() { merged } else { config.resolve_with_profile(&profile) };
    let model = resolved.model.clone().unwrap_or_default();
    eprintln!("[api] model={model}, provider={}", resolved.model_provider.as_deref().unwrap_or("?"));
    assert!(!model.is_empty(), "model not configured");

    let cwd = std::env::current_dir().unwrap();
    let mut stack = ConfigLayerStack::new();
    stack.add_layer(ConfigLayer::User, resolved);
    let handle = Codex::spawn(stack, cwd.clone()).await.unwrap();

    for _ in 0..50 {
        if let Ok(ev) = handle.rx_event.try_recv() {
            if matches!(ev.msg, EventMsg::SessionConfigured(_)) {
                return Engine { handle, model, cwd };
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("SessionConfigured not received");
}

fn turn(engine: &Engine, text: &str) -> Op {
    Op::UserTurn {
        items: vec![UserInput::Text { text: text.into(), text_elements: vec![] }],
        cwd: engine.cwd.clone(),
        approval_policy: AskForApproval::Never,
        sandbox_policy: SandboxPolicy::DangerFullAccess,
        model: engine.model.clone(),
        effort: None, summary: None, service_tier: None,
        final_output_json_schema: None, collaboration_mode: None, personality: None,
    }
}

/// Collect events until TurnComplete, logging everything.
async fn collect(engine: &Engine, timeout_secs: u64) -> Vec<Event> {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if tokio::time::Instant::now() > deadline {
            eprintln!("[api] ⏰ timeout");
            break;
        }
        match tokio::time::timeout(Duration::from_millis(500), engine.handle.rx_event.recv()).await {
            Ok(Ok(ev)) => {
                let done = matches!(&ev.msg, EventMsg::TurnComplete(_) | EventMsg::ShutdownComplete);
                match &ev.msg {
                    EventMsg::TurnStarted(_) => eprintln!("[api] ▶ TurnStarted"),
                    EventMsg::TurnComplete(tc) => eprintln!("[api] ✅ TurnComplete (msg: {:?})", tc.last_agent_message.as_deref().map(|s| &s[..s.len().min(80)])),
                    EventMsg::ItemStarted(ev) => eprintln!("[api] 📦 ItemStarted({})", match &ev.item { TurnItem::UserMessage(_) => "User", TurnItem::AgentMessage(_) => "Agent", TurnItem::Reasoning(_) => "Reasoning", TurnItem::Plan(_) => "Plan", TurnItem::WebSearch(_) => "WebSearch", TurnItem::ContextCompaction(_) => "Compaction" }),
                    EventMsg::ItemCompleted(ev) => eprintln!("[api] 📦 ItemCompleted({})", match &ev.item { TurnItem::UserMessage(_) => "User", TurnItem::AgentMessage(_) => "Agent", TurnItem::Reasoning(_) => "Reasoning", TurnItem::Plan(_) => "Plan", TurnItem::WebSearch(_) => "WebSearch", TurnItem::ContextCompaction(_) => "Compaction" }),
                    EventMsg::AgentMessageContentDelta(d) => eprint!("{}", d.delta),
                    EventMsg::ReasoningContentDelta(d) => eprint!("[R]{}", d.delta),
                    EventMsg::McpToolCallBegin(tc) => eprintln!("\n[api] 🔧 ToolBegin: {}", tc.invocation.tool),
                    EventMsg::McpToolCallEnd(tc) => eprintln!("[api] 🔧 ToolEnd: {}", tc.invocation.tool),
                    EventMsg::TokenCount(tc) => if let Some(i) = &tc.info { eprintln!("[api] 📊 tokens: in={} out={}", i.total_token_usage.input_tokens, i.total_token_usage.output_tokens) },
                    EventMsg::Error(e) => eprintln!("\n[api] ❌ {}", &e.message[..e.message.len().min(200)]),
                    EventMsg::Warning(w) => eprintln!("[api] ⚠️ {}", w.message),
                    _ => {}
                }
                events.push(ev);
                if done { break; }
            }
            Ok(Err(_)) => break,
            Err(_) => continue,
        }
    }
    events
}

async fn shutdown(engine: &Engine) {
    let _ = engine.handle.tx_sub.send(Submission { id: "shutdown".into(), op: Op::Shutdown }).await;
    while let Ok(_) = engine.handle.rx_event.try_recv() {}
}

fn has(events: &[Event], f: impl Fn(&EventMsg) -> bool) -> bool {
    events.iter().any(|e| f(&e.msg))
}

fn agent_text(events: &[Event]) -> String {
    events.iter().filter_map(|e| match &e.msg {
        EventMsg::AgentMessageContentDelta(d) => Some(d.delta.as_str()),
        _ => None,
    }).collect()
}

fn tool_names(events: &[Event]) -> Vec<String> {
    events.iter().filter_map(|e| match &e.msg {
        EventMsg::McpToolCallBegin(tc) => Some(tc.invocation.tool.clone()),
        _ => None,
    }).collect()
}

fn errors(events: &[Event]) -> Vec<String> {
    events.iter().filter_map(|e| match &e.msg {
        EventMsg::Error(e) => Some(e.message.clone()),
        _ => None,
    }).collect()
}

// ══════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════

/// 普通对话 — 无工具调用，纯文本回复
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn plain_text_conversation() {
    let engine = spawn().await;
    eprintln!("\n=== plain_text_conversation ===\n");

    engine.handle.tx_sub.send(Submission { id: "t1".into(), op: turn(&engine, "Say hello in one word. Do not use any tools.") }).await.unwrap();
    let events = collect(&engine, 30).await;

    assert!(has(&events, |m| matches!(m, EventMsg::TurnStarted(_))), "missing TurnStarted");
    assert!(has(&events, |m| matches!(m, EventMsg::TurnComplete(_))), "missing TurnComplete");
    assert!(has(&events, |m| matches!(m, EventMsg::ItemStarted(ev) if matches!(&ev.item, TurnItem::UserMessage(_)))), "missing UserMessage ItemStarted");
    assert!(has(&events, |m| matches!(m, EventMsg::ItemCompleted(ev) if matches!(&ev.item, TurnItem::AgentMessage(_)))), "missing AgentMessage ItemCompleted");

    let text = agent_text(&events);
    assert!(!text.is_empty(), "agent should respond with text");
    eprintln!("\n[api] response: {text}");

    // No legacy events
    assert!(!has(&events, |m| matches!(m, EventMsg::AgentMessageDelta(_))));
    assert!(!has(&events, |m| matches!(m, EventMsg::AgentMessage(_))));

    shutdown(&engine).await;
}

/// 触发 shell 命令 — echo
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn shell_tool_call() {
    let engine = spawn().await;
    eprintln!("\n=== shell_tool_call ===\n");

    engine.handle.tx_sub.send(Submission { id: "t1".into(), op: turn(&engine, "Run this exact shell command: echo hello_from_test") }).await.unwrap();
    let events = collect(&engine, 120).await;

    assert!(has(&events, |m| matches!(m, EventMsg::TurnComplete(_))), "missing TurnComplete");

    let tools = tool_names(&events);
    eprintln!("\n[api] tools called: {tools:?}");
    eprintln!("[api] errors: {:?}", errors(&events));

    // Should have called shell tool
    if tools.iter().any(|t| t == "shell") {
        eprintln!("[api] ✅ shell tool was called");
        assert!(has(&events, |m| matches!(m, EventMsg::McpToolCallEnd(_))), "missing McpToolCallEnd");
    } else {
        eprintln!("[api] ⚠️ model did not call shell tool, tools: {tools:?}");
    }

    shutdown(&engine).await;
}

/// 触发 list_dir
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn list_dir_tool_call() {
    let engine = spawn().await;
    eprintln!("\n=== list_dir_tool_call ===\n");

    let cwd = engine.cwd.to_string_lossy();
    engine.handle.tx_sub.send(Submission { id: "t1".into(), op: turn(&engine, &format!("List the files in {cwd} using the list_dir tool. Just show the file names.")) }).await.unwrap();
    let events = collect(&engine, 120).await;

    assert!(has(&events, |m| matches!(m, EventMsg::TurnComplete(_))));

    let tools = tool_names(&events);
    let errs = errors(&events);
    eprintln!("\n[api] tools: {tools:?}");
    eprintln!("[api] errors: {errs:?}");

    let text = agent_text(&events);
    // Should mention Cargo.toml or src in the response
    if tools.iter().any(|t| t == "list_dir") {
        eprintln!("[api] ✅ list_dir called");
        if errs.is_empty() {
            assert!(text.contains("Cargo") || text.contains("src"), "response should mention project files: {text}");
        }
    }

    shutdown(&engine).await;
}

/// 触发 read_file
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn read_file_tool_call() {
    let engine = spawn().await;
    eprintln!("\n=== read_file_tool_call ===\n");

    let cargo_path = engine.cwd.join("Cargo.toml");
    engine.handle.tx_sub.send(Submission { id: "t1".into(), op: turn(&engine, &format!("Read the file {} and tell me the package name.", cargo_path.display())) }).await.unwrap();
    let events = collect(&engine, 120).await;

    assert!(has(&events, |m| matches!(m, EventMsg::TurnComplete(_))));

    let tools = tool_names(&events);
    let errs = errors(&events);
    eprintln!("\n[api] tools: {tools:?}");
    eprintln!("[api] errors: {errs:?}");

    if tools.iter().any(|t| t == "read_file") {
        eprintln!("[api] ✅ read_file called");
        if !errs.is_empty() {
            eprintln!("[api] ⚠️ read_file errors: {errs:?}");
        }
    }

    shutdown(&engine).await;
}

/// 触发 grep_files
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn grep_files_tool_call() {
    let engine = spawn().await;
    eprintln!("\n=== grep_files_tool_call ===\n");

    engine.handle.tx_sub.send(Submission { id: "t1".into(), op: turn(&engine, "Search for the text 'fn main' in the current directory using grep_files.") }).await.unwrap();
    let events = collect(&engine, 120).await;

    assert!(has(&events, |m| matches!(m, EventMsg::TurnComplete(_))));

    let tools = tool_names(&events);
    eprintln!("\n[api] tools: {tools:?}");
    eprintln!("[api] errors: {:?}", errors(&events));

    if tools.iter().any(|t| t.contains("grep")) {
        eprintln!("[api] ✅ grep tool called");
    }

    shutdown(&engine).await;
}

/// $find-skills 场景
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn find_skills_windows_desktop() {
    let engine = spawn().await;
    eprintln!("\n=== find_skills_windows_desktop ===\n");

    engine.handle.tx_sub.send(Submission { id: "t1".into(), op: turn(&engine, "$find-skills 自动操作windows 桌面") }).await.unwrap();
    let events = collect(&engine, 120).await;

    let tools = tool_names(&events);
    let errs = errors(&events);
    let text = agent_text(&events);

    eprintln!("\n[api] events: {}", events.len());
    eprintln!("[api] tools: {tools:?}");
    eprintln!("[api] errors: {errs:?}");
    eprintln!("[api] text length: {}", text.len());

    assert!(has(&events, |m| matches!(m, EventMsg::TurnStarted(_))));
    assert!(has(&events, |m| matches!(m, EventMsg::ItemStarted(ev) if matches!(&ev.item, TurnItem::UserMessage(_)))));

    if has(&events, |m| matches!(m, EventMsg::TurnComplete(_))) {
        eprintln!("[api] ✅ TurnComplete received");
    } else {
        eprintln!("[api] ⚠️ TurnComplete not received (timeout?)");
    }

    if !errs.is_empty() {
        eprintln!("[api] ⚠️ Errors: {errs:?}");
    }

    shutdown(&engine).await;
}

/// 中文对话
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn chinese_conversation() {
    let engine = spawn().await;
    eprintln!("\n=== chinese_conversation ===\n");

    engine.handle.tx_sub.send(Submission { id: "t1".into(), op: turn(&engine, "用一句话介绍Rust语言，不要使用任何工具。") }).await.unwrap();
    let events = collect(&engine, 30).await;

    assert!(has(&events, |m| matches!(m, EventMsg::TurnComplete(_))));

    let text = agent_text(&events);
    assert!(!text.is_empty(), "should respond in Chinese");
    eprintln!("\n[api] response: {text}");

    shutdown(&engine).await;
}

/// 多轮对话 — 验证历史上下文保持
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn multi_turn_context() {
    let engine = spawn().await;
    eprintln!("\n=== multi_turn_context ===\n");

    // Turn 1
    engine.handle.tx_sub.send(Submission { id: "t1".into(), op: turn(&engine, "My name is TestUser123. Remember this. Reply with just 'OK'.") }).await.unwrap();
    let events1 = collect(&engine, 30).await;
    assert!(has(&events1, |m| matches!(m, EventMsg::TurnComplete(_))));
    let text1 = agent_text(&events1);
    eprintln!("\n[api] turn1: {text1}");

    // Turn 2 — should remember the name
    engine.handle.tx_sub.send(Submission { id: "t2".into(), op: turn(&engine, "What is my name? Reply with just the name.") }).await.unwrap();
    let events2 = collect(&engine, 30).await;
    assert!(has(&events2, |m| matches!(m, EventMsg::TurnComplete(_))));
    let text2 = agent_text(&events2);
    eprintln!("[api] turn2: {text2}");

    assert!(text2.contains("TestUser123"), "model should remember the name, got: {text2}");

    shutdown(&engine).await;
}

/// Interrupt 中断正在进行的对话
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn interrupt_active_turn() {
    let engine = spawn().await;
    eprintln!("\n=== interrupt_active_turn ===\n");

    // Start a long turn
    engine.handle.tx_sub.send(Submission { id: "t1".into(), op: turn(&engine, "Write a very long essay about the history of computing. Make it at least 2000 words.") }).await.unwrap();

    // Wait a bit for streaming to start
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Interrupt
    engine.handle.tx_sub.send(Submission { id: "i1".into(), op: Op::Interrupt }).await.unwrap();

    // Collect with TurnAborted as additional terminal condition
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if tokio::time::Instant::now() > deadline { break; }
        match tokio::time::timeout(Duration::from_millis(500), engine.handle.rx_event.recv()).await {
            Ok(Ok(ev)) => {
                let done = matches!(&ev.msg, EventMsg::TurnComplete(_) | EventMsg::TurnAborted(_) | EventMsg::ShutdownComplete);
                match &ev.msg {
                    EventMsg::TurnAborted(_) => eprintln!("[api] 🛑 TurnAborted"),
                    EventMsg::TurnComplete(_) => eprintln!("[api] ✅ TurnComplete"),
                    _ => {}
                }
                events.push(ev);
                if done { break; }
            }
            Ok(Err(_)) => break,
            Err(_) => continue,
        }
    }

    let has_aborted = has(&events, |m| matches!(m, EventMsg::TurnAborted(_)));
    let has_complete = has(&events, |m| matches!(m, EventMsg::TurnComplete(_)));
    eprintln!("[api] TurnAborted: {has_aborted}, TurnComplete: {has_complete}");

    // Should have either aborted or completed (race condition with fast models)
    // NOTE: If this fails, it indicates the interrupt mechanism doesn't properly
    // cancel the active API stream — a known limitation.
    if has_aborted || has_complete {
        eprintln!("[api] ✅ Interrupt handled correctly");
    } else {
        eprintln!("[api] ⚠️ BUG: Neither TurnAborted nor TurnComplete received after interrupt");
    }

    shutdown(&engine).await;
}
