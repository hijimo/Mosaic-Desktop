use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ── Network & Read-Only Access ───────────────────────────────────

/// Whether outbound network access is available to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkAccess {
    #[default]
    Restricted,
    Enabled,
}

impl NetworkAccess {
    pub fn is_enabled(self) -> bool {
        matches!(self, NetworkAccess::Enabled)
    }
}

/// How read-only file access is granted inside a restricted sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ReadOnlyAccess {
    Restricted {
        #[serde(default = "default_true")]
        include_platform_defaults: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        readable_roots: Vec<PathBuf>,
    },
    #[default]
    FullAccess,
}

fn default_true() -> bool {
    true
}

impl ReadOnlyAccess {
    pub fn has_full_disk_read_access(&self) -> bool {
        matches!(self, ReadOnlyAccess::FullAccess)
    }

    pub fn get_readable_roots_with_cwd(&self, cwd: &std::path::Path) -> Vec<PathBuf> {
        match self {
            ReadOnlyAccess::FullAccess => Vec::new(),
            ReadOnlyAccess::Restricted { readable_roots, .. } => {
                let mut roots = readable_roots.clone();
                roots.push(cwd.to_path_buf());
                roots.dedup();
                roots
            }
        }
    }
}

// ── Sandbox Policy ───────────────────────────────────────────────

/// A writable root path with read-only subpaths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableRoot {
    pub root: PathBuf,
    pub read_only_subpaths: Vec<PathBuf>,
}

impl WritableRoot {
    pub fn is_path_writable(&self, path: &std::path::Path) -> bool {
        if !path.starts_with(&self.root) {
            return false;
        }
        for subpath in &self.read_only_subpaths {
            if path.starts_with(subpath) {
                return false;
            }
        }
        true
    }
}

/// Determines execution restrictions for model shell commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SandboxPolicy {
    /// No restrictions whatsoever.
    #[serde(rename = "danger-full-access")]
    DangerFullAccess,

    /// Read-only access configuration.
    #[serde(rename = "read-only")]
    ReadOnly {
        #[serde(
            default,
            skip_serializing_if = "ReadOnlyAccess::has_full_disk_read_access"
        )]
        access: ReadOnlyAccess,
    },

    /// Process is already in an external sandbox.
    #[serde(rename = "external-sandbox")]
    ExternalSandbox {
        #[serde(default)]
        network_access: NetworkAccess,
    },

    /// Read + write to workspace directories.
    #[serde(rename = "workspace-write")]
    WorkspaceWrite {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        writable_roots: Vec<PathBuf>,
        #[serde(
            default,
            skip_serializing_if = "ReadOnlyAccess::has_full_disk_read_access"
        )]
        read_only_access: ReadOnlyAccess,
        #[serde(default)]
        network_access: bool,
        #[serde(default)]
        exclude_tmpdir_env_var: bool,
        #[serde(default)]
        exclude_slash_tmp: bool,
    },
}

impl SandboxPolicy {
    pub fn new_read_only_policy() -> Self {
        SandboxPolicy::ReadOnly {
            access: ReadOnlyAccess::FullAccess,
        }
    }

    pub fn new_workspace_write_policy() -> Self {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![],
            read_only_access: ReadOnlyAccess::FullAccess,
            network_access: false,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    }

