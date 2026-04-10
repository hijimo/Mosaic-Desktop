//! Persistence policy: decides which rollout items are written to disk.

use crate::protocol::event::EventMsg;

/// Controls how much detail is persisted in rollout files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EventPersistenceMode {
    /// Only core conversation events (messages, turns, compaction).
    #[default]
    Limited,
    /// Also includes tool call results, errors, etc.
    Extended,
}

/// A rollout item that can be persisted.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RolloutItem {
    /// Session metadata (first line of a rollout file).
    SessionMeta(SessionMetaLine),
    /// A protocol event.
    EventMsg(EventMsg),
    /// A compaction marker.
    Compacted(CompactedItem),
    /// Turn context snapshot.
    TurnContext(TurnContextItem),
    /// A structured response item (message, function call, function output).
    ResponseItem(crate::protocol::types::ResponseItem),
}

/// Session metadata written as the first line of a rollout file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SessionMetaLine {
    pub meta: SessionMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitInfo>,
}

/// Core session metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SessionMeta {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<String>,
    pub timestamp: String,
    pub cwd: std::path::PathBuf,
    pub cli_version: String,
    pub source: SessionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_nickname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mode: Option<String>,
}

/// Where the session originated.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    #[default]
    Desktop,
    Cli,
    Exec,
    Api,
    Mcp,
    SubAgent(SubAgentSource),
    /// Catch-all for unknown/future variants during deserialization.
    /// IMPORTANT: Must remain the last variant for `#[serde(other)]` to work.
    #[serde(other)]
    Unknown,
}

/// How a sub-agent session was created.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentSource {
    Review,
    Compact,
    ThreadSpawn {
        parent_thread_id: crate::protocol::thread_id::ThreadId,
        depth: i32,
        #[serde(default)]
        agent_nickname: Option<String>,
        #[serde(default, alias = "agent_type")]
        agent_role: Option<String>,
    },
    MemoryConsolidation,
    Other(String),
}

impl std::fmt::Display for SessionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionSource::Desktop => f.write_str("desktop"),
            SessionSource::Cli => f.write_str("cli"),
            SessionSource::Exec => f.write_str("exec"),
            SessionSource::Api => f.write_str("api"),
            SessionSource::Mcp => f.write_str("mcp"),
            SessionSource::SubAgent(sub) => write!(f, "subagent_{sub}"),
            SessionSource::Unknown => f.write_str("unknown"),
        }
    }
}

impl std::fmt::Display for SubAgentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubAgentSource::Review => f.write_str("review"),
            SubAgentSource::Compact => f.write_str("compact"),
            SubAgentSource::ThreadSpawn { .. } => f.write_str("thread_spawn"),
            SubAgentSource::MemoryConsolidation => f.write_str("memory_consolidation"),
            SubAgentSource::Other(s) => write!(f, "other({s})"),
        }
    }
}

impl SessionSource {
    pub fn get_nickname(&self) -> Option<String> {
        match self {
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn { agent_nickname, .. }) => {
                agent_nickname.clone()
            }
            SessionSource::SubAgent(SubAgentSource::MemoryConsolidation) => {
                Some("Morpheus".to_string())
            }
            _ => None,
        }
    }

    pub fn get_agent_role(&self) -> Option<String> {
        match self {
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn { agent_role, .. }) => {
                agent_role.clone()
            }
            SessionSource::SubAgent(SubAgentSource::MemoryConsolidation) => {
                Some("memory builder".to_string())
            }
            _ => None,
        }
    }
}

/// Git repository information captured at session start.
pub use crate::core::git_info::GitInfo;

/// Compaction marker — indicates the conversation was compacted.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CompactedItem {
    pub message: String,
    /// Structured replacement history (if available). When present, this
    /// replaces the entire conversation history up to this point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_history: Option<Vec<crate::protocol::types::ResponseInputItem>>,
}

/// Snapshot of turn context at the start of each turn.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TurnContextItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub cwd: std::path::PathBuf,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realtime_active: Option<bool>,
}

/// A single line in a rollout JSONL file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RolloutLine {
    pub timestamp: String,
    #[serde(flatten)]
    pub item: RolloutItem,
}

/// Whether a rollout item should be persisted for the given mode.
pub fn is_persisted(item: &RolloutItem, mode: EventPersistenceMode) -> bool {
    match item {
        RolloutItem::SessionMeta(_)
        | RolloutItem::Compacted(_)
        | RolloutItem::TurnContext(_)
        | RolloutItem::ResponseItem(_) => true,
        RolloutItem::EventMsg(ev) => should_persist_event(ev, mode),
    }
}

/// Determine whether an event should be persisted.
fn should_persist_event(ev: &EventMsg, mode: EventPersistenceMode) -> bool {
    match mode {
        EventPersistenceMode::Limited => is_limited_event(ev),
        EventPersistenceMode::Extended => is_limited_event(ev) || is_extended_event(ev),
    }
}

/// Events always persisted (core conversation flow).
fn is_limited_event(ev: &EventMsg) -> bool {
    matches!(
        ev,
        EventMsg::UserMessage(_)
            | EventMsg::AgentMessage(_)
            | EventMsg::AgentReasoning(_)
            | EventMsg::AgentReasoningRawContent(_)
            | EventMsg::TokenCount(_)
            | EventMsg::ContextCompacted(_)
            | EventMsg::EnteredReviewMode(_)
            | EventMsg::ExitedReviewMode(_)
            | EventMsg::ThreadRolledBack(_)
            | EventMsg::UndoCompleted(_)
            | EventMsg::TurnAborted(_)
            | EventMsg::TurnStarted(_)
            | EventMsg::TurnComplete(_)
    )
}

/// Events persisted only in Extended mode.
fn is_extended_event(ev: &EventMsg) -> bool {
    matches!(
        ev,
        EventMsg::Error(_)
            | EventMsg::WebSearchBegin(_)
            | EventMsg::WebSearchEnd(_)
            | EventMsg::ExecCommandEnd(_)
            | EventMsg::PatchApplyEnd(_)
            | EventMsg::McpToolCallBegin(_)
            | EventMsg::McpToolCallEnd(_)
            | EventMsg::ElicitationRequest(_)
            | EventMsg::ElicitationResponse(_)
            | EventMsg::ViewImageToolCall(_)
            | EventMsg::CollabAgentSpawnEnd(_)
            | EventMsg::CollabAgentInteractionEnd(_)
            | EventMsg::CollabWaitingEnd(_)
            | EventMsg::CollabCloseEnd(_)
            | EventMsg::CollabResumeEnd(_)
            | EventMsg::DynamicToolCallRequest(_)
            | EventMsg::DynamicToolCallResponse(_)
    )
}
