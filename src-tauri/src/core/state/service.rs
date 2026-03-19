//! Session-scoped services shared across turns.

use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::core::agent::AgentControl;
use crate::core::analytics_client::AnalyticsEventsClient;
use crate::core::client::ModelClient;
use crate::core::exec_policy::ExecPolicyManager;
use crate::core::file_watcher::FileWatcher;
use crate::core::hooks::HookRegistry;
use crate::core::mcp_client::McpConnectionManager;
use crate::core::models_manager::manager::ModelsManager;
use crate::core::rollout::RolloutRecorder;
use crate::core::shell::Shell;
use crate::core::shell_snapshot::ShellSnapshot;
use crate::core::skills::SkillsManager;
use crate::core::state_db::StateDb as StateDbHandle;
use crate::core::tools::network_approval::NetworkApprovalService;
use crate::core::tools::sandboxing::ApprovalStore;
use crate::core::unified_exec::UnifiedExecProcessManager;

/// Long-lived, session-scoped services shared across all turns.
///
/// These are initialized once when the session starts and remain alive
/// for the entire session lifetime. They are conceptually immutable
/// (or interior-mutable via Arc/Mutex) and do not change between turns.
pub struct SessionServices {
    pub mcp_connection_manager: Arc<RwLock<McpConnectionManager>>,
    pub mcp_startup_cancellation_token: Mutex<CancellationToken>,
    pub unified_exec_manager: UnifiedExecProcessManager,
    pub analytics_events_client: AnalyticsEventsClient,
    pub hooks: Mutex<HookRegistry>,
    pub rollout: Mutex<Option<RolloutRecorder>>,
    pub user_shell: Arc<Shell>,
    pub shell_snapshot_tx: tokio::sync::watch::Sender<Option<Arc<ShellSnapshot>>>,
    pub exec_policy: ExecPolicyManager,
    pub models_manager: Arc<ModelsManager>,
    pub tool_approvals: Mutex<ApprovalStore>,
    pub skills_manager: Arc<SkillsManager>,
    pub mcp_manager: Arc<McpConnectionManager>,
    pub file_watcher: Arc<FileWatcher>,
    pub agent_control: AgentControl,
    pub network_approval: Arc<NetworkApprovalService>,
    pub state_db: Option<StateDbHandle>,
    /// Session-scoped model client shared across turns.
    pub model_client: ModelClient,
}