    pub fn has_full_disk_read_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. } => true,
            SandboxPolicy::ReadOnly { access } => access.has_full_disk_read_access(),
            SandboxPolicy::WorkspaceWrite {
                read_only_access, ..
            } => read_only_access.has_full_disk_read_access(),
        }
    }

    pub fn has_full_disk_write_access(&self) -> bool {
        matches!(
            self,
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. }
        )
    }

    pub fn has_full_network_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ExternalSandbox { network_access } => network_access.is_enabled(),
            SandboxPolicy::ReadOnly { .. } => false,
            SandboxPolicy::WorkspaceWrite { network_access, .. } => *network_access,
        }
    }

    pub fn get_readable_roots_with_cwd(&self, cwd: &std::path::Path) -> Vec<PathBuf> {
        match self {
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. } => Vec::new(),
            SandboxPolicy::ReadOnly { access } => access.get_readable_roots_with_cwd(cwd),
            SandboxPolicy::WorkspaceWrite {
                read_only_access, ..
            } => {
                let mut roots = read_only_access.get_readable_roots_with_cwd(cwd);
                roots.extend(
                    self.get_writable_roots_with_cwd(cwd)
                        .into_iter()
                        .map(|wr| wr.root),
                );
                roots.dedup();
                roots
            }
        }
    }

    pub fn get_writable_roots_with_cwd(&self, cwd: &std::path::Path) -> Vec<WritableRoot> {
        match self {
            SandboxPolicy::WorkspaceWrite { writable_roots, .. } => {
                let mut roots: Vec<WritableRoot> = writable_roots
                    .iter()
                    .map(|r| WritableRoot {
                        root: r.clone(),
                        read_only_subpaths: vec![],
                    })
                    .collect();
                roots.push(WritableRoot {
                    root: cwd.to_path_buf(),
                    read_only_subpaths: vec![],
                });
                roots
            }
            _ => Vec::new(),
        }
    }
}

// ── Approval Policy ──────────────────────────────────────────────

/// Fine-grained rejection controls for approval prompts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectConfig {
    pub sandbox_approval: bool,
    pub rules: bool,
    pub mcp_elicitations: bool,
}

impl RejectConfig {
    pub const fn rejects_sandbox_approval(self) -> bool {
        self.sandbox_approval
    }
    pub const fn rejects_rules_approval(self) -> bool {
        self.rules
    }
    pub const fn rejects_mcp_elicitations(self) -> bool {
        self.mcp_elicitations
    }
}

/// Determines the conditions under which the user is consulted to approve
/// running the command proposed by the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AskForApproval {
    /// Only "known safe" read-only commands are auto-approved.
    #[serde(rename = "untrusted")]
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
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicyRuleAction {
    Allow,
    Deny,
}

/// Network policy amendment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPolicyAmendment {
    pub host: String,
    pub action: NetworkPolicyRuleAction,
}

/// User's decision in response to an approval request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ApprovedExecpolicyAmendment {
        proposed_execpolicy_amendment: ExecPolicyAmendment,
    },
    ApprovedForSession,
    NetworkPolicyAmendment {
        network_policy_amendment: NetworkPolicyAmendment,
    },
    #[default]
    Denied,
    Abort,
}

// ── Elicitation ──────────────────────────────────────────────────

/// User's decision for an MCP elicitation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElicitationAction {
    Accept,
    Decline,
    Cancel,
}

// ── Turn Status ──────────────────────────────────────────────────

/// Reason a turn was aborted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnAbortReason {
    Interrupted,
    Replaced,
    ReviewEnded,
}

// ── Model Configuration ──────────────────────────────────────────

/// Model reasoning effort level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    #[default]
    Medium,
    High,
}

/// Reasoning summary configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningSummary {
    #[default]
    Auto,
    Concise,
    Detailed,
    None,
}

/// Service tier selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    Fast,
}

/// Controls output length/detail on GPT-5 models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    Low,
    #[default]
    Medium,
    High,
}

/// Web search tool mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WebSearchMode {
    Disabled,
    #[default]
    Cached,
    Live,
}

/// Sandbox mode for TOML config (simplified enum, distinct from SandboxPolicy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    #[serde(rename = "read-only")]
    #[default]
    ReadOnly,
    #[serde(rename = "workspace-write")]
    WorkspaceWrite,
    #[serde(rename = "danger-full-access")]
    DangerFullAccess,
}

/// Login method restriction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForcedLoginMethod {
    Chatgpt,
    Api,
}

/// Trust level for a project directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    Trusted,
    Untrusted,
}

// ── Collaboration Mode ───────────────────────────────────────────

/// Collaboration mode kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModeKind {
    Plan,
    #[default]
    #[serde(
        alias = "code",
        alias = "pair_programming",
        alias = "execute",
        alias = "custom"
    )]
    Default,
    #[doc(hidden)]
    #[serde(skip_serializing, skip_deserializing)]
    PairProgramming,
    #[doc(hidden)]
    #[serde(skip_serializing, skip_deserializing)]
    Execute,
}

