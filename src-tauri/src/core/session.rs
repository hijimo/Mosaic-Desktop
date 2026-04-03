use std::path::PathBuf;

use crate::config::toml_types::ConfigToml;
use crate::config::ConfigLayerStack;
use crate::core::context_manager::history::ContextManager;
use crate::core::hooks::HookRegistry;
use crate::core::mcp_client::McpConnectionManager;
use crate::core::rollout::policy::TurnContextItem;
use crate::core::rollout::reconstruction::PreviousTurnSettings;
use crate::core::tools::router::ToolRouter;
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{ErrorEvent, Event, EventMsg};
use crate::protocol::types::{
    AskForApproval, CollaborationMode, Effort, Personality, ReasoningSummary, ResponseInputItem,
    SandboxPolicy, ServiceTier, TokenUsageInfo, TurnContextOverrides,
};

// ── ModelInfo ────────────────────────────────────────────────────

/// Information about the model used for the current turn.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    pub model: String,
    pub provider: String,
    pub context_window: Option<i64>,
}

// ── PendingApproval ──────────────────────────────────────────────

/// Tracks a pending approval request during a turn.
#[derive(Debug, Clone)]
pub enum PendingApproval {
    Exec {
        call_id: String,
        command: Vec<String>,
        cwd: PathBuf,
    },
    Patch {
        call_id: String,
        turn_id: String,
        changes: std::collections::HashMap<PathBuf, crate::protocol::types::FileChange>,
        cwd: PathBuf,
    },
}

// ── TurnContext ──────────────────────────────────────────────────

/// Per-turn execution context, created at the start of each turn.
#[derive(Debug, Clone)]
pub struct TurnContext {
    pub model_info: ModelInfo,
    pub sandbox_policy: SandboxPolicy,
    pub approval_policy: AskForApproval,
    pub cwd: PathBuf,
    pub effort: Option<Effort>,
    pub summary: Option<ReasoningSummary>,
    pub service_tier: Option<ServiceTier>,
    pub collaboration_mode: Option<CollaborationMode>,
    pub personality: Option<Personality>,
}

impl TurnContext {
    /// Create a new TurnContext inheriting defaults from the resolved config.
    pub fn from_config(config: &ConfigToml, cwd: PathBuf) -> Self {
        let sandbox_policy = match config.sandbox_mode {
            Some(crate::protocol::types::SandboxMode::DangerFullAccess) => {
                SandboxPolicy::DangerFullAccess
            }
            Some(crate::protocol::types::SandboxMode::WorkspaceWrite) => {
                SandboxPolicy::new_workspace_write_policy()
            }
            _ => SandboxPolicy::new_read_only_policy(),
        };

        Self {
            model_info: ModelInfo {
                model: config.model.clone().unwrap_or_else(|| "default".into()),
                provider: config.model_provider.clone().unwrap_or_default(),
                context_window: config.model_context_window,
            },
            sandbox_policy,
            approval_policy: config.approval_policy.clone().unwrap_or_default(),
            cwd,
            effort: config.model_reasoning_effort,
            summary: config.model_reasoning_summary,
            service_tier: config.service_tier,
            collaboration_mode: None,
            personality: config.personality,
        }
    }

    /// Apply overrides to this TurnContext, modifying only specified fields.
    pub fn apply_overrides(&mut self, overrides: &TurnContextOverrides) {
        if let Some(model) = &overrides.model {
            self.model_info.model = model.clone();
        }
        if let Some(policy) = &overrides.sandbox_policy {
            self.sandbox_policy = policy.clone();
        }
        if let Some(policy) = &overrides.approval_policy {
            self.approval_policy = policy.clone();
        }
        if let Some(cwd) = &overrides.cwd {
            self.cwd = cwd.clone();
        }
        if let Some(mode) = &overrides.collaboration_mode {
            self.collaboration_mode = Some(mode.clone());
        }
        if let Some(p) = &overrides.personality {
            self.personality = Some(*p);
        }
    }
}

// ── SessionState ─────────────────────────────────────────────────

/// Mutable state protected by a Mutex inside Session.
/// Tracks per-turn lifecycle and conversation history via ContextManager.
#[derive(Debug)]
pub struct SessionInternalState {
    /// Conversation history managed by ContextManager (truncation + normalization).
    pub history: ContextManager,
    /// Whether a turn is currently active.
    pub turn_active: bool,
    /// Pending approval request, if any.
    pub pending_approval: Option<PendingApproval>,
    /// Current turn context (set when a turn starts).
    pub turn_context: Option<TurnContext>,
    /// Custom instructions from a ReviewDecision, forwarded to the Agent on the next turn.
    pub custom_instructions: Option<String>,
    /// Session-level command allow list (command prefixes approved via `ApprovedForSession`).
    pub exec_allow_list: Vec<Vec<String>>,
    /// Settings from the last surviving user turn (for resume/fork).
    pub previous_turn_settings: Option<PreviousTurnSettings>,
    /// Active MCP tool selection (restored on resume).
    pub active_mcp_tool_selection: Option<Vec<String>>,
}

impl SessionInternalState {
    pub fn new() -> Self {
        Self {
            history: ContextManager::new(),
            turn_active: false,
            pending_approval: None,
            turn_context: None,
            custom_instructions: None,
            exec_allow_list: Vec::new(),
            previous_turn_settings: None,
            active_mcp_tool_selection: None,
        }
    }
}

impl Default for SessionInternalState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Session ──────────────────────────────────────────────────────

