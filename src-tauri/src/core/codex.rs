use std::path::PathBuf;
use std::sync::Arc;

use async_channel::{Receiver, Sender};
use tokio::sync::Mutex;

use crate::config::ConfigLayerStack;
use crate::protocol::error::CodexError;
use crate::protocol::event::{Event, EventMsg, SessionConfiguredEvent};
use crate::protocol::submission::{Op, Submission};
use crate::protocol::types::TurnContextOverrides;

use super::session::Session;
use super::skills::SkillLoader;
use super::tools::handlers::dynamic::DynamicToolHandler;

/// Handle returned by `Codex::spawn`, giving the caller access to the SQ/EQ channels.
pub struct CodexHandle {
    /// Send submissions into the core engine.
    pub tx_sub: Sender<Submission>,
    /// Receive events from the core engine.
    pub rx_event: Receiver<Event>,
}

/// Core engine that processes the SQ/EQ loop.
pub struct Codex {
    /// Submission queue receiver (client → engine).
    sq_rx: Receiver<Submission>,
    /// Event queue sender (engine → client).
    eq_tx: Sender<Event>,
    /// Active session.
    session: Arc<Mutex<Option<Session>>>,
    /// Configuration stack (kept for session creation).
    config: ConfigLayerStack,
    /// Working directory.
    cwd: PathBuf,
    /// Whether the engine is running.
    running: Arc<Mutex<bool>>,
    /// Skill loader.
    skill_loader: Arc<Mutex<SkillLoader>>,
    /// Dynamic tool handler for managing dynamic tool lifecycle.
    dynamic_tool_handler: Arc<Mutex<DynamicToolHandler>>,
}

impl Codex {
    pub fn new(
        sq_rx: Receiver<Submission>,
        eq_tx: Sender<Event>,
        config: ConfigLayerStack,
        cwd: PathBuf,
    ) -> Self {
        let skill_loader = SkillLoader::new(vec![cwd.join(".codex/skills")]);
        let dynamic_tool_handler = DynamicToolHandler::new(eq_tx.clone());
        Self {
            sq_rx,
            eq_tx,
            session: Arc::new(Mutex::new(None)),
            config,
            cwd,
            running: Arc::new(Mutex::new(false)),
            skill_loader: Arc::new(Mutex::new(skill_loader)),
            dynamic_tool_handler: Arc::new(Mutex::new(dynamic_tool_handler)),
        }
    }

