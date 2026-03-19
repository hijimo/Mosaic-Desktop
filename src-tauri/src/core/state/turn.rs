//! Turn-scoped state and active turn metadata scaffolding.

use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;

use crate::protocol::types::{DynamicToolResponse, ResponseInputItem, ReviewDecision};
use crate::core::tasks::{SessionTask, TaskKind};
use crate::core::session::TurnContext;

/// Metadata about the currently running turn.
pub struct ActiveTurn {
    pub tasks: IndexMap<String, RunningTask>,
    pub turn_state: Arc<Mutex<TurnState>>,
}

impl Default for ActiveTurn {
    fn default() -> Self {
        Self {
            tasks: IndexMap::new(),
            turn_state: Arc::new(Mutex::new(TurnState::default())),
        }
    }
}

pub struct RunningTask {
    pub done: Arc<Notify>,
    pub kind: TaskKind,
    pub task: Arc<dyn SessionTask>,
    pub cancellation_token: CancellationToken,
    pub handle: Arc<AbortOnDropHandle<()>>,
    pub turn_context: Arc<TurnContext>,
}

impl ActiveTurn {
    pub fn add_task(&mut self, sub_id: String, task: RunningTask) {
        self.tasks.insert(sub_id, task);
    }

    pub fn remove_task(&mut self, sub_id: &str) -> bool {
        self.tasks.swap_remove(sub_id);
        self.tasks.is_empty()
    }

    pub fn drain_tasks(&mut self) -> Vec<RunningTask> {
        self.tasks.drain(..).map(|(_, task)| task).collect()
    }

    /// Clear any pending approvals and input buffered for the current turn.
    pub async fn clear_pending(&self) {
        let mut ts = self.turn_state.lock().await;
        ts.clear_pending();
    }
}

/// Mutable state for a single turn.
#[derive(Default)]
pub struct TurnState {
    pending_approvals: HashMap<String, oneshot::Sender<ReviewDecision>>,
    pending_user_input: HashMap<String, oneshot::Sender<serde_json::Value>>,
    pending_dynamic_tools: HashMap<String, oneshot::Sender<DynamicToolResponse>>,
    pending_input: Vec<ResponseInputItem>,
}

impl TurnState {
    pub fn insert_pending_approval(
        &mut self,
        key: String,
        tx: oneshot::Sender<ReviewDecision>,
    ) -> Option<oneshot::Sender<ReviewDecision>> {
        self.pending_approvals.insert(key, tx)
    }

    pub fn remove_pending_approval(
        &mut self,
        key: &str,
    ) -> Option<oneshot::Sender<ReviewDecision>> {
        self.pending_approvals.remove(key)
    }

    pub fn clear_pending(&mut self) {
        self.pending_approvals.clear();
        self.pending_user_input.clear();
        self.pending_dynamic_tools.clear();
        self.pending_input.clear();
    }

    pub fn insert_pending_user_input(
        &mut self,
        key: String,
        tx: oneshot::Sender<serde_json::Value>,
    ) -> Option<oneshot::Sender<serde_json::Value>> {
        self.pending_user_input.insert(key, tx)
    }

    pub fn remove_pending_user_input(
        &mut self,
        key: &str,
    ) -> Option<oneshot::Sender<serde_json::Value>> {
        self.pending_user_input.remove(key)
    }

    pub fn insert_pending_dynamic_tool(
        &mut self,
        key: String,
        tx: oneshot::Sender<DynamicToolResponse>,
    ) -> Option<oneshot::Sender<DynamicToolResponse>> {
        self.pending_dynamic_tools.insert(key, tx)
    }

    pub fn remove_pending_dynamic_tool(
        &mut self,
        key: &str,
    ) -> Option<oneshot::Sender<DynamicToolResponse>> {
        self.pending_dynamic_tools.remove(key)
    }

    pub fn push_pending_input(&mut self, input: ResponseInputItem) {
        self.pending_input.push(input);
    }

    pub fn take_pending_input(&mut self) -> Vec<ResponseInputItem> {
        std::mem::take(&mut self.pending_input)
    }

    pub fn has_pending_input(&self) -> bool {
        !self.pending_input.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_turn_default_is_empty() {
        let turn = ActiveTurn::default();
        assert!(turn.tasks.is_empty());
    }

    #[test]
    fn turn_state_clear_pending() {
        let mut ts = TurnState::default();
        let (tx, _rx) = oneshot::channel();
        ts.insert_pending_approval("a".into(), tx);
        ts.push_pending_input(ResponseInputItem::text_message("user", "hi".into()));
        assert!(ts.has_pending_input());

        ts.clear_pending();
        assert!(!ts.has_pending_input());
        assert!(ts.remove_pending_approval("a").is_none());
    }

    #[test]
    fn turn_state_pending_input_take() {
        let mut ts = TurnState::default();
        assert!(!ts.has_pending_input());
        assert!(ts.take_pending_input().is_empty());

        ts.push_pending_input(ResponseInputItem::text_message("user", "msg1".into()));
        ts.push_pending_input(ResponseInputItem::text_message("user", "msg2".into()));
        assert!(ts.has_pending_input());

        let items = ts.take_pending_input();
        assert_eq!(items.len(), 2);
        assert!(!ts.has_pending_input());
    }

    #[test]
    fn turn_state_pending_approval_insert_remove() {
        let mut ts = TurnState::default();
        let (tx, _rx) = oneshot::channel();
        assert!(ts.insert_pending_approval("key1".into(), tx).is_none());
        assert!(ts.remove_pending_approval("key1").is_some());
        assert!(ts.remove_pending_approval("key1").is_none());
    }

    #[test]
    fn turn_state_pending_user_input_insert_remove() {
        let mut ts = TurnState::default();
        let (tx, _rx) = oneshot::channel();
        assert!(ts.insert_pending_user_input("key1".into(), tx).is_none());
        assert!(ts.remove_pending_user_input("key1").is_some());
        assert!(ts.remove_pending_user_input("key1").is_none());
    }

    #[test]
    fn turn_state_pending_dynamic_tool_insert_remove() {
        let mut ts = TurnState::default();
        let (tx, _rx) = oneshot::channel();
        assert!(ts.insert_pending_dynamic_tool("key1".into(), tx).is_none());
        assert!(ts.remove_pending_dynamic_tool("key1").is_some());
        assert!(ts.remove_pending_dynamic_tool("key1").is_none());
    }
}
