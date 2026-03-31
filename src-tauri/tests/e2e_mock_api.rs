//! E2E integration tests using a mock SSE server.
//!
//! Exercises the REAL Codex engine by injecting a custom provider
//! whose base_url points to a local mock HTTP server.
//! Zero modifications to production code.

use std::collections::HashMap;
use tauri_app_lib::config::{ConfigLayer, ConfigLayerStack, ConfigToml};
use tauri_app_lib::core::codex::Codex;
use tauri_app_lib::protocol::event::{Event, EventMsg};
use tauri_app_lib::protocol::submission::{Op, Submission};
use tauri_app_lib::protocol::types::{AskForApproval, SandboxPolicy, UserInput};
use tauri_app_lib::provider::ModelProviderInfo;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ── Mock SSE Server ──────────────────────────────────────────────

struct MockServer {
    addr: std::net::SocketAddr,
    _handle: tokio::task::JoinHandle<()>,
}

impl MockServer {
    async fn start(sse_body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let body = sse_body.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 16384];
                    let _ = stream.read(&mut buf).await;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n{body}"
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Self {
            addr,
            _handle: handle,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// Start a multi-round mock server that returns different SSE for each request.
async fn start_multi_round_server(
    rounds: Vec<String>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let rounds = std::sync::Arc::new(std::sync::Mutex::new(rounds.into_iter()));
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let rounds = rounds.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16384];
                let _ = stream.read(&mut buf).await;
                let body = rounds.lock().unwrap().next().unwrap_or_default();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (addr, handle)
}

fn sse(event_type: &str, data: serde_json::Value) -> String {
    format!("event: {event_type}\ndata: {}\n\n", data)
}

// ── Helpers ──────────────────────────────────────────────────────

fn make_codex(
    base_url: &str,
) -> (
    async_channel::Sender<Submission>,
    async_channel::Receiver<Event>,
    Codex,
) {
    std::env::set_var("MOSAIC_TEST_API_KEY", "test-key");
    let (sq_tx, sq_rx) = async_channel::unbounded();
    let (eq_tx, eq_rx) = async_channel::unbounded();

    let mut providers = HashMap::new();
    providers.insert(
        "mock".to_string(),
        ModelProviderInfo {
            name: "Mock".into(),
            base_url: Some(base_url.into()),
            env_key: Some("MOSAIC_TEST_API_KEY".into()),
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: Default::default(),
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            supports_websockets: false,
            requires_openai_auth: false,
        },
    );

    let mut config = ConfigLayerStack::new();
    config.add_layer(
        ConfigLayer::Session,
        ConfigToml {
            model: Some("mock-model".into()),
            model_provider: Some("mock".into()),
            model_providers: providers,
            ..Default::default()
        },
    );

    let codex = Codex::new(sq_rx, eq_tx, config, std::env::current_dir().unwrap());
    (sq_tx, eq_rx, codex)
}

fn user_turn(text: &str) -> Op {
    Op::UserTurn {
        items: vec![UserInput::Text {
            text: text.into(),
            text_elements: vec![],
        }],
        cwd: std::env::current_dir().unwrap(),
        approval_policy: AskForApproval::Never,
        sandbox_policy: SandboxPolicy::new_read_only_policy(),
        model: "mock-model".into(),
        effort: None,
        summary: None,
        service_tier: None,
        final_output_json_schema: None,
        collaboration_mode: None,
        personality: None,
    }
}

fn drain(rx: &async_channel::Receiver<Event>) -> Vec<Event> {
    let mut out = vec![];
    while let Ok(ev) = rx.try_recv() {
        out.push(ev);
    }
    out
}

// ── V-01: Pure text response ─────────────────────────────────────

#[tokio::test]
async fn v01_text_response_emits_full_v2_sequence() {
    let body = format!(
        "{}{}{}",
        sse(
            "response.output_text.delta",
            serde_json::json!({"type":"response.output_text.delta","delta":"Hello "})
        ),
        sse(
            "response.output_text.delta",
            serde_json::json!({"type":"response.output_text.delta","delta":"world!"})
        ),
        sse(
            "response.completed",
            serde_json::json!({"type":"response.completed","response":{"id":"r1","output":[],"usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}})
        ),
    );
    let server = MockServer::start(body).await;
    let (sq_tx, eq_rx, codex) = make_codex(&server.base_url());

    sq_tx
        .send(Submission {
            id: "t1".into(),
            op: user_turn("hi"),
        })
        .await
        .unwrap();
    sq_tx
        .send(Submission {
            id: "s1".into(),
            op: Op::Shutdown,
        })
        .await
        .unwrap();
    codex.run().await.unwrap();
    let events = drain(&eq_rx);

    // v2 structured events
    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::ItemStarted(ev) if matches!(&ev.item, tauri_app_lib::protocol::items::TurnItem::UserMessage(_)))));
    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::ItemCompleted(ev) if matches!(&ev.item, tauri_app_lib::protocol::items::TurnItem::UserMessage(_)))));
    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::ItemStarted(ev) if matches!(&ev.item, tauri_app_lib::protocol::items::TurnItem::AgentMessage(_)))));

    let deltas: Vec<&str> = events
        .iter()
        .filter_map(|e| match &e.msg {
            EventMsg::AgentMessageContentDelta(d) => Some(d.delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec!["Hello ", "world!"]);

    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::ItemCompleted(ev) if matches!(&ev.item, tauri_app_lib::protocol::items::TurnItem::AgentMessage(_)))));
    assert!(events
        .iter()
        .any(|e| matches!(&e.msg, EventMsg::TurnStarted(_))));
    assert!(events
        .iter()
        .any(|e| matches!(&e.msg, EventMsg::TurnComplete(_))));
}

