use std::path::PathBuf;

use crate::config::toml_types::ConfigToml;
use crate::config::ConfigLayerStack;
use crate::core::hooks::HookRegistry;
use crate::core::mcp_client::McpConnectionManager;
use crate::core::tools::router::ToolRouter;
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{ErrorEvent, Event, EventMsg};
use crate::protocol::types::{
    AskForApproval, CollaborationMode, Effort, Personality, ReasoningSummary, ResponseInputItem,
    SandboxPolicy, ServiceTier, TurnContextOverrides,
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
        path: PathBuf,
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
#[derive(Debug)]
pub struct SessionState {
    /// Ordered conversation history.
    pub history: Vec<ResponseInputItem>,
    /// Whether a turn is currently active.
    pub turn_active: bool,
    /// Pending approval request, if any.
    pub pending_approval: Option<PendingApproval>,
    /// Current turn context (set when a turn starts).
    pub turn_context: Option<TurnContext>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            turn_active: false,
            pending_approval: None,
            turn_context: None,
        }
    }
}

impl Default for SessionState {
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
    state: tokio::sync::Mutex<SessionState>,
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
}

impl Session {
    pub fn new(
        cwd: PathBuf,
        config: ConfigLayerStack,
        tx_event: async_channel::Sender<Event>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            state: tokio::sync::Mutex::new(SessionState::new()),
            config,
            active_profile: None,
            tool_router: tokio::sync::Mutex::new(ToolRouter::new(
                crate::core::tools::ToolRegistry::new(),
                McpConnectionManager::new(),
            )),
            mcp_manager: McpConnectionManager::new(),
            hooks: tokio::sync::Mutex::new(HookRegistry::new()),
            tx_event,
            cwd,
            turn_counter: tokio::sync::Mutex::new(0),
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
        let mut session_config = ConfigToml::default();
        session_config.model = Some(model);
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

    /// Append items to the conversation history, preserving insertion order.
    pub async fn add_to_history(&self, items: Vec<ResponseInputItem>) {
        let mut state = self.state.lock().await;
        state.history.extend(items);
    }

    /// Get a snapshot of the current history.
    pub async fn history(&self) -> Vec<ResponseInputItem> {
        self.state.lock().await.history.clone()
    }

    /// Get the current history length.
    pub async fn history_len(&self) -> usize {
        self.state.lock().await.history.len()
    }

    /// Rollback the history by removing the last `steps` entries.
    /// Remaining entries preserve their original order.
    pub async fn rollback(&self, steps: usize) -> Result<(), CodexError> {
        let mut state = self.state.lock().await;
        let len = state.history.len();
        if steps > len {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                format!("cannot rollback {steps} steps: history only has {len} entries"),
            ));
        }
        state.history.truncate(len - steps);
        Ok(())
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

    pub async fn hooks(&self) -> tokio::sync::MutexGuard<'_, HookRegistry> {
        self.hooks.lock().await
    }

    pub fn tx_event(&self) -> &async_channel::Sender<Event> {
        &self.tx_event
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigLayerStack;

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
            ResponseInputItem::Message {
                role: "user".into(),
                content: "hello".into(),
            },
            ResponseInputItem::Message {
                role: "assistant".into(),
                content: "hi".into(),
            },
        ];
        session.add_to_history(items).await;

        session
            .add_to_history(vec![ResponseInputItem::Message {
                role: "user".into(),
                content: "how are you".into(),
            }])
            .await;

        let history = session.history().await;
        assert_eq!(history.len(), 3);

        // Verify order
        assert!(
            matches!(&history[0], ResponseInputItem::Message { content, .. } if content == "hello")
        );
        assert!(
            matches!(&history[1], ResponseInputItem::Message { content, .. } if content == "hi")
        );
        assert!(
            matches!(&history[2], ResponseInputItem::Message { content, .. } if content == "how are you")
        );
    }

    #[tokio::test]
    async fn rollback_removes_last_n_entries() {
        let (session, _rx) = make_session();

        let items: Vec<ResponseInputItem> = (0..5)
            .map(|i| ResponseInputItem::Message {
                role: "user".into(),
                content: format!("msg-{i}"),
            })
            .collect();
        session.add_to_history(items).await;

        session.rollback(2).await.unwrap();

        let history = session.history().await;
        assert_eq!(history.len(), 3);
        assert!(
            matches!(&history[0], ResponseInputItem::Message { content, .. } if content == "msg-0")
        );
        assert!(
            matches!(&history[1], ResponseInputItem::Message { content, .. } if content == "msg-1")
        );
        assert!(
            matches!(&history[2], ResponseInputItem::Message { content, .. } if content == "msg-2")
        );
    }

    #[tokio::test]
    async fn rollback_all_entries() {
        let (session, _rx) = make_session();
        session
            .add_to_history(vec![ResponseInputItem::Message {
                role: "user".into(),
                content: "test".into(),
            }])
            .await;

        session.rollback(1).await.unwrap();
        assert_eq!(session.history_len().await, 0);
    }

    #[tokio::test]
    async fn rollback_exceeding_length_errors() {
        let (session, _rx) = make_session();
        session
            .add_to_history(vec![ResponseInputItem::Message {
                role: "user".into(),
                content: "test".into(),
            }])
            .await;

        let err = session.rollback(5).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn rollback_zero_is_noop() {
        let (session, _rx) = make_session();
        session
            .add_to_history(vec![ResponseInputItem::Message {
                role: "user".into(),
                content: "test".into(),
            }])
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
}