/// Active session managing conversation state, tools, MCP, and hooks.
pub struct Session {
    /// Unique session identifier.
    id: String,
    /// Protected mutable state.
    state: tokio::sync::Mutex<SessionInternalState>,
    /// Configuration stack for resolving layered config.
    config: ConfigLayerStack,
    /// Active profile name (if any).
    active_profile: Option<String>,
    /// Tool router for dispatching tool calls.
    tool_router: tokio::sync::Mutex<ToolRouter>,
    /// MCP connection manager.
    mcp_manager: McpConnectionManager,
    /// Hook registry.
    hooks: tokio::sync::Mutex<HookRegistry>,
    /// Event queue sender.
    tx_event: async_channel::Sender<Event>,
    /// Working directory.
    cwd: PathBuf,
    /// Turn counter for generating turn IDs.
    turn_counter: tokio::sync::Mutex<u64>,
    /// Runtime config requirements that can constrain turn-level tool exposure.
    config_requirements: tokio::sync::RwLock<crate::config::ConfigRequirements>,
}

impl Session {
    fn resolve_constrained_web_search_mode_for_turn(
        web_search_mode: &crate::config::Constrained<crate::protocol::types::WebSearchMode>,
        sandbox_policy: &SandboxPolicy,
    ) -> crate::protocol::types::WebSearchMode {
        let preferred = web_search_mode.value();

        if matches!(sandbox_policy, SandboxPolicy::DangerFullAccess)
            && preferred != crate::protocol::types::WebSearchMode::Disabled
        {
            for mode in [
                crate::protocol::types::WebSearchMode::Live,
                crate::protocol::types::WebSearchMode::Cached,
                crate::protocol::types::WebSearchMode::Disabled,
            ] {
                if web_search_mode.can_set(&mode).is_ok() {
                    return mode;
                }
            }
        } else {
            if web_search_mode.can_set(&preferred).is_ok() {
                return preferred;
            }

            for mode in [
                crate::protocol::types::WebSearchMode::Cached,
                crate::protocol::types::WebSearchMode::Live,
                crate::protocol::types::WebSearchMode::Disabled,
            ] {
                if web_search_mode.can_set(&mode).is_ok() {
                    return mode;
                }
            }
        }

        crate::protocol::types::WebSearchMode::Disabled
    }

    fn web_search_mode_from_tool_specs(
        specs: &[serde_json::Value],
    ) -> Option<crate::protocol::types::WebSearchMode> {
        specs.iter().find_map(|spec| {
            if spec.get("type") != Some(&serde_json::Value::String("web_search".to_string())) {
                return None;
            }

            match spec
                .get("external_web_access")
                .and_then(|value| value.as_bool())
            {
                Some(true) => Some(crate::protocol::types::WebSearchMode::Live),
                Some(false) | None => Some(crate::protocol::types::WebSearchMode::Cached),
            }
        })
    }

    fn apply_web_search_mode_to_tool_specs(
        specs: &mut Vec<serde_json::Value>,
        mode: Option<crate::protocol::types::WebSearchMode>,
    ) {
        specs.retain(|spec| {
            spec.get("type") != Some(&serde_json::Value::String("web_search".to_string()))
        });

        match mode {
            Some(crate::protocol::types::WebSearchMode::Cached) => specs.push(serde_json::json!({
                "type": "web_search",
                "external_web_access": false,
            })),
            Some(crate::protocol::types::WebSearchMode::Live) => specs.push(serde_json::json!({
                "type": "web_search",
                "external_web_access": true,
            })),
            Some(crate::protocol::types::WebSearchMode::Disabled) | None => {}
        }
    }

    fn resolve_web_search_mode_for_turn(
        preferred: Option<crate::protocol::types::WebSearchMode>,
        sandbox_policy: &SandboxPolicy,
    ) -> Option<crate::protocol::types::WebSearchMode> {
        preferred.and_then(|preferred| {
            let constrained = crate::config::Constrained::allow_any(preferred);
            match Self::resolve_constrained_web_search_mode_for_turn(&constrained, sandbox_policy) {
                crate::protocol::types::WebSearchMode::Disabled => None,
                mode => Some(mode),
            }
        })
    }

    fn resolve_web_search_mode_for_turn_with_constraints(
        preferred: Option<crate::protocol::types::WebSearchMode>,
        constraints: &crate::config::Constrained<crate::protocol::types::WebSearchMode>,
        sandbox_policy: &SandboxPolicy,
    ) -> Option<crate::protocol::types::WebSearchMode> {
        let preferred = preferred?;

        if matches!(sandbox_policy, SandboxPolicy::DangerFullAccess)
            && preferred != crate::protocol::types::WebSearchMode::Disabled
        {
            for mode in [
                crate::protocol::types::WebSearchMode::Live,
                crate::protocol::types::WebSearchMode::Cached,
                crate::protocol::types::WebSearchMode::Disabled,
            ] {
                if constraints.can_set(&mode).is_ok() {
                    return match mode {
                        crate::protocol::types::WebSearchMode::Disabled => None,
                        other => Some(other),
                    };
                }
            }
        } else {
            if constraints.can_set(&preferred).is_ok() {
                return match preferred {
                    crate::protocol::types::WebSearchMode::Disabled => None,
                    other => Some(other),
                };
            }

            for mode in [
                crate::protocol::types::WebSearchMode::Cached,
                crate::protocol::types::WebSearchMode::Live,
                crate::protocol::types::WebSearchMode::Disabled,
            ] {
                if constraints.can_set(&mode).is_ok() {
                    return match mode {
                        crate::protocol::types::WebSearchMode::Disabled => None,
                        other => Some(other),
                    };
                }
            }
        }

        None
    }