/// The modes visible in the TUI collaboration mode picker.
pub const TUI_VISIBLE_COLLABORATION_MODES: [ModeKind; 2] = [ModeKind::Default, ModeKind::Plan];

impl ModeKind {
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Plan => "Plan",
            Self::Default => "Default",
            Self::PairProgramming => "Pair Programming",
            Self::Execute => "Execute",
        }
    }

    pub const fn is_tui_visible(self) -> bool {
        matches!(self, Self::Plan | Self::Default)
    }

    pub const fn allows_request_user_input(self) -> bool {
        matches!(self, Self::Plan)
    }
}

/// A mask for collaboration mode settings, allowing partial updates.
/// All fields except `name` are optional, enabling selective overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationModeMask {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ModeKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// `Some(Some(effort))` = set, `Some(None)` = clear, `None` = keep.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<Option<Effort>>,
    /// `Some(Some(text))` = set, `Some(None)` = clear, `None` = keep.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub developer_instructions: Option<Option<String>>,
}

/// Settings for a collaboration mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationModeSettings {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<Effort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub developer_instructions: Option<String>,
}

/// Collaboration mode for a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollaborationMode {
    pub mode: ModeKind,
    pub settings: CollaborationModeSettings,
}

// ── Personality ──────────────────────────────────────────────────

/// Agent personality configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Personality {
    None,
    Friendly,
    Pragmatic,
}

// ── Realtime Conversation ────────────────────────────────────────

/// Realtime conversation start parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationStartParams {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Realtime audio frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealtimeAudioFrame {
    pub data: String,
    pub sample_rate: u32,
    pub num_channels: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub samples_per_channel: Option<u32>,
}

/// Realtime conversation audio parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationAudioParams {
    pub frame: RealtimeAudioFrame,
}

/// Realtime conversation text parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationTextParams {
    pub text: String,
}

// ── User Input ───────────────────────────────────────────────────

/// Byte range within a text buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

/// UI-defined span within user input text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextElement {
    pub byte_range: ByteRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

impl TextElement {
    pub fn new(byte_range: ByteRange, placeholder: Option<String>) -> Self {
        Self {
            byte_range,
            placeholder,
        }
    }

    /// Return the placeholder text, or the slice from `source` if no placeholder is set.
    pub fn placeholder<'a>(&'a self, source: &'a str) -> Option<&'a str> {
        if let Some(ref p) = self.placeholder {
            Some(p.as_str())
        } else {
            source.get(self.byte_range.start..self.byte_range.end)
        }
    }
}

/// User input item (tagged enum matching reference source).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum UserInput {
    Text {
        text: String,
        #[serde(default)]
        text_elements: Vec<TextElement>,
    },
    Image {
        image_url: String,
    },
    LocalImage {
        path: PathBuf,
    },
    /// Any local file attached by the user (images, PDFs, etc.)
    AttachedFile {
        name: String,
        path: PathBuf,
    },
    /// Skill selected by the user.
    Skill {
        name: String,
        path: PathBuf,
    },
    /// Explicit mention selected by the user.
    Mention {
        name: String,
        path: String,
    },
}

// ── Content Items ────────────────────────────────────────────────

/// Multi-modal content item (matches source project's ContentItem).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentItem {
    InputText { text: String },
    InputImage { image_url: String },
    OutputText { text: String },
}

// ── ResponseItem (model output) ──────────────────────────────────

/// Structured model output item — matches source project's `ResponseItem`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseItem {
    Message {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        role: String,
        content: Vec<ContentItem>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_turn: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<MessagePhase>,
    },
    Reasoning {
        #[serde(default)]
        id: String,
        summary: Vec<ReasoningSummaryItem>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<Vec<ReasoningContentItem>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
    },
    FunctionCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        name: String,
        arguments: String,
        call_id: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: FunctionCallOutputPayload,
    },
    LocalShellCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        status: LocalShellStatus,
        action: LocalShellAction,
    },
    GhostSnapshot {
        ghost_commit: serde_json::Value,
    },
    CustomToolCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        call_id: String,
        name: String,
        input: String,
    },
    CustomToolCallOutput {
        call_id: String,
        output: FunctionCallOutputPayload,
    },
    WebSearchCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<WebSearchAction>,
    },
    #[serde(alias = "compaction_summary")]
    Compaction {
        encrypted_content: String,
    },
    #[serde(other)]
    Other,
}

