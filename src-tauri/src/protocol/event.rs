use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::types::{
    AskForApproval, CodexErrorInfo, DynamicToolCallOutputContentItem, DynamicToolCallRequest,
    ExecCommandSource, ExecCommandStatus, FileChange, McpInvocation, McpStartupFailure,
    McpStartupStatus, ModeKind, ParsedCommand, PatchApplyStatus, SandboxPolicy, TokenUsageInfo,
    TurnAbortReason,
};

// ── Event wrapper ────────────────────────────────────────────────

/// Event Queue Entry — events from agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub msg: EventMsg,
}

// ── Individual event structs ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEvent {
    pub message: String,
    #[serde(
        default,
        rename = "codexErrorInfo",
        skip_serializing_if = "Option::is_none"
    )]
    pub codex_error_info: Option<CodexErrorInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WarningEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfiguredEvent {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub model: String,
    #[serde(rename = "modelProviderId")]
    pub model_provider_id: String,
    #[serde(rename = "approvalPolicy")]
    pub approval_policy: AskForApproval,
    #[serde(rename = "sandboxPolicy")]
    pub sandbox_policy: SandboxPolicy,
    pub cwd: PathBuf,
    #[serde(rename = "historyLogId")]
    pub history_log_id: u64,
    #[serde(rename = "historyEntryCount")]
    pub history_entry_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadNameUpdatedEvent {
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(rename = "threadName", skip_serializing_if = "Option::is_none")]
    pub thread_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartedEvent {
    #[serde(rename = "turnId")]
    pub turn_id: String,
    #[serde(rename = "modelContextWindow", skip_serializing_if = "Option::is_none")]
    pub model_context_window: Option<i64>,
    #[serde(default, rename = "collaborationModeKind")]
    pub collaboration_mode_kind: ModeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnCompleteEvent {
    #[serde(rename = "turnId")]
    pub turn_id: String,
    #[serde(rename = "lastAgentMessage", skip_serializing_if = "Option::is_none")]
    pub last_agent_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnAbortedEvent {
    #[serde(rename = "turnId", skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub reason: TurnAbortReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TokenCountEvent {
    pub info: Option<TokenUsageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentReasoningDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanDeltaEvent {
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(rename = "turnId")]
    pub turn_id: String,
    #[serde(rename = "itemId")]
    pub item_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompactedEvent;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRolledBackEvent {
    #[serde(rename = "numTurns")]
    pub num_turns: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemStartedEvent {
    #[serde(rename = "turnId")]
    pub turn_id: String,
    #[serde(rename = "itemId")]
    pub item_id: String,
    #[serde(rename = "itemType")]
    pub item_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemCompletedEvent {
    #[serde(rename = "turnId")]
    pub turn_id: String,
    #[serde(rename = "itemId")]
    pub item_id: String,
    #[serde(rename = "itemType")]
    pub item_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RawResponseItemEvent {
    pub item: serde_json::Value,
}

// ── Command execution events ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecCommandBeginEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    #[serde(rename = "turnId")]
    pub turn_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    #[serde(rename = "parsedCmd")]
    pub parsed_cmd: Vec<ParsedCommand>,
    #[serde(default)]
    pub source: ExecCommandSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecCommandEndEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    #[serde(rename = "turnId")]
    pub turn_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    #[serde(rename = "parsedCmd")]
    pub parsed_cmd: Vec<ParsedCommand>,
    #[serde(default)]
    pub source: ExecCommandSource,
    pub stdout: String,
    pub stderr: String,
    #[serde(rename = "exitCode")]
    pub exit_code: i32,
    #[serde(rename = "formattedOutput")]
    pub formatted_output: String,
    pub status: ExecCommandStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecCommandOutputDeltaEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    pub delta: String,
}

// ── Approval request events ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecApprovalRequestEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    #[serde(rename = "turnId")]
    pub turn_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    #[serde(rename = "parsedCmd")]
    pub parsed_cmd: Vec<ParsedCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPatchApprovalRequestEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    #[serde(rename = "turnId")]
    pub turn_id: String,
    pub changes: HashMap<PathBuf, FileChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationRequestEvent {
    #[serde(rename = "serverName")]
    pub server_name: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

// ── Patch events ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PatchApplyBeginEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    #[serde(default, rename = "turnId")]
    pub turn_id: String,
    #[serde(rename = "autoApproved")]
    pub auto_approved: bool,
    pub changes: HashMap<PathBuf, FileChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PatchApplyEndEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    #[serde(default, rename = "turnId")]
    pub turn_id: String,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    #[serde(default)]
    pub changes: HashMap<PathBuf, FileChange>,
    pub status: PatchApplyStatus,
}

// ── MCP events ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallBeginEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    pub invocation: McpInvocation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallEndEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    pub invocation: McpInvocation,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpStartupUpdateEvent {
    pub server: String,
    pub status: McpStartupStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpStartupCompleteEvent {
    pub ready: Vec<String>,
    pub failed: Vec<McpStartupFailure>,
    pub cancelled: Vec<String>,
}

// ── Dynamic tool events ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolCallResponseEvent {
    #[serde(rename = "callId")]
    pub call_id: String,
    #[serde(rename = "turnId")]
    pub turn_id: String,
    pub tool: String,
    pub arguments: serde_json::Value,
    #[serde(rename = "contentItems")]
    pub content_items: Vec<DynamicToolCallOutputContentItem>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Undo events ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UndoStartedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UndoCompletedEvent {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ── Stream error event ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StreamErrorEvent {
    pub message: String,
    #[serde(
        default,
        rename = "codexErrorInfo",
        skip_serializing_if = "Option::is_none"
    )]
    pub codex_error_info: Option<CodexErrorInfo>,
    #[serde(
        default,
        rename = "additionalDetails",
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_details: Option<String>,
}

// ── Misc events ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundEventEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeprecationNoticeEvent {
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnDiffEvent {
    #[serde(rename = "unifiedDiff")]
    pub unified_diff: String,
}

// ── EventMsg enum ────────────────────────────────────────────────

/// All possible event messages emitted by the core engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum EventMsg {
    // --- Errors & warnings ---
    Error(ErrorEvent),
    Warning(WarningEvent),

    // --- Session lifecycle ---
    SessionConfigured(SessionConfiguredEvent),
    ThreadNameUpdated(ThreadNameUpdatedEvent),

    // --- Turn lifecycle ---
    TurnStarted(TurnStartedEvent),
    TurnComplete(TurnCompleteEvent),
    TurnAborted(TurnAbortedEvent),

    // --- Token usage ---
    TokenCount(TokenCountEvent),

    // --- Agent messages ---
    AgentMessage(AgentMessageEvent),
    AgentMessageDelta(AgentMessageDeltaEvent),
    AgentReasoningDelta(AgentReasoningDeltaEvent),

    // --- Structured items ---
    ItemStarted(ItemStartedEvent),
    ItemCompleted(ItemCompletedEvent),
    RawResponseItem(RawResponseItemEvent),
    PlanDelta(PlanDeltaEvent),

    // --- Command execution ---
    ExecCommandBegin(ExecCommandBeginEvent),
    ExecCommandEnd(ExecCommandEndEvent),
    ExecCommandOutputDelta(ExecCommandOutputDeltaEvent),

    // --- Approval requests ---
    ExecApprovalRequest(ExecApprovalRequestEvent),
    ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent),
    ElicitationRequest(ElicitationRequestEvent),

    // --- Patch application ---
    PatchApplyBegin(PatchApplyBeginEvent),
    PatchApplyEnd(PatchApplyEndEvent),

    // --- MCP ---
    McpToolCallBegin(McpToolCallBeginEvent),
    McpToolCallEnd(McpToolCallEndEvent),
    McpStartupUpdate(McpStartupUpdateEvent),
    McpStartupComplete(McpStartupCompleteEvent),

    // --- Dynamic tools ---
    DynamicToolCallRequest(DynamicToolCallRequest),
    DynamicToolCallResponse(DynamicToolCallResponseEvent),

    // --- History / context ---
    ContextCompacted(ContextCompactedEvent),
    ThreadRolledBack(ThreadRolledBackEvent),
    TurnDiff(TurnDiffEvent),

    // --- Undo ---
    UndoStarted(UndoStartedEvent),
    UndoCompleted(UndoCompletedEvent),

    // --- Stream errors ---
    StreamError(StreamErrorEvent),

    // --- Misc ---
    BackgroundEvent(BackgroundEventEvent),
    DeprecationNotice(DeprecationNoticeEvent),
    ShutdownComplete,
}
