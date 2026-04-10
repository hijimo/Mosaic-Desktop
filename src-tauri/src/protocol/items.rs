use serde::{Deserialize, Serialize};

use super::event::{
    AgentMessageEvent, AgentReasoningEvent, AgentReasoningRawContentEvent, ContextCompactedEvent,
    EventMsg, UserMessageEvent, WebSearchEndEvent,
};
use super::types::{ByteRange, MessagePhase, TextElement, UserInput, WebSearchAction};
use std::collections::HashMap;
use std::path::PathBuf;

// ── TurnItem ─────────────────────────────────────────────────────

/// Structured item within a turn, used by v2 `ItemStarted`/`ItemCompleted`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TurnItem {
    UserMessage(UserMessageItem),
    AgentMessage(AgentMessageItem),
    Plan(PlanItem),
    Reasoning(ReasoningItem),
    WebSearch(WebSearchItem),
    ContextCompaction(ContextCompactionItem),
    CommandExecution(CommandExecutionItem),
    McpToolCall(McpToolCallItem),
    DynamicToolCall(DynamicToolCallItem),
    FileChange(FileChangeItem),
    ImageView(ImageViewItem),
    EnteredReviewMode(EnteredReviewModeItem),
    ExitedReviewMode(ExitedReviewModeItem),
    CollabToolCall(CollabToolCallItem),
    Elicitation(ElicitationItem),
}

impl TurnItem {
    pub fn id(&self) -> &str {
        match self {
            Self::UserMessage(i) => &i.id,
            Self::AgentMessage(i) => &i.id,
            Self::Plan(i) => &i.id,
            Self::Reasoning(i) => &i.id,
            Self::WebSearch(i) => &i.id,
            Self::ContextCompaction(i) => &i.id,
            Self::CommandExecution(i) => &i.id,
            Self::McpToolCall(i) => &i.id,
            Self::DynamicToolCall(i) => &i.id,
            Self::FileChange(i) => &i.id,
            Self::ImageView(i) => &i.id,
            Self::EnteredReviewMode(i) => &i.id,
            Self::ExitedReviewMode(i) => &i.id,
            Self::CollabToolCall(i) => &i.id,
            Self::Elicitation(i) => &i.id,
        }
    }
}

