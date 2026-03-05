use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Network & Read-Only Access ───────────────────────────────────

/// Whether outbound network access is available to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum NetworkAccess {
    #[default]
    Restricted,
    Enabled,
}

/// How read-only file access is granted inside a restricted sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ReadOnlyAccess {
    Restricted {
        #[serde(default = "default_true", rename = "includePlatformDefaults")]
        include_platform_defaults: bool,
        #[serde(
            default,
            rename = "readableRoots",
            skip_serializing_if = "Vec::is_empty"
        )]
        readable_roots: Vec<PathBuf>,
    },
    #[default]
    FullAccess,
}

fn default_true() -> bool {
    true
}

// ── Sandbox Policy ───────────────────────────────────────────────

/// Determines execution restrictions for model shell commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum SandboxPolicy {
    /// No restrictions whatsoever.
    DangerFullAccess,

    /// Read-only access configuration.
    ReadOnly {
        #[serde(default)]
        access: ReadOnlyAccess,
    },

    /// Process is already in an external sandbox.
    ExternalSandbox {
        #[serde(default, rename = "networkAccess")]
        network_access: NetworkAccess,
    },

    /// Read + write to workspace directories.
    WorkspaceWrite {
        #[serde(
            default,
            rename = "writableRoots",
            skip_serializing_if = "Vec::is_empty"
        )]
        writable_roots: Vec<PathBuf>,
        #[serde(default, rename = "readOnlyAccess")]
        read_only_access: ReadOnlyAccess,
        #[serde(default, rename = "networkAccess")]
        network_access: bool,
        #[serde(default, rename = "excludeTmpdirEnvVar")]
        exclude_tmpdir_env_var: bool,
        #[serde(default, rename = "excludeSlashTmp")]
        exclude_slash_tmp: bool,
    },
}

// ── Approval Policy ──────────────────────────────────────────────

/// Fine-grained rejection controls for approval prompts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectConfig {
    pub sandbox_approval: bool,
    pub rules: bool,
    pub mcp_elicitations: bool,
}

/// Determines the conditions under which the user is consulted to approve
/// running the command proposed by the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AskForApproval {
    /// Only "known safe" read-only commands are auto-approved.
    UnlessTrusted,
    /// DEPRECATED: auto-approve in sandbox, escalate on failure.
    OnFailure,
    /// The model decides when to ask the user for approval.
    #[default]
    OnRequest,
    /// Fine-grained rejection controls.
    Reject(RejectConfig),
    /// Never ask the user to approve commands.
    Never,
}

// ── Review Decision ──────────────────────────────────────────────

/// Proposed execpolicy change to allow commands starting with this prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExecPolicyAmendment {
    pub command: Vec<String>,
}

/// Network policy rule action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkPolicyRuleAction {
    Allow,
    Deny,
}

/// Network policy amendment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPolicyAmendment {
    pub host: String,
    pub action: NetworkPolicyRuleAction,
}

/// User's decision in response to an approval request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ReviewDecision {
    Approved,
    ApprovedExecpolicyAmendment {
        #[serde(rename = "proposedExecpolicyAmendment")]
        proposed_execpolicy_amendment: ExecPolicyAmendment,
    },
    ApprovedForSession,
    NetworkPolicyAmendment {
        #[serde(rename = "networkPolicyAmendment")]
        network_policy_amendment: NetworkPolicyAmendment,
    },
    #[default]
    Denied,
    Abort,
}

// ── Elicitation ──────────────────────────────────────────────────

/// User's decision for an MCP elicitation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ElicitationAction {
    Accept,
    Decline,
    Cancel,
}

// ── Turn Status ──────────────────────────────────────────────────

/// Reason a turn was aborted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnAbortReason {
    Interrupted,
    Replaced,
    ReviewEnded,
}

// ── Model Configuration ──────────────────────────────────────────

/// Model reasoning effort level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Effort {
    Low,
    #[default]
    Medium,
    High,
}

/// Reasoning summary configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ReasoningSummary {
    #[default]
    Auto,
    Concise,
    Detailed,
    None,
}

/// Service tier selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceTier {
    Fast,
}

// ── Collaboration Mode ───────────────────────────────────────────

/// Collaboration mode kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ModeKind {
    Plan,
    #[default]
    Default,
}

/// Settings for a collaboration mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationModeSettings {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<Effort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub developer_instructions: Option<String>,
}

/// Collaboration mode for a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationMode {
    pub mode: ModeKind,
    pub settings: CollaborationModeSettings,
}

// ── Personality ──────────────────────────────────────────────────

/// Agent personality configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Personality {
    None,
    Friendly,
    Pragmatic,
}

// ── Realtime Conversation ────────────────────────────────────────

/// Realtime conversation start parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationStartParams {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Realtime audio frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeAudioFrame {
    pub data: String,
    pub sample_rate: u32,
    pub num_channels: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub samples_per_channel: Option<u32>,
}

/// Realtime conversation audio parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAudioParams {
    pub frame: RealtimeAudioFrame,
}

