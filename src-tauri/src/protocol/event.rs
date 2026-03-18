use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use super::types::{
    AskForApproval, CallToolResult, CodexErrorInfo, DynamicToolCallOutputContentItem,
    DynamicToolCallRequest, ExecCommandSource, ExecCommandStatus, ExecOutputStream,
    ExecPolicyAmendment, FileChange, McpInvocation, McpStartupFailure, McpStartupStatus, ModeKind,
    ModelRerouteReason, NetworkApprovalContext, NetworkPolicyAmendment, ParsedCommand,
    PatchApplyStatus, RateLimitSnapshot, ReviewDecision, ReviewRequest, SandboxPolicy,
    TextElement, TokenUsageInfo, TurnAbortReason,
};

// ── Event wrapper ────────────────────────────────────────────────

/// Event Queue Entry — events from agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub id: String,
    pub msg: EventMsg,
}

// ── Individual event structs ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorEvent {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_error_info: Option<CodexErrorInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WarningEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelRerouteEvent {
    pub from_model: String,
    pub to_model: String,
    pub reason: ModelRerouteReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionConfiguredEvent {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_name: Option<String>,
    pub model: String,
    pub model_provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<AskForApproval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_policy: Option<SandboxPolicy>,
    pub cwd: PathBuf,
    pub history_log_id: u64,
    pub history_entry_count: usize,
    #[serde(default)]
    pub mode: ModeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<super::types::Effort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<super::types::ReasoningSummary>,
    #[serde(default)]
    pub can_append: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThreadNameUpdatedEvent {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnStartedEvent {
    pub turn_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_context_window: Option<i64>,
    #[serde(default)]
    pub collaboration_mode_kind: ModeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnCompleteEvent {
    pub turn_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_agent_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnAbortedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub reason: TurnAbortReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenCountEvent {
    pub info: Option<TokenUsageInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limits: Option<RateLimitSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMessageEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserMessageEvent {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    #[serde(default)]
    pub local_images: Vec<PathBuf>,
    #[serde(default)]
    pub text_elements: Vec<TextElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMessageDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentReasoningEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentReasoningDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentReasoningRawContentEvent {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentReasoningRawContentDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentReasoningSectionBreakEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

// ── Structured item events ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemStartedEvent {
    pub thread_id: String,
    pub turn_id: String,
    pub item: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemCompletedEvent {
    pub thread_id: String,
    pub turn_id: String,
    pub item: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMessageContentDeltaEvent {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanDeltaEvent {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReasoningContentDeltaEvent {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub delta: String,
    #[serde(default)]
    pub summary_index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReasoningRawContentDeltaEvent {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub delta: String,
    #[serde(default)]
    pub content_index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawResponseItemEvent {
    pub item: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextCompactedEvent;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThreadRolledBackEvent {
    pub num_turns: u32,
}

// ── Command execution events ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecCommandBeginEvent {
    pub call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<String>,
    pub turn_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub parsed_cmd: Vec<ParsedCommand>,
    #[serde(default)]
    pub source: ExecCommandSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_input: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecCommandEndEvent {
    pub call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<String>,
    pub turn_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub parsed_cmd: Vec<ParsedCommand>,
    #[serde(default)]
    pub source: ExecCommandSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_input: Option<String>,
    pub stdout: String,
    pub stderr: String,
    #[serde(default)]
    pub aggregated_output: String,
    pub exit_code: i32,
    pub duration: Duration,
    pub formatted_output: String,
    pub status: ExecCommandStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecCommandOutputDeltaEvent {
    pub call_id: String,
    pub delta: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<ExecOutputStream>,
}

// ── Terminal interaction event ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalInteractionEvent {
    pub call_id: String,
    pub process_id: String,
    pub stdin: String,
}

// ── View image event ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ViewImageToolCallEvent {
    pub call_id: String,
    pub path: PathBuf,
}

// ── Approval request events ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecApprovalRequestEvent {
    pub call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(default)]
    pub turn_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_approval_context: Option<NetworkApprovalContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_network_policy_amendments: Option<Vec<NetworkPolicyAmendment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_permissions: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_decisions: Option<Vec<ReviewDecision>>,
    pub parsed_cmd: Vec<ParsedCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestUserInputEvent {
    pub id: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApplyPatchApprovalRequestEvent {
    pub call_id: String,
    #[serde(default)]
    pub turn_id: String,
    pub changes: HashMap<PathBuf, FileChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElicitationRequestEvent {
    pub server_name: String,
    pub request_id: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

// ── Patch events ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatchApplyBeginEvent {
    pub call_id: String,
    #[serde(default)]
    pub turn_id: String,
    pub auto_approved: bool,
    pub changes: HashMap<PathBuf, FileChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatchApplyEndEvent {
    pub call_id: String,
    #[serde(default)]
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
pub struct McpToolCallBeginEvent {
    pub call_id: String,
    pub invocation: McpInvocation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolCallEndEvent {
    pub call_id: String,
    pub invocation: McpInvocation,
    pub duration: Duration,
    pub result: Result<CallToolResult, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpStartupUpdateEvent {
    pub server: String,
    pub status: McpStartupStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct McpStartupCompleteEvent {
    pub ready: Vec<String>,
    pub failed: Vec<McpStartupFailure>,
    pub cancelled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpListToolsResponseEvent {
    pub tools: HashMap<String, serde_json::Value>,
}

// ── Web search events ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebSearchBeginEvent {
    pub call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebSearchEndEvent {
    pub call_id: String,
    pub query: String,
    pub action: serde_json::Value,
}

// ── Dynamic tool events ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicToolCallResponseEvent {
    pub call_id: String,
    pub turn_id: String,
    pub tool: String,
    pub arguments: serde_json::Value,
    pub content_items: Vec<DynamicToolCallOutputContentItem>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<Duration>,
}

// ── Undo events ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UndoStartedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UndoCompletedEvent {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ── Stream error event ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamErrorEvent {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_error_info: Option<CodexErrorInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_details: Option<String>,
}

// ── Misc events ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackgroundEventEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeprecationNoticeEvent {
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnDiffEvent {
    pub unified_diff: String,
}

// ── History / skills / prompts response events ───────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetHistoryEntryResponseEvent {
    pub offset: usize,
    pub log_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListCustomPromptsResponseEvent {
    pub custom_prompts: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListSkillsResponseEvent {
    pub skills: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListRemoteSkillsResponseEvent {
    pub skills: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteSkillDownloadedEvent {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

// ── Plan update event ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanUpdateEvent {
    pub plan: serde_json::Value,
}

// ── Review mode events ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExitedReviewModeEvent {
    pub review_output: Option<serde_json::Value>,
}

// ── Realtime conversation events ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeConversationStartedEvent {
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeConversationRealtimeEvent {
    pub event: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeConversationClosedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── Collab events ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabAgentSpawnBeginEvent {
    pub call_id: String,
    pub sender_thread_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabAgentSpawnEndEvent {
    pub call_id: String,
    pub agents: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabAgentInteractionBeginEvent {
    pub call_id: String,
    pub sender_thread_id: String,
    pub receiver_thread_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabAgentInteractionEndEvent {
    pub call_id: String,
    pub sender_thread_id: String,
    pub receiver_thread_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabWaitingBeginEvent {
    pub call_id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabWaitingEndEvent {
    pub call_id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabCloseBeginEvent {
    pub call_id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabCloseEndEvent {
    pub call_id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabResumeBeginEvent {
    pub call_id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollabResumeEndEvent {
    pub call_id: String,
    pub thread_id: String,
}

// ── EventMsg enum ────────────────────────────────────────────────

/// All possible event messages emitted by the core engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EventMsg {
    // --- Errors & warnings ---
    Error(ErrorEvent),
    Warning(WarningEvent),

    // --- Realtime conversation ---
    RealtimeConversationStarted(RealtimeConversationStartedEvent),
    RealtimeConversationRealtime(RealtimeConversationRealtimeEvent),
    RealtimeConversationClosed(RealtimeConversationClosedEvent),

    // --- Model reroute ---
    ModelReroute(ModelRerouteEvent),

    // --- Context management ---
    ContextCompacted(ContextCompactedEvent),
    ThreadRolledBack(ThreadRolledBackEvent),

    // --- Turn lifecycle ---
    #[serde(rename = "task_started", alias = "turn_started")]
    TurnStarted(TurnStartedEvent),
    #[serde(rename = "task_complete", alias = "turn_complete")]
    TurnComplete(TurnCompleteEvent),
    TurnAborted(TurnAbortedEvent),

    // --- Token usage ---
    TokenCount(TokenCountEvent),

    // --- Agent messages ---
    AgentMessage(AgentMessageEvent),
    UserMessage(UserMessageEvent),
    AgentMessageDelta(AgentMessageDeltaEvent),
    AgentReasoning(AgentReasoningEvent),
    AgentReasoningDelta(AgentReasoningDeltaEvent),
    AgentReasoningRawContent(AgentReasoningRawContentEvent),
    AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent),
    AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent),

    // --- Session lifecycle ---
    SessionConfigured(SessionConfiguredEvent),
    ThreadNameUpdated(ThreadNameUpdatedEvent),

    // --- MCP ---
    McpStartupUpdate(McpStartupUpdateEvent),
    McpStartupComplete(McpStartupCompleteEvent),
    McpToolCallBegin(McpToolCallBeginEvent),
    McpToolCallEnd(McpToolCallEndEvent),
    McpListToolsResponse(McpListToolsResponseEvent),

    // --- Web search ---
    WebSearchBegin(WebSearchBeginEvent),
    WebSearchEnd(WebSearchEndEvent),

    // --- Command execution ---
    ExecCommandBegin(ExecCommandBeginEvent),
    ExecCommandOutputDelta(ExecCommandOutputDeltaEvent),
    TerminalInteraction(TerminalInteractionEvent),
    ExecCommandEnd(ExecCommandEndEvent),

    // --- View image ---
    ViewImageToolCall(ViewImageToolCallEvent),

    // --- Approval requests ---
    ExecApprovalRequest(ExecApprovalRequestEvent),
    RequestUserInput(RequestUserInputEvent),
    ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent),
    ElicitationRequest(ElicitationRequestEvent),

    // --- Dynamic tools ---
    DynamicToolCallRequest(DynamicToolCallRequest),
    DynamicToolCallResponse(DynamicToolCallResponseEvent),

    // --- Patch application ---
    PatchApplyBegin(PatchApplyBeginEvent),
    PatchApplyEnd(PatchApplyEndEvent),

    // --- Structured items ---
    RawResponseItem(RawResponseItemEvent),
    ItemStarted(ItemStartedEvent),
    ItemCompleted(ItemCompletedEvent),
    AgentMessageContentDelta(AgentMessageContentDeltaEvent),
    PlanDelta(PlanDeltaEvent),
    ReasoningContentDelta(ReasoningContentDeltaEvent),
    ReasoningRawContentDelta(ReasoningRawContentDeltaEvent),

    // --- History / context ---
    TurnDiff(TurnDiffEvent),
    GetHistoryEntryResponse(GetHistoryEntryResponseEvent),
    ListCustomPromptsResponse(ListCustomPromptsResponseEvent),

    // --- Skills ---
    ListSkillsResponse(ListSkillsResponseEvent),
    ListRemoteSkillsResponse(ListRemoteSkillsResponseEvent),
    RemoteSkillDownloaded(RemoteSkillDownloadedEvent),
    SkillsUpdateAvailable,

    // --- Plan ---
    PlanUpdate(PlanUpdateEvent),

    // --- Review mode ---
    EnteredReviewMode(ReviewRequest),
    ExitedReviewMode(ExitedReviewModeEvent),

    // --- Undo ---
    UndoStarted(UndoStartedEvent),
    UndoCompleted(UndoCompletedEvent),

    // --- Stream errors ---
    StreamError(StreamErrorEvent),

    // --- Misc ---
    BackgroundEvent(BackgroundEventEvent),
    DeprecationNotice(DeprecationNoticeEvent),
    ShutdownComplete,

    // --- Collab ---
    CollabAgentSpawnBegin(CollabAgentSpawnBeginEvent),
    CollabAgentSpawnEnd(CollabAgentSpawnEndEvent),
    CollabAgentInteractionBegin(CollabAgentInteractionBeginEvent),
    CollabAgentInteractionEnd(CollabAgentInteractionEndEvent),
    CollabWaitingBegin(CollabWaitingBeginEvent),
    CollabWaitingEnd(CollabWaitingEndEvent),
    CollabCloseBegin(CollabCloseBeginEvent),
    CollabCloseEnd(CollabCloseEndEvent),
    CollabResumeBegin(CollabResumeBeginEvent),
    CollabResumeEnd(CollabResumeEndEvent),
}