/// Reasoning summary entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningSummaryItem {
    SummaryText { text: String },
}

/// Reasoning content entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningContentItem {
    ReasoningText { text: String },
    Text { text: String },
}

// ── Local Shell Types ─────────────────────────────────────────────

/// Status of a local shell call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalShellStatus {
    Completed,
    InProgress,
    Incomplete,
}

/// Action for a local shell call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalShellAction {
    Exec(LocalShellExecAction),
}

/// Exec action details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalShellExecAction {
    pub command: Vec<String>,
    pub timeout_ms: Option<u64>,
    pub working_directory: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub user: Option<String>,
}

/// Web search action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSearchAction {
    Search {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queries: Option<Vec<String>>,
    },
    OpenPage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    FindInPage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
    },
    #[serde(other)]
    Other,
}

// ── ResponseInputItem (conversation history) ─────────────────────

/// Items in the conversation history sent back to the API.
/// Retains legacy variants (FunctionCall, FunctionOutput) for backward compatibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseInputItem {
    Message {
        role: String,
        content: Vec<ContentItem>,
    },
    /// Function call from the model.
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    /// Function call output. Also deserializes from legacy "function_output" type tag.
    #[serde(alias = "function_output")]
    FunctionCallOutput {
        call_id: String,
        output: FunctionCallOutputPayload,
    },
    McpToolCallOutput {
        call_id: String,
        result: Result<CallToolResult, String>,
    },
    CustomToolCallOutput {
        call_id: String,
        output: FunctionCallOutputPayload,
    },
}

impl ResponseInputItem {
    /// Convenience: create a text message.
    pub fn text_message(role: &str, text: String) -> Self {
        Self::Message {
            role: role.to_string(),
            content: vec![ContentItem::InputText { text }],
        }
    }

    /// Extract plain text from a Message's content items.
    pub fn message_text(&self) -> Option<String> {
        match self {
            Self::Message { content, .. } if !content.is_empty() => {
                let text: String = content
                    .iter()
                    .filter_map(|c| match c {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            Some(text.as_str())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            }
            _ => None,
        }
    }
}

/// Legacy alias: FunctionOutput → FunctionCallOutput
impl ResponseInputItem {
    /// Legacy constructor matching old `FunctionOutput` variant.
    pub fn function_output(call_id: String, output: FunctionCallOutputPayload) -> Self {
        Self::FunctionCallOutput { call_id, output }
    }
}

impl ResponseItem {
    pub fn to_input_item(&self) -> Option<ResponseInputItem> {
        match self {
            ResponseItem::Message { role, content, .. } => Some(ResponseInputItem::Message {
                role: role.clone(),
                content: content.clone(),
            }),
            ResponseItem::FunctionCall {
                call_id,
                name,
                arguments,
                ..
            } => Some(ResponseInputItem::FunctionCall {
                call_id: call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
            }),
            ResponseItem::FunctionCallOutput { call_id, output } => {
                Some(ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: output.clone(),
                })
            }
            ResponseItem::CustomToolCallOutput { call_id, output } => {
                Some(ResponseInputItem::CustomToolCallOutput {
                    call_id: call_id.clone(),
                    output: output.clone(),
                })
            }
            _ => None,
        }
    }
}

impl From<ResponseItem> for ResponseInputItem {
    fn from(item: ResponseItem) -> Self {
        item.to_input_item().unwrap_or_else(|| Self::Message {
            role: "system".to_string(),
            content: vec![ContentItem::InputText {
                text: "[unhandled item]".to_string(),
            }],
        })
    }
}

/// Payload for function call output.
/// On the wire: either a plain string or an array of content items.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCallOutputPayload {
    pub body: FunctionCallOutputBody,
    pub success: Option<bool>,
}

impl FunctionCallOutputPayload {
    pub fn from_text(content: String) -> Self {
        Self {
            body: FunctionCallOutputBody::Text(content),
            success: None,
        }
    }