/// Realtime conversation text parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationTextParams {
    pub text: String,
}

// ── User Input ───────────────────────────────────────────────────

/// User input item (tagged enum matching reference source).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum UserInput {
    Text {
        text: String,
    },
    Image {
        #[serde(rename = "imageUrl")]
        image_url: String,
    },
    LocalImage {
        path: PathBuf,
    },
}

// ── Response / History Items ─────────────────────────────────────

/// Items in the conversation history / model response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ResponseInputItem {
    Message {
        role: String,
        content: String,
    },
    FunctionCall {
        #[serde(rename = "callId")]
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionOutput {
        #[serde(rename = "callId")]
        call_id: String,
        output: FunctionCallOutputPayload,
    },
}

/// Payload for function call output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCallOutputPayload {
    pub content: ContentOrItems,
}

/// Multi-modal content item for function outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ContentItem {
    Text { text: String },
    Image { url: String },
    InputAudio { data: Vec<u8> },
}

/// Either a plain string or a list of content items (untagged).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum ContentOrItems {
    String(String),
    Items(Vec<ContentItem>),
}

// ── Dynamic Tools ────────────────────────────────────────────────

/// Specification for a dynamically registered tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Request to invoke a dynamic tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolCallRequest {
    pub call_id: String,
    pub turn_id: String,
    pub tool: String,
    pub arguments: serde_json::Value,
}

/// Output content item from a dynamic tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum DynamicToolCallOutputContentItem {
    InputText {
        text: String,
    },
    InputImage {
        #[serde(rename = "imageUrl")]
        image_url: String,
    },
}

/// Response from a dynamic tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolResponse {
    pub content_items: Vec<DynamicToolCallOutputContentItem>,
    pub success: bool,
}

// ── File Changes ─────────────────────────────────────────────────

/// A file change in a patch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum FileChange {
    Add {
        content: String,
    },
    Delete {
        content: String,
    },
    Update {
        #[serde(rename = "unifiedDiff")]
        unified_diff: String,
        #[serde(rename = "movePath", skip_serializing_if = "Option::is_none")]
        move_path: Option<PathBuf>,
    },
}

// ── MCP Types ────────────────────────────────────────────────────

/// MCP tool invocation details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInvocation {
    pub server: String,
    pub tool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

/// MCP server refresh configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRefreshConfig {
    pub mcp_servers: serde_json::Value,
    pub mcp_oauth_credentials_store_mode: serde_json::Value,
}

// ── Context Overrides ────────────────────────────────────────────

/// Overrides that can be applied to the current TurnContext.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnContextOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_policy: Option<SandboxPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<AskForApproval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collaboration_mode: Option<CollaborationMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personality: Option<Personality>,
}

// ── Network Approval ─────────────────────────────────────────────

/// Network approval protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkApprovalProtocol {
    Http,
    Https,
    Socks5Tcp,
    Socks5Udp,
}

/// Network approval context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkApprovalContext {
    pub host: String,
    pub protocol: NetworkApprovalProtocol,
}

// ── Parsed Command ───────────────────────────────────────────────

/// A parsed command token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedCommand {
    pub program: String,
    pub args: Vec<String>,
}

// ── Error Types ──────────────────────────────────────────────────

/// Detailed error info for Codex errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexErrorInfo {
    ContextWindowExceeded,
    UsageLimitExceeded,
    ServerOverloaded,
    HttpConnectionFailed {
        #[serde(rename = "httpStatusCode")]
        http_status_code: Option<u16>,
    },
    ResponseStreamConnectionFailed {
        #[serde(rename = "httpStatusCode")]
        http_status_code: Option<u16>,
    },
    InternalServerError,
    Unauthorized,
    BadRequest,
    SandboxError,
    ResponseStreamDisconnected {
        #[serde(rename = "httpStatusCode")]
        http_status_code: Option<u16>,
    },
    ResponseTooManyFailedAttempts {
        #[serde(rename = "httpStatusCode")]
        http_status_code: Option<u16>,
    },
    ThreadRollbackFailed,
    Other,
}

// ── Exec Command Types ───────────────────────────────────────────

/// Source of a command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ExecCommandSource {
    #[default]
    Agent,
    UserShell,
}

/// Status of a completed command execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecCommandStatus {
    Completed,
    Failed,
    Declined,
}

/// Status of a completed patch application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PatchApplyStatus {
    Completed,
    Failed,
    Declined,
}

// ── Token Usage ──────────────────────────────────────────────────

/// Token usage statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

/// Token usage info with totals and context window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageInfo {
    pub total_token_usage: TokenUsage,
    pub last_token_usage: TokenUsage,
    pub model_context_window: Option<i64>,
}

// ── MCP Startup ──────────────────────────────────────────────────

/// MCP server startup status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum McpStartupStatus {
    Starting,
    Ready,
    Failed { error: String },
    Cancelled,
}

/// MCP startup failure info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStartupFailure {
    pub server: String,
    pub error: String,
}