    fn tools_config_from_resolved_config(config: &ConfigToml) -> crate::core::tools::ToolsConfig {
        let web_search_mode = if let Some(mode) = config.web_search {
            match mode {
                crate::protocol::types::WebSearchMode::Disabled => None,
                other => Some(other),
            }
        } else {
            match config
                .tools
                .as_ref()
                .and_then(|tools| tools.enable_web_search)
            {
                Some(true) => Some(crate::protocol::types::WebSearchMode::Live),
                Some(false) | None => None,
            }
        };

        crate::core::tools::ToolsConfig {
            collab_tools: true,
            web_search_mode,
            ..Default::default()
        }
    }

    pub fn new(
        cwd: PathBuf,
        config: ConfigLayerStack,
        tx_event: async_channel::Sender<Event>,
    ) -> Self {
        Self::new_with_agent_control(cwd, config, tx_event, None)
    }

    pub fn new_with_agent_control(
        cwd: PathBuf,
        config: ConfigLayerStack,
        tx_event: async_channel::Sender<Event>,
        agent_control: Option<std::sync::Arc<crate::core::agent::control::AgentControl>>,
    ) -> Self {
        let resolved_config = config.merge();
        let mut router = ToolRouter::from_config(
            Self::tools_config_from_resolved_config(&resolved_config),
            agent_control.is_some(),
        );

        if let Some(ctrl) = agent_control {
            router.registry_mut().register(Box::new(
                crate::core::tools::handlers::multi_agents::MultiAgentHandler::new(
                    ctrl,
                    config.clone(),
                ),
            ));
        }

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            state: tokio::sync::Mutex::new(SessionInternalState::new()),
            config,
            active_profile: None,
            tool_router: tokio::sync::Mutex::new(router),
            mcp_manager: McpConnectionManager::new(),
            hooks: tokio::sync::Mutex::new(HookRegistry::new()),
            tx_event,
            cwd,
            turn_counter: tokio::sync::Mutex::new(0),
            config_requirements: tokio::sync::RwLock::new(
                crate::config::ConfigRequirements::default(),
            ),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    /// Resolve the effective config, optionally applying the active profile.
    pub fn resolved_config(&self) -> ConfigToml {
        match &self.active_profile {
            Some(profile) => self.config.resolve_with_profile(profile),
            None => self.config.merge(),
        }
    }

    pub fn set_active_profile(&mut self, profile: Option<String>) {
        self.active_profile = profile;
    }

    /// Get the current model name from resolved config.
    pub fn model(&self) -> String {
        self.resolved_config()
            .model
            .unwrap_or_else(|| "default".into())
    }

    /// Set the model by updating the session-level config layer.
    pub fn set_model(&mut self, model: String) {
        let session_config = ConfigToml {
            model: Some(model),
            ..Default::default()
        };
        self.config
            .add_layer(crate::config::ConfigLayer::Session, session_config);
    }

    // ── Turn lifecycle ───────────────────────────────────────────

    /// Start a new turn. Returns an error if a turn is already active.
    pub async fn start_turn(&self) -> Result<String, CodexError> {
        let mut state = self.state.lock().await;
        if state.turn_active {
            // Emit error event for concurrent turn rejection
            let _ = self
                .tx_event
                .send(Event {
                    id: uuid::Uuid::new_v4().to_string(),
                    msg: EventMsg::Error(ErrorEvent {
                        message: "a turn is already active".into(),
                        codex_error_info: None,
                    }),
                })
                .await;
            return Err(CodexError::new(
                ErrorCode::SessionError,
                "a turn is already active",
            ));
        }

        state.turn_active = true;

        // Create TurnContext from resolved config
        let config = self.resolved_config();
        state.turn_context = Some(TurnContext::from_config(&config, self.cwd.clone()));

        let mut counter = self.turn_counter.lock().await;
        *counter += 1;
        let turn_id = format!("turn-{counter}");

        Ok(turn_id)
    }

    /// Complete the current turn.
    pub async fn complete_turn(&self) {
        let mut state = self.state.lock().await;
        state.turn_active = false;
        state.pending_approval = None;
    }

    /// Interrupt the current turn.
    pub async fn interrupt(&self) {
        let mut state = self.state.lock().await;
        state.turn_active = false;
        state.pending_approval = None;
        state.turn_context = None;
    }

    /// Check if a turn is currently active.
    pub async fn is_turn_active(&self) -> bool {
        self.state.lock().await.turn_active
    }

    // ── TurnContext access ───────────────────────────────────────

    /// Apply overrides to the current TurnContext.
    pub async fn apply_turn_context_overrides(
        &self,
        overrides: &TurnContextOverrides,
    ) -> Result<(), CodexError> {
        let mut state = self.state.lock().await;
        match &mut state.turn_context {
            Some(ctx) => {
                ctx.apply_overrides(overrides);
                Ok(())
            }
            None => Err(CodexError::new(
                ErrorCode::SessionError,
                "no active turn context to override",
            )),
        }
    }

    /// Get a snapshot of the current TurnContext.
    pub async fn turn_context(&self) -> Option<TurnContext> {
        self.state.lock().await.turn_context.clone()
    }

    // ── History management ───────────────────────────────────────

    /// Append items to the conversation history via ContextManager.
    pub async fn add_to_history(&self, items: Vec<ResponseInputItem>) {
        let mut state = self.state.lock().await;
        state.history.record_items(items);
    }

    /// Get a snapshot of the current history.
    pub async fn history(&self) -> Vec<ResponseInputItem> {
        self.state.lock().await.history.raw_items().to_vec()
    }

    /// Get the current history length.
    pub async fn history_len(&self) -> usize {
        self.state.lock().await.history.len()
    }

    /// Rollback the history by removing the last `steps` entries.
    pub async fn rollback(&self, steps: usize) -> Result<(), CodexError> {
        let mut state = self.state.lock().await;
        let len = state.history.len();
        if steps > len {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                format!("cannot rollback {steps} steps: history only has {len} entries"),
            ));
        }
        let mut items = state.history.raw_items().to_vec();
        items.truncate(len - steps);
        state.history.replace(items);
        Ok(())
    }

    // ── History compaction ────────────────────────────────────────

    /// Compact the conversation history using the given truncation policy.
    ///
    /// For `KeepRecent` and `KeepRecentTokens`, truncation is applied directly.
    /// For `AutoCompact`, the provided `summarize_fn` is called to generate a
    /// summary of older messages.
    ///
    /// Emits a `ContextCompacted` event if the history was actually shortened.
    /// If the history is already within the policy threshold, this is a no-op
    /// (idempotency).
    pub async fn compact_history<F, Fut>(
        &self,
        policy: &crate::core::truncation::TruncationPolicy,
        summarize_fn: Option<F>,
    ) -> Result<bool, CodexError>
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = Result<String, CodexError>>,
    {
        let current_history = {
            let state = self.state.lock().await;
            state.history.raw_items().to_vec()
        };

        let result = crate::core::compact::compact(&current_history, policy, summarize_fn).await?;
        let changed = result.changed;

        if changed {
            let mut state = self.state.lock().await;
            state.history.replace(result.history);
        }

        if changed {
            let _ = self
                .tx_event
                .send(Event {
                    id: uuid::Uuid::new_v4().to_string(),
                    msg: EventMsg::ContextCompacted(crate::protocol::event::ContextCompactedEvent),
                })
                .await;
        }

        Ok(changed)
    }

    /// Compact the conversation history by calling a remote API endpoint.
    pub async fn compact_history_remote(
        &self,
        endpoint: &str,
        model: &str,
    ) -> Result<bool, CodexError> {
        let current_history = {
            let state = self.state.lock().await;
            state.history.raw_items().to_vec()
        };

        let result =
            crate::core::compact::compact_remote(&current_history, endpoint, model).await?;
        let changed = result.changed;

        if changed {
            let mut state = self.state.lock().await;
            state.history.replace(result.history);
        }

        if changed {
            let _ = self
                .tx_event
                .send(Event {
                    id: uuid::Uuid::new_v4().to_string(),
                    msg: EventMsg::ContextCompacted(crate::protocol::event::ContextCompactedEvent),
                })
                .await;
        }

        Ok(changed)
    }

    // ── Resume/fork state ────────────────────────────────────────

    pub async fn set_reference_context_item(&self, item: Option<TurnContextItem>) {
        self.state
            .lock()
            .await
            .history
            .set_reference_context_item(item);
    }

    pub async fn set_token_info(&self, info: Option<TokenUsageInfo>) {
        self.state.lock().await.history.set_token_info(info);
    }

    pub async fn token_info(&self) -> Option<TokenUsageInfo> {
        self.state.lock().await.history.token_info().cloned()
    }

    pub async fn set_previous_turn_settings(&self, settings: Option<PreviousTurnSettings>) {
        self.state.lock().await.previous_turn_settings = settings;
    }

    pub async fn previous_turn_settings(&self) -> Option<PreviousTurnSettings> {
        self.state.lock().await.previous_turn_settings.clone()
    }

    pub async fn set_mcp_tool_selection(&self, tool_names: Vec<String>) {
        let mut state = self.state.lock().await;
        if tool_names.is_empty() {
            state.active_mcp_tool_selection = None;
        } else {
            let mut selected = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for name in tool_names {
                if seen.insert(name.clone()) {
                    selected.push(name);
                }
            }
            state.active_mcp_tool_selection = Some(selected);
        }
    }

    pub async fn clear_mcp_tool_selection(&self) {
        self.state.lock().await.active_mcp_tool_selection = None;
    }

    // ── Pending approval ─────────────────────────────────────────

    /// Set a pending approval request.
    pub async fn set_pending_approval(&self, approval: PendingApproval) {
        let mut state = self.state.lock().await;
        state.pending_approval = Some(approval);
    }

    /// Take (consume) the pending approval, returning it if present.
    pub async fn take_pending_approval(&self) -> Option<PendingApproval> {
        let mut state = self.state.lock().await;
        state.pending_approval.take()
    }

    // ── Component access ─────────────────────────────────────────

    pub fn mcp_manager(&self) -> &McpConnectionManager {
        &self.mcp_manager
    }

    pub async fn tool_router(&self) -> tokio::sync::MutexGuard<'_, ToolRouter> {
        self.tool_router.lock().await
    }

    pub async fn set_config_requirements(&self, requirements: crate::config::ConfigRequirements) {
        *self.config_requirements.write().await = requirements;
    }

    pub async fn config_requirements(&self) -> crate::config::ConfigRequirements {
        self.config_requirements.read().await.clone()
    }

    pub async fn collect_tool_specs_for_current_turn(&self) -> Vec<serde_json::Value> {
        let turn_context = self.turn_context().await;
        let config_requirements = self.config_requirements().await;
        let mut specs = {
            let router = self.tool_router().await;
            router.collect_tool_specs()
        };

        let preferred = Self::web_search_mode_from_tool_specs(&specs);
        let resolved = turn_context
            .as_ref()
            .map(|ctx| {
                Self::resolve_web_search_mode_for_turn_with_constraints(
                    preferred,
                    &config_requirements.web_search_mode,
                    &ctx.sandbox_policy,
                )
            })
            .unwrap_or(preferred);

        Self::apply_web_search_mode_to_tool_specs(&mut specs, resolved);
        specs
    }

    pub async fn hooks(&self) -> tokio::sync::MutexGuard<'_, HookRegistry> {
        self.hooks.lock().await
    }

    pub fn tx_event(&self) -> &async_channel::Sender<Event> {
        &self.tx_event
    }

    // ── ReviewDecision semantics ─────────────────────────────────

    /// Store custom instructions from a ReviewDecision to be forwarded to the
    /// Agent on the next turn.
    pub async fn set_custom_instructions(&self, instructions: String) {
        let mut state = self.state.lock().await;
        state.custom_instructions = Some(instructions);
    }

    /// Take (consume) any pending custom instructions.
    pub async fn take_custom_instructions(&self) -> Option<String> {
        let mut state = self.state.lock().await;
        state.custom_instructions.take()
    }

    /// Add a command prefix to the session-level allow list so that future
    /// executions of commands matching this prefix skip approval.
    pub async fn add_to_exec_allow_list(&self, prefix: Vec<String>) {
        let mut state = self.state.lock().await;
        if !state.exec_allow_list.contains(&prefix) {
            state.exec_allow_list.push(prefix);
        }
    }

    /// Check whether a command is covered by the session-level allow list.
    pub async fn is_exec_allow_listed(&self, command: &[String]) -> bool {
        let state = self.state.lock().await;
        state.exec_allow_list.iter().any(|prefix| {
            command.len() >= prefix.len() && command.iter().zip(prefix.iter()).all(|(a, b)| a == b)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigLayerStack;
    use crate::config::{Constrained, ConstraintError, RequirementSource};
    use crate::core::tools::ToolKind;
    use std::sync::Arc;

    fn make_session() -> (Session, async_channel::Receiver<Event>) {
        let (tx, rx) = async_channel::unbounded();
        let session = Session::new(PathBuf::from("/tmp/test"), ConfigLayerStack::new(), tx);
        (session, rx)
    }

    #[tokio::test]
    async fn session_has_unique_id() {
        let (s1, _) = make_session();
        let (s2, _) = make_session();
        assert_ne!(s1.id(), s2.id());
    }

    #[tokio::test]
    async fn session_default_router_contains_stable_tools_only() {
        let (session, _rx) = make_session();
        let router = session.tool_router.lock().await;
        let names: Vec<String> = router
            .configured_specs()
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(names.len(), 5);
        assert!(names.contains(&"shell".to_string()));
        assert!(names.contains(&"apply_patch".to_string()));
        assert!(names.contains(&"list_dir".to_string()));
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"grep_files".to_string()));
        assert!(!names.contains(&"spawn_agent".to_string()));
    }

    #[tokio::test]
    async fn session_config_can_enable_cached_web_search_spec() {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                web_search: Some(crate::protocol::types::WebSearchMode::Cached),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/tmp/test"), stack, tx);

        let router = session.tool_router.lock().await;
        let specs = router.collect_tool_specs();
        assert!(specs
            .iter()
            .any(|spec| { spec["type"] == "web_search" && spec["external_web_access"] == false }));
    }

    #[tokio::test]
    async fn session_explicit_disabled_web_search_mode_overrides_tools_toggle() {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                web_search: Some(crate::protocol::types::WebSearchMode::Disabled),
                tools: Some(crate::config::toml_types::ToolsToml {
                    enable_web_search: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/tmp/test"), stack, tx);

        let router = session.tool_router.lock().await;
        let names: Vec<String> = router
            .configured_specs()
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert!(!names.contains(&"web_search".to_string()));
    }

    #[tokio::test]
    async fn session_tools_config_can_enable_live_web_search_via_tools_toggle() {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                tools: Some(crate::config::toml_types::ToolsToml {
                    enable_web_search: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/tmp/test"), stack, tx);

        let router = session.tool_router.lock().await;
        let specs = router.collect_tool_specs();
        assert!(specs
            .iter()
            .any(|spec| { spec["type"] == "web_search" && spec["external_web_access"] == true }));
    }

    #[tokio::test]
    async fn session_explicit_web_search_mode_takes_precedence_over_tools_toggle() {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                web_search: Some(crate::protocol::types::WebSearchMode::Cached),
                tools: Some(crate::config::toml_types::ToolsToml {
                    enable_web_search: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/tmp/test"), stack, tx);

        let router = session.tool_router.lock().await;
        let specs = router.collect_tool_specs();
        assert!(specs
            .iter()
            .any(|spec| { spec["type"] == "web_search" && spec["external_web_access"] == false }));
        assert!(!specs
            .iter()
            .any(|spec| { spec["type"] == "web_search" && spec["external_web_access"] == true }));
    }

    #[tokio::test]
    async fn session_tools_toggle_false_does_not_override_explicit_web_search_mode() {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                web_search: Some(crate::protocol::types::WebSearchMode::Live),
                tools: Some(crate::config::toml_types::ToolsToml {
                    enable_web_search: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/tmp/test"), stack, tx);

        let router = session.tool_router.lock().await;
        let specs = router.collect_tool_specs();
        assert!(specs
            .iter()
            .any(|spec| { spec["type"] == "web_search" && spec["external_web_access"] == true }));
    }

    #[test]
    fn resolve_web_search_mode_for_turn_prefers_live_for_danger_full_access() {
        let mode = Session::resolve_web_search_mode_for_turn(
            Some(crate::protocol::types::WebSearchMode::Cached),
            &SandboxPolicy::DangerFullAccess,
        );

        assert_eq!(mode, Some(crate::protocol::types::WebSearchMode::Live));
    }

    #[test]
    fn resolve_web_search_mode_for_turn_preserves_absent_mode() {
        let mode =
            Session::resolve_web_search_mode_for_turn(None, &SandboxPolicy::DangerFullAccess);

        assert_eq!(mode, None);
    }

    #[test]
    fn resolve_constrained_web_search_mode_for_turn_uses_preference_for_read_only() {
        let web_search_mode = Constrained::allow_any(crate::protocol::types::WebSearchMode::Cached);
        let mode = Session::resolve_constrained_web_search_mode_for_turn(
            &web_search_mode,
            &SandboxPolicy::new_read_only_policy(),
        );

        assert_eq!(mode, crate::protocol::types::WebSearchMode::Cached);
    }

    #[test]
    fn resolve_constrained_web_search_mode_for_turn_respects_disabled_for_danger_full_access() {
        let web_search_mode =
            Constrained::allow_any(crate::protocol::types::WebSearchMode::Disabled);
        let mode = Session::resolve_constrained_web_search_mode_for_turn(
            &web_search_mode,
            &SandboxPolicy::DangerFullAccess,
        );

        assert_eq!(mode, crate::protocol::types::WebSearchMode::Disabled);
    }

    #[test]
    fn resolve_constrained_web_search_mode_for_turn_falls_back_when_live_is_disallowed() {
        let allowed = [
            crate::protocol::types::WebSearchMode::Disabled,
            crate::protocol::types::WebSearchMode::Cached,
        ];
        let web_search_mode = Constrained::new(
            crate::protocol::types::WebSearchMode::Cached,
            move |candidate| {
                if allowed.contains(candidate) {
                    Ok(())
                } else {
                    Err(ConstraintError::InvalidValue {
                        field_name: "web_search_mode",
                        candidate: format!("{candidate:?}"),
                        allowed: format!("{allowed:?}"),
                        requirement_source: RequirementSource::Unknown,
                    })
                }
            },
        )
        .unwrap();
        let mode = Session::resolve_constrained_web_search_mode_for_turn(
            &web_search_mode,
            &SandboxPolicy::DangerFullAccess,
        );

        assert_eq!(mode, crate::protocol::types::WebSearchMode::Cached);
    }

    #[test]
    fn resolve_web_search_mode_for_turn_with_constraints_can_remove_web_search() {
        let allowed = [crate::protocol::types::WebSearchMode::Disabled];
        let constraints = Constrained::new(
            crate::protocol::types::WebSearchMode::Disabled,
            move |candidate| {
                if allowed.contains(candidate) {
                    Ok(())
                } else {
                    Err(ConstraintError::InvalidValue {
                        field_name: "web_search_mode",
                        candidate: format!("{candidate:?}"),
                        allowed: format!("{allowed:?}"),
                        requirement_source: RequirementSource::Unknown,
                    })
                }
            },
        )
        .unwrap();

        let mode = Session::resolve_web_search_mode_for_turn_with_constraints(
            Some(crate::protocol::types::WebSearchMode::Cached),
            &constraints,
            &SandboxPolicy::DangerFullAccess,
        );

        assert_eq!(mode, None);
    }

    #[tokio::test]
    async fn session_collect_tool_specs_for_current_turn_promotes_cached_web_search_to_live() {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                web_search: Some(crate::protocol::types::WebSearchMode::Cached),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/tmp/test"), stack, tx);
        session.start_turn().await.unwrap();
        session
            .apply_turn_context_overrides(&TurnContextOverrides {
                model: None,
                sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
                approval_policy: None,
                cwd: None,
                collaboration_mode: None,
                personality: None,
            })
            .await
            .unwrap();

        let specs = session.collect_tool_specs_for_current_turn().await;
        assert!(specs
            .iter()
            .any(|spec| { spec["type"] == "web_search" && spec["external_web_access"] == true }));
        assert!(!specs
            .iter()
            .any(|spec| { spec["type"] == "web_search" && spec["external_web_access"] == false }));
    }

    #[tokio::test]
    async fn session_collect_tool_specs_for_current_turn_respects_injected_web_search_constraints()
    {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                web_search: Some(crate::protocol::types::WebSearchMode::Cached),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/tmp/test"), stack, tx);
        session
            .set_config_requirements(
                crate::config::ConfigRequirements::try_from(
                    crate::config::ConfigRequirementsToml {
                        allowed_web_search_modes: Some(vec![
                            crate::config::WebSearchModeRequirement::Cached,
                        ]),
                        ..Default::default()
                    },
                )
                .unwrap(),
            )
            .await;
        session.start_turn().await.unwrap();
        session
            .apply_turn_context_overrides(&TurnContextOverrides {
                model: None,
                sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
                approval_policy: None,
                cwd: None,
                collaboration_mode: None,
                personality: None,
            })
            .await
            .unwrap();

        let specs = session.collect_tool_specs_for_current_turn().await;
        assert!(specs
            .iter()
            .any(|spec| { spec["type"] == "web_search" && spec["external_web_access"] == false }));
        assert!(!specs
            .iter()
            .any(|spec| { spec["type"] == "web_search" && spec["external_web_access"] == true }));
    }

    #[tokio::test]
    async fn session_collect_tool_specs_for_current_turn_can_drop_web_search_when_constraints_disable_it(
    ) {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                web_search: Some(crate::protocol::types::WebSearchMode::Cached),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/tmp/test"), stack, tx);
        session
            .set_config_requirements(
                crate::config::ConfigRequirements::try_from(
                    crate::config::ConfigRequirementsToml {
                        allowed_web_search_modes: Some(vec![]),
                        ..Default::default()
                    },
                )
                .unwrap(),
            )
            .await;
        session.start_turn().await.unwrap();
        session
            .apply_turn_context_overrides(&TurnContextOverrides {
                model: None,
                sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
                approval_policy: None,
                cwd: None,
                collaboration_mode: None,
                personality: None,
            })
            .await
            .unwrap();

        let specs = session.collect_tool_specs_for_current_turn().await;
        assert!(!specs.iter().any(|spec| spec["type"] == "web_search"));
    }

    #[tokio::test]
    async fn session_with_agent_control_adds_collab_specs() {
        let (tx, _rx) = async_channel::unbounded();
        let agent_control = Arc::new(crate::core::agent::control::AgentControl::default());
        let session = Session::new_with_agent_control(
            PathBuf::from("/tmp/test"),
            ConfigLayerStack::new(),
            tx,
            Some(agent_control),
        );
        let router = session.tool_router.lock().await;
        let names: Vec<String> = router
            .configured_specs()
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert!(names.contains(&"spawn_agent".to_string()));
        assert!(names.contains(&"send_input".to_string()));
        assert!(names.contains(&"resume_agent".to_string()));
        assert!(names.contains(&"wait".to_string()));
        assert!(names.contains(&"close_agent".to_string()));
        assert!(router
            .registry()
            .find(&ToolKind::Builtin("spawn_agent".to_string()))
            .is_some());
    }

    #[tokio::test]
    async fn start_turn_creates_context() {
        let (session, _rx) = make_session();
        let turn_id = session.start_turn().await.unwrap();
        assert_eq!(turn_id, "turn-1");
        assert!(session.is_turn_active().await);
        assert!(session.turn_context().await.is_some());
    }

    #[tokio::test]
    async fn reject_concurrent_turn() {
        let (session, rx) = make_session();
        session.start_turn().await.unwrap();

        let err = session.start_turn().await.unwrap_err();
        assert_eq!(err.code, ErrorCode::SessionError);

        // Should have emitted an error event
        let event = rx.try_recv().unwrap();
        assert!(matches!(event.msg, EventMsg::Error(_)));
    }

    #[tokio::test]
    async fn complete_turn_allows_new_turn() {
        let (session, _rx) = make_session();
        session.start_turn().await.unwrap();
        session.complete_turn().await;

        assert!(!session.is_turn_active().await);
        let turn_id = session.start_turn().await.unwrap();
        assert_eq!(turn_id, "turn-2");
    }

    #[tokio::test]
    async fn interrupt_resets_turn_state() {
        let (session, _rx) = make_session();
        session.start_turn().await.unwrap();
        session.interrupt().await;

        assert!(!session.is_turn_active().await);
        assert!(session.turn_context().await.is_none());
    }

    #[tokio::test]
    async fn turn_context_inherits_from_config() {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                model: Some("gpt-4o".into()),
                model_provider: Some("openai".into()),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/workspace"), stack, tx);
        session.start_turn().await.unwrap();

        let ctx = session.turn_context().await.unwrap();
        assert_eq!(ctx.model_info.model, "gpt-4o");
        assert_eq!(ctx.model_info.provider, "openai");
        assert_eq!(ctx.cwd, PathBuf::from("/workspace"));
    }

    #[tokio::test]
    async fn override_turn_context_partial() {
        let (session, _rx) = make_session();
        session.start_turn().await.unwrap();

        let original = session.turn_context().await.unwrap();
        let original_cwd = original.cwd.clone();

        let overrides = TurnContextOverrides {
            model: Some("gpt-4o-mini".into()),
            sandbox_policy: None,
            approval_policy: None,
            cwd: None,
            collaboration_mode: None,
            personality: None,
        };
        session
            .apply_turn_context_overrides(&overrides)
            .await
            .unwrap();

        let updated = session.turn_context().await.unwrap();
        assert_eq!(updated.model_info.model, "gpt-4o-mini");
        // cwd should remain unchanged
        assert_eq!(updated.cwd, original_cwd);
    }

    #[tokio::test]
    async fn override_without_active_context_errors() {
        let (session, _rx) = make_session();
        let overrides = TurnContextOverrides {
            model: Some("test".into()),
            sandbox_policy: None,
            approval_policy: None,
            cwd: None,
            collaboration_mode: None,
            personality: None,
        };
        let err = session
            .apply_turn_context_overrides(&overrides)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::SessionError);
    }

    #[tokio::test]
    async fn history_preserves_insertion_order() {
        let (session, _rx) = make_session();

        let items = vec![
            ResponseInputItem::text_message("user", "hello".to_string()),
            ResponseInputItem::text_message("assistant", "hi".to_string()),
        ];
        session.add_to_history(items).await;

        session
            .add_to_history(vec![ResponseInputItem::text_message(
                "user",
                "how are you".to_string(),
            )])
            .await;

        let history = session.history().await;
        assert_eq!(history.len(), 3);

        // Verify order
        assert!(history[0].message_text().as_deref() == Some("hello"));
        assert!(history[1].message_text().as_deref() == Some("hi"));
        assert!(history[2].message_text().as_deref() == Some("how are you"));
    }

    #[tokio::test]
    async fn rollback_removes_last_n_entries() {
        let (session, _rx) = make_session();

        let items: Vec<ResponseInputItem> = (0..5)
            .map(|i| ResponseInputItem::text_message("user", format!("msg-{i}")))
            .collect();
        session.add_to_history(items).await;

        session.rollback(2).await.unwrap();

        let history = session.history().await;
        assert_eq!(history.len(), 3);
        assert!(history[0].message_text().as_deref() == Some("msg-0"));
        assert!(history[1].message_text().as_deref() == Some("msg-1"));
        assert!(history[2].message_text().as_deref() == Some("msg-2"));
    }

    #[tokio::test]
    async fn rollback_all_entries() {
        let (session, _rx) = make_session();
        session
            .add_to_history(vec![ResponseInputItem::text_message(
                "user",
                "test".to_string(),
            )])
            .await;

        session.rollback(1).await.unwrap();
        assert_eq!(session.history_len().await, 0);
    }

    #[tokio::test]
    async fn rollback_exceeding_length_errors() {
        let (session, _rx) = make_session();
        session
            .add_to_history(vec![ResponseInputItem::text_message(
                "user",
                "test".to_string(),
            )])
            .await;

        let err = session.rollback(5).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn rollback_zero_is_noop() {
        let (session, _rx) = make_session();
        session
            .add_to_history(vec![ResponseInputItem::text_message(
                "user",
                "test".to_string(),
            )])
            .await;

        session.rollback(0).await.unwrap();
        assert_eq!(session.history_len().await, 1);
    }

    #[tokio::test]
    async fn model_from_config() {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                model: Some("claude-3".into()),
                ..Default::default()
            },
        );
        let session = Session::new(PathBuf::from("/tmp"), stack, tx);
        assert_eq!(session.model(), "claude-3");
    }

    #[tokio::test]
    async fn set_model_updates_session_layer() {
        let (tx, _rx) = async_channel::unbounded();
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            crate::config::ConfigLayer::User,
            ConfigToml {
                model: Some("old-model".into()),
                ..Default::default()
            },
        );
        let mut session = Session::new(PathBuf::from("/tmp"), stack, tx);
        session.set_model("new-model".into());
        // Session layer has highest precedence, so it overrides User layer
        assert_eq!(session.model(), "new-model");
    }

    #[tokio::test]
    async fn pending_approval_lifecycle() {
        let (session, _rx) = make_session();

        assert!(session.take_pending_approval().await.is_none());

        session
            .set_pending_approval(PendingApproval::Exec {
                call_id: "c1".into(),
                command: vec!["rm".into(), "-rf".into()],
                cwd: PathBuf::from("/tmp"),
            })
            .await;

        let approval = session.take_pending_approval().await;
        assert!(approval.is_some());
        assert!(matches!(approval.unwrap(), PendingApproval::Exec { .. }));

        // After take, should be None
        assert!(session.take_pending_approval().await.is_none());
    }

    // ── ReviewDecision semantics tests ───────────────────────────

    #[tokio::test]
    async fn custom_instructions_set_and_take() {
        let (session, _rx) = make_session();

        // Initially no custom instructions
        assert!(session.take_custom_instructions().await.is_none());

        session
            .set_custom_instructions("focus on error handling".into())
            .await;
        let instructions = session.take_custom_instructions().await;
        assert_eq!(instructions.as_deref(), Some("focus on error handling"));

        // After take, should be None (consumed)
        assert!(session.take_custom_instructions().await.is_none());
    }

    #[tokio::test]
    async fn exec_allow_list_add_and_check() {
        let (session, _rx) = make_session();

        // Not allow-listed initially
        assert!(
            !session
                .is_exec_allow_listed(&["ls".into(), "-la".into()])
                .await
        );

        session.add_to_exec_allow_list(vec!["ls".into()]).await;

        // Prefix match: "ls -la" matches prefix ["ls"]
        assert!(
            session
                .is_exec_allow_listed(&["ls".into(), "-la".into()])
                .await
        );
        // Exact match
        assert!(session.is_exec_allow_listed(&["ls".into()]).await);
        // Non-matching command
        assert!(!session.is_exec_allow_listed(&["rm".into()]).await);
    }

    #[tokio::test]
    async fn exec_allow_list_deduplicates() {
        let (session, _rx) = make_session();

        session.add_to_exec_allow_list(vec!["echo".into()]).await;
        session.add_to_exec_allow_list(vec!["echo".into()]).await;

        let state = session.state.lock().await;
        assert_eq!(state.exec_allow_list.len(), 1);
    }

    #[tokio::test]
    async fn exec_allow_list_empty_command_not_matched() {
        let (session, _rx) = make_session();

        session
            .add_to_exec_allow_list(vec!["git".into(), "status".into()])
            .await;

        // Empty command should not match any prefix
        assert!(!session.is_exec_allow_listed(&[]).await);
    }
}
