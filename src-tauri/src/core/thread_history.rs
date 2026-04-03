//! Build thread history (Vec<TurnGroup>) from rollout items.
//!
//! Ported from codex-main's `ThreadHistoryBuilder` with adaptations for
//! Mosaic's type system.

use crate::core::rollout::policy::{CompactedItem, RolloutItem};
use crate::protocol::event::*;
use crate::protocol::items::*;
use crate::protocol::types::{
    AgentStatus, DynamicToolCallRequest, FileChange, MessagePhase, UserInput, WebSearchAction,
};
use std::collections::HashMap;
use std::path::PathBuf;

// ── Public types ─────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TurnGroup {
    pub turn_id: String,
    pub items: Vec<TurnItem>,
    #[serde(default)]
    pub status: TurnStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<TurnError>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub enum TurnStatus {
    #[default]
    Completed,
    Interrupted,
    Failed,
    InProgress,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TurnError {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_error_info: Option<crate::protocol::types::CodexErrorInfo>,
}

// ── Builder ──────────────────────────────────────────────────────

pub fn build_turn_groups_from_rollout_items(items: &[RolloutItem]) -> Vec<TurnGroup> {
    let mut builder = ThreadHistoryBuilder::new();
    for item in items {
        builder.handle_rollout_item(item);
    }
    builder.finish()
}

struct PendingTurn {
    id: String,
    items: Vec<TurnItem>,
    status: TurnStatus,
    error: Option<TurnError>,
    opened_explicitly: bool,
    saw_compaction: bool,
}

pub struct ThreadHistoryBuilder {
    turns: Vec<TurnGroup>,
    current_turn: Option<PendingTurn>,
    next_item_index: i64,
}

impl ThreadHistoryBuilder {
    pub fn new() -> Self {
        Self {
            turns: Vec::new(),
            current_turn: None,
            next_item_index: 1,
        }
    }

    pub fn finish(mut self) -> Vec<TurnGroup> {
        self.finish_current_turn();
        self.turns
    }

    pub fn handle_rollout_item(&mut self, item: &RolloutItem) {
        match item {
            RolloutItem::EventMsg(event) => self.handle_event(event),
            RolloutItem::Compacted(_) => {
                self.ensure_turn().saw_compaction = true;
            }
            RolloutItem::ResponseItem(ri) => self.handle_response_item(ri),
            RolloutItem::TurnContext(_) | RolloutItem::SessionMeta(_) => {}
        }
    }

    fn handle_response_item(&mut self, ri: &crate::protocol::types::ResponseItem) {
        use crate::protocol::types::{ContentItem, ResponseItem};
        match ri {
            ResponseItem::Message {
                role,
                content,
                phase,
                id,
                ..
            } => {
                if role == "user" {
                    if crate::core::event_mapping::is_contextual_user_message(content) {
                        return;
                    }
                    let item_id = self.next_item_id();
                    let user_content: Vec<crate::protocol::types::UserInput> = content
                        .iter()
                        .filter_map(|c| match c {
                            ContentItem::InputText { text } => {
                                // Detect serialized attached_file: [attached_file:name](path)
                                if let Some(rest) = text.strip_prefix("[attached_file:") {
                                    if let Some(paren) = rest.find("](") {
                                        let name = &rest[..paren];
                                        let path = &rest[paren + 2..rest.len() - 1];
                                        return Some(crate::protocol::types::UserInput::AttachedFile {
                                            name: name.to_string(),
                                            path: std::path::PathBuf::from(path),
                                        });
                                    }
                                }
                                Some(crate::protocol::types::UserInput::Text {
                                    text: text.clone(),
                                    text_elements: vec![],
                                })
                            }
                            ContentItem::InputImage { image_url } => {
                                if let Some(path) = image_url.strip_prefix("file://") {
                                    Some(crate::protocol::types::UserInput::LocalImage {
                                        path: std::path::PathBuf::from(path),
                                    })
                                } else {
                                    Some(crate::protocol::types::UserInput::Image {
                                        image_url: image_url.clone(),
                                    })
                                }
                            }
                            _ => None,
                        })
                        .collect();
                    if !user_content.is_empty() {
                        // Close implicit turn on new user message
                        if let Some(turn) = self.current_turn.as_ref() {
                            if !turn.opened_explicitly
                                && !(turn.saw_compaction && turn.items.is_empty())
                            {
                                self.finish_current_turn();
                            }
                        }
                        self.ensure_turn()
                            .items
                            .push(TurnItem::UserMessage(UserMessageItem {
                                id: item_id,
                                content: user_content,
                            }));
                    }
                } else if role == "assistant" {
                    let text: String = content
                        .iter()
                        .filter_map(|c| match c {
                            ContentItem::OutputText { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect();
                    if !text.is_empty() {
                        let item_id = self.next_item_id();
                        self.ensure_turn()
                            .items
                            .push(TurnItem::AgentMessage(AgentMessageItem {
                                id: item_id,
                                content: vec![AgentMessageContent::Text { text }],
                                phase: phase.clone(),
                            }));
                    }
                }
            }
            ResponseItem::Reasoning {
                id,
                summary,
                content,
                ..
            } => {
                let summary_texts: Vec<String> = summary
                    .iter()
                    .filter_map(|s| match s {
                        crate::protocol::types::ReasoningSummaryItem::SummaryText { text } => {
                            Some(text.clone())
                        }
                    })
                    .collect();
                let raw_texts: Vec<String> = content
                    .as_ref()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|c| match c {
                                crate::protocol::types::ReasoningContentItem::Text { text } => {
                                    Some(text.clone())
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if !summary_texts.is_empty() || !raw_texts.is_empty() {
                    // Try merge with last reasoning
                    if let Some(ct) = self.current_turn.as_mut() {
                        if let Some(TurnItem::Reasoning(r)) = ct.items.last_mut() {
                            r.summary_text.extend(summary_texts);
                            r.raw_content.extend(raw_texts);
                            return;
                        }
                    }
                    let item_id = if id.is_empty() {
                        self.next_item_id()
                    } else {
                        id.clone()
                    };
                    self.ensure_turn()
                        .items
                        .push(TurnItem::Reasoning(ReasoningItem {
                            id: item_id,
                            summary_text: summary_texts,
                            raw_content: raw_texts,
                        }));
                }
            }
            // FunctionCall, FunctionCallOutput, etc. — not rendered as TurnItems
            _ => {
                // Use event_mapping for web_search_call etc.
                if let Some(ti) = crate::core::event_mapping::parse_turn_item(ri) {
                    self.ensure_turn().items.push(ti);
                }
            }
        }
    }

    pub fn handle_event(&mut self, event: &EventMsg) {
        match event {
            EventMsg::UserMessage(p) => self.handle_user_message(p),
            EventMsg::AgentMessage(p) => self.handle_agent_message(&p.message, &p.phase),
            EventMsg::AgentReasoning(p) => self.handle_agent_reasoning(p),
            EventMsg::AgentReasoningRawContent(p) => self.handle_agent_reasoning_raw(p),
            EventMsg::WebSearchEnd(p) => self.handle_web_search_end(p),
            EventMsg::ExecCommandEnd(p) => self.handle_exec_command_end(p),
            EventMsg::McpToolCallEnd(p) => self.handle_mcp_tool_call_end(p),
            EventMsg::PatchApplyEnd(p) => self.handle_patch_apply_end(p),
            EventMsg::ApplyPatchApprovalRequest(p) => self.handle_patch_approval(p),
            EventMsg::DynamicToolCallRequest(p) => self.handle_dynamic_request(p),
            EventMsg::DynamicToolCallResponse(p) => self.handle_dynamic_response(p),
            EventMsg::ViewImageToolCall(p) => self.handle_view_image(p),
            EventMsg::CollabAgentSpawnEnd(p) => self.handle_collab_spawn_end(p),
            EventMsg::CollabAgentInteractionEnd(p) => self.handle_collab_interaction_end(p),
            EventMsg::CollabWaitingEnd(p) => self.handle_collab_waiting_end(p),
            EventMsg::CollabCloseEnd(p) => self.handle_collab_close_end(p),
            EventMsg::CollabResumeEnd(p) => self.handle_collab_resume_end(p),
            EventMsg::ContextCompacted(_) => {
                let id = self.next_item_id();
                self.ensure_turn()
                    .items
                    .push(TurnItem::ContextCompaction(ContextCompactionItem { id }));
            }
            EventMsg::EnteredReviewMode(p) => {
                let id = self.next_item_id();
                let review = p
                    .user_facing_hint
                    .clone()
                    .unwrap_or_else(|| "Review requested.".to_string());
                self.ensure_turn()
                    .items
                    .push(TurnItem::EnteredReviewMode(EnteredReviewModeItem {
                        id,
                        review,
                    }));
            }
            EventMsg::ExitedReviewMode(p) => {
                let id = self.next_item_id();
                let review = p
                    .review_output
                    .as_ref()
                    .map(|o| {
                        if o.overall_explanation.trim().is_empty() {
                            "Reviewer failed to output a response.".to_string()
                        } else {
                            o.overall_explanation.trim().to_string()
                        }
                    })
                    .unwrap_or_else(|| "Reviewer failed to output a response.".to_string());
                self.ensure_turn()
                    .items
                    .push(TurnItem::ExitedReviewMode(ExitedReviewModeItem {
                        id,
                        review,
                    }));
            }
            EventMsg::ItemStarted(p) => self.handle_item_started(p),
            EventMsg::ItemCompleted(p) => self.handle_item_completed(p),
            EventMsg::Error(p) => self.handle_error(p),
            EventMsg::ThreadRolledBack(p) => self.handle_thread_rollback(p),
            EventMsg::TurnAborted(p) => self.handle_turn_aborted(p),
            EventMsg::TurnStarted(p) => self.handle_turn_started(p),
            EventMsg::TurnComplete(p) => self.handle_turn_complete(p),
            _ => {}
        }
    }

    // ── Turn lifecycle ───────────────────────────────────────────

    fn handle_turn_started(&mut self, p: &TurnStartedEvent) {
        self.finish_current_turn();
        self.current_turn = Some(PendingTurn {
            id: p.turn_id.clone(),
            items: Vec::new(),
            status: TurnStatus::InProgress,
            error: None,
            opened_explicitly: true,
            saw_compaction: false,
        });
    }

    fn handle_turn_complete(&mut self, p: &TurnCompleteEvent) {
        let mark = |s: &mut TurnStatus| {
            if matches!(s, TurnStatus::Completed | TurnStatus::InProgress) {
                *s = TurnStatus::Completed;
            }
        };
        if let Some(ct) = self.current_turn.as_mut().filter(|t| t.id == p.turn_id) {
            mark(&mut ct.status);
            self.finish_current_turn();
            return;
        }
        if let Some(t) = self.turns.iter_mut().find(|t| t.turn_id == p.turn_id) {
            mark(&mut t.status);
            return;
        }
        if let Some(ct) = self.current_turn.as_mut() {
            mark(&mut ct.status);
            self.finish_current_turn();
        }
    }

    fn handle_turn_aborted(&mut self, p: &TurnAbortedEvent) {
        if let Some(tid) = p.turn_id.as_deref() {
            if let Some(ct) = self.current_turn.as_mut().filter(|t| t.id == tid) {
                ct.status = TurnStatus::Interrupted;
                return;
            }
            if let Some(t) = self.turns.iter_mut().find(|t| t.turn_id == tid) {
                t.status = TurnStatus::Interrupted;
                return;
            }
        }
        if let Some(ct) = self.current_turn.as_mut() {
            ct.status = TurnStatus::Interrupted;
        }
    }

    fn handle_error(&mut self, p: &ErrorEvent) {
        if let Some(ref info) = p.codex_error_info {
            if !info.affects_turn_status() {
                return;
            }
        }
        let Some(turn) = self.current_turn.as_mut() else {
            return;
        };
        turn.status = TurnStatus::Failed;
        turn.error = Some(TurnError {
            message: p.message.clone(),
            codex_error_info: p.codex_error_info.clone(),
        });
    }

    fn handle_thread_rollback(&mut self, p: &ThreadRolledBackEvent) {
        self.finish_current_turn();
        let n = p.num_turns as usize;
        if n >= self.turns.len() {
            self.turns.clear();
        } else {
            self.turns.truncate(self.turns.len() - n);
        }
        let item_count: usize = self.turns.iter().map(|t| t.items.len()).sum();
        self.next_item_index = (item_count + 1) as i64;
    }

    fn finish_current_turn(&mut self) {
        if let Some(turn) = self.current_turn.take() {
            if turn.items.is_empty() && !turn.opened_explicitly && !turn.saw_compaction {
                return;
            }
            self.turns.push(TurnGroup {
                turn_id: turn.id,
                items: turn.items,
                status: turn.status,
                error: turn.error,
            });
        }
    }

    // ── Message handlers ─────────────────────────────────────────

    fn handle_user_message(&mut self, p: &UserMessageEvent) {
        // Close implicit turn on new user message (codex-main behavior)
        if let Some(turn) = self.current_turn.as_ref() {
            if !turn.opened_explicitly && !(turn.saw_compaction && turn.items.is_empty()) {
                self.finish_current_turn();
            }
        }
        let id = self.next_item_id();
        let mut content = Vec::new();
        if !p.message.trim().is_empty() {
            content.push(UserInput::Text {
                text: p.message.clone(),
                text_elements: p.text_elements.clone(),
            });
        }
        if let Some(images) = &p.images {
            for url in images {
                content.push(UserInput::Image {
                    image_url: url.clone(),
                });
            }
        }
        for path in &p.local_images {
            content.push(UserInput::LocalImage { path: path.clone() });
        }
        self.ensure_turn()
            .items
            .push(TurnItem::UserMessage(UserMessageItem { id, content }));
    }

    fn handle_agent_message(&mut self, text: &str, phase: &Option<MessagePhase>) {
        if text.is_empty() {
            return;
        }
        let id = self.next_item_id();
        self.ensure_turn()
            .items
            .push(TurnItem::AgentMessage(AgentMessageItem {
                id,
                content: vec![AgentMessageContent::Text {
                    text: text.to_string(),
                }],
                phase: phase.clone(),
            }));
    }

    fn handle_agent_reasoning(&mut self, p: &AgentReasoningEvent) {
        if p.text.is_empty() {
            return;
        }
        // Try merge first
        if let Some(ct) = self.current_turn.as_mut() {
            if let Some(TurnItem::Reasoning(r)) = ct.items.last_mut() {
                r.summary_text.push(p.text.clone());
                return;
            }
        }
        let id = self.next_item_id();
        self.ensure_turn()
            .items
            .push(TurnItem::Reasoning(ReasoningItem {
                id,
                summary_text: vec![p.text.clone()],
                raw_content: vec![],
            }));
    }

    fn handle_agent_reasoning_raw(&mut self, p: &AgentReasoningRawContentEvent) {
        if p.text.is_empty() {
            return;
        }
        if let Some(ct) = self.current_turn.as_mut() {
            if let Some(TurnItem::Reasoning(r)) = ct.items.last_mut() {
                r.raw_content.push(p.text.clone());
                return;
            }
        }
        let id = self.next_item_id();
        self.ensure_turn()
            .items
            .push(TurnItem::Reasoning(ReasoningItem {
                id,
                summary_text: vec![],
                raw_content: vec![p.text.clone()],
            }));
    }

    fn handle_item_started(&mut self, p: &ItemStartedEvent) {
        if let crate::protocol::items::TurnItem::Plan(plan) = &p.item {
            if !plan.text.is_empty() {
                self.upsert_item_in_turn_id(&p.turn_id, TurnItem::Plan(plan.clone()));
            }
        }
    }

    fn handle_item_completed(&mut self, p: &ItemCompletedEvent) {
        if let crate::protocol::items::TurnItem::Plan(plan) = &p.item {
            if !plan.text.is_empty() {
                self.upsert_item_in_turn_id(&p.turn_id, TurnItem::Plan(plan.clone()));
            }
        }
    }

    // ── Tool call handlers ───────────────────────────────────────

    fn handle_web_search_end(&mut self, p: &WebSearchEndEvent) {
        let item = TurnItem::WebSearch(WebSearchItem {
            id: p.call_id.clone(),
            query: p.query.clone(),
            action: Some(p.action.clone()),
        });
        self.upsert_item_in_current_turn(item);
    }

    // ── Collab handlers ──────────────────────────────────────────

    fn handle_collab_spawn_end(&mut self, p: &CollabAgentSpawnEndEvent) {
        let collab_status = to_collab_agent_state(&p.status);
        let tool_status = if matches!(p.status, AgentStatus::Errored(_) | AgentStatus::NotFound) {
            CollabAgentToolCallStatus::Failed
        } else {
            CollabAgentToolCallStatus::Completed
        };
        let receiver_ids: Vec<String> = p.new_thread_id.iter().cloned().collect();
        let agents_states = p
            .new_thread_id
            .iter()
            .map(|id| (id.clone(), collab_status.clone()))
            .collect();
        let item = TurnItem::CollabToolCall(CollabToolCallItem {
            id: p.call_id.clone(),
            tool: CollabAgentTool::SpawnAgent,
            status: tool_status,
            sender_thread_id: p.sender_thread_id.clone(),
            receiver_thread_ids: receiver_ids,
            prompt: Some(p.prompt.clone()),
            agents_states,
        });
        self.upsert_item_in_current_turn(item);
    }

    fn handle_collab_interaction_end(&mut self, p: &CollabAgentInteractionEndEvent) {
        let collab_status = to_collab_agent_state(&p.status);
        let tool_status = if matches!(p.status, AgentStatus::Errored(_) | AgentStatus::NotFound) {
            CollabAgentToolCallStatus::Failed
        } else {
            CollabAgentToolCallStatus::Completed
        };
        let item = TurnItem::CollabToolCall(CollabToolCallItem {
            id: p.call_id.clone(),
            tool: CollabAgentTool::SendInput,
            status: tool_status,
            sender_thread_id: p.sender_thread_id.clone(),
            receiver_thread_ids: vec![p.receiver_thread_id.clone()],
            prompt: Some(p.prompt.clone()),
            agents_states: [(p.receiver_thread_id.clone(), collab_status)]
                .into_iter()
                .collect(),
        });
        self.upsert_item_in_current_turn(item);
    }

    fn handle_collab_waiting_end(&mut self, p: &CollabWaitingEndEvent) {
        let mut receiver_ids: Vec<String> = p.statuses.keys().cloned().collect();
        receiver_ids.sort();
        let agents_states: HashMap<String, CollabAgentState> = p
            .statuses
            .iter()
            .map(|(id, s)| (id.clone(), to_collab_agent_state(s)))
            .collect();
        let has_error = p
            .statuses
            .values()
            .any(|s| matches!(s, AgentStatus::Errored(_) | AgentStatus::NotFound));
        let tool_status = if has_error {
            CollabAgentToolCallStatus::Failed
        } else {
            CollabAgentToolCallStatus::Completed
        };
        let item = TurnItem::CollabToolCall(CollabToolCallItem {
            id: p.call_id.clone(),
            tool: CollabAgentTool::Wait,
            status: tool_status,
            sender_thread_id: p.sender_thread_id.clone(),
            receiver_thread_ids: receiver_ids,
            prompt: None,
            agents_states,
        });
        self.upsert_item_in_current_turn(item);
    }

    fn handle_collab_close_end(&mut self, p: &CollabCloseEndEvent) {
        let collab_status = to_collab_agent_state(&p.status);
        let tool_status = if matches!(p.status, AgentStatus::Errored(_) | AgentStatus::NotFound) {
            CollabAgentToolCallStatus::Failed
        } else {
            CollabAgentToolCallStatus::Completed
        };
        let item = TurnItem::CollabToolCall(CollabToolCallItem {
            id: p.call_id.clone(),
            tool: CollabAgentTool::CloseAgent,
            status: tool_status,
            sender_thread_id: p.sender_thread_id.clone(),
            receiver_thread_ids: vec![p.receiver_thread_id.clone()],
            prompt: None,
            agents_states: [(p.receiver_thread_id.clone(), collab_status)]
                .into_iter()
                .collect(),
        });
        self.upsert_item_in_current_turn(item);
    }

    fn handle_collab_resume_end(&mut self, p: &CollabResumeEndEvent) {
        let collab_status = to_collab_agent_state(&p.status);
        let tool_status = if matches!(p.status, AgentStatus::Errored(_) | AgentStatus::NotFound) {
            CollabAgentToolCallStatus::Failed
        } else {
            CollabAgentToolCallStatus::Completed
        };
        let item = TurnItem::CollabToolCall(CollabToolCallItem {
            id: p.call_id.clone(),
            tool: CollabAgentTool::ResumeAgent,
            status: tool_status,
            sender_thread_id: p.sender_thread_id.clone(),
            receiver_thread_ids: vec![p.receiver_thread_id.clone()],
            prompt: None,
            agents_states: [(p.receiver_thread_id.clone(), collab_status)]
                .into_iter()
                .collect(),
        });
        self.upsert_item_in_current_turn(item);
    }

    fn handle_exec_command_end(&mut self, p: &ExecCommandEndEvent) {
        let cmd_str = p.command.join(" ");
        let status = match p.status {
            crate::protocol::types::ExecCommandStatus::Completed => {
                CommandExecutionStatus::Completed
            }
            crate::protocol::types::ExecCommandStatus::Failed => CommandExecutionStatus::Failed,
            crate::protocol::types::ExecCommandStatus::Declined => CommandExecutionStatus::Declined,
        };
        let output = if p.aggregated_output.is_empty() {
            None
        } else {
            Some(p.aggregated_output.clone())
        };
        let duration_ms = i64::try_from(p.duration.as_millis()).ok();
        let command_actions: Vec<CommandAction> = p
            .parsed_cmd
            .iter()
            .map(|pc| match pc {
                crate::protocol::types::ParsedCommand::Read { cmd, name, path } => {
                    CommandAction::Read {
                        command: cmd.clone(),
                        name: name.clone(),
                        path: path.clone(),
                    }
                }
                crate::protocol::types::ParsedCommand::ListFiles { cmd, path } => {
                    CommandAction::ListFiles {
                        command: cmd.clone(),
                        path: path.clone(),
                    }
                }
                crate::protocol::types::ParsedCommand::Search { cmd, query, path } => {
                    CommandAction::Search {
                        command: cmd.clone(),
                        query: query.clone(),
                        path: path.clone(),
                    }
                }
                crate::protocol::types::ParsedCommand::Unknown { cmd } => CommandAction::Unknown {
                    command: cmd.clone(),
                },
            })
            .collect();
        let item = TurnItem::CommandExecution(CommandExecutionItem {
            id: p.call_id.clone(),
            command: cmd_str,
            cwd: p.cwd.clone(),
            process_id: p.process_id.clone(),
            status,
            command_actions,
            aggregated_output: output,
            exit_code: Some(p.exit_code),
            duration_ms,
        });
        self.upsert_item_in_turn_id(&p.turn_id, item);
    }

    fn handle_mcp_tool_call_end(&mut self, p: &McpToolCallEndEvent) {
        let (status, result, error) = match &p.result {
            Ok(_) => (McpToolCallStatus::Completed, None, None),
            Err(msg) => (
                McpToolCallStatus::Failed,
                None,
                Some(McpToolCallError {
                    message: msg.clone(),
                }),
            ),
        };
        let duration_ms = i64::try_from(p.duration.as_millis()).ok();
        let item = TurnItem::McpToolCall(McpToolCallItem {
            id: p.call_id.clone(),
            server: p.invocation.server.clone(),
            tool: p.invocation.tool.clone(),
            status,
            arguments: p
                .invocation
                .arguments
                .clone()
                .unwrap_or(serde_json::Value::Null),
            result,
            error,
            duration_ms,
        });
        self.upsert_item_in_current_turn(item);
    }

    fn handle_patch_apply_end(&mut self, p: &PatchApplyEndEvent) {
        let status = match p.status {
            crate::protocol::types::PatchApplyStatus::Completed => PatchApplyItemStatus::Completed,
            crate::protocol::types::PatchApplyStatus::Failed => PatchApplyItemStatus::Failed,
            crate::protocol::types::PatchApplyStatus::Declined => PatchApplyItemStatus::Declined,
        };
        let item = TurnItem::FileChange(FileChangeItem {
            id: p.call_id.clone(),
            changes: convert_patch_changes(&p.changes),
            status,
        });
        if p.turn_id.is_empty() {
            self.upsert_item_in_current_turn(item);
        } else {
            self.upsert_item_in_turn_id(&p.turn_id, item);
        }
    }

    fn handle_patch_approval(&mut self, p: &ApplyPatchApprovalRequestEvent) {
        let item = TurnItem::FileChange(FileChangeItem {
            id: p.call_id.clone(),
            changes: convert_patch_changes(&p.changes),
            status: PatchApplyItemStatus::InProgress,
        });
        if p.turn_id.is_empty() {
            self.upsert_item_in_current_turn(item);
        } else {
            self.upsert_item_in_turn_id(&p.turn_id, item);
        }
    }

    fn handle_dynamic_request(&mut self, p: &DynamicToolCallRequest) {
        let item = TurnItem::DynamicToolCall(DynamicToolCallItem {
            id: p.call_id.clone(),
            tool: p.tool.clone(),
            arguments: p.arguments.clone(),
            status: DynamicToolCallStatus::InProgress,
            content_items: None,
            success: None,
            duration_ms: None,
        });
        if p.turn_id.is_empty() {
            self.upsert_item_in_current_turn(item);
        } else {
            self.upsert_item_in_turn_id(&p.turn_id, item);
        }
    }

    fn handle_dynamic_response(&mut self, p: &DynamicToolCallResponseEvent) {
        let status = if p.success {
            DynamicToolCallStatus::Completed
        } else {
            DynamicToolCallStatus::Failed
        };
        let duration_ms = p
            .duration
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        let item = TurnItem::DynamicToolCall(DynamicToolCallItem {
            id: p.call_id.clone(),
            tool: p.tool.clone(),
            arguments: p.arguments.clone(),
            status,
            content_items: Some(p.content_items.clone()),
            success: Some(p.success),
            duration_ms,
        });
        if p.turn_id.is_empty() {
            self.upsert_item_in_current_turn(item);
        } else {
            self.upsert_item_in_turn_id(&p.turn_id, item);
        }
    }

    fn handle_view_image(&mut self, p: &ViewImageToolCallEvent) {
        let item = TurnItem::ImageView(ImageViewItem {
            id: p.call_id.clone(),
            path: p.path.to_string_lossy().into_owned(),
        });
        self.upsert_item_in_current_turn(item);
    }

    // ── Helpers ───────────────────────────────────────────────────

    fn ensure_turn(&mut self) -> &mut PendingTurn {
        if self.current_turn.is_none() {
            self.current_turn = Some(PendingTurn {
                id: uuid::Uuid::now_v7().to_string(),
                items: Vec::new(),
                status: TurnStatus::Completed,
                error: None,
                opened_explicitly: false,
                saw_compaction: false,
            });
        }
        self.current_turn.as_mut().unwrap()
    }

    fn next_item_id(&mut self) -> String {
        let id = format!("item-{}", self.next_item_index);
        self.next_item_index += 1;
        id
    }

    fn upsert_item_in_current_turn(&mut self, item: TurnItem) {
        let turn = self.ensure_turn();
        upsert_turn_item(&mut turn.items, item);
    }

    fn upsert_item_in_turn_id(&mut self, turn_id: &str, item: TurnItem) {
        if let Some(ct) = self.current_turn.as_mut().filter(|t| t.id == turn_id) {
            upsert_turn_item(&mut ct.items, item);
            return;
        }
        if let Some(t) = self.turns.iter_mut().find(|t| t.turn_id == turn_id) {
            upsert_turn_item(&mut t.items, item);
            return;
        }
        // Unknown turn_id — append to current turn as fallback
        self.upsert_item_in_current_turn(item);
    }
}

fn upsert_turn_item(items: &mut Vec<TurnItem>, item: TurnItem) {
    if let Some(existing) = items.iter_mut().find(|e| e.id() == item.id()) {
        *existing = item;
        return;
    }
    items.push(item);
}

fn to_collab_agent_state(status: &AgentStatus) -> CollabAgentState {
    match status {
        AgentStatus::PendingInit => CollabAgentState {
            status: CollabAgentStatus::PendingInit,
            message: None,
        },
        AgentStatus::Running => CollabAgentState {
            status: CollabAgentStatus::Running,
            message: None,
        },
        AgentStatus::Completed(msg) => CollabAgentState {
            status: CollabAgentStatus::Completed,
            message: msg.clone(),
        },
        AgentStatus::Errored(msg) => CollabAgentState {
            status: CollabAgentStatus::Errored,
            message: Some(msg.clone()),
        },
        AgentStatus::Shutdown => CollabAgentState {
            status: CollabAgentStatus::Shutdown,
            message: None,
        },
        AgentStatus::NotFound => CollabAgentState {
            status: CollabAgentStatus::NotFound,
            message: None,
        },
    }
}

fn convert_patch_changes(changes: &HashMap<PathBuf, FileChange>) -> Vec<FileUpdateChange> {
    let mut result: Vec<FileUpdateChange> = changes
        .iter()
        .map(|(path, change)| {
            let (kind, diff) = match change {
                FileChange::Add { content } => (PatchChangeKind::Add, content.clone()),
                FileChange::Delete { content } => (PatchChangeKind::Delete, content.clone()),
                FileChange::Update {
                    unified_diff,
                    move_path,
                } => {
                    let d = if let Some(p) = move_path {
                        format!("{unified_diff}\n\nMoved to: {}", p.display())
                    } else {
                        unified_diff.clone()
                    };
                    (
                        PatchChangeKind::Update {
                            move_path: move_path.clone(),
                        },
                        d,
                    )
                }
            };
            FileUpdateChange {
                path: path.to_string_lossy().into_owned(),
                kind,
                diff,
            }
        })
        .collect();
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rollout::policy::RolloutItem;
    use crate::protocol::event::*;
    use crate::protocol::types::{McpInvocation, ModeKind, TurnAbortReason};

    fn ev(e: EventMsg) -> RolloutItem {
        RolloutItem::EventMsg(e)
    }

    fn user(msg: &str) -> RolloutItem {
        ev(EventMsg::UserMessage(UserMessageEvent {
            message: msg.into(),
            images: None,
            text_elements: vec![],
            local_images: vec![],
        }))
    }

    fn agent(msg: &str) -> RolloutItem {
        ev(EventMsg::AgentMessage(AgentMessageEvent {
            message: msg.into(),
            phase: None,
        }))
    }

    fn turn_start(id: &str) -> RolloutItem {
        ev(EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: id.into(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }))
    }

    fn turn_end(id: &str) -> RolloutItem {
        ev(EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: id.into(),
            last_agent_message: None,
        }))
    }

    fn rollback(n: u32) -> RolloutItem {
        ev(EventMsg::ThreadRolledBack(ThreadRolledBackEvent {
            num_turns: n,
        }))
    }

    fn reasoning(text: &str) -> RolloutItem {
        ev(EventMsg::AgentReasoning(AgentReasoningEvent {
            text: text.into(),
        }))
    }

    fn reasoning_raw(text: &str) -> RolloutItem {
        ev(EventMsg::AgentReasoningRawContent(
            AgentReasoningRawContentEvent { text: text.into() },
        ))
    }

    #[test]
    fn basic_two_turn_conversation() {
        let items = vec![
            turn_start("t1"),
            user("hello"),
            agent("hi"),
            turn_end("t1"),
            turn_start("t2"),
            user("bye"),
            agent("goodbye"),
            turn_end("t2"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].turn_id, "t1");
        assert_eq!(turns[0].items.len(), 2);
        assert_eq!(turns[0].status, TurnStatus::Completed);
        assert_eq!(turns[1].turn_id, "t2");
        assert_eq!(turns[1].items.len(), 2);
    }

    #[test]
    fn implicit_turn_split_on_user_message() {
        let items = vec![user("first"), agent("a1"), user("second"), agent("a2")];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].items.len(), 2);
        assert_eq!(turns[1].items.len(), 2);
    }

    #[test]
    fn rollback_removes_turns() {
        let items = vec![
            turn_start("t1"),
            user("u1"),
            agent("a1"),
            turn_end("t1"),
            turn_start("t2"),
            user("u2"),
            agent("a2"),
            turn_end("t2"),
            rollback(1),
            turn_start("t3"),
            user("u3"),
            agent("a3"),
            turn_end("t3"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].turn_id, "t1");
        assert_eq!(turns[1].turn_id, "t3");
    }

    #[test]
    fn turn_aborted_marks_interrupted() {
        let items = vec![
            turn_start("t1"),
            user("u1"),
            agent("working..."),
            ev(EventMsg::TurnAborted(TurnAbortedEvent {
                turn_id: Some("t1".into()),
                reason: TurnAbortReason::Replaced,
            })),
            turn_start("t2"),
            user("retry"),
            agent("done"),
            turn_end("t2"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].status, TurnStatus::Interrupted);
        assert_eq!(turns[1].status, TurnStatus::Completed);
    }

    #[test]
    fn error_marks_turn_failed() {
        let items = vec![
            turn_start("t1"),
            user("u1"),
            ev(EventMsg::Error(ErrorEvent {
                message: "boom".into(),
                codex_error_info: Some(crate::protocol::types::CodexErrorInfo::BadRequest),
            })),
            turn_end("t1"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].status, TurnStatus::Failed);
        assert_eq!(turns[0].error.as_ref().unwrap().message, "boom");
        assert!(turns[0].error.as_ref().unwrap().codex_error_info.is_some());
    }

    #[test]
    fn rollback_error_does_not_mark_turn_failed() {
        let items = vec![
            turn_start("t1"),
            user("hello"),
            agent("done"),
            ev(EventMsg::Error(ErrorEvent {
                message: "rollback failed".into(),
                codex_error_info: Some(
                    crate::protocol::types::CodexErrorInfo::ThreadRollbackFailed,
                ),
            })),
            turn_end("t1"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns[0].status, TurnStatus::Completed);
        assert!(turns[0].error.is_none());
    }

    #[test]
    fn out_of_turn_error_does_not_create_turn() {
        let items = vec![
            turn_start("t1"),
            user("hello"),
            turn_end("t1"),
            ev(EventMsg::Error(ErrorEvent {
                message: "request-level failure".into(),
                codex_error_info: Some(crate::protocol::types::CodexErrorInfo::BadRequest),
            })),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].status, TurnStatus::Completed);
    }

    #[test]
    fn reasoning_merges_consecutive() {
        let items = vec![
            turn_start("t1"),
            user("q"),
            reasoning("think1"),
            reasoning("think2"),
            reasoning_raw("raw1"),
            agent("answer"),
            reasoning("new_block"),
            turn_end("t1"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns[0].items.len(), 4); // user, reasoning(merged), agent, reasoning(new)
        if let TurnItem::Reasoning(r) = &turns[0].items[1] {
            assert_eq!(r.summary_text, vec!["think1", "think2"]);
            assert_eq!(r.raw_content, vec!["raw1"]);
        } else {
            panic!("expected Reasoning");
        }
    }

    #[test]
    fn upsert_updates_existing_item_by_id() {
        let items = vec![
            turn_start("t1"),
            user("run"),
            ev(EventMsg::WebSearchEnd(WebSearchEndEvent {
                call_id: "ws1".into(),
                query: "".into(),
                action: crate::protocol::types::WebSearchAction::Other,
            })),
            ev(EventMsg::WebSearchEnd(WebSearchEndEvent {
                call_id: "ws1".into(),
                query: "codex".into(),
                action: crate::protocol::types::WebSearchAction::Other,
            })),
            turn_end("t1"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        // Should have user + 1 web search (upserted, not 2)
        assert_eq!(turns[0].items.len(), 2);
        if let TurnItem::WebSearch(ws) = &turns[0].items[1] {
            assert_eq!(ws.query, "codex");
        } else {
            panic!("expected WebSearch");
        }
    }

    #[test]
    fn exec_command_routed_to_original_turn() {
        let items = vec![
            turn_start("t-a"),
            user("first"),
            turn_end("t-a"),
            turn_start("t-b"),
            user("second"),
            ev(EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id: "exec-late".into(),
                process_id: None,
                turn_id: "t-a".into(),
                command: vec!["echo".into(), "done".into()],
                cwd: std::path::PathBuf::from("/tmp"),
                parsed_cmd: vec![],
                source: crate::protocol::types::ExecCommandSource::Agent,
                interaction_input: None,
                stdout: String::new(),
                stderr: String::new(),
                aggregated_output: "done\n".into(),
                exit_code: 0,
                duration: std::time::Duration::from_millis(5),
                formatted_output: String::new(),
                status: crate::protocol::types::ExecCommandStatus::Completed,
            })),
            turn_end("t-b"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].turn_id, "t-a");
        assert_eq!(turns[0].items.len(), 2); // user + exec
        assert_eq!(turns[1].items.len(), 1); // user only
    }

    #[test]
    fn empty_rollout_returns_empty() {
        assert!(build_turn_groups_from_rollout_items(&[]).is_empty());
    }

    #[test]
    fn explicit_empty_turn_preserved() {
        let items = vec![turn_start("t1"), turn_end("t1")];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns.len(), 1);
        assert!(turns[0].items.is_empty());
    }

    #[test]
    fn mcp_tool_call_end_creates_item() {
        let items = vec![
            turn_start("t1"),
            user("run mcp"),
            ev(EventMsg::McpToolCallEnd(McpToolCallEndEvent {
                call_id: "mcp-1".into(),
                invocation: McpInvocation {
                    server: "docs".into(),
                    tool: "lookup".into(),
                    arguments: Some(serde_json::json!({"id":"123"})),
                },
                duration: std::time::Duration::from_millis(8),
                result: Err("boom".into()),
            })),
            turn_end("t1"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns[0].items.len(), 2);
        if let TurnItem::McpToolCall(m) = &turns[0].items[1] {
            assert_eq!(m.tool, "lookup");
            assert_eq!(m.status, McpToolCallStatus::Failed);
            assert_eq!(m.error.as_ref().unwrap().message, "boom");
        } else {
            panic!("expected McpToolCall");
        }
    }

    #[test]
    fn collab_spawn_end_creates_item() {
        let items = vec![
            turn_start("t1"),
            user("spawn agent"),
            ev(EventMsg::CollabAgentSpawnEnd(CollabAgentSpawnEndEvent {
                call_id: "spawn-1".into(),
                sender_thread_id: "parent".into(),
                new_thread_id: Some("child-1".into()),
                new_agent_nickname: Some("explorer".into()),
                new_agent_role: None,
                prompt: "do the thing".into(),
                status: crate::protocol::types::AgentStatus::Completed(None),
            })),
            turn_end("t1"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        assert_eq!(turns[0].items.len(), 2);
        if let TurnItem::CollabToolCall(c) = &turns[0].items[1] {
            assert_eq!(c.tool, CollabAgentTool::SpawnAgent);
            assert_eq!(c.status, CollabAgentToolCallStatus::Completed);
            assert_eq!(c.sender_thread_id, "parent");
            assert_eq!(c.receiver_thread_ids, vec!["child-1"]);
            assert_eq!(c.prompt.as_deref(), Some("do the thing"));
        } else {
            panic!("expected CollabToolCall");
        }
    }

    #[test]
    fn collab_waiting_end_detects_errors() {
        let items = vec![
            turn_start("t1"),
            user("wait"),
            ev(EventMsg::CollabWaitingEnd(CollabWaitingEndEvent {
                call_id: "wait-1".into(),
                sender_thread_id: "parent".into(),
                statuses: [
                    (
                        "a".into(),
                        crate::protocol::types::AgentStatus::Completed(None),
                    ),
                    (
                        "b".into(),
                        crate::protocol::types::AgentStatus::Errored("oops".into()),
                    ),
                ]
                .into_iter()
                .collect(),
            })),
            turn_end("t1"),
        ];
        let turns = build_turn_groups_from_rollout_items(&items);
        if let TurnItem::CollabToolCall(c) = &turns[0].items[1] {
            assert_eq!(c.tool, CollabAgentTool::Wait);
            assert_eq!(c.status, CollabAgentToolCallStatus::Failed);
        } else {
            panic!("expected CollabToolCall");
        }
    }
}
