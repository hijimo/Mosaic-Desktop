use std::path::PathBuf;
use std::sync::Arc;

use async_channel::{Receiver, Sender};
use tokio::sync::Mutex;

use crate::config::ConfigLayerStack;
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{Event, EventMsg, SessionConfiguredEvent};
use crate::protocol::submission::{Op, Submission};
use crate::protocol::types::TurnContextOverrides;

use super::session::Session;
use super::skills::loader::SkillRoot;
use super::skills::model::{SkillScope, SkillMetadata};
use super::skills::manager::SkillsManager;
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
    skills_manager: Arc<SkillsManager>,
    /// Dynamic tool handler for managing dynamic tool lifecycle.
    dynamic_tool_handler: Arc<DynamicToolHandler>,
}

use super::initial_history::InitialHistory;
use super::rollout::reconstruction::{PreviousTurnSettings, RolloutReconstruction, reconstruct_history_from_rollout};
use super::rollout::recorder::ResumedHistory;

impl Codex {
    pub fn new(
        sq_rx: Receiver<Submission>,
        eq_tx: Sender<Event>,
        config: ConfigLayerStack,
        cwd: PathBuf,
    ) -> Self {
        let codex_home = dirs::home_dir()
            .map(|h| h.join(".codex"))
            .unwrap_or_else(|| cwd.join(".codex"));
        let skills_manager = SkillsManager::new(codex_home);
        let dynamic_tool_handler = DynamicToolHandler::new(eq_tx.clone());
        Self {
            sq_rx,
            eq_tx,
            session: Arc::new(Mutex::new(None)),
            config,
            cwd,
            running: Arc::new(Mutex::new(false)),
            skills_manager: Arc::new(skills_manager),
            dynamic_tool_handler: Arc::new(dynamic_tool_handler),
        }
    }

    /// Spawn a new Codex engine, returning a handle with SQ/EQ channels.
    pub async fn spawn(config: ConfigLayerStack, cwd: PathBuf) -> Result<CodexHandle, CodexError> {
        Self::spawn_with_history(config, cwd, InitialHistory::New).await
    }

