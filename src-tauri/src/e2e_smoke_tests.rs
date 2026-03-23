//! End-to-end smoke tests: config → Codex engine → API.
//!
//! Run: cargo test --manifest-path src-tauri/Cargo.toml e2e_smoke -- --nocapture

#[cfg(test)]
mod e2e_smoke {
    use crate::config::{ConfigLayer, ConfigLayerStack};
    use crate::core::codex::Codex;
    use crate::protocol::event::EventMsg;
    use crate::protocol::submission::{Op, Submission};
    use crate::protocol::types::{AskForApproval, SandboxPolicy, UserInput};
    use std::path::PathBuf;
    use std::time::Duration;

    struct TestEngine {
        handle: crate::core::codex::CodexHandle,
        model: String,
        cwd: PathBuf,
    }

    /// Load config, stripping sections that cause parse errors.
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
                if let Ok(parsed) = crate::config::deserialize_toml(&cleaned.join("\n")) {
                    stack.add_layer(ConfigLayer::User, parsed);
                }
            }
        }
        stack
    }

    /// Spawn Codex engine, wait for session_configured. Panics if no config.
    async fn spawn_engine() -> TestEngine {
        let config = load_config();
        let merged = config.merge();
        let profile = merged.profile.clone().unwrap_or_default();
        let resolved = if profile.is_empty() {
            merged
        } else {
            config.resolve_with_profile(&profile)
        };
        let model = resolved.model.clone().unwrap_or_default();
        let provider = resolved.model_provider.clone().unwrap_or_default();
        eprintln!("[e2e] profile={profile}, model={model}, provider={provider}");

        assert!(!model.is_empty(), "model not configured in ~/.codex/config.toml");
        assert!(!provider.is_empty(), "provider not configured in ~/.codex/config.toml");

        let cwd = std::env::current_dir().unwrap();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(ConfigLayer::User, resolved);

        let handle = Codex::spawn(stack, cwd.clone()).await.expect("Codex::spawn");

        for _ in 0..50 {
            if let Ok(ev) = handle.rx_event.try_recv() {
                if matches!(ev.msg, EventMsg::SessionConfigured(_)) {
                    return TestEngine { handle, model, cwd };
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("session_configured not received");
    }

    /// Send an op and collect events until a matcher returns true or timeout.
    async fn send_and_collect(
        engine: &TestEngine,
        id: &str,
        op: Op,
        timeout_secs: u64,
        mut on_event: impl FnMut(&EventMsg) -> bool,
    ) -> Vec<String> {
        engine
            .handle
            .tx_sub
            .send(Submission { id: id.into(), op })
            .await
            .expect("send op");

        let mut types = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

        loop {
            if tokio::time::Instant::now() > deadline {
                break;
            }
            match tokio::time::timeout(Duration::from_millis(200), engine.handle.rx_event.recv())
                .await
            {
                Ok(Ok(ev)) => {
                    let done = on_event(&ev.msg);
                    let name = format!("{:?}", std::mem::discriminant(&ev.msg));
                    types.push(name);
                    if done {
                        break;
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => continue,
            }
        }
        types
    }

    async fn shutdown(engine: &TestEngine) {
        let _ = engine
            .handle
            .tx_sub
            .send(Submission {
                id: "shutdown".into(),
                op: Op::Shutdown,
            })
            .await;
    }

    // ── Test: skill discovery ────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_skills_returns_response() {
        let engine = spawn_engine().await;

        let mut skills: Vec<serde_json::Value> = Vec::new();
        let op = Op::ListSkills {
            cwds: vec![engine.cwd.clone()],
            force_reload: false,
        };

        send_and_collect(&engine, "list-skills", op, 10, |msg| {
            if let EventMsg::ListSkillsResponse(resp) = msg {
                skills = resp.skills.clone();
                eprintln!("[e2e] ListSkillsResponse: {} skills found", skills.len());
                for s in &skills {
                    eprintln!("[e2e]   skill: {}", s);
                }
                true
            } else {
                false
            }
        })
        .await;

        // Should receive the response (even if empty)
        // The important thing is the pipeline works without error
        eprintln!("[e2e] ✅ list_skills completed, {} skills", skills.len());
        assert!(!skills.is_empty(), "should discover at least one skill");

        shutdown(&engine).await;
    }

    // ── Test: skill invocation via user_turn ─────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn skill_in_user_turn_reaches_api() {
        let engine = spawn_engine().await;

        // First discover skills
        let mut skills: Vec<serde_json::Value> = Vec::new();
        send_and_collect(
            &engine,
            "discover",
            Op::ListSkills {
                cwds: vec![engine.cwd.clone()],
                force_reload: false,
            },
            10,
            |msg| {
                if let EventMsg::ListSkillsResponse(resp) = msg {
                    skills = resp.skills.clone();
                    true
                } else {
                    false
                }
            },
        )
        .await;

        // Build items: if we found a skill, include it; always include text
        let mut items: Vec<UserInput> = Vec::new();
        if let Some(skill) = skills.first() {
            let name = skill["name"].as_str().unwrap_or("unknown").to_string();
            // Skills are loaded from cwd, path is the skill directory
            let path = skill["path"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or_else(|| engine.cwd.clone());
            eprintln!("[e2e] using skill: {name}");
            items.push(UserInput::Skill { name, path });
        } else {
            eprintln!("[e2e] no skills found, testing plain user_turn");
        }
        items.push(UserInput::Text {
            text: "Say hello in one word.".into(),
            text_elements: vec![],
        });

        let model = engine.model.clone();
        let cwd = engine.cwd.clone();
        let mut agent_text = String::new();
        let mut event_names = Vec::new();

        send_and_collect(
            &engine,
            "skill-turn",
            Op::UserTurn {
                items,
                cwd,
                approval_policy: AskForApproval::Never,
                sandbox_policy: SandboxPolicy::DangerFullAccess,
                model,
                effort: None,
                summary: None,
                service_tier: None,
                final_output_json_schema: None,
                collaboration_mode: None,
                personality: None,
            },
            30,
            |msg| match msg {
                EventMsg::TurnStarted(_) => {
                    event_names.push("task_started".to_string());
                    eprintln!("[e2e] event: task_started");
                    false
                }
                EventMsg::AgentMessageDelta(d) => {
                    agent_text.push_str(&d.delta);
                    false
                }
                EventMsg::TurnComplete(tc) => {
                    if let Some(ref m) = tc.last_agent_message {
                        agent_text = m.clone();
                    }
                    event_names.push("task_complete".to_string());
                    eprintln!("[e2e] event: task_complete");
                    true
                }
                EventMsg::Error(e) => {
                    event_names.push("error".to_string());
                    eprintln!("[e2e] ERROR: {}", &e.message[..500.min(e.message.len())]);
                    true
                }
                EventMsg::StreamError(e) => {
                    event_names.push("stream_error".to_string());
                    eprintln!("[e2e] STREAM_ERROR: {}", e.message);
                    true
                }
                _ => false,
            },
        )
        .await;

        eprintln!("[e2e] events: {event_names:?}");
        eprintln!("[e2e] agent_text: {agent_text}");

        assert!(
            event_names.contains(&"task_started".to_string()),
            "missing task_started: {event_names:?}"
        );

        if event_names.contains(&"task_complete".to_string()) {
            assert!(!agent_text.is_empty(), "agent should have responded");
            eprintln!("[e2e] ✅ SUCCESS: AI responded: {agent_text}");
        } else {
            let reached_api = event_names
                .iter()
                .any(|t| t == "error" || t == "stream_error");
            assert!(reached_api, "no API response: {event_names:?}");
            eprintln!("[e2e] ⚠️ API returned error (transport verified)");
        }

        shutdown(&engine).await;
    }

    // ── Test: list custom prompts ────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_custom_prompts_returns_response() {
        let engine = spawn_engine().await;

        let mut prompts: Vec<serde_json::Value> = Vec::new();

        send_and_collect(&engine, "list-prompts", Op::ListCustomPrompts, 10, |msg| {
            if let EventMsg::ListCustomPromptsResponse(resp) = msg {
                prompts = resp.custom_prompts.clone();
                eprintln!(
                    "[e2e] ListCustomPromptsResponse: {} prompts",
                    prompts.len()
                );
                for p in &prompts {
                    eprintln!("[e2e]   prompt: {}", p);
                }
                true
            } else {
                false
            }
        })
        .await;

        eprintln!("[e2e] ✅ list_custom_prompts completed, {} prompts", prompts.len());

        shutdown(&engine).await;
    }
}
