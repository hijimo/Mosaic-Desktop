pub mod agent;
pub mod analytics_client;
pub mod client;
pub mod codex;
pub mod compact;
pub mod context_manager;
pub mod custom_prompts;
pub mod event_mapping;
pub mod exec_policy;
pub mod external_agent_config;
pub mod features;
pub mod file_watcher;
pub mod git_info;
pub mod hooks;
pub mod initial_history;
pub mod mcp_client;
pub mod mcp_server;
pub mod memories;
pub mod message_history;
pub mod models_manager;
pub mod network_policy_decision;
pub mod patch;
pub mod project_doc;
pub mod realtime;
pub mod review_format;
pub mod review_prompts;
pub mod rollout;
#[cfg(target_os = "macos")]
pub mod seatbelt;
pub mod session;
pub mod shell;
pub mod shell_snapshot;
pub mod skills;
pub mod state;
pub mod state_db;
pub mod tasks;
pub mod text_encoding;
pub mod thread_history;
pub mod thread_manager;
pub mod tools;
pub mod truncation;
pub mod turn_diff_tracker;
pub mod unified_exec;

// Re-export primary types for convenient access.
pub use agent::{
    agent_status_from_event, exceeds_thread_spawn_depth_limit, is_final, next_thread_spawn_depth,
    AgentRoleConfig, SpawnGuards, SpawnReservation, DEFAULT_ROLE_NAME,
};
pub use agent::{
    run_batch_jobs, AgentControl, AgentInstance, BatchJobConfig, BatchResult, Guards,
    SpawnAgentOptions,
};
pub use analytics_client::{AnalyticsEventsClient, TrackEventsContext};
pub use codex::{Codex, CodexHandle};
pub use compact::{compact, compact_remote, emit_compacted_if_changed, CompactResult};
pub use context_manager::{ContextManager, TokenUsageBreakdown};
pub use exec_policy::{
    collect_policy_files, load_exec_policy, render_decision_for_unmatched_command,
    ExecApprovalRequirement, ExecPolicyError, ExecPolicyManager, ExecPolicyUpdateError,
};
pub use external_agent_config::{
    ExternalAgentConfigDetectOptions, ExternalAgentConfigMigrationItem,
    ExternalAgentConfigMigrationItemType, ExternalAgentConfigService,
};
pub use features::{Feature, FeatureSpec, Features, FeaturesToml, Stage, FEATURES};
pub use file_watcher::{FileWatcher, FileWatcherEvent, WatchRegistration};
pub use git_info::{
    collect_git_info, current_branch_name, default_branch_name, get_git_remote_urls,
    get_git_remote_urls_assume_git_repo, get_git_repo_root, get_has_changes, get_head_commit_hash,
    git_diff_to_remote, local_git_branches, recent_commits, resolve_root_git_project_for_trust,
    CommitLogEntry, GitDiffToRemote, GitInfo,
};
pub use hooks::{HookDefinition, HookEvent, HookEventKind, HookHandler, HookRegistry, HookResult};
pub use mcp_client::{McpConnectionManager, McpConnectionState, McpToolInfo};
pub use mcp_server::McpServer;
pub use memories::{
    memory_root, read_memory_summary, start_memories_startup_task, DEFAULT_MAX_RAW_MEMORIES,
};
pub use message_history::{
    append_entry as append_history_entry, history_metadata, lookup as lookup_history,
    HistoryConfig, HistoryEntry, HistoryPersistence,
};
pub use models_manager::cache::ModelsCacheManager;
pub use models_manager::collaboration_mode_presets::{
    builtin_collaboration_mode_presets, CollaborationModesConfig,
};
pub use models_manager::manager::{ModelsManager, RefreshStrategy};
pub use models_manager::model_info::{ModelDescriptor, ModelsResponse};
pub use network_policy_decision::{
    BlockedRequest, NetworkDecisionSource, NetworkPolicyDecision, NetworkPolicyDecisionPayload,
};
pub use patch::{PatchApplicator, PatchResult};
pub use project_doc::{
    discover_project_doc_paths, get_user_instructions, read_project_docs, ProjectDocOptions,
    DEFAULT_PROJECT_DOC_FILENAME, LOCAL_PROJECT_DOC_FILENAME,
};
pub use realtime::{RealtimeConversationManager, RealtimeSession};
pub use review_format::{format_review_findings_block, render_review_output_text};
pub use review_prompts::{
    resolve_review_request, review_prompt, user_facing_hint, ResolvedReviewRequest,
};
pub use rollout::{
    append_thread_name, find_thread_name_by_id, find_thread_path_by_name_str, RolloutRecorder,
    RolloutRecorderParams,
};
#[cfg(target_os = "macos")]
pub use seatbelt::{
    create_seatbelt_command_args, spawn_command_under_seatbelt, MACOS_PATH_TO_SEATBELT_EXECUTABLE,
};
pub use session::{ModelInfo, PendingApproval, Session, SessionInternalState, TurnContext};
pub use shell::{
    default_user_shell, detect_shell_type, get_shell, shell_from_path, Shell, ShellType,
};
pub use shell_snapshot::ShellSnapshot;
pub use skills::{
    build_implicit_skill_path_indexes, build_skill_name_counts, collect_env_var_dependencies,
    collect_explicit_skill_mentions, collect_explicit_skill_mentions_from_text,
    compile_skill_permissions, detect_implicit_skill_invocation, disabled_paths_from_entries,
    extract_tool_mentions, install_system_skills, load_skills_from_roots,
    normalize_permission_paths, normalize_skill_path, render_skills_section, resolve_dependencies,
    skill_roots_for_cwd, system_cache_root_dir, tool_kind_for_path, MacOsSkillPermissions,
    ResolvedDependencies, SkillDependencies, SkillDependencyInfo, SkillError, SkillInterface,
    SkillLoadOutcome, SkillMetadata, SkillPermissions, SkillPolicy, SkillRoot, SkillScope,
    SkillToolDependency, SkillsManager, ToolMentionKind,
};
pub use state::{ActiveTurn, RunningTask, SessionServices, SessionState, TurnState};
pub use state_db::StateDb;
pub use tasks::{SessionTask, TaskContext, TaskKind};
pub use text_encoding::bytes_to_string_smart;
pub use thread_manager::{CodexThread, NewThread, ThreadManager};
pub use tools::router::ToolRouter;
pub use tools::{ToolHandler, ToolInfo, ToolKind, ToolRegistry};
pub use truncation::TruncationPolicy;
pub use turn_diff_tracker::TurnDiffTracker;
pub use unified_exec::{
    ExecCommand, ExecCommandRequest, ExecResult, HeadTailBuffer, ProcessEntry, ProcessManager,
    ProcessMgrHandle, ProcessStore, UnifiedExecError, UnifiedExecProcess,
    UnifiedExecProcessManager, UnifiedExecResponse, WriteStdinRequest,
};
