use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::types::{
    AskForApproval, CollaborationMode, ConversationAudioParams, ConversationStartParams,
    ConversationTextParams, DynamicToolResponse, Effort, ElicitationAction, McpServerRefreshConfig,
    Personality, ReasoningSummary, ReviewDecision, SandboxPolicy, ServiceTier, UserInput,
};

/// Submission Queue Entry — requests from user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Submission {
    pub id: String,
    pub op: Op,
}

/// All possible operations that can be submitted to the core engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Op {
    // --- Core conversation ---
    UserTurn {
        items: Vec<UserInput>,
        cwd: PathBuf,
        #[serde(rename = "approvalPolicy")]
        approval_policy: AskForApproval,
        #[serde(rename = "sandboxPolicy")]
        sandbox_policy: SandboxPolicy,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        effort: Option<Effort>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<ReasoningSummary>,
        #[serde(
            default,
            rename = "serviceTier",
            skip_serializing_if = "Option::is_none"
        )]
        service_tier: Option<ServiceTier>,
        #[serde(
            rename = "finalOutputJsonSchema",
            skip_serializing_if = "Option::is_none"
        )]
        final_output_json_schema: Option<serde_json::Value>,
        #[serde(rename = "collaborationMode", skip_serializing_if = "Option::is_none")]
        collaboration_mode: Option<CollaborationMode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        personality: Option<Personality>,
    },

    /// Legacy user input.
    UserInput {
        items: Vec<UserInput>,
        #[serde(
            rename = "finalOutputJsonSchema",
            skip_serializing_if = "Option::is_none"
        )]
        final_output_json_schema: Option<serde_json::Value>,
    },

    UserInputAnswer {
        id: String,
        response: serde_json::Value,
    },

    Interrupt,
    Shutdown,

    // --- Approval operations ---
    ExecApproval {
        id: String,
        #[serde(default, rename = "turnId", skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        decision: ReviewDecision,
    },
    PatchApproval {
        id: String,
        decision: ReviewDecision,
    },
    ResolveElicitation {
        #[serde(rename = "serverName")]
        server_name: String,
        #[serde(rename = "requestId")]
        request_id: String,
        decision: ElicitationAction,
    },

    // --- Context override ---
    OverrideTurnContext {
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<PathBuf>,
        #[serde(rename = "approvalPolicy", skip_serializing_if = "Option::is_none")]
        approval_policy: Option<AskForApproval>,
        #[serde(rename = "sandboxPolicy", skip_serializing_if = "Option::is_none")]
        sandbox_policy: Option<SandboxPolicy>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        effort: Option<Effort>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<ReasoningSummary>,
        #[serde(rename = "serviceTier", skip_serializing_if = "Option::is_none")]
        service_tier: Option<ServiceTier>,
        #[serde(rename = "collaborationMode", skip_serializing_if = "Option::is_none")]
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
        #[serde(default, rename = "forceReload")]
        force_reload: bool,
    },
    ListCustomPrompts,

    // --- Realtime conversation ---
    RealtimeConversationStart(ConversationStartParams),
    RealtimeConversationAudio(ConversationAudioParams),
    RealtimeConversationText(ConversationTextParams),
    RealtimeConversationClose,

    // --- Context management ---
    Compact,
    Undo,
    ThreadRollback {
        #[serde(rename = "numTurns")]
        num_turns: u32,
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
