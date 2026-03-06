use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::types::{
    AskForApproval, CollaborationMode, ConversationAudioParams, ConversationStartParams,
    ConversationTextParams, DynamicToolResponse, Effort, ElicitationAction, McpServerRefreshConfig,
    Personality, ReasoningSummary, RemoteSkillHazelnutScope, RemoteSkillProductSurface,
    ReviewDecision, ReviewRequest, SandboxPolicy, ServiceTier, UserInput,
};

/// Submission Queue Entry — requests from user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Submission {
    pub id: String,
    pub op: Op,
}

/// All possible operations that can be submitted to the core engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Op {
    // --- Core conversation ---
    UserTurn {
        items: Vec<UserInput>,
        cwd: PathBuf,
        approval_policy: AskForApproval,
        sandbox_policy: SandboxPolicy,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        effort: Option<Effort>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<ReasoningSummary>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        service_tier: Option<ServiceTier>,
        #[serde(skip_serializing_if = "Option::is_none")]
        final_output_json_schema: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        collaboration_mode: Option<CollaborationMode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        personality: Option<Personality>,
    },

    /// Legacy user input.
    UserInput {
        items: Vec<UserInput>,
        #[serde(skip_serializing_if = "Option::is_none")]
        final_output_json_schema: Option<serde_json::Value>,
    },

    #[serde(rename = "user_input_answer", alias = "request_user_input_response")]
    UserInputAnswer {
        id: String,
        response: serde_json::Value,
    },

    Interrupt,
    Shutdown,

    // --- Approval operations ---
    ExecApproval {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        decision: ReviewDecision,
    },
    PatchApproval {
        id: String,
        decision: ReviewDecision,
    },
    ResolveElicitation {
        server_name: String,
        request_id: String,
        decision: ElicitationAction,
    },

    // --- Context override ---
    OverrideTurnContext {
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<PathBuf>,
        #[serde(skip_serializing_if = "Option::is_none")]
        approval_policy: Option<AskForApproval>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sandbox_policy: Option<SandboxPolicy>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        effort: Option<Effort>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<ReasoningSummary>,
        #[serde(skip_serializing_if = "Option::is_none")]
        service_tier: Option<ServiceTier>,
        #[serde(skip_serializing_if = "Option::is_none")]
        collaboration_mode: Option<CollaborationMode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        personality: Option<Personality>,
    },

    // --- Dynamic tools ---
    DynamicToolResponse {
        id: String,
        response: DynamicToolResponse,
    },

    // --- History management ---
    AddToHistory {
        text: String,
    },
    GetHistoryEntryRequest {
        offset: usize,
        log_id: u64,
    },

    // --- MCP management ---
    ListMcpTools,
    RefreshMcpServers {
        config: McpServerRefreshConfig,
    },

    // --- Config & skills ---
    ReloadUserConfig,
    ListSkills {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cwds: Vec<PathBuf>,
        #[serde(default)]
        force_reload: bool,
    },
    ListCustomPrompts,
    ListRemoteSkills {
        hazelnut_scope: RemoteSkillHazelnutScope,
        product_surface: RemoteSkillProductSurface,
        #[serde(skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
    },
    DownloadRemoteSkill {
        hazelnut_id: String,
    },

    // --- Realtime conversation ---
    RealtimeConversationStart(ConversationStartParams),
    RealtimeConversationAudio(ConversationAudioParams),
    RealtimeConversationText(ConversationTextParams),
    RealtimeConversationClose,

    // --- Context management ---
    Compact,
    Undo,
    ThreadRollback {
        num_turns: u32,
    },

    // --- Review ---
    Review {
        review_request: ReviewRequest,
    },

    // --- Misc ---
    SetThreadName {
        name: String,
    },
    DropMemories,
    UpdateMemories,
    RunUserShellCommand {
        command: String,
    },
    ListModels,
    CleanBackgroundTerminals,
}