    /// Spawn a new Codex engine, returning a handle with SQ/EQ channels.
    ///
    /// The engine runs its submission_loop on a background Tokio task.
    /// The caller communicates via the returned `CodexHandle`.
    pub async fn spawn(config: ConfigLayerStack, cwd: PathBuf) -> Result<CodexHandle, CodexError> {
        let (sq_tx, sq_rx) = async_channel::unbounded();
        let (eq_tx, eq_rx) = async_channel::unbounded();

        let codex = Self::new(sq_rx, eq_tx, config, cwd);

        tokio::spawn(async move {
            if let Err(e) = codex.run().await {
                // Engine crashed — try to emit a final error event if possible.
                let _ = codex
                    .emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                        message: format!("engine crashed: {}", e.message),
                        codex_error_info: None,
                    }))
                    .await;
            }
        });

        Ok(CodexHandle {
            tx_sub: sq_tx,
            rx_event: eq_rx,
        })
    }

    /// Start the SQ/EQ processing loop.
    pub async fn run(&self) -> Result<(), CodexError> {
        {
            let mut running = self.running.lock().await;
            *running = true;
        }

        // Emit SessionConfigured on startup
        let merged_config = self.config.merge();
        let mut session = Session::new(self.cwd.clone(), self.config.clone(), self.eq_tx.clone());
        // Apply the profile from config so TurnContext picks up model_provider etc.
        session.set_active_profile(merged_config.profile.clone());
        let session_id = session.id().to_string();

        self.emit(EventMsg::SessionConfigured(SessionConfiguredEvent {
            session_id: session_id.clone(),
            forked_from_id: None,
            thread_name: None,
            model: merged_config
                .model
                .clone()
                .unwrap_or_else(|| "default".into()),
            model_provider_id: merged_config.model_provider.clone().unwrap_or_default(),
            approval_policy: merged_config.approval_policy.clone(),
            sandbox_policy: None,
            cwd: self.cwd.clone(),
            history_log_id: 0,
            history_entry_count: 0,
        }))
        .await;

        {
            let mut s = self.session.lock().await;
            *s = Some(session);
        }

        // Notify hooks of session start
        {
            let session_guard = self.session.lock().await;
            if let Some(s) = session_guard.as_ref() {
                s.hooks().await.notify_session_start(s.id());
            }
        }

        // Main loop: process submissions
        while let Ok(submission) = self.sq_rx.recv().await {
            let is_shutdown = matches!(&submission.op, Op::Shutdown);
            if let Err(e) = self.handle_op(submission.id, submission.op).await {
                self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                    message: e.message,
                    codex_error_info: None,
                }))
                .await;
            }
            if is_shutdown || !*self.running.lock().await {
                break;
            }
        }

        Ok(())
    }

    /// Stop the engine.
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
    }

    /// Dispatch a single Op.
    async fn handle_op(&self, _id: String, op: Op) -> Result<(), CodexError> {
        match op {
            Op::Interrupt => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    s.interrupt().await;
                    self.emit(EventMsg::TurnAborted(
                        crate::protocol::event::TurnAbortedEvent {
                            turn_id: None,
                            reason: crate::protocol::types::TurnAbortReason::Interrupted,
                        },
                    ))
                    .await;
                }
            }
            Op::Shutdown => {
                // Notify hooks of session end
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    s.hooks().await.notify_session_end(s.id());
                }
                drop(session_guard);
                self.stop().await;
                self.emit(EventMsg::ShutdownComplete).await;
            }
            Op::UserTurn {
                items,
                cwd,
                approval_policy,
                sandbox_policy,
                model,
                effort: _effort,
                summary: _summary,
                service_tier: _service_tier,
                collaboration_mode,
                personality,
                ..
            } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    let turn_id = s.start_turn().await?;

                    // Apply submitted parameters as overrides on the TurnContext
                    let overrides = TurnContextOverrides {
                        model: Some(model),
                        sandbox_policy: Some(sandbox_policy),
                        approval_policy: Some(approval_policy),
                        cwd: Some(cwd),
                        collaboration_mode,
                        personality,
                    };
                    let _ = s.apply_turn_context_overrides(&overrides).await;

                    self.run_turn(s, &turn_id, items).await;
                }
            }
            Op::UserInput { items, .. } => {
                // Legacy path — simplified turn without overrides
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    let turn_id = s.start_turn().await?;
                    self.run_turn(s, &turn_id, items).await;
                }
            }
            Op::ExecApproval { decision, .. } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    // Abort decision interrupts the current turn
                    if matches!(decision, crate::protocol::types::ReviewDecision::Abort) {
                        s.interrupt().await;
                        self.emit(EventMsg::TurnAborted(
                            crate::protocol::event::TurnAbortedEvent {
                                turn_id: None,
                                reason: crate::protocol::types::TurnAbortReason::Interrupted,
                            },
                        ))
                        .await;
                    } else if let Some(pending) = s.take_pending_approval().await {
                        let approved = matches!(
                            decision,
                            crate::protocol::types::ReviewDecision::Approved
                                | crate::protocol::types::ReviewDecision::ApprovedForSession
                                | crate::protocol::types::ReviewDecision::ApprovedExecpolicyAmendment { .. }
                        );
                        if let super::session::PendingApproval::Exec { call_id, .. } = &pending {
                            if approved {
                                // TODO: execute the approved command via sandbox
                                let _ = call_id;
                            }
                            // Declined: the pending approval is consumed, turn continues
                        }
                    }
                }
            }
            Op::PatchApproval { decision, .. } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    if matches!(decision, crate::protocol::types::ReviewDecision::Abort) {
                        s.interrupt().await;
                        self.emit(EventMsg::TurnAborted(
                            crate::protocol::event::TurnAbortedEvent {
                                turn_id: None,
                                reason: crate::protocol::types::TurnAbortReason::Interrupted,
                            },
                        ))
                        .await;
                    } else if let Some(pending) = s.take_pending_approval().await {
                        let approved = matches!(
                            decision,
                            crate::protocol::types::ReviewDecision::Approved
                                | crate::protocol::types::ReviewDecision::ApprovedForSession
                        );
                        if let super::session::PendingApproval::Patch { call_id, .. } = &pending {
                            if approved {
                                // TODO: apply the approved patch
                                let _ = call_id;
                            }
                        }
                    }
                }
            }
            Op::ResolveElicitation { .. } => {
                // TODO: forward elicitation decision to MCP manager
            }
            Op::OverrideTurnContext {
                model,
                cwd,
                approval_policy,
                sandbox_policy,
                collaboration_mode,
                personality,
                ..
            } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    let overrides = TurnContextOverrides {
                        model,
                        sandbox_policy,
                        approval_policy,
                        cwd,
                        collaboration_mode,
                        personality,
                    };
                    let _ = s.apply_turn_context_overrides(&overrides).await;
                }
            }
            Op::AddToHistory { text } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    s.add_to_history(vec![crate::protocol::types::ResponseInputItem::Message {
                        role: "user".into(),
                        content: text,
                    }])
                    .await;
                }
            }
            Op::GetHistoryEntryRequest { offset, log_id } => {
                let session_guard = self.session.lock().await;
                let entry = if let Some(s) = session_guard.as_ref() {
                    s.history()
                        .await
                        .get(offset)
                        .and_then(|item| serde_json::to_value(item).ok())
                } else {
                    None
                };
                self.emit(EventMsg::GetHistoryEntryResponse(
                    crate::protocol::event::GetHistoryEntryResponseEvent {
                        offset,
                        log_id,
                        entry,
                    },
                ))
                .await;
            }
            Op::SetThreadName { name } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    self.emit(EventMsg::ThreadNameUpdated(
                        crate::protocol::event::ThreadNameUpdatedEvent {
                            thread_id: s.id().to_string(),
                            thread_name: Some(name),
                        },
                    ))
                    .await;
                }
            }
            Op::ListModels => {
                // TODO: return actual model list from config/models.json
                // For now emit an empty list so consumers don't hang waiting
                self.emit(EventMsg::ListCustomPromptsResponse(
                    crate::protocol::event::ListCustomPromptsResponseEvent {
                        custom_prompts: vec![],
                    },
                ))
                .await;
            }
            Op::Review { review_request } => {
                self.emit(EventMsg::EnteredReviewMode(review_request)).await;
            }
            Op::Compact => {
                // TODO: invoke context compaction
                self.emit(EventMsg::ContextCompacted(
                    crate::protocol::event::ContextCompactedEvent,
                ))
                .await;
            }
            Op::Undo => {
                self.emit(EventMsg::UndoStarted(
                    crate::protocol::event::UndoStartedEvent { message: None },
                ))
                .await;
                // TODO: actual undo logic (file rollback)
                self.emit(EventMsg::UndoCompleted(
                    crate::protocol::event::UndoCompletedEvent {
                        success: true,
                        message: None,
                    },
                ))
                .await;
            }
            Op::ThreadRollback { num_turns } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    s.rollback(num_turns as usize).await?;
                    self.emit(EventMsg::ThreadRolledBack(
                        crate::protocol::event::ThreadRolledBackEvent { num_turns },
                    ))
                    .await;
                }
            }
            Op::ReloadUserConfig => {
                // TODO: reload config from disk and update self.config
            }
            Op::ListMcpTools => {
                let session_guard = self.session.lock().await;
                let tools = if let Some(s) = session_guard.as_ref() {
                    let servers = s.mcp_manager().connected_servers().await;
                    servers
                        .into_iter()
                        .map(|name| (name, serde_json::Value::Array(vec![])))
                        .collect()
                } else {
                    std::collections::HashMap::new()
                };
                self.emit(EventMsg::McpListToolsResponse(
                    crate::protocol::event::McpListToolsResponseEvent { tools },
                ))
                .await;
            }
            Op::ListSkills { .. } => {
                let mut loader = self.skill_loader.lock().await;
                let _ = loader.load_all().await;
                let skills = loader
                    .loaded_skills()
                    .iter()
                    .filter_map(|s| {
                        serde_json::to_value(serde_json::json!({
                            "name": s.name,
                            "description": s.description,
                        }))
                        .ok()
                    })
                    .collect();
                self.emit(EventMsg::ListSkillsResponse(
                    crate::protocol::event::ListSkillsResponseEvent { skills },
                ))
                .await;
            }
            Op::ListCustomPrompts => {
                self.emit(EventMsg::ListCustomPromptsResponse(
                    crate::protocol::event::ListCustomPromptsResponseEvent {
                        custom_prompts: vec![],
                    },
                ))
                .await;
            }
            Op::RealtimeConversationStart(params) => {
                self.emit(EventMsg::RealtimeConversationStarted(
                    crate::protocol::event::RealtimeConversationStartedEvent {
                        session_id: params.session_id,
                    },
                ))
                .await;
            }
            Op::RealtimeConversationClose => {
                self.emit(EventMsg::RealtimeConversationClosed(
                    crate::protocol::event::RealtimeConversationClosedEvent { reason: None },
                ))
                .await;
            }
            Op::DynamicToolResponse { id, response } => {
                let handler = self.dynamic_tool_handler.lock().await;
                if let Err(e) = handler.resolve_call(&id, response).await {
                    self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                        message: format!(
                            "failed to resolve dynamic tool call '{id}': {}",
                            e.message
                        ),
                        codex_error_info: None,
                    }))
                    .await;
                }
            }
            Op::RefreshMcpServers { .. } => {
                // Delegate to MCP manager — trigger reconnection of all configured servers
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    let servers = s.mcp_manager().connected_servers().await;
                    self.emit(EventMsg::McpStartupComplete(
                        crate::protocol::event::McpStartupCompleteEvent {
                            ready: servers,
                            failed: vec![],
                            cancelled: vec![],
                        },
                    ))
                    .await;
                }
            }
            Op::CleanBackgroundTerminals => {
                // Clean up any background terminal processes
                // Currently a no-op as terminal management is not yet implemented
            }
            Op::UserInputAnswer { .. }
            | Op::ListRemoteSkills { .. }
            | Op::DownloadRemoteSkill { .. }
            | Op::RealtimeConversationAudio(_)
            | Op::RealtimeConversationText(_)
            | Op::DropMemories
            | Op::UpdateMemories
            | Op::RunUserShellCommand { .. } => {
                // TODO: implement these ops
            }
        }
        Ok(())
    }

    /// Execute a full turn: emit bracket events, store user input, dispatch tool calls.
    ///
    /// This is the core turn execution logic shared by `UserTurn` and `UserInput` ops.
    /// The sequence is: TurnStarted → user input stored → model interaction (tool calls) → TurnComplete.
    async fn run_turn(
        &self,
        session: &Session,
        turn_id: &str,
        items: Vec<crate::protocol::types::UserInput>,
    ) {
        // Notify hooks of turn start
        session.hooks().await.notify_turn_start(turn_id);

        let mode_kind = session
            .turn_context()
            .await
            .and_then(|ctx| ctx.collaboration_mode.as_ref().map(|m| m.mode))
            .unwrap_or(crate::protocol::types::ModeKind::Default);

        // Emit TurnStarted bracket event
        self.emit(EventMsg::TurnStarted(
            crate::protocol::event::TurnStartedEvent {
                turn_id: turn_id.to_string(),
                model_context_window: session
                    .turn_context()
                    .await
                    .and_then(|ctx| ctx.model_info.context_window),
                collaboration_mode_kind: mode_kind,
            },
        ))
        .await;

        // Store user input in history
        let user_text: String = items
            .iter()
            .filter_map(|item| match item {
                crate::protocol::types::UserInput::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !user_text.is_empty() {
            session
                .add_to_history(vec![crate::protocol::types::ResponseInputItem::Message {
                    role: "user".into(),
                    content: user_text,
                }])
                .await;
        }

        // Resolve model and provider from turn context + config
        let (model, base_url, api_key, extra_headers) = {
            let ctx = session.turn_context().await;
            let base_merged = self.config.merge();
            // Apply profile if configured (e.g. profile = "azure")
            let merged = if let Some(ref profile) = base_merged.profile {
                self.config.resolve_with_profile(profile)
            } else {
                base_merged
            };

            eprintln!("[run_turn] profile={:?}", merged.profile);
            eprintln!("[run_turn] model={:?}", merged.model);
            eprintln!("[run_turn] model_provider={:?}", merged.model_provider);
            eprintln!(
                "[run_turn] model_providers keys={:?}",
                merged.model_providers.keys().collect::<Vec<_>>()
            );

            let model = merged
                .model
                .clone()
                .or_else(|| {
                    ctx.as_ref()
                        .map(|c| c.model_info.model.clone())
                        .filter(|s| s != "default" && !s.is_empty())
                })
                .unwrap_or_else(|| "gpt-4o".into());

            let provider_id = merged
                .model_provider
                .clone()
                .or_else(|| {
                    ctx.as_ref()
                        .map(|c| c.model_info.provider.clone())
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or_default();

            eprintln!(
                "[run_turn] provider_id={:?} ctx_provider={:?}",
                provider_id,
                ctx.as_ref().map(|c| &c.model_info.provider)
            );
            let provider_info =
                crate::provider::resolve_provider(&provider_id, &merged.model_providers)
                    .ok()
                    .flatten()
                    .or_else(|| {
                        crate::provider::resolve_provider("openai", &merged.model_providers)
                            .ok()
                            .flatten()
                    });

            match provider_info {
                Some(info) => {
                    eprintln!(
                        "[run_turn] resolved provider: name={:?} base_url={:?} env_key={:?}",
                        info.name, info.base_url, info.env_key
                    );
                    let api_key = match info.api_key() {
                        Ok(Some(k)) => k,
                        Ok(None) => {
                            eprintln!(
                                "[run_turn] api_key() returned None — env_key={:?}",
                                info.env_key
                            );
                            self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                                message: format!(
                                    "No API key configured for provider '{provider_id}'"
                                ),
                                codex_error_info: None,
                            }))
                            .await;
                            session.hooks().await.notify_turn_complete(turn_id);
                            session.complete_turn().await;
                            self.emit(EventMsg::TurnComplete(
                                crate::protocol::event::TurnCompleteEvent {
                                    turn_id: turn_id.to_string(),
                                    last_agent_message: None,
                                },
                            ))
                            .await;
                            return;
                        }
                        Err(e) => {
                            self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                                message: e.message,
                                codex_error_info: None,
                            }))
                            .await;
                            session.hooks().await.notify_turn_complete(turn_id);
                            session.complete_turn().await;
                            self.emit(EventMsg::TurnComplete(
                                crate::protocol::event::TurnCompleteEvent {
                                    turn_id: turn_id.to_string(),
                                    last_agent_message: None,
                                },
                            ))
                            .await;
                            return;
                        }
                    };
                    let provider = info.to_provider();
                    let base_url = provider.url_for_path("responses");
                    let headers = info.resolved_headers();
                    (model, base_url, api_key, headers)
                }
                None => {
                    self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                        message: format!("Unknown provider '{provider_id}'. Configure model_provider in settings."),
                        codex_error_info: None,
                    }))
                    .await;
                    session.hooks().await.notify_turn_complete(turn_id);
                    session.complete_turn().await;
                    self.emit(EventMsg::TurnComplete(
                        crate::protocol::event::TurnCompleteEvent {
                            turn_id: turn_id.to_string(),
                            last_agent_message: None,
                        },
                    ))
                    .await;
                    return;
                }
            }
        };

        // Build API input from history
        let history_items = session.history().await;
        let api_input: Vec<serde_json::Value> = history_items
            .iter()
            .map(crate::core::client::history_item_to_api)
            .collect();

        // Call the Responses API and stream events
        let mut last_agent_message: Option<String> = None;
        let mut accumulated_text = String::new();

        match crate::core::client::stream_response(
            &base_url,
            &api_key,
            &extra_headers,
            &model,
            None, // instructions
            api_input,
            None, // previous_response_id
        )
        .await
        {
            Err(e) => {
                self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                    message: format!("Failed to start API stream: {}", e.message),
                    codex_error_info: None,
                }))
                .await;
            }
            Ok(mut stream) => {
                use futures::StreamExt;
                eprintln!("[run_turn] stream started, waiting for events");
                while let Some(event_result) = stream.next().await {
                    eprintln!("[run_turn] got event: {:?}", event_result.as_ref().map(|e| std::mem::discriminant(e)));
                    match event_result {
                        Err(e) => {
                            self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                                message: format!("Stream error: {}", e.message),
                                codex_error_info: None,
                            }))
                            .await;
                            break;
                        }
                        Ok(crate::core::client::ResponseEvent::OutputTextDelta { delta }) => {
                            if !delta.is_empty() {
                                accumulated_text.push_str(&delta);
                                self.emit(EventMsg::AgentMessageDelta(
                                    crate::protocol::event::AgentMessageDeltaEvent {
                                        delta: delta.clone(),
                                    },
                                ))
                                .await;
                            }
                        }
                        Ok(crate::core::client::ResponseEvent::OutputItemDone { item }) => {
                            // Extract assistant message text from completed items
                            if item.get("type").and_then(|t| t.as_str()) == Some("message")
                                && item.get("role").and_then(|r| r.as_str()) == Some("assistant")
                            {
                                let text: String = item
                                    .get("content")
                                    .and_then(|c| c.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|c| {
                                                if c.get("type").and_then(|t| t.as_str())
                                                    == Some("output_text")
                                                {
                                                    c.get("text")
                                                        .and_then(|t| t.as_str())
                                                        .map(str::to_string)
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect::<Vec<_>>()
                                            .join("")
                                    })
                                    .unwrap_or_default();

                                if !text.is_empty() {
                                    last_agent_message = Some(text.clone());
                                    // Store assistant response in history
                                    session
                                        .add_to_history(vec![
                                            crate::protocol::types::ResponseInputItem::Message {
                                                role: "assistant".into(),
                                                content: text.clone(),
                                            },
                                        ])
                                        .await;
                                    // Emit full AgentMessage if we didn't stream deltas
                                    if accumulated_text.is_empty() {
                                        self.emit(EventMsg::AgentMessage(
                                            crate::protocol::event::AgentMessageEvent {
                                                message: text,
                                            },
                                        ))
                                        .await;
                                    }
                                }
                            }
                        }
                        Ok(crate::core::client::ResponseEvent::Done { .. }) => {
                            // If we accumulated delta text, emit the full message now
                            if !accumulated_text.is_empty() && last_agent_message.is_none() {
                                last_agent_message = Some(accumulated_text.clone());
                                session
                                    .add_to_history(vec![
                                        crate::protocol::types::ResponseInputItem::Message {
                                            role: "assistant".into(),
                                            content: accumulated_text.clone(),
                                        },
                                    ])
                                    .await;
                                self.emit(EventMsg::AgentMessage(
                                    crate::protocol::event::AgentMessageEvent {
                                        message: accumulated_text.clone(),
                                    },
                                ))
                                .await;
                            }
                            break;
                        }
                        Ok(crate::core::client::ResponseEvent::Skip) => {}
                        Ok(crate::core::client::ResponseEvent::Failed { code, message }) => {
                            self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                                message: format!("API error [{code}]: {message}"),
                                codex_error_info: None,
                            }))
                            .await;
                            break;
                        }
                    }
                }
            }
        }

        // Notify hooks and complete the turn
        session.hooks().await.notify_turn_complete(turn_id);
        session.complete_turn().await;

        // Emit TurnComplete bracket event
        self.emit(EventMsg::TurnComplete(
            crate::protocol::event::TurnCompleteEvent {
                turn_id: turn_id.to_string(),
                last_agent_message,
            },
        ))
        .await;
    }

    /// Dispatch a single tool call through the ToolRouter, emitting bracket events.
    ///
    /// Returns the tool result on success, or emits an Error event on failure.
    #[allow(dead_code)]
    async fn dispatch_tool_call(
        &self,
        session: &Session,
        _turn_id: &str,
        call_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        // Emit McpToolCallBegin
        self.emit(EventMsg::McpToolCallBegin(
            crate::protocol::event::McpToolCallBeginEvent {
                call_id: call_id.to_string(),
                invocation: crate::protocol::types::McpInvocation {
                    server: String::new(),
                    tool: tool_name.to_string(),
                    arguments: Some(arguments.clone()),
                },
            },
        ))
        .await;

        let start = std::time::Instant::now();

        // Route through ToolRouter
        let result = session
            .tool_router()
            .await
            .route_tool_call(tool_name, arguments)
            .await;

        let duration = start.elapsed();

        match &result {
            Ok(value) => {
                self.emit(EventMsg::McpToolCallEnd(
                    crate::protocol::event::McpToolCallEndEvent {
                        call_id: call_id.to_string(),
                        invocation: crate::protocol::types::McpInvocation {
                            server: String::new(),
                            tool: tool_name.to_string(),
                            arguments: None,
                        },
                        duration,
                        result: Ok(crate::protocol::types::CallToolResult {
                            content: Some(value.clone()),
                            is_error: Some(false),
                        }),
                    },
                ))
                .await;

                // Store function output in history
                session
                    .add_to_history(vec![
                        crate::protocol::types::ResponseInputItem::FunctionOutput {
                            call_id: call_id.to_string(),
                            output: crate::protocol::types::FunctionCallOutputPayload {
                                content: crate::protocol::types::ContentOrItems::String(
                                    value.to_string(),
                                ),
                            },
                        },
                    ])
                    .await;
            }
            Err(e) => {
                // Emit Error event for failed tool call
                self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                    message: format!("tool call '{tool_name}' failed: {}", e.message),
                    codex_error_info: None,
                }))
                .await;

                self.emit(EventMsg::McpToolCallEnd(
                    crate::protocol::event::McpToolCallEndEvent {
                        call_id: call_id.to_string(),
                        invocation: crate::protocol::types::McpInvocation {
                            server: String::new(),
                            tool: tool_name.to_string(),
                            arguments: None,
                        },
                        duration,
                        result: Err(e.message.clone()),
                    },
                ))
                .await;
            }
        }

        result
    }

    /// Dispatch a dynamic tool call, sending the request event and waiting for the response.
    #[allow(dead_code)]
    async fn dispatch_dynamic_tool_call(
        &self,
        turn_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<crate::protocol::types::DynamicToolResponse, CodexError> {
        let handler = self.dynamic_tool_handler.lock().await;
        handler.invoke(tool_name, turn_id, arguments).await
    }

    async fn emit(&self, msg: EventMsg) {
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            msg,
        };
        let _ = self.eq_tx.send(event).await;
    }

    pub async fn is_running(&self) -> bool {
        *self.running.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigLayerStack;
    use crate::protocol::types::{AskForApproval, SandboxPolicy, UserInput};

    /// Helper: create a Codex with unbounded channels and return (sq_tx, eq_rx, codex).
    fn make_codex() -> (
        async_channel::Sender<Submission>,
        async_channel::Receiver<Event>,
        Codex,
    ) {
        let (sq_tx, sq_rx) = async_channel::unbounded();
        let (eq_tx, eq_rx) = async_channel::unbounded();
        let codex = Codex::new(
            sq_rx,
            eq_tx,
            ConfigLayerStack::new(),
            std::env::current_dir().unwrap(),
        );
        (sq_tx, eq_rx, codex)
    }

    /// Drain all events from the EQ receiver.
    fn drain_events(rx: &async_channel::Receiver<Event>) -> Vec<Event> {
        let mut events = vec![];
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        events
    }

    #[tokio::test]
    async fn codex_start_emits_session_configured() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "s1".into(),
                op: Op::Shutdown,
            })
            .await
            .unwrap();

        codex.run().await.unwrap();

        let events = drain_events(&eq_rx);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::SessionConfigured(_))));
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::ShutdownComplete)));
    }

    #[tokio::test]
    async fn spawn_creates_handle_and_processes_shutdown() {
        let handle = Codex::spawn(ConfigLayerStack::new(), std::env::current_dir().unwrap())
            .await
            .unwrap();

        // Send shutdown
        handle
            .tx_sub
            .send(Submission {
                id: "s1".into(),
                op: Op::Shutdown,
            })
            .await
            .unwrap();

        // Wait a bit for the engine to process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let events = drain_events(&handle.rx_event);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::SessionConfigured(_))));
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::ShutdownComplete)));
    }

    #[tokio::test]
    async fn user_turn_emits_bracket_events() {
        let (sq_tx, eq_rx, codex) = make_codex();

        // Send a UserTurn followed by Shutdown
        sq_tx
            .send(Submission {
                id: "t1".into(),
                op: Op::UserTurn {
                    items: vec![UserInput::Text {
                        text: "hello".into(),
                        text_elements: vec![],
                    }],
                    cwd: std::env::current_dir().unwrap(),
                    approval_policy: AskForApproval::Never,
                    sandbox_policy: SandboxPolicy::new_read_only_policy(),
                    model: "test-model".into(),
                    effort: None,
                    summary: None,
                    service_tier: None,
                    final_output_json_schema: None,
                    collaboration_mode: None,
                    personality: None,
                },
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

        let events = drain_events(&eq_rx);
        let event_types: Vec<&str> = events
            .iter()
            .map(|e| match &e.msg {
                EventMsg::SessionConfigured(_) => "SessionConfigured",
                EventMsg::TurnStarted(_) => "TurnStarted",
                EventMsg::TurnComplete(_) => "TurnComplete",
                EventMsg::ShutdownComplete => "ShutdownComplete",
                _ => "Other",
            })
            .collect();

        // Verify bracket: TurnStarted must come before TurnComplete
        let turn_started_pos = event_types.iter().position(|&t| t == "TurnStarted");
        let turn_complete_pos = event_types.iter().position(|&t| t == "TurnComplete");
        assert!(turn_started_pos.is_some(), "TurnStarted event missing");
        assert!(turn_complete_pos.is_some(), "TurnComplete event missing");
        assert!(
            turn_started_pos.unwrap() < turn_complete_pos.unwrap(),
            "TurnStarted must precede TurnComplete"
        );
    }

    #[tokio::test]
    async fn interrupt_emits_turn_aborted() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "i1".into(),
                op: Op::Interrupt,
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

        let events = drain_events(&eq_rx);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::TurnAborted(_))));
    }

    #[tokio::test]
    async fn add_to_history_stores_text() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "h1".into(),
                op: Op::AddToHistory {
                    text: "test message".into(),
                },
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

        // Verify no error events were emitted
        let events = drain_events(&eq_rx);
        assert!(!events.iter().any(|e| matches!(&e.msg, EventMsg::Error(_))));
    }

    #[tokio::test]
    async fn override_turn_context_without_active_turn_emits_error() {
        let (sq_tx, eq_rx, codex) = make_codex();

        // Override without an active turn should fail silently (no session error event
        // because the override returns Err which is caught by handle_op)
        sq_tx
            .send(Submission {
                id: "o1".into(),
                op: Op::OverrideTurnContext {
                    model: Some("new-model".into()),
                    cwd: None,
                    approval_policy: None,
                    sandbox_policy: None,
                    effort: None,
                    summary: None,
                    service_tier: None,
                    collaboration_mode: None,
                    personality: None,
                },
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

        // The override silently does nothing when no turn context exists
        // (the session returns Err but handle_op ignores it with `let _ =`)
        let events = drain_events(&eq_rx);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::ShutdownComplete)));
    }

    #[tokio::test]
    async fn list_mcp_tools_emits_response() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "m1".into(),
                op: Op::ListMcpTools,
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

        let events = drain_events(&eq_rx);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::McpListToolsResponse(_))));
    }

    #[tokio::test]
    async fn list_skills_emits_response() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "sk1".into(),
                op: Op::ListSkills {
                    cwds: vec![],
                    force_reload: false,
                },
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

        let events = drain_events(&eq_rx);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::ListSkillsResponse(_))));
    }

    #[tokio::test]
    async fn list_custom_prompts_emits_response() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "cp1".into(),
                op: Op::ListCustomPrompts,
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

        let events = drain_events(&eq_rx);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::ListCustomPromptsResponse(_))));
    }

    #[tokio::test]
    async fn thread_rollback_emits_event() {
        let (sq_tx, eq_rx, codex) = make_codex();

        // First add some history via a UserTurn
        sq_tx
            .send(Submission {
                id: "t1".into(),
                op: Op::UserTurn {
                    items: vec![UserInput::Text {
                        text: "msg1".into(),
                        text_elements: vec![],
                    }],
                    cwd: std::env::current_dir().unwrap(),
                    approval_policy: AskForApproval::Never,
                    sandbox_policy: SandboxPolicy::new_read_only_policy(),
                    model: "test".into(),
                    effort: None,
                    summary: None,
                    service_tier: None,
                    final_output_json_schema: None,
                    collaboration_mode: None,
                    personality: None,
                },
            })
            .await
            .unwrap();

        // Rollback 1 entry
        sq_tx
            .send(Submission {
                id: "r1".into(),
                op: Op::ThreadRollback { num_turns: 1 },
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

        let events = drain_events(&eq_rx);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::ThreadRolledBack(_))));
    }

    #[tokio::test]
    async fn dynamic_tool_response_with_unknown_id_emits_error() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "d1".into(),
                op: Op::DynamicToolResponse {
                    id: "nonexistent_call".into(),
                    response: crate::protocol::types::DynamicToolResponse {
                        content_items: vec![],
                        success: true,
                    },
                },
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

        let events = drain_events(&eq_rx);
        assert!(
            events.iter().any(|e| matches!(&e.msg, EventMsg::Error(_))),
            "should emit error for unknown dynamic tool call id"
        );
    }

    #[tokio::test]
    async fn realtime_conversation_start_emits_event() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "rt1".into(),
                op: Op::RealtimeConversationStart(
                    crate::protocol::types::ConversationStartParams {
                        prompt: "hello".into(),
                        session_id: Some("sess-1".into()),
                    },
                ),
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

        let events = drain_events(&eq_rx);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::RealtimeConversationStarted(_))));
    }

    #[tokio::test]
    async fn realtime_conversation_close_emits_event() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "rc1".into(),
                op: Op::RealtimeConversationClose,
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

        let events = drain_events(&eq_rx);
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::RealtimeConversationClosed(_))));
    }

    #[tokio::test]
    async fn legacy_user_input_emits_bracket_events() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "u1".into(),
                op: Op::UserInput {
                    items: vec![UserInput::Text {
                        text: "legacy input".into(),
                        text_elements: vec![],
                    }],
                    final_output_json_schema: None,
                },
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

        let events = drain_events(&eq_rx);
        assert!(
            events
                .iter()
                .any(|e| matches!(&e.msg, EventMsg::TurnStarted(_))),
            "legacy UserInput should emit TurnStarted"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(&e.msg, EventMsg::TurnComplete(_))),
            "legacy UserInput should emit TurnComplete"
        );
    }
}
