pub mod agent;
pub mod analytics_client;
pub mod client;
pub mod codex;
pub mod compact;
pub mod context_manager;
pub mod exec_policy;
pub mod external_agent_config;
pub mod file_watcher;
pub mod git_info;
pub mod features;
pub mod hooks;
pub mod mcp_client;
pub mod mcp_server;
pub mod memories;
pub mod message_history;
pub mod models_manager;
pub mod network_policy_decision;
pub mod patch;
pub mod project_doc;
pub mod realtime;
pub mod rollout;
#[cfg(target_os = "macos")]
pub mod seatbelt;
pub mod session;
pub mod shell;
pub mod shell_snapshot;
pub mod skills;
pub mod tasks;
pub mod state_db;
pub mod text_encoding;
pub mod thread_manager;
pub mod tools;
pub mod truncation;
pub mod turn_diff_tracker;
pub mod unified_exec;

// Re-export primary types for convenient access.
pub use agent::{
    run_batch_jobs, AgentControl, AgentInstance, BatchJobConfig, BatchResult, Guards,
    SpawnAgentOptions,
};
pub use codex::{Codex, CodexHandle};
pub use compact::{compact, compact_remote, emit_compacted_if_changed, CompactResult};
pub use context_manager::{ContextManager, TokenUsageBreakdown};
pub use exec_policy::{
    ExecApprovalRequirement, ExecPolicyManager, ExecPolicyError, ExecPolicyUpdateError,
    load_exec_policy, collect_policy_files, render_decision_for_unmatched_command,
};
pub use external_agent_config::{
    ExternalAgentConfigDetectOptions, ExternalAgentConfigMigrationItem,
    ExternalAgentConfigMigrationItemType, ExternalAgentConfigService,
};
pub use git_info::{
    collect_git_info, get_git_repo_root, get_git_remote_urls,
    get_git_remote_urls_assume_git_repo, get_head_commit_hash, get_has_changes,
    resolve_root_git_project_for_trust, current_branch_name, default_branch_name,
    local_git_branches, recent_commits, git_diff_to_remote,
    GitInfo, GitDiffToRemote, CommitLogEntry,
};
pub use features::{Feature, FeatureSpec, FeaturesToml, Features, Stage, FEATURES};
pub use hooks::{HookDefinition, HookEvent, HookEventKind, HookHandler, HookRegistry, HookResult};
pub use mcp_client::{McpConnectionManager, McpConnectionState, McpToolInfo};
pub use mcp_server::McpServer;
pub use memories::{memory_root, read_memory_summary, start_memories_startup_task, DEFAULT_MAX_RAW_MEMORIES};
pub use models_manager::manager::{ModelsManager, RefreshStrategy};
pub use models_manager::model_info::{ModelDescriptor, ModelsResponse};
pub use models_manager::cache::ModelsCacheManager;
pub use message_history::{
    append_entry as append_history_entry, history_metadata, lookup as lookup_history,
    HistoryConfig, HistoryEntry, HistoryPersistence,
};
pub use network_policy_decision::{
    BlockedRequest, NetworkDecisionSource, NetworkPolicyDecision, NetworkPolicyDecisionPayload,
};
pub use patch::{PatchApplicator, PatchResult};
pub use project_doc::{
    discover_project_doc_paths, get_user_instructions, read_project_docs, ProjectDocOptions,
    DEFAULT_PROJECT_DOC_FILENAME, LOCAL_PROJECT_DOC_FILENAME,
};
pub use realtime::{RealtimeConversationManager, RealtimeSession};
pub use rollout::{
    RolloutRecorder, RolloutRecorderParams,
    append_thread_name, find_thread_name_by_id, find_thread_path_by_name_str,
};
#[cfg(target_os = "macos")]
pub use seatbelt::{
    create_seatbelt_command_args, spawn_command_under_seatbelt,
    MACOS_PATH_TO_SEATBELT_EXECUTABLE,
};
pub use session::{ModelInfo, PendingApproval, Session, SessionState, TurnContext};
pub use skills::{
    load_skills_from_roots, render_skills_section, install_system_skills, system_cache_root_dir,
    SkillDependencies, SkillError, SkillInterface, SkillLoadOutcome, SkillMetadata, SkillPolicy,
    SkillRoot, SkillScope, SkillsManager, SkillToolDependency,
};
pub use tools::router::ToolRouter;
pub use tools::{ToolHandler, ToolInfo, ToolKind, ToolRegistry};
pub use tasks::{SessionTask, TaskContext, TaskKind};
pub use analytics_client::{AnalyticsEventsClient, TrackEventsContext};
pub use truncation::TruncationPolicy;
pub use turn_diff_tracker::TurnDiffTracker;
pub use thread_manager::{CodexThread, NewThread, ThreadManager};
pub use text_encoding::bytes_to_string_smart;
pub use shell::{Shell, ShellType, detect_shell_type, default_user_shell, get_shell, shell_from_path};
pub use shell_snapshot::ShellSnapshot;
pub use file_watcher::{FileWatcher, FileWatcherEvent, WatchRegistration};
pub use state_db::StateDb;
pub use unified_exec::{
    ProcessManager, ProcessMgrHandle, ExecResult, ExecCommand,
    UnifiedExecError, HeadTailBuffer, UnifiedExecProcess,
    UnifiedExecProcessManager,
    ExecCommandRequest, WriteStdinRequest, UnifiedExecResponse,
    ProcessStore, ProcessEntry,
};