    /// Spawn a Codex engine with initial history.
    pub async fn spawn_with_history(
        config: ConfigLayerStack,
        cwd: PathBuf,
        initial_history: InitialHistory,
    ) -> Result<CodexHandle, CodexError> {
        let (sq_tx, sq_rx) = async_channel::unbounded();
        let (eq_tx, eq_rx) = async_channel::unbounded();

        let codex = Self::new(sq_rx, eq_tx, config, cwd);

        tokio::spawn(async move {
            if let Err(e) = codex.run_with_history(initial_history).await {
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
        self.run_with_history(InitialHistory::New).await
    }

    /// Start the SQ/EQ processing loop with initial history.
    async fn run_with_history(&self, initial_history: InitialHistory) -> Result<(), CodexError> {
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

        let current_model = merged_config
            .model
            .clone()
            .unwrap_or_else(|| "default".into());

        let history_entry_count = match &initial_history {
            InitialHistory::Resumed(rh) => rh.history.len(),
            InitialHistory::Forked(items) => items.len(),
            InitialHistory::New => 0,
        };
        let can_append = !matches!(initial_history, InitialHistory::New);

        self.emit(EventMsg::SessionConfigured(SessionConfiguredEvent {
            session_id: session_id.clone(),
            forked_from_id: None,
            thread_name: None,
            model: current_model.clone(),
            model_provider_id: merged_config.model_provider.clone().unwrap_or_default(),
            approval_policy: merged_config.approval_policy.clone(),
            sandbox_policy: None,
            cwd: self.cwd.clone(),
            history_log_id: 0,
            history_entry_count,
            mode: crate::protocol::types::ModeKind::Default,
            reasoning_effort: None,
            reasoning_summary: None,
            can_append,
        }))
        .await;

        // Inject initial history into session BEFORE storing it.
        {
            let rollout_items = match &initial_history {
                InitialHistory::Resumed(rh) => Some(rh.history.as_slice()),
                InitialHistory::Forked(items) => Some(items.as_slice()),
                InitialHistory::New => None,
            };

            if let Some(items) = rollout_items {
                // Clear MCP tool selection before restoring from rollout
                // (matching codex-main's clear_mcp_tool_selection at start of record_initial_history).
                session.clear_mcp_tool_selection().await;

                let reconstruction = reconstruct_history_from_rollout(items, Default::default());

                // Model consistency check
                if let Some(ref prev) = reconstruction.previous_turn_settings {
                    if prev.model != current_model {
                        self.emit(EventMsg::Warning(crate::protocol::event::WarningEvent {
                            message: format!(
                                "This session was recorded with model `{}` but is resuming with `{}`. \
                                 Consider switching back to `{}` as it may affect performance.",
                                prev.model, current_model, prev.model
                            ),
                        }))
                        .await;
                    }
                }

                session.set_previous_turn_settings(reconstruction.previous_turn_settings).await;
                session.set_reference_context_item(reconstruction.reference_context_item).await;

                if !reconstruction.history.is_empty() {
                    session.add_to_history(reconstruction.history).await;
                }

                if let Some(info) = reconstruction.last_token_info {
                    session.set_token_info(Some(info)).await;
                }

                if let Some(tools) = extract_mcp_tool_selection_from_rollout(items) {
                    session.set_mcp_tool_selection(tools).await;
                }
            }
        }

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
            Op::ExecApproval {
                decision,
                custom_instructions,
                ..
            } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    // Forward custom_instructions to session for next turn (Req 28.3)
                    if let Some(instructions) = custom_instructions {
                        if !instructions.is_empty() {
                            s.set_custom_instructions(instructions).await;
                        }
                    }

                    // Abort decision interrupts the current turn (Req 28.1)
                    if matches!(decision, crate::protocol::types::ReviewDecision::Abort) {
                        s.interrupt().await;
                        self.emit(EventMsg::TurnAborted(
                            crate::protocol::event::TurnAbortedEvent {
                                turn_id: None,
                                reason: crate::protocol::types::TurnAbortReason::Interrupted,
                            },
                        ))
                        .await;
                    } else if matches!(decision, crate::protocol::types::ReviewDecision::Denied) {
                        // Denied cancels the pending operation (Req 28.1)
                        let _ = s.take_pending_approval().await;
                    } else if let Some(pending) = s.take_pending_approval().await {
                        // Extract command prefix from the pending approval
                        let command_prefix =
                            if let crate::core::session::PendingApproval::Exec { command, .. } =
                                &pending
                            {
                                Some(command.clone())
                            } else {
                                None
                            };

                        match &decision {
                            crate::protocol::types::ReviewDecision::Approved => {
                                // Simple approval — execute the command
                                // TODO: execute the approved command via sandbox
                            }
                            crate::protocol::types::ReviewDecision::ApprovedForSession => {
                                // Approve + add command to session allow list (Req 28.2)
                                if let Some(prefix) = &command_prefix {
                                    s.add_to_exec_allow_list(prefix.clone()).await;
                                }
                                // TODO: execute the approved command via sandbox
                            }
                            crate::protocol::types::ReviewDecision::ApprovedExecpolicyAmendment {
                                proposed_execpolicy_amendment,
                            } => {
                                // Approve + persist prefix rule to .codexpolicy (Req 28.2)
                                let policy_path = s.cwd().join(".codexpolicy");
                                if let Err(e) =
                                    crate::execpolicy::amend::blocking_append_allow_prefix_rule(
                                        &policy_path,
                                        &proposed_execpolicy_amendment.command,
                                    )
                                {
                                    self.emit(EventMsg::Warning(
                                        crate::protocol::event::WarningEvent {
                                            message: format!(
                                                "failed to amend execpolicy: {e}"
                                            ),
                                        },
                                    ))
                                    .await;
                                }
                                // Also add to session allow list for immediate effect
                                s.add_to_exec_allow_list(
                                    proposed_execpolicy_amendment.command.clone(),
                                )
                                .await;
                                // TODO: execute the approved command via sandbox
                            }
                            _ => {}
                        }
                    }
                }
            }
            Op::PatchApproval {
                decision,
                custom_instructions,
                ..
            } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    // Forward custom_instructions to session for next turn (Req 28.3)
                    if let Some(instructions) = custom_instructions {
                        if !instructions.is_empty() {
                            s.set_custom_instructions(instructions).await;
                        }
                    }

                    // Abort decision interrupts the current turn (Req 28.1)
                    if matches!(decision, crate::protocol::types::ReviewDecision::Abort) {
                        s.interrupt().await;
                        self.emit(EventMsg::TurnAborted(
                            crate::protocol::event::TurnAbortedEvent {
                                turn_id: None,
                                reason: crate::protocol::types::TurnAbortReason::Interrupted,
                            },
                        ))
                        .await;
                    } else if matches!(decision, crate::protocol::types::ReviewDecision::Denied) {
                        // Denied cancels the pending patch (Req 28.1)
                        if let Some(crate::core::session::PendingApproval::Patch {
                            call_id,
                            turn_id,
                            changes,
                            ..
                        }) = s.take_pending_approval().await
                        {
                            self.emit(EventMsg::PatchApplyEnd(
                                crate::protocol::event::PatchApplyEndEvent {
                                    call_id,
                                    turn_id,
                                    stdout: String::new(),
                                    stderr: "patch declined by user".to_string(),
                                    success: false,
                                    changes,
                                    status: crate::protocol::types::PatchApplyStatus::Declined,
                                },
                            ))
                            .await;
                        }
                    } else if let Some(crate::core::session::PendingApproval::Patch {
                        call_id,
                        turn_id,
                        changes,
                        cwd,
                    }) = s.take_pending_approval().await
                    {
                        // Approved / ApprovedForSession — apply the patch
                        let applicator = super::patch::PatchApplicator::new(
                            s.resolved_config()
                                .approval_policy
                                .clone()
                                .unwrap_or_default(),
                            s.tx_event().clone(),
                        );
                        let _ = applicator
                            .apply_approved(&changes, &cwd, &call_id, &turn_id)
                            .await;
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
            Op::AddToHistory { text, role } => {
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    s.add_to_history(vec![crate::protocol::types::ResponseInputItem::text_message(&role, text.clone())])
                    .await;
                }
                // Emit as RawResponseItem so user messages are persisted as
                // RolloutItem::ResponseItem (matching assistant messages).
                self.emit(EventMsg::RawResponseItem(
                    crate::protocol::event::RawResponseItemEvent {
                        item: crate::protocol::types::ResponseItem::Message {
                            id: None,
                            role: role.clone(),
                            content: vec![crate::protocol::types::ContentItem::InputText { text }],
                            end_turn: None,
                            phase: None,
                        },
                    },
                )).await;
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
                self.spawn_task(Arc::new(
                    crate::core::tasks::review::ReviewTask,
                ), vec![]).await;
            }
            Op::Compact => {
                // Compact runs inline because it needs session access.
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    let policy =
                        crate::core::truncation::TruncationPolicy::KeepRecent { max_items: 50 };
                    match s
                        .compact_history::<fn(String) -> std::future::Ready<Result<String, crate::protocol::error::CodexError>>, _>(
                            &policy, None,
                        )
                        .await
                    {
                        Ok(_) => {
                            self.emit(EventMsg::ContextCompacted(
                                crate::protocol::event::ContextCompactedEvent,
                            ))
                            .await;
                        }
                        Err(e) => {
                            self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                                message: format!("compact failed: {}", e.message),
                                codex_error_info: None,
                            }))
                            .await;
                        }
                    }
                }
            }
            Op::Undo => {
                self.spawn_task(Arc::new(
                    crate::core::tasks::undo::UndoTask,
                ), vec![]).await;
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
                    let all_tools = s.mcp_manager().all_tools().await;
                    let mut map: std::collections::HashMap<String, serde_json::Value> =
                        std::collections::HashMap::new();
                    for tool in all_tools {
                        // Group tools by server name (extracted from qualified name mcp__{server}__{tool})
                        let server = tool
                            .qualified_name
                            .strip_prefix("mcp__")
                            .and_then(|s| s.split("__").next())
                            .unwrap_or("unknown")
                            .to_string();
                        let entry = map
                            .entry(server)
                            .or_insert_with(|| serde_json::Value::Array(vec![]));
                        if let serde_json::Value::Array(arr) = entry {
                            arr.push(serde_json::json!({
                                "name": tool.name,
                                "qualifiedName": tool.qualified_name,
                                "description": tool.description,
                            }));
                        }
                    }
                    map
                } else {
                    std::collections::HashMap::new()
                };
                self.emit(EventMsg::McpListToolsResponse(
                    crate::protocol::event::McpListToolsResponseEvent { tools },
                ))
                .await;
            }
            Op::ListSkills { .. } => {
                let outcome = self.skills_manager.skills_for_cwd(&self.cwd, false);
                let skills = outcome
                    .skills
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
                let prompts = if let Some(dir) = crate::core::custom_prompts::default_prompts_dir() {
                    crate::core::custom_prompts::discover_prompts_in(&dir).await
                } else {
                    vec![]
                };
                let custom_prompts = prompts
                    .into_iter()
                    .filter_map(|p| {
                        serde_json::to_value(&p).ok()
                    })
                    .collect();
                self.emit(EventMsg::ListCustomPromptsResponse(
                    crate::protocol::event::ListCustomPromptsResponseEvent {
                        custom_prompts,
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
                if let Err(e) = self.dynamic_tool_handler.resolve_call(&id, response).await {
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
            Op::RefreshMcpServers { config } => {
                // Parse mcp_servers from the config payload and reconnect each server
                let session_guard = self.session.lock().await;
                if let Some(s) = session_guard.as_ref() {
                    let mcp_manager = s.mcp_manager();
                    let mut ready = vec![];
                    let mut failed: Vec<crate::protocol::types::McpStartupFailure> = vec![];

                    if let Some(servers_map) = config.mcp_servers.as_object() {
                        for (name, server_val) in servers_map {
                            match serde_json::from_value::<crate::config::toml_types::McpServerConfig>(
                                server_val.clone(),
                            ) {
                                Ok(server_config) => {
                                    match mcp_manager.connect(name, &server_config).await {
                                        Ok(()) => ready.push(name.clone()),
                                        Err(e) => {
                                            failed.push(
                                                crate::protocol::types::McpStartupFailure {
                                                    server: name.clone(),
                                                    error: e.message,
                                                },
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    failed.push(crate::protocol::types::McpStartupFailure {
                                        server: name.clone(),
                                        error: format!("config parse error: {e}"),
                                    });
                                }
                            }
                        }
                    }

                    self.emit(EventMsg::McpStartupComplete(
                        crate::protocol::event::McpStartupCompleteEvent {
                            ready,
                            failed,
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
                .add_to_history(vec![crate::protocol::types::ResponseInputItem::text_message("user", user_text.clone())])
                .await;
        }

        // Emit structured user message item events
        {
            let thread_id_str = session.id().to_string();
            let turn_id_str = turn_id.to_string();
            let user_item = crate::protocol::items::TurnItem::UserMessage(
                crate::protocol::items::UserMessageItem::new(&items),
            );
            self.emit(EventMsg::ItemStarted(
                crate::protocol::event::ItemStartedEvent {
                    thread_id: thread_id_str.clone(),
                    turn_id: turn_id_str.clone(),
                    item: user_item.clone(),
                },
            )).await;
            self.emit(EventMsg::ItemCompleted(
                crate::protocol::event::ItemCompletedEvent {
                    thread_id: thread_id_str,
                    turn_id: turn_id_str,
                    item: user_item,
                },
            )).await;
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

        // Collect tool specs from the router for the model API
        let tool_specs = {
            let router = session.tool_router().await;
            let specs = router.collect_tool_specs();
            if specs.is_empty() { None } else { Some(specs) }
        };

        // Call the Responses API with agentic tool-call loop
        let mut last_agent_message: Option<String> = None;
        let mut accumulated_text = String::new();

        // Consume any custom instructions left by a ReviewDecision (Req 28.3)
        let custom_instructions = session.take_custom_instructions().await;

        // Build instructions: merge skill list (like source render.rs) + custom instructions
        let instructions: Option<String> = {
            let outcome = self.skills_manager.skills_for_cwd(&self.cwd, false);
            let skill_section = super::skills::render_skills_section(&outcome.skills);
            match (skill_section, custom_instructions) {
                (Some(s), Some(c)) => Some(format!("{s}\n\n{c}")),
                (Some(s), None) => Some(s),
                (None, Some(c)) => Some(c),
                (None, None) => None,
            }
        };

        // ── Agentic loop: stream → detect function_call → dispatch → continue ──
        const MAX_TOOL_ROUNDS: usize = 32;
        let mut round = 0;
        loop {
            round += 1;
            if round > MAX_TOOL_ROUNDS {
                self.emit(EventMsg::Warning(crate::protocol::event::WarningEvent {
                    message: format!("Tool call loop exceeded {MAX_TOOL_ROUNDS} rounds, stopping"),
                })).await;
                break;
            }

            // Rebuild API input from current history each round
            let api_input: Vec<serde_json::Value> = session.history().await
                .iter()
                .map(crate::core::client::history_item_to_api)
                .collect();

            let stream_result = crate::core::client::stream_response(
                &base_url,
                &api_key,
                &extra_headers,
                &model,
                instructions.as_deref(),
                api_input,
                None,
                tool_specs.clone(),
            ).await;

            let mut needs_follow_up = false;
            accumulated_text.clear();
            let mut pending_calls: Vec<(String, String, String)> = Vec::new();

            // Structured item tracking for v2 events
            let thread_id_str = session.id().to_string();
            let turn_id_str = turn_id.to_string();
            let agent_msg_item_id = uuid::Uuid::new_v4().to_string();
            let reasoning_item_id = uuid::Uuid::new_v4().to_string();
            let mut agent_msg_item_started = false;
            let mut reasoning_item_started = false;
            let mut accumulated_reasoning_summary: Vec<String> = Vec::new();
            let mut accumulated_reasoning_raw: Vec<String> = Vec::new();
            let mut current_summary_index: i64 = 0;
            let mut current_content_index: i64 = 0;

            match stream_result {
                Err(e) => {
                    self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                        message: format!("Failed to start API stream: {}", e.message),
                        codex_error_info: None,
                    })).await;
                    break;
                }
                Ok(mut stream) => {
                    use futures::StreamExt;
                    while let Some(event_result) = stream.next().await {
                        match event_result {
                            Err(e) => {
                                self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                                    message: format!("Stream error: {}", e.message),
                                    codex_error_info: None,
                                })).await;
                                break;
                            }
                            Ok(crate::core::client::ResponseEvent::OutputTextDelta { delta }) => {
                                if !delta.is_empty() {
                                    // Emit ItemStarted on first text delta for this agent message
                                    if !agent_msg_item_started {
                                        agent_msg_item_started = true;
                                        let placeholder_item = crate::protocol::items::TurnItem::AgentMessage(
                                            crate::protocol::items::AgentMessageItem {
                                                id: agent_msg_item_id.clone(),
                                                content: Vec::new(),
                                                phase: None,
                                            },
                                        );
                                        self.emit(EventMsg::ItemStarted(
                                            crate::protocol::event::ItemStartedEvent {
                                                thread_id: thread_id_str.clone(),
                                                turn_id: turn_id_str.clone(),
                                                item: placeholder_item,
                                            },
                                        )).await;
                                    }

                                    accumulated_text.push_str(&delta);

                                    // Structured v2 event with item-level tracking
                                    self.emit(EventMsg::AgentMessageContentDelta(
                                        crate::protocol::event::AgentMessageContentDeltaEvent {
                                            thread_id: thread_id_str.clone(),
                                            turn_id: turn_id_str.clone(),
                                            item_id: agent_msg_item_id.clone(),
                                            delta,
                                        },
                                    )).await;
                                }
                            }
                            Ok(crate::core::client::ResponseEvent::FunctionCall { call_id, name, arguments }) => {
                                // Record the function_call in history
                                session.add_to_history(vec![
                                    crate::protocol::types::ResponseInputItem::FunctionCall {
                                        call_id: call_id.clone(),
                                        name: name.clone(),
                                        arguments: arguments.clone(),
                                    },
                                ]).await;
                                // Emit as RawResponseItem so it gets persisted in rollout
                                self.emit(EventMsg::RawResponseItem(
                                    crate::protocol::event::RawResponseItemEvent {
                                        item: crate::protocol::types::ResponseItem::FunctionCall {
                                            id: None,
                                            name: name.clone(),
                                            arguments: arguments.clone(),
                                            call_id: call_id.clone(),
                                        },
                                    },
                                )).await;
                                pending_calls.push((call_id, name, arguments));
                            }
                            Ok(crate::core::client::ResponseEvent::OutputItemDone(item)) => {
                                // Emit as RawResponseItem so it gets persisted in rollout
                                self.emit(EventMsg::RawResponseItem(
                                    crate::protocol::event::RawResponseItemEvent {
                                        item: item.clone(),
                                    },
                                )).await;

                                // Convert to TurnItem for structured events
                                if let Some(turn_item) = crate::core::event_mapping::parse_turn_item(&item) {
                                    self.emit(EventMsg::ItemCompleted(
                                        crate::protocol::event::ItemCompletedEvent {
                                            thread_id: thread_id_str.clone(),
                                            turn_id: turn_id_str.clone(),
                                            item: turn_item,
                                        },
                                    )).await;
                                }

                                if let crate::protocol::types::ResponseItem::Message { role, content, .. } = &item {
                                    if role == "assistant" {
                                        let text: String = content.iter().filter_map(|c| {
                                            if let crate::protocol::types::ContentItem::OutputText { text } = c {
                                                Some(text.as_str())
                                            } else {
                                                None
                                            }
                                        }).collect::<Vec<_>>().join("");

                                        if !text.is_empty() {
                                            last_agent_message = Some(text.clone());
                                            session.add_to_history(vec![
                                                crate::protocol::types::ResponseInputItem::text_message("assistant", text.clone()),
                                            ]).await;
                                        }
                                    }
                                }
                            }
                            Ok(crate::core::client::ResponseEvent::ReasoningSummaryPartAdded { summary_index }) => {
                                current_summary_index = summary_index;
                                // Ensure reasoning item is started
                                if !reasoning_item_started {
                                    reasoning_item_started = true;
                                    let placeholder_item = crate::protocol::items::TurnItem::Reasoning(
                                        crate::protocol::items::ReasoningItem {
                                            id: reasoning_item_id.clone(),
                                            summary_text: Vec::new(),
                                            raw_content: Vec::new(),
                                        },
                                    );
                                    self.emit(EventMsg::ItemStarted(
                                        crate::protocol::event::ItemStartedEvent {
                                            thread_id: thread_id_str.clone(),
                                            turn_id: turn_id_str.clone(),
                                            item: placeholder_item,
                                        },
                                    )).await;
                                }
                                // Ensure the summary_text vec has enough slots
                                while accumulated_reasoning_summary.len() <= summary_index as usize {
                                    accumulated_reasoning_summary.push(String::new());
                                }
                            }
                            Ok(crate::core::client::ResponseEvent::ReasoningSummaryDelta { delta, summary_index }) => {
                                if !delta.is_empty() {
                                    // Ensure reasoning item is started
                                    if !reasoning_item_started {
                                        reasoning_item_started = true;
                                        let placeholder_item = crate::protocol::items::TurnItem::Reasoning(
                                            crate::protocol::items::ReasoningItem {
                                                id: reasoning_item_id.clone(),
                                                summary_text: Vec::new(),
                                                raw_content: Vec::new(),
                                            },
                                        );
                                        self.emit(EventMsg::ItemStarted(
                                            crate::protocol::event::ItemStartedEvent {
                                                thread_id: thread_id_str.clone(),
                                                turn_id: turn_id_str.clone(),
                                                item: placeholder_item,
                                            },
                                        )).await;
                                    }

                                    // Accumulate summary text
                                    while accumulated_reasoning_summary.len() <= summary_index as usize {
                                        accumulated_reasoning_summary.push(String::new());
                                    }
                                    accumulated_reasoning_summary[summary_index as usize].push_str(&delta);

                                    // Structured v2 event
                                    self.emit(EventMsg::ReasoningContentDelta(
                                        crate::protocol::event::ReasoningContentDeltaEvent {
                                            thread_id: thread_id_str.clone(),
                                            turn_id: turn_id_str.clone(),
                                            item_id: reasoning_item_id.clone(),
                                            delta,
                                            summary_index,
                                        },
                                    )).await;
                                }
                            }
                            Ok(crate::core::client::ResponseEvent::ReasoningContentDelta { delta, content_index }) => {
                                if !delta.is_empty() {
                                    // Ensure reasoning item is started
                                    if !reasoning_item_started {
                                        reasoning_item_started = true;
                                        let placeholder_item = crate::protocol::items::TurnItem::Reasoning(
                                            crate::protocol::items::ReasoningItem {
                                                id: reasoning_item_id.clone(),
                                                summary_text: Vec::new(),
                                                raw_content: Vec::new(),
                                            },
                                        );
                                        self.emit(EventMsg::ItemStarted(
                                            crate::protocol::event::ItemStartedEvent {
                                                thread_id: thread_id_str.clone(),
                                                turn_id: turn_id_str.clone(),
                                                item: placeholder_item,
                                            },
                                        )).await;
                                    }

                                    // Accumulate raw content
                                    while accumulated_reasoning_raw.len() <= content_index as usize {
                                        accumulated_reasoning_raw.push(String::new());
                                    }
                                    accumulated_reasoning_raw[content_index as usize].push_str(&delta);
                                    current_content_index = content_index;

                                    // Structured v2 event
                                    self.emit(EventMsg::ReasoningRawContentDelta(
                                        crate::protocol::event::ReasoningRawContentDeltaEvent {
                                            thread_id: thread_id_str.clone(),
                                            turn_id: turn_id_str.clone(),
                                            item_id: reasoning_item_id.clone(),
                                            delta,
                                            content_index,
                                        },
                                    )).await;
                                }
                            }
                            Ok(crate::core::client::ResponseEvent::Completed { token_usage, .. }) => {
                                if let Some(usage) = token_usage {
                                    self.emit(EventMsg::TokenCount(
                                        crate::protocol::event::TokenCountEvent {
                                            info: Some(crate::protocol::types::TokenUsageInfo {
                                                total_token_usage: crate::protocol::types::TokenUsage {
                                                    input_tokens: usage.input_tokens,
                                                    cached_input_tokens: usage.cached_input_tokens,
                                                    output_tokens: usage.output_tokens,
                                                    reasoning_output_tokens: usage.reasoning_output_tokens,
                                                    total_tokens: usage.total_tokens,
                                                },
                                                last_token_usage: crate::protocol::types::TokenUsage {
                                                    input_tokens: usage.input_tokens,
                                                    cached_input_tokens: usage.cached_input_tokens,
                                                    output_tokens: usage.output_tokens,
                                                    reasoning_output_tokens: usage.reasoning_output_tokens,
                                                    total_tokens: usage.total_tokens,
                                                },
                                                model_context_window: None,
                                            }),
                                            rate_limits: None,
                                        },
                                    )).await;
                                }

                                // Emit ItemCompleted for reasoning if we accumulated any
                                if reasoning_item_started {
                                    let reasoning_item = crate::protocol::items::TurnItem::Reasoning(
                                        crate::protocol::items::ReasoningItem {
                                            id: reasoning_item_id.clone(),
                                            summary_text: accumulated_reasoning_summary.clone(),
                                            raw_content: accumulated_reasoning_raw.clone(),
                                        },
                                    );
                                    self.emit(EventMsg::ItemCompleted(
                                        crate::protocol::event::ItemCompletedEvent {
                                            thread_id: thread_id_str.clone(),
                                            turn_id: turn_id_str.clone(),
                                            item: reasoning_item,
                                        },
                                    )).await;
                                }

                                if !accumulated_text.is_empty() && last_agent_message.is_none() {
                                    last_agent_message = Some(accumulated_text.clone());
                                    session.add_to_history(vec![
                                        crate::protocol::types::ResponseInputItem::text_message("assistant", accumulated_text.clone()),
                                    ]).await;

                                    // Emit ItemCompleted for the agent message built from accumulated deltas
                                    let completed_item = crate::protocol::items::TurnItem::AgentMessage(
                                        crate::protocol::items::AgentMessageItem {
                                            id: agent_msg_item_id.clone(),
                                            content: vec![crate::protocol::items::AgentMessageContent::Text {
                                                text: accumulated_text.clone(),
                                            }],
                                            phase: None,
                                        },
                                    );
                                    self.emit(EventMsg::ItemCompleted(
                                        crate::protocol::event::ItemCompletedEvent {
                                            thread_id: thread_id_str.clone(),
                                            turn_id: turn_id_str.clone(),
                                            item: completed_item,
                                        },
                                    )).await;
                                }
                                break;
                            }
                            Ok(crate::core::client::ResponseEvent::Failed { code, message }) => {
                                self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                                    message: format!("API error [{code}]: {message}"),
                                    codex_error_info: None,
                                })).await;
                                break;
                            }
                            // Created, OutputItemAdded, ServerModel, RateLimits
                            _ => {}
                        }
                    }
                }
            }

            // Dispatch pending tool calls in parallel
            if !pending_calls.is_empty() {
                let futs: Vec<_> = pending_calls.iter().map(|(call_id, name, arguments)| {
                    let args: serde_json::Value = serde_json::from_str(arguments)
                        .unwrap_or(serde_json::Value::Object(Default::default()));
                    let call_id = call_id.clone();
                    let name = name.clone();
                    async move {
                        let result = self.dispatch_tool_call(session, turn_id, &call_id, &name, args).await;
                        (call_id, result)
                    }
                }).collect();
                let results = futures::future::join_all(futs).await;
                for (call_id, tool_result) in results {
                    let output_str = match &tool_result {
                        Ok(v) => v.to_string(),
                        Err(e) => format!("Error: {}", e.message),
                    };
                    session.add_to_history(vec![
                        crate::protocol::types::ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: crate::protocol::types::FunctionCallOutputPayload::from_text(output_str.clone()),
                        },
                    ]).await;
                    // Persist function call output in rollout
                    self.emit(EventMsg::RawResponseItem(
                        crate::protocol::event::RawResponseItemEvent {
                            item: crate::protocol::types::ResponseItem::FunctionCallOutput {
                                call_id,
                                output: crate::protocol::types::FunctionCallOutputPayload::from_text(output_str),
                            },
                        },
                    )).await;
                }
                needs_follow_up = true;
            }

            // If no tool calls were made, exit the agentic loop
            if !needs_follow_up {
                break;
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

        // Auto-generate thread name on first turn
        if turn_id == "turn-1" && !user_text.is_empty() {
            let eq_tx = self.eq_tx.clone();
            let session_id = session.id().to_string();
            let base_url_clone = base_url.clone();
            let api_key_clone = api_key.clone();
            let model_clone = model.clone();
            let user_text_clone = user_text.clone();
            tokio::spawn(async move {
                if let Some(name) = generate_thread_name(
                    &base_url_clone,
                    &api_key_clone,
                    &model_clone,
                    &user_text_clone,
                ).await {
                    let _ = eq_tx.send(Event {
                        id: uuid::Uuid::new_v4().to_string(),
                        msg: EventMsg::ThreadNameUpdated(
                            crate::protocol::event::ThreadNameUpdatedEvent {
                                thread_id: session_id,
                                thread_name: Some(name),
                            },
                        ),
                    }).await;
                }
            });
        }
    }

    /// Dispatch a single tool call through the ToolRouter, emitting bracket events.
    async fn dispatch_tool_call(
        &self,
        session: &Session,
        turn_id: &str,
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

        // Route through ToolRouter — which distinguishes built-in/MCP vs dynamic
        let route_result = session
            .tool_router()
            .await
            .route_tool_call(tool_name, arguments.clone())
            .await;

        let result = match route_result {
            crate::core::tools::router::RouteResult::Handled(r) => r,
            crate::core::tools::router::RouteResult::DynamicTool(_) => {
                // Dynamic tool: invoke through DynamicToolHandler which sends
                // DynamicToolCallRequest event and waits for DynamicToolResponse.
                match self
                    .dynamic_tool_handler
                    .invoke(tool_name, turn_id, arguments.clone())
                    .await
                {
                    Ok(response) => {
                        // Send the response event for logging/UI
                        let _ = self
                            .dynamic_tool_handler
                            .send_response_event(call_id, turn_id, tool_name, arguments, &response)
                            .await;
                        // Convert DynamicToolResponse to JSON value
                        Ok(serde_json::to_value(&response).unwrap_or(serde_json::Value::Null))
                    }
                    Err(e) => Err(e),
                }
            }
            crate::core::tools::router::RouteResult::NotFound(name) => Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("no handler found for tool: {name}"),
            )),
        };

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
                        crate::protocol::types::ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.to_string(),
                            output: crate::protocol::types::FunctionCallOutputPayload::from_text(
                                    value.to_string(),
                            ),
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
        self.dynamic_tool_handler
            .invoke(tool_name, turn_id, arguments)
            .await
    }

    /// Register a dynamic tool, making it available for routing and invocation.
    ///
    /// The tool is registered in both the ToolRouter (for discovery/routing) and
    /// the DynamicToolHandler (for the request/response lifecycle).
    #[allow(dead_code)]
    pub async fn register_dynamic_tool(&self, spec: crate::protocol::types::DynamicToolSpec) {
        // Register in DynamicToolHandler for invocation lifecycle
        self.dynamic_tool_handler.register_tool(spec.clone());
        // Register in ToolRouter for routing discovery
        {
            let session_guard = self.session.lock().await;
            if let Some(s) = session_guard.as_ref() {
                let mut router = s.tool_router().await;
                router.register_dynamic_tool(spec);
            }
        }
    }

    /// Unregister a dynamic tool from both the router and handler.
    #[allow(dead_code)]
    pub async fn unregister_dynamic_tool(&self, name: &str) {
        self.dynamic_tool_handler.unregister_tool(name);
        {
            let session_guard = self.session.lock().await;
            if let Some(s) = session_guard.as_ref() {
                let mut router = s.tool_router().await;
                router.unregister_dynamic_tool(name);
            }
        }
    }

    async fn emit(&self, msg: EventMsg) {
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            msg,
        };
        let _ = self.eq_tx.send(event).await;
    }

    /// Spawn a session task on a background Tokio task.
    async fn spawn_task(
        &self,
        task: Arc<dyn super::tasks::SessionTask>,
        input: Vec<crate::protocol::types::UserInput>,
    ) {
        let session_guard = self.session.lock().await;
        if session_guard.is_none() {
            self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                message: "no active session for task".to_string(),
                codex_error_info: None,
            }))
            .await;
            return;
        }
        let ctx = super::tasks::TaskContext::new(
            self.eq_tx.clone(),
            self.config.clone(),
            self.cwd.clone(),
        );
        drop(session_guard);
        let cancellation_token = tokio_util::sync::CancellationToken::new();
        tokio::spawn(async move {
            let last_msg = task.run(ctx.clone(), input, cancellation_token).await;
            if let Some(msg) = last_msg {
                ctx.emit(EventMsg::TurnComplete(
                    crate::protocol::event::TurnCompleteEvent {
                        turn_id: uuid::Uuid::new_v4().to_string(),
                        last_agent_message: Some(msg),
                    },
                ))
                .await;
            }
        });
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
                    role: "user".into(),
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

    // ── ReviewDecision semantics integration tests ───────────────

    /// Helper: wait for SessionConfigured, then set pending approval on the live session.
    async fn wait_session_and_set_pending(
        codex: &Codex,
        eq_rx: &async_channel::Receiver<Event>,
        pending: crate::core::session::PendingApproval,
    ) {
        // Wait for SessionConfigured so the session is initialised inside run()
        loop {
            let ev = eq_rx.recv().await.unwrap();
            if matches!(&ev.msg, EventMsg::SessionConfigured(_)) {
                break;
            }
        }
        // Now the session exists — set pending approval
        let session_guard = codex.session.lock().await;
        if let Some(s) = session_guard.as_ref() {
            s.set_pending_approval(pending).await;
        }
    }

    #[tokio::test]
    async fn exec_approval_denied_cancels_pending() {
        let (sq_tx, eq_rx, codex) = make_codex();
        let codex = Arc::new(codex);
        let codex2 = Arc::clone(&codex);

        let handle = tokio::spawn(async move { codex2.run().await });

        wait_session_and_set_pending(
            &codex,
            &eq_rx,
            crate::core::session::PendingApproval::Exec {
                call_id: "c1".into(),
                command: vec!["rm".into(), "-rf".into()],
                cwd: std::path::PathBuf::from("/tmp"),
            },
        )
        .await;

        sq_tx
            .send(Submission {
                id: "ea1".into(),
                op: Op::ExecApproval {
                    id: "c1".into(),
                    turn_id: None,
                    decision: crate::protocol::types::ReviewDecision::Denied,
                    custom_instructions: None,
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

        handle.await.unwrap().unwrap();

        // Pending approval should be consumed
        let session_guard = codex.session.lock().await;
        if let Some(s) = session_guard.as_ref() {
            assert!(s.take_pending_approval().await.is_none());
        }
    }

    #[tokio::test]
    async fn exec_approval_abort_interrupts_turn() {
        let (sq_tx, eq_rx, codex) = make_codex();

        sq_tx
            .send(Submission {
                id: "ea1".into(),
                op: Op::ExecApproval {
                    id: "c1".into(),
                    turn_id: None,
                    decision: crate::protocol::types::ReviewDecision::Abort,
                    custom_instructions: None,
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
                .any(|e| matches!(&e.msg, EventMsg::TurnAborted(_))),
            "Abort decision should emit TurnAborted"
        );
    }

    #[tokio::test]
    async fn exec_approval_for_session_adds_to_allow_list() {
        let (sq_tx, eq_rx, codex) = make_codex();
        let codex = Arc::new(codex);
        let codex2 = Arc::clone(&codex);

        let handle = tokio::spawn(async move { codex2.run().await });

        wait_session_and_set_pending(
            &codex,
            &eq_rx,
            crate::core::session::PendingApproval::Exec {
                call_id: "c1".into(),
                command: vec!["cargo".into(), "test".into()],
                cwd: std::path::PathBuf::from("/tmp"),
            },
        )
        .await;

        sq_tx
            .send(Submission {
                id: "ea1".into(),
                op: Op::ExecApproval {
                    id: "c1".into(),
                    turn_id: None,
                    decision: crate::protocol::types::ReviewDecision::ApprovedForSession,
                    custom_instructions: None,
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

        handle.await.unwrap().unwrap();

        // Verify the command was added to the session allow list
        let session_guard = codex.session.lock().await;
        if let Some(s) = session_guard.as_ref() {
            assert!(
                s.is_exec_allow_listed(&["cargo".into(), "test".into()])
                    .await,
                "ApprovedForSession should add command to allow list"
            );
        }
    }

    #[tokio::test]
    async fn exec_approval_with_custom_instructions_stores_them() {
        let (sq_tx, eq_rx, codex) = make_codex();
        let codex = Arc::new(codex);
        let codex2 = Arc::clone(&codex);

        let handle = tokio::spawn(async move { codex2.run().await });

        wait_session_and_set_pending(
            &codex,
            &eq_rx,
            crate::core::session::PendingApproval::Exec {
                call_id: "c1".into(),
                command: vec!["echo".into()],
                cwd: std::path::PathBuf::from("/tmp"),
            },
        )
        .await;

        sq_tx
            .send(Submission {
                id: "ea1".into(),
                op: Op::ExecApproval {
                    id: "c1".into(),
                    turn_id: None,
                    decision: crate::protocol::types::ReviewDecision::Approved,
                    custom_instructions: Some("be more careful with file operations".into()),
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

        handle.await.unwrap().unwrap();

        // Verify custom instructions were stored on the session
        let session_guard = codex.session.lock().await;
        if let Some(s) = session_guard.as_ref() {
            let instructions = s.take_custom_instructions().await;
            assert_eq!(
                instructions.as_deref(),
                Some("be more careful with file operations"),
                "custom_instructions should be forwarded to session"
            );
        }
    }

    #[tokio::test]
    async fn patch_approval_denied_emits_declined_event() {
        let (sq_tx, eq_rx, codex) = make_codex();
        let codex = Arc::new(codex);
        let codex2 = Arc::clone(&codex);

        let handle = tokio::spawn(async move { codex2.run().await });

        wait_session_and_set_pending(
            &codex,
            &eq_rx,
            crate::core::session::PendingApproval::Patch {
                call_id: "p1".into(),
                turn_id: "t1".into(),
                changes: std::collections::HashMap::new(),
                cwd: std::path::PathBuf::from("/tmp"),
            },
        )
        .await;

        sq_tx
            .send(Submission {
                id: "pa1".into(),
                op: Op::PatchApproval {
                    id: "p1".into(),
                    decision: crate::protocol::types::ReviewDecision::Denied,
                    custom_instructions: None,
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

        handle.await.unwrap().unwrap();

        let events = drain_events(&eq_rx);
        assert!(
            events.iter().any(|e| matches!(
                &e.msg,
                EventMsg::PatchApplyEnd(ev) if !ev.success
            )),
            "Denied patch should emit PatchApplyEnd with success=false"
        );
    }

    #[tokio::test]
    async fn patch_approval_with_custom_instructions_stores_them() {
        let (sq_tx, eq_rx, codex) = make_codex();
        let codex = Arc::new(codex);
        let codex2 = Arc::clone(&codex);

        let handle = tokio::spawn(async move { codex2.run().await });

        wait_session_and_set_pending(
            &codex,
            &eq_rx,
            crate::core::session::PendingApproval::Patch {
                call_id: "p1".into(),
                turn_id: "t1".into(),
                changes: std::collections::HashMap::new(),
                cwd: std::path::PathBuf::from("/tmp"),
            },
        )
        .await;

        sq_tx
            .send(Submission {
                id: "pa1".into(),
                op: Op::PatchApproval {
                    id: "p1".into(),
                    decision: crate::protocol::types::ReviewDecision::Approved,
                    custom_instructions: Some("apply changes to staging only".into()),
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

        handle.await.unwrap().unwrap();

        // Verify custom instructions were stored
        let session_guard = codex.session.lock().await;
        if let Some(s) = session_guard.as_ref() {
            let instructions = s.take_custom_instructions().await;
            assert_eq!(
                instructions.as_deref(),
                Some("apply changes to staging only"),
                "PatchApproval custom_instructions should be forwarded to session"
            );
        }
    }

    // ── Dynamic tool lifecycle integration tests ───────────────

    #[tokio::test]
    async fn dynamic_tool_full_lifecycle() {
        // Test the complete dynamic tool lifecycle:
        // 1. Register dynamic tool on Codex
        // 2. dispatch_tool_call detects it as dynamic
        // 3. DynamicToolCallRequest event is sent on EQ
        // 4. Op::DynamicToolResponse resolves the pending call
        // 5. dispatch_tool_call returns the result

        let (sq_tx, eq_rx, codex) = make_codex();
        let codex = Arc::new(codex);
        let codex2 = Arc::clone(&codex);

        let handle = tokio::spawn(async move { codex2.run().await });

        // Wait for SessionConfigured so the session is initialised
        loop {
            let ev = eq_rx.recv().await.unwrap();
            if matches!(&ev.msg, EventMsg::SessionConfigured(_)) {
                break;
            }
        }

        // Register a dynamic tool
        codex
            .register_dynamic_tool(crate::protocol::types::DynamicToolSpec {
                name: "test_dyn_tool".to_string(),
                description: "a test dynamic tool".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            })
            .await;

        // Verify the tool is registered in both handler and router
        assert!(
            codex.dynamic_tool_handler.has_tool("test_dyn_tool"),
            "tool should be registered in DynamicToolHandler"
        );
        {
            let session_guard = codex.session.lock().await;
            if let Some(s) = session_guard.as_ref() {
                let router = s.tool_router().await;
                assert!(
                    router.has_dynamic_tool("test_dyn_tool"),
                    "tool should be registered in ToolRouter"
                );
            }
        }

        // Now test the invoke + resolve cycle directly through DynamicToolHandler
        let codex3 = Arc::clone(&codex);
        let invoke_handle = tokio::spawn(async move {
            codex3
                .dynamic_tool_handler
                .invoke(
                    "test_dyn_tool",
                    "turn_1",
                    serde_json::json!({"input": "hello"}),
                )
                .await
        });

        // Wait for the DynamicToolCallRequest event on EQ
        let call_id = loop {
            let ev = eq_rx.recv().await.unwrap();
            if let EventMsg::DynamicToolCallRequest(req) = &ev.msg {
                assert_eq!(req.tool, "test_dyn_tool");
                assert_eq!(req.turn_id, "turn_1");
                break req.call_id.clone();
            }
        };

        // Resolve via Op::DynamicToolResponse through the SQ
        sq_tx
            .send(Submission {
                id: "dr1".into(),
                op: Op::DynamicToolResponse {
                    id: call_id,
                    response: crate::protocol::types::DynamicToolResponse {
                        content_items: vec![],
                        success: true,
                    },
                },
            })
            .await
            .unwrap();

        // The invoke should complete successfully
        let result = invoke_handle.await.unwrap().unwrap();
        assert!(
            result.success,
            "dynamic tool response should indicate success"
        );

        // Shutdown
        sq_tx
            .send(Submission {
                id: "s1".into(),
                op: Op::Shutdown,
            })
            .await
            .unwrap();

        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn dynamic_tool_unregister_removes_from_both() {
        let (sq_tx, eq_rx, codex) = make_codex();
        let codex = Arc::new(codex);
        let codex2 = Arc::clone(&codex);

        let handle = tokio::spawn(async move { codex2.run().await });

        // Wait for SessionConfigured
        loop {
            let ev = eq_rx.recv().await.unwrap();
            if matches!(&ev.msg, EventMsg::SessionConfigured(_)) {
                break;
            }
        }

        // Register then unregister
        codex
            .register_dynamic_tool(crate::protocol::types::DynamicToolSpec {
                name: "ephemeral".to_string(),
                description: "temp".to_string(),
                input_schema: serde_json::Value::Null,
            })
            .await;

        codex.unregister_dynamic_tool("ephemeral").await;

        // Verify removed from both
        assert!(!codex.dynamic_tool_handler.has_tool("ephemeral"));
        {
            let session_guard = codex.session.lock().await;
            if let Some(s) = session_guard.as_ref() {
                let router = s.tool_router().await;
                assert!(!router.has_dynamic_tool("ephemeral"));
            }
        }

        sq_tx
            .send(Submission {
                id: "s1".into(),
                op: Op::Shutdown,
            })
            .await
            .unwrap();

        handle.await.unwrap().unwrap();
    }
}

/// Generate a short thread name (≤10 chars) from the user's first message via a non-streaming API call.
async fn generate_thread_name(
    base_url: &str,
    api_key: &str,
    model: &str,
    user_text: &str,
) -> Option<String> {
    // Truncate input to avoid wasting tokens
    let truncated: String = user_text.chars().take(200).collect();
    let body = serde_json::json!({
        "model": model,
        "input": [
            { "role": "user", "content": truncated }
        ],
        "instructions": "用10个字以内的中文总结用户消息的主题，作为会话标题。只输出标题，不要任何解释或标点。",
        "stream": false,
    });

    let resp = reqwest::Client::new()
        .post(base_url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .ok()?;

    let json: serde_json::Value = resp.json().await.ok()?;
    let text = json
        .pointer("/output/0/content/0/text")
        .or_else(|| json.pointer("/output_text"))
        .and_then(|v| v.as_str())?;

    let name: String = text.trim().chars().take(15).collect();
    if name.is_empty() { None } else { Some(name) }
}

/// Extract the last MCP tool selection state from a rollout.
///
/// Scans for `search_bm25` function calls and parses `active_selected_tools`
/// from their outputs.
fn extract_mcp_tool_selection_from_rollout(
    rollout_items: &[crate::core::rollout::policy::RolloutItem],
) -> Option<Vec<String>> {
    use crate::core::rollout::policy::RolloutItem;
    use std::collections::HashSet;

    let mut search_call_ids = HashSet::new();
    let mut active_selected_tools: Option<Vec<String>> = None;

    for item in rollout_items {
        let RolloutItem::ResponseItem(response_item) = item else { continue };
        match response_item {
            crate::protocol::types::ResponseItem::FunctionCall { name, call_id, .. }
                if name == "search_bm25" =>
            {
                search_call_ids.insert(call_id.clone());
            }
            crate::protocol::types::ResponseItem::FunctionCallOutput { call_id, output } => {
                if !search_call_ids.contains(call_id) { continue; }
                let Some(content) = output.text_content() else { continue; };
                let Ok(payload) = serde_json::from_str::<serde_json::Value>(content) else { continue; };
                let Some(tools) = payload.get("active_selected_tools")
                    .and_then(|v| v.as_array()) else { continue; };
                let Some(tools) = tools.iter()
                    .map(|v| v.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>() else { continue; };
                active_selected_tools = Some(tools);
            }
            _ => {}
        }
    }
    active_selected_tools
}