    pub fn text_content(&self) -> Option<&str> {
        match &self.body {
            FunctionCallOutputBody::Text(s) => Some(s),
            _ => None,
        }
    }
}

impl Default for FunctionCallOutputPayload {
    fn default() -> Self {
        Self {
            body: FunctionCallOutputBody::Text(String::new()),
            success: None,
        }
    }
}

/// The wire body of a function call output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FunctionCallOutputBody {
    Text(String),
    ContentItems(Vec<FunctionCallOutputContentItem>),
}

impl Default for FunctionCallOutputBody {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

/// Content items that can appear in function call output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FunctionCallOutputContentItem {
    InputText { text: String },
    InputImage { image_url: String },
}

// Custom serde for FunctionCallOutputPayload: serialize body directly
impl Serialize for FunctionCallOutputPayload {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.body.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FunctionCallOutputPayload {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let body = FunctionCallOutputBody::deserialize(deserializer)?;
        Ok(Self {
            body,
            success: None,
        })
    }
}

impl std::fmt::Display for FunctionCallOutputPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.body {
            FunctionCallOutputBody::Text(s) => f.write_str(s),
            FunctionCallOutputBody::ContentItems(items) => {
                write!(f, "{}", serde_json::to_string(items).unwrap_or_default())
            }
        }
    }
}

/// Legacy type alias — kept for backward compatibility during migration.
pub type ContentOrItems = FunctionCallOutputBody;

// ── Dynamic Tools ────────────────────────────────────────────────

/// Specification for a dynamically registered tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynamicToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Request to invoke a dynamic tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynamicToolCallRequest {
    pub call_id: String,
    pub turn_id: String,
    pub tool: String,
    pub arguments: serde_json::Value,
}

/// Output content item from a dynamic tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DynamicToolCallOutputContentItem {
    InputText { text: String },
    InputImage { image_url: String },
}

/// Response from a dynamic tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynamicToolResponse {
    pub content_items: Vec<DynamicToolCallOutputContentItem>,
    pub success: bool,
}

// ── File Changes ─────────────────────────────────────────────────

/// A file change in a patch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileChange {
    Add {
        content: String,
    },
    Delete {
        content: String,
    },
    Update {
        unified_diff: String,
        move_path: Option<PathBuf>,
    },
}

// ── MCP Types ────────────────────────────────────────────────────

/// MCP tool invocation details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpInvocation {
    pub server: String,
    pub tool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

/// MCP tool call result (simplified from reference).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallToolResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// MCP server refresh configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerRefreshConfig {
    pub mcp_servers: serde_json::Value,
    pub mcp_oauth_credentials_store_mode: serde_json::Value,
}

// ── Context Overrides ────────────────────────────────────────────

/// Overrides that can be applied to the current TurnContext.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[serde(rename_all = "snake_case")]
pub enum NetworkApprovalProtocol {
    Http,
    #[serde(alias = "https_connect", alias = "http-connect")]
    Https,
    Socks5Tcp,
    Socks5Udp,
}

/// Network approval context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkApprovalContext {
    pub host: String,
    pub protocol: NetworkApprovalProtocol,
}

// ── Parsed Command ───────────────────────────────────────────────

/// A parsed command with semantic type information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParsedCommand {
    Read {
        cmd: String,
        name: String,
        path: PathBuf,
    },
    ListFiles {
        cmd: String,
        path: Option<String>,
    },
    Search {
        cmd: String,
        query: Option<String>,
        path: Option<String>,
    },
    Unknown {
        cmd: String,
    },
}

// ── Error Types ──────────────────────────────────────────────────

/// Detailed error info for Codex errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexErrorInfo {
    ContextWindowExceeded,
    UsageLimitExceeded,
    ServerOverloaded,
    HttpConnectionFailed { http_status_code: Option<u16> },
    ResponseStreamConnectionFailed { http_status_code: Option<u16> },
    InternalServerError,
    Unauthorized,
    BadRequest,
    SandboxError,
    ResponseStreamDisconnected { http_status_code: Option<u16> },
    ResponseTooManyFailedAttempts { http_status_code: Option<u16> },
    ThreadRollbackFailed,
    Other,
}