// ── Sub-item types ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserMessageItem {
    pub id: String,
    pub content: Vec<UserInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentMessageContent {
    Text { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessageItem {
    pub id: String,
    pub content: Vec<AgentMessageContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<MessagePhase>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanItem {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningItem {
    pub id: String,
    pub summary_text: Vec<String>,
    #[serde(default)]
    pub raw_content: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchItem {
    pub id: String,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<WebSearchAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextCompactionItem {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandExecutionItem {
    pub id: String,
    pub command: String,
    pub cwd: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<String>,
    pub status: CommandExecutionStatus,
    #[serde(default)]
    pub command_actions: Vec<CommandAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregated_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommandExecutionStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CommandAction {
    Read {
        command: String,
        name: String,
        path: PathBuf,
    },
    ListFiles {
        command: String,
        path: Option<String>,
    },
    Search {
        command: String,
        query: Option<String>,
        path: Option<String>,
    },
    Unknown {
        command: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolCallItem {
    pub id: String,
    pub server: String,
    pub tool: String,
    pub status: McpToolCallStatus,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<McpToolCallResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<McpToolCallError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum McpToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolCallResult {
    #[serde(default)]
    pub content: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolCallError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynamicToolCallItem {
    pub id: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    pub status: DynamicToolCallStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_items: Option<Vec<crate::protocol::types::DynamicToolCallOutputContentItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DynamicToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileChangeItem {
    pub id: String,
    pub changes: Vec<FileUpdateChange>,
    pub status: PatchApplyItemStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PatchApplyItemStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileUpdateChange {
    pub path: String,
    pub kind: PatchChangeKind,
    #[serde(default)]
    pub diff: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PatchChangeKind {
    Add,
    Delete,
    Update {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        move_path: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageViewItem {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnteredReviewModeItem {
    pub id: String,
    pub review: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExitedReviewModeItem {
    pub id: String,
    pub review: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElicitationItem {
    pub id: String,
    pub server_name: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// User's decision: "accept", "decline", or "cancel". Populated from ElicitationResponse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_action: Option<String>,
    /// User-provided form data (when action is accept in form mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollabToolCallItem {
    pub id: String,
    pub tool: CollabAgentTool,
    pub status: CollabAgentToolCallStatus,
    pub sender_thread_id: String,
    #[serde(default)]
    pub receiver_thread_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default)]
    pub agents_states: HashMap<String, CollabAgentState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CollabAgentTool {
    SpawnAgent,
    SendInput,
    ResumeAgent,
    Wait,
    CloseAgent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CollabAgentToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollabAgentState {
    pub status: CollabAgentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CollabAgentStatus {
    PendingInit,
    Running,
    Completed,
    Errored,
    Shutdown,
    NotFound,
}

impl Default for ContextCompactionItem {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

// ── Legacy event conversion ──────────────────────────────────────

impl UserMessageItem {
    pub fn new(content: &[UserInput]) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.to_vec(),
        }
    }

    pub fn message(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                UserInput::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn text_elements(&self) -> Vec<TextElement> {
        let mut out = Vec::new();
        let mut offset = 0usize;
        for input in &self.content {
            if let UserInput::Text {
                text,
                text_elements,
            } = input
            {
                for elem in text_elements {
                    out.push(TextElement::new(
                        ByteRange {
                            start: offset + elem.byte_range.start,
                            end: offset + elem.byte_range.end,
                        },
                        elem.placeholder(text).map(str::to_string),
                    ));
                }
                offset += text.len();
            }
        }
        out
    }

    pub fn image_urls(&self) -> Vec<String> {
        self.content
            .iter()
            .filter_map(|c| match c {
                UserInput::Image { image_url } => Some(image_url.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn local_image_paths(&self) -> Vec<std::path::PathBuf> {
        self.content
            .iter()
            .filter_map(|c| match c {
                UserInput::LocalImage { path } => Some(path.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn as_legacy_event(&self) -> EventMsg {
        EventMsg::UserMessage(UserMessageEvent {
            message: self.message(),
            images: Some(self.image_urls()),
            local_images: self.local_image_paths(),
            text_elements: self.text_elements(),
        })
    }
}

impl AgentMessageItem {
    pub fn new(content: &[AgentMessageContent]) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.to_vec(),
            phase: None,
        }
    }

    pub fn as_legacy_events(&self) -> Vec<EventMsg> {
        self.content
            .iter()
            .map(|c| match c {
                AgentMessageContent::Text { text } => EventMsg::AgentMessage(AgentMessageEvent {
                    message: text.clone(),
                    phase: self.phase.clone(),
                }),
            })
            .collect()
    }
}

impl ReasoningItem {
    pub fn as_legacy_events(&self, show_raw: bool) -> Vec<EventMsg> {
        let mut events: Vec<EventMsg> = self
            .summary_text
            .iter()
            .map(|s| EventMsg::AgentReasoning(AgentReasoningEvent { text: s.clone() }))
            .collect();
        if show_raw {
            for entry in &self.raw_content {
                events.push(EventMsg::AgentReasoningRawContent(
                    AgentReasoningRawContentEvent {
                        text: entry.clone(),
                    },
                ));
            }
        }
        events
    }
}

impl WebSearchItem {
    pub fn as_legacy_event(&self) -> EventMsg {
        EventMsg::WebSearchEnd(WebSearchEndEvent {
            call_id: self.id.clone(),
            query: self.query.clone(),
            action: self.action.clone().unwrap_or(WebSearchAction::Other),
        })
    }
}

impl ContextCompactionItem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn as_legacy_event(&self) -> EventMsg {
        EventMsg::ContextCompacted(ContextCompactedEvent)
    }
}

impl TurnItem {
    pub fn as_legacy_events(&self, show_raw_reasoning: bool) -> Vec<EventMsg> {
        match self {
            Self::UserMessage(i) => vec![i.as_legacy_event()],
            Self::AgentMessage(i) => i.as_legacy_events(),
            Self::Plan(_) => Vec::new(),
            Self::Reasoning(i) => i.as_legacy_events(show_raw_reasoning),
            Self::WebSearch(i) => vec![i.as_legacy_event()],
            Self::ContextCompaction(i) => vec![i.as_legacy_event()],
            Self::CommandExecution(_)
            | Self::McpToolCall(_)
            | Self::DynamicToolCall(_)
            | Self::FileChange(_)
            | Self::ImageView(_)
            | Self::EnteredReviewMode(_)
            | Self::ExitedReviewMode(_)
            | Self::CollabToolCall(_)
            | Self::Elicitation(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_item_agent_message_roundtrip() {
        let item = TurnItem::AgentMessage(AgentMessageItem {
            id: "test-1".into(),
            content: vec![AgentMessageContent::Text {
                text: "hello".into(),
            }],
            phase: None,
        });
        let json = serde_json::to_string(&item).unwrap();
        let parsed: TurnItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, parsed);
    }

    #[test]
    fn turn_item_id_returns_inner_id() {
        let item = TurnItem::Plan(PlanItem {
            id: "plan-42".into(),
            text: "step 1".into(),
        });
        assert_eq!(item.id(), "plan-42");
    }
}