// ── V-02: Tool call ──────────────────────────────────────────────

#[tokio::test]
async fn v02_tool_call_emits_mcp_events() {
    let round1 = format!(
        "{}{}",
        sse(
            "response.output_item.done",
            serde_json::json!({"type":"response.output_item.done","item":{"type":"function_call","id":"fc1","call_id":"c1","name":"shell","arguments":"{\"command\":[\"echo\",\"test\"]}"}})
        ),
        sse(
            "response.completed",
            serde_json::json!({"type":"response.completed","response":{"id":"r1","output":[],"usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}})
        ),
    );
    let round2 = format!(
        "{}{}",
        sse(
            "response.output_text.delta",
            serde_json::json!({"type":"response.output_text.delta","delta":"Done!"})
        ),
        sse(
            "response.completed",
            serde_json::json!({"type":"response.completed","response":{"id":"r2","output":[],"usage":{"input_tokens":20,"output_tokens":10,"total_tokens":30}}})
        ),
    );

    let (addr, handle) = start_multi_round_server(vec![round1, round2]).await;
    let base_url = format!("http://{}", addr);
    let (sq_tx, eq_rx, codex) = make_codex(&base_url);

    sq_tx
        .send(Submission {
            id: "t1".into(),
            op: user_turn("run echo"),
        })
        .await
        .unwrap();
    sq_tx
        .send(Submission {
            id: "s1".into(),
            op: Op::Shutdown,
        })
        .await
        .unwrap();
    codex.run().await.unwrap();
    handle.abort();
    let events = drain(&eq_rx);

    let begin_pos = events
        .iter()
        .position(|e| matches!(&e.msg, EventMsg::McpToolCallBegin(_)));
    let end_pos = events
        .iter()
        .position(|e| matches!(&e.msg, EventMsg::McpToolCallEnd(_)));
    assert!(begin_pos.is_some(), "missing McpToolCallBegin");
    assert!(end_pos.is_some(), "missing McpToolCallEnd");
    assert!(begin_pos.unwrap() < end_pos.unwrap());

    let deltas: Vec<&str> = events
        .iter()
        .filter_map(|e| match &e.msg {
            EventMsg::AgentMessageContentDelta(d) => Some(d.delta.as_str()),
            _ => None,
        })
        .collect();
    assert!(deltas.contains(&"Done!"));
}

// ── V-03: Reasoning + text ────────────────────────────────────────

#[tokio::test]
async fn v03_reasoning_then_text() {
    let body = format!(
        "{}{}{}{}",
        sse(
            "response.reasoning_summary_text.delta",
            serde_json::json!({"type":"response.reasoning_summary_text.delta","delta":"thinking...","summary_index":0})
        ),
        sse(
            "response.reasoning_summary_text.delta",
            serde_json::json!({"type":"response.reasoning_summary_text.delta","delta":" more","summary_index":0})
        ),
        sse(
            "response.output_text.delta",
            serde_json::json!({"type":"response.output_text.delta","delta":"Answer: 42"})
        ),
        sse(
            "response.completed",
            serde_json::json!({"type":"response.completed","response":{"id":"r1","output":[],"usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}})
        ),
    );
    let server = MockServer::start(body).await;
    let (sq_tx, eq_rx, codex) = make_codex(&server.base_url());

    sq_tx
        .send(Submission {
            id: "t1".into(),
            op: user_turn("think"),
        })
        .await
        .unwrap();
    sq_tx
        .send(Submission {
            id: "s1".into(),
            op: Op::Shutdown,
        })
        .await
        .unwrap();
    codex.run().await.unwrap();
    let events = drain(&eq_rx);

    // Reasoning events
    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::ItemStarted(ev) if matches!(&ev.item, tauri_app_lib::protocol::items::TurnItem::Reasoning(_)))));
    let reasoning_deltas: Vec<&str> = events
        .iter()
        .filter_map(|e| match &e.msg {
            EventMsg::ReasoningContentDelta(d) => Some(d.delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(reasoning_deltas, vec!["thinking...", " more"]);

    let reasoning_completed = events.iter().find_map(|e| match &e.msg {
        EventMsg::ItemCompleted(ev) => match &ev.item {
            tauri_app_lib::protocol::items::TurnItem::Reasoning(r) => Some(r),
            _ => None,
        },
        _ => None,
    });
    assert!(reasoning_completed.is_some());
    assert_eq!(
        reasoning_completed.unwrap().summary_text,
        vec!["thinking... more"]
    );

    // Agent message events
    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::ItemStarted(ev) if matches!(&ev.item, tauri_app_lib::protocol::items::TurnItem::AgentMessage(_)))));
    let agent_deltas: Vec<&str> = events
        .iter()
        .filter_map(|e| match &e.msg {
            EventMsg::AgentMessageContentDelta(d) => Some(d.delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(agent_deltas, vec!["Answer: 42"]);
}

// ── V-04: Empty response ─────────────────────────────────────────

#[tokio::test]
async fn v04_empty_response_no_agent_message() {
    let body = sse(
        "response.completed",
        serde_json::json!({"type":"response.completed","response":{"id":"r1","output":[],"usage":{"input_tokens":5,"output_tokens":0,"total_tokens":5}}}),
    );
    let server = MockServer::start(body).await;
    let (sq_tx, eq_rx, codex) = make_codex(&server.base_url());

    sq_tx
        .send(Submission {
            id: "t1".into(),
            op: user_turn("hi"),
        })
        .await
        .unwrap();
    sq_tx
        .send(Submission {
            id: "s1".into(),
            op: Op::Shutdown,
        })
        .await
        .unwrap();
    codex.run().await.unwrap();
    let events = drain(&eq_rx);

    assert!(!events.iter().any(|e| matches!(&e.msg, EventMsg::ItemCompleted(ev) if matches!(&ev.item, tauri_app_lib::protocol::items::TurnItem::AgentMessage(_)))));
    assert!(events
        .iter()
        .any(|e| matches!(&e.msg, EventMsg::TurnComplete(_))));
}

// ── V-05: API error ──────────────────────────────────────────────

#[tokio::test]
async fn v05_api_error_emits_error_and_completes() {
    let body = sse(
        "error",
        serde_json::json!({"type":"error","code":"rate_limit","message":"Too many requests"}),
    );
    let server = MockServer::start(body).await;
    let (sq_tx, eq_rx, codex) = make_codex(&server.base_url());

    sq_tx
        .send(Submission {
            id: "t1".into(),
            op: user_turn("hi"),
        })
        .await
        .unwrap();
    sq_tx
        .send(Submission {
            id: "s1".into(),
            op: Op::Shutdown,
        })
        .await
        .unwrap();
    codex.run().await.unwrap();
    let events = drain(&eq_rx);

    assert!(events.iter().any(|e| matches!(&e.msg, EventMsg::Error(_))));
    assert!(events
        .iter()
        .any(|e| matches!(&e.msg, EventMsg::TurnComplete(_))));
}

// ── V-06: Token usage ────────────────────────────────────────────

#[tokio::test]
async fn v06_token_usage_emitted() {
    let body = format!(
        "{}{}",
        sse(
            "response.output_text.delta",
            serde_json::json!({"type":"response.output_text.delta","delta":"hi"})
        ),
        sse(
            "response.completed",
            serde_json::json!({"type":"response.completed","response":{"id":"r1","output":[],"usage":{"input_tokens":100,"output_tokens":50,"total_tokens":150}}})
        ),
    );
    let server = MockServer::start(body).await;
    let (sq_tx, eq_rx, codex) = make_codex(&server.base_url());

    sq_tx
        .send(Submission {
            id: "t1".into(),
            op: user_turn("hi"),
        })
        .await
        .unwrap();
    sq_tx
        .send(Submission {
            id: "s1".into(),
            op: Op::Shutdown,
        })
        .await
        .unwrap();
    codex.run().await.unwrap();
    let events = drain(&eq_rx);

    let tc = events.iter().find_map(|e| match &e.msg {
        EventMsg::TokenCount(t) => Some(t),
        _ => None,
    });
    assert!(tc.is_some());
    let info = tc.unwrap().info.as_ref().unwrap();
    assert_eq!(info.total_token_usage.input_tokens, 100);
    assert_eq!(info.total_token_usage.output_tokens, 50);
}

// ── V-07: No legacy events ──────────────────────────────────────

#[tokio::test]
async fn v07_no_legacy_events() {
    let body = format!(
        "{}{}",
        sse(
            "response.output_text.delta",
            serde_json::json!({"type":"response.output_text.delta","delta":"text"})
        ),
        sse(
            "response.completed",
            serde_json::json!({"type":"response.completed","response":{"id":"r1","output":[],"usage":{"input_tokens":5,"output_tokens":2,"total_tokens":7}}})
        ),
    );
    let server = MockServer::start(body).await;
    let (sq_tx, eq_rx, codex) = make_codex(&server.base_url());

    sq_tx
        .send(Submission {
            id: "t1".into(),
            op: user_turn("hi"),
        })
        .await
        .unwrap();
    sq_tx
        .send(Submission {
            id: "s1".into(),
            op: Op::Shutdown,
        })
        .await
        .unwrap();
    codex.run().await.unwrap();
    let events = drain(&eq_rx);

    assert!(!events
        .iter()
        .any(|e| matches!(&e.msg, EventMsg::AgentMessageDelta(_))));
    assert!(!events
        .iter()
        .any(|e| matches!(&e.msg, EventMsg::AgentMessage(_))));
    assert!(!events
        .iter()
        .any(|e| matches!(&e.msg, EventMsg::UserMessage(_))));
    assert!(!events
        .iter()
        .any(|e| matches!(&e.msg, EventMsg::AgentReasoningDelta(_))));
}