impl CodexErrorInfo {
    pub fn affects_turn_status(&self) -> bool {
        !matches!(self, Self::ThreadRollbackFailed)
    }
}

// ── Exec Command Types ───────────────────────────────────────────

/// Source of a command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecCommandSource {
    #[default]
    Agent,
    UserShell,
    UnifiedExecStartup,
    UnifiedExecInteraction,
}

/// Status of a completed command execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecCommandStatus {
    Completed,
    Failed,
    Declined,
}

/// Status of a completed patch application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchApplyStatus {
    Completed,
    Failed,
    Declined,
}

// ── Token Usage ──────────────────────────────────────────────────

/// Token usage statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

/// Token usage info with totals and context window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenUsageInfo {
    pub total_token_usage: TokenUsage,
    pub last_token_usage: TokenUsage,
    pub model_context_window: Option<i64>,
}

/// Snapshot of rate-limit state for a single metered limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimitSnapshot {
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub primary: Option<RateLimitWindow>,
    pub secondary: Option<RateLimitWindow>,
    pub credits: Option<CreditsSnapshot>,
    pub plan_type: Option<PlanType>,
}

/// A single rate-limit window (primary or secondary).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimitWindow {
    pub used_percent: f64,
    pub window_minutes: Option<i64>,
    pub resets_at: Option<i64>,
}

/// Credits balance snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditsSnapshot {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}

/// Account plan type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanType {
    #[default]
    Free,
    Go,
    Plus,
    Pro,
    Team,
    Business,
    Enterprise,
    Edu,
    #[serde(other)]
    Unknown,
}

// ── MCP Startup ──────────────────────────────────────────────────

/// MCP server startup status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum McpStartupStatus {
    Starting,
    Ready,
    Failed { error: String },
    Cancelled,
}

/// MCP startup failure info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpStartupFailure {
    pub server: String,
    pub error: String,
}

// ── Exec Output Stream ───────────────────────────────────────────

/// Which stream produced an output chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecOutputStream {
    Stdout,
    Stderr,
}

// ── Agent Status ─────────────────────────────────────────────────

/// Agent lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    #[default]
    PendingInit,
    Running,
    Completed(Option<String>),
    Errored(String),
    Shutdown,
    NotFound,
}

// ── Review Request ───────────────────────────────────────────────

/// Review target specifying what to review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ReviewTarget {
    UncommittedChanges,
    #[serde(rename_all = "camelCase")]
    BaseBranch {
        branch: String,
    },
    #[serde(rename_all = "camelCase")]
    Commit {
        sha: String,
        title: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Custom {
        instructions: String,
    },
}

/// Review request payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub target: ReviewTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_facing_hint: Option<String>,
}

/// Structured review result produced by a child review session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewOutputEvent {
    pub findings: Vec<ReviewFinding>,
    pub overall_correctness: String,
    pub overall_explanation: String,
    pub overall_confidence_score: f32,
}

impl Default for ReviewOutputEvent {
    fn default() -> Self {
        Self {
            findings: Vec::new(),
            overall_correctness: String::default(),
            overall_explanation: String::default(),
            overall_confidence_score: 0.0,
        }
    }
}

/// A single review finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub title: String,
    pub body: String,
    pub confidence_score: f32,
    pub priority: i32,
    pub code_location: ReviewCodeLocation,
}

/// Location of code related to a review finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewCodeLocation {
    pub absolute_file_path: PathBuf,
    pub line_range: ReviewLineRange,
}

/// Inclusive line range in a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewLineRange {
    pub start: u32,
    pub end: u32,
}

// ── Remote Skills ────────────────────────────────────────────────

/// Hazelnut scope for remote skills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteSkillHazelnutScope {
    User,
    Organization,
}

/// Product surface for remote skills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteSkillProductSurface {
    Codex,
    ChatGpt,
}

// ── Message Phase ────────────────────────────────────────────────

/// Phase of an agent message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessagePhase {
    Commentary,
    FinalAnswer,
}

// ── Model Reroute ────────────────────────────────────────────────

/// Reason for model reroute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRerouteReason {
    HighRiskCyberActivity,
}
