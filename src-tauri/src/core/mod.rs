pub mod agent;
pub mod client;
pub mod codex;
pub mod compact;
pub mod external_agent_config;
pub mod hooks;
pub mod mcp_client;
pub mod mcp_server;
pub mod patch;
pub mod realtime;
pub mod session;
pub mod skills;
pub mod tools;
pub mod truncation;

// Re-export primary types for convenient access.
pub use agent::{
    run_batch_jobs, AgentControl, AgentInstance, BatchJobConfig, BatchResult, Guards,
    SpawnAgentOptions,
};
pub use codex::{Codex, CodexHandle};
pub use compact::{compact, compact_remote, emit_compacted_if_changed, CompactResult};
pub use external_agent_config::{
    ExternalAgentConfigDetectOptions, ExternalAgentConfigMigrationItem,
    ExternalAgentConfigMigrationItemType, ExternalAgentConfigService,
};
pub use hooks::{HookDefinition, HookEvent, HookEventKind, HookHandler, HookRegistry, HookResult};
pub use mcp_client::{McpConnectionManager, McpConnectionState, McpToolInfo};
pub use mcp_server::McpServer;
pub use patch::{PatchApplicator, PatchResult};
pub use realtime::{RealtimeConversationManager, RealtimeSession};
pub use session::{ModelInfo, PendingApproval, Session, SessionState, TurnContext};
pub use skills::{
    list_skills, load_skills_from_roots, SkillDependencies, SkillInterface, SkillLoadOutcome,
    SkillLoader, SkillMetadata, SkillPolicy, SkillRoot, SkillScope,
};
pub use tools::router::ToolRouter;
pub use tools::{ToolHandler, ToolInfo, ToolKind, ToolRegistry};
pub use truncation::TruncationPolicy;
