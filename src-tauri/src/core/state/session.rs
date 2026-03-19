//! Session-wide mutable state.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::core::context_manager::ContextManager;
use crate::protocol::types::ResponseInputItem;

/// Persistent, session-scoped state that tracks conversation history,
/// token usage, rate limits, and other per-session bookkeeping.
///
/// This is the Mosaic equivalent of Codex's `state/session.rs`.
pub struct SessionState {
    pub history: ContextManager,
    pub server_reasoning_included: bool,
    pub dependency_env: HashMap<String, String>,
    pub mcp_dependency_prompted: HashSet<String>,
    pub active_mcp_tool_selection: Option<Vec<String>>,
    pub active_connector_selection: HashSet<String>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            history: ContextManager::new(),
            server_reasoning_included: false,
            dependency_env: HashMap::new(),
            mcp_dependency_prompted: HashSet::new(),
            active_mcp_tool_selection: None,
            active_connector_selection: HashSet::new(),
        }
    }

    // ── History helpers ──────────────────────────────────────────

    pub fn record_items(&mut self, items: impl IntoIterator<Item = ResponseInputItem>) {
        self.history.record_items(items);
    }

    pub fn clone_history(&self) -> ContextManager {
        self.history.clone()
    }

    pub fn replace_history(&mut self, items: Vec<ResponseInputItem>) {
        self.history.replace(items);
    }

    // ── Token / reasoning helpers ────────────────────────────────

    pub fn update_token_usage(&mut self, total_tokens: i64) {
        self.history.update_token_usage(total_tokens);
    }

    pub fn estimate_total_tokens(&self) -> i64 {
        self.history.estimate_total_tokens()
    }

    pub fn set_server_reasoning_included(&mut self, included: bool) {
        self.server_reasoning_included = included;
    }

    pub fn server_reasoning_included(&self) -> bool {
        self.server_reasoning_included
    }

    // ── MCP dependency tracking ──────────────────────────────────

    pub fn record_mcp_dependency_prompted<I>(&mut self, names: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.mcp_dependency_prompted.extend(names);
    }

    pub fn mcp_dependency_prompted(&self) -> HashSet<String> {
        self.mcp_dependency_prompted.clone()
    }

    pub fn set_dependency_env(&mut self, values: HashMap<String, String>) {
        for (key, value) in values {
            self.dependency_env.insert(key, value);
        }
    }

    pub fn dependency_env(&self) -> HashMap<String, String> {
        self.dependency_env.clone()
    }

    // ── MCP tool selection ───────────────────────────────────────

    pub fn merge_mcp_tool_selection(&mut self, tool_names: Vec<String>) -> Vec<String> {
        if tool_names.is_empty() {
            return self.active_mcp_tool_selection.clone().unwrap_or_default();
        }

        let mut merged = self.active_mcp_tool_selection.take().unwrap_or_default();
        let mut seen: HashSet<String> = merged.iter().cloned().collect();

        for tool_name in tool_names {
            if seen.insert(tool_name.clone()) {
                merged.push(tool_name);
            }
        }

        self.active_mcp_tool_selection = Some(merged.clone());
        merged
    }

    pub fn set_mcp_tool_selection(&mut self, tool_names: Vec<String>) {
        if tool_names.is_empty() {
            self.active_mcp_tool_selection = None;
            return;
        }

        let mut selected = Vec::new();
        let mut seen = HashSet::new();
        for tool_name in tool_names {
            if seen.insert(tool_name.clone()) {
                selected.push(tool_name);
            }
        }

        self.active_mcp_tool_selection = if selected.is_empty() {
            None
        } else {
            Some(selected)
        };
    }

    pub fn get_mcp_tool_selection(&self) -> Option<Vec<String>> {
        self.active_mcp_tool_selection.clone()
    }

    pub fn clear_mcp_tool_selection(&mut self) {
        self.active_mcp_tool_selection = None;
    }

    // ── Connector selection ──────────────────────────────────────

    pub fn merge_connector_selection<I>(&mut self, connector_ids: I) -> HashSet<String>
    where
        I: IntoIterator<Item = String>,
    {
        self.active_connector_selection.extend(connector_ids);
        self.active_connector_selection.clone()
    }

    pub fn get_connector_selection(&self) -> HashSet<String> {
        self.active_connector_selection.clone()
    }

    pub fn clear_connector_selection(&mut self) {
        self.active_connector_selection.clear();
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_state_is_empty() {
        let state = SessionState::new();
        assert!(state.history.is_empty());
        assert!(!state.server_reasoning_included);
        assert!(state.dependency_env.is_empty());
        assert!(state.mcp_dependency_prompted.is_empty());
        assert!(state.active_mcp_tool_selection.is_none());
        assert!(state.active_connector_selection.is_empty());
    }

    #[test]
    fn merge_mcp_tool_selection_deduplicates_and_preserves_order() {
        let mut state = SessionState::new();

        let merged = state.merge_mcp_tool_selection(vec![
            "mcp__rmcp__echo".to_string(),
            "mcp__rmcp__image".to_string(),
            "mcp__rmcp__echo".to_string(),
        ]);
        assert_eq!(
            merged,
            vec!["mcp__rmcp__echo".to_string(), "mcp__rmcp__image".to_string()]
        );

        let merged = state.merge_mcp_tool_selection(vec![
            "mcp__rmcp__image".to_string(),
            "mcp__rmcp__search".to_string(),
        ]);
        assert_eq!(
            merged,
            vec![
                "mcp__rmcp__echo".to_string(),
                "mcp__rmcp__image".to_string(),
                "mcp__rmcp__search".to_string(),
            ]
        );
    }

    #[test]
    fn merge_mcp_tool_selection_empty_input_is_noop() {
        let mut state = SessionState::new();
        state.merge_mcp_tool_selection(vec![
            "mcp__rmcp__echo".to_string(),
            "mcp__rmcp__image".to_string(),
        ]);

        let merged = state.merge_mcp_tool_selection(Vec::new());
        assert_eq!(
            merged,
            vec!["mcp__rmcp__echo".to_string(), "mcp__rmcp__image".to_string()]
        );
    }

    #[test]
    fn clear_mcp_tool_selection_removes_selection() {
        let mut state = SessionState::new();
        state.merge_mcp_tool_selection(vec!["mcp__rmcp__echo".to_string()]);
        state.clear_mcp_tool_selection();
        assert_eq!(state.get_mcp_tool_selection(), None);
    }

    #[test]
    fn set_mcp_tool_selection_deduplicates_and_preserves_order() {
        let mut state = SessionState::new();
        state.merge_mcp_tool_selection(vec!["mcp__rmcp__old".to_string()]);

        state.set_mcp_tool_selection(vec![
            "mcp__rmcp__echo".to_string(),
            "mcp__rmcp__image".to_string(),
            "mcp__rmcp__echo".to_string(),
            "mcp__rmcp__search".to_string(),
        ]);

        assert_eq!(
            state.get_mcp_tool_selection(),
            Some(vec![
                "mcp__rmcp__echo".to_string(),
                "mcp__rmcp__image".to_string(),
                "mcp__rmcp__search".to_string(),
            ])
        );
    }

    #[test]
    fn set_mcp_tool_selection_empty_input_clears_selection() {
        let mut state = SessionState::new();
        state.merge_mcp_tool_selection(vec!["mcp__rmcp__echo".to_string()]);
        state.set_mcp_tool_selection(Vec::new());
        assert_eq!(state.get_mcp_tool_selection(), None);
    }

    #[test]
    fn merge_connector_selection_deduplicates_entries() {
        let mut state = SessionState::new();
        let merged = state.merge_connector_selection([
            "calendar".to_string(),
            "calendar".to_string(),
            "drive".to_string(),
        ]);
        assert_eq!(
            merged,
            HashSet::from(["calendar".to_string(), "drive".to_string()])
        );
    }

    #[test]
    fn clear_connector_selection_removes_entries() {
        let mut state = SessionState::new();
        state.merge_connector_selection(["calendar".to_string()]);
        state.clear_connector_selection();
        assert_eq!(state.get_connector_selection(), HashSet::new());
    }

    #[test]
    fn dependency_env_merges() {
        let mut state = SessionState::new();
        state.set_dependency_env(HashMap::from([("A".into(), "1".into())]));
        state.set_dependency_env(HashMap::from([("B".into(), "2".into())]));
        let env = state.dependency_env();
        assert_eq!(env.get("A"), Some(&"1".to_string()));
        assert_eq!(env.get("B"), Some(&"2".to_string()));
    }

    #[test]
    fn mcp_dependency_prompted_accumulates() {
        let mut state = SessionState::new();
        state.record_mcp_dependency_prompted(["a".into(), "b".into()]);
        state.record_mcp_dependency_prompted(["b".into(), "c".into()]);
        let prompted = state.mcp_dependency_prompted();
        assert_eq!(prompted.len(), 3);
        assert!(prompted.contains("a"));
        assert!(prompted.contains("b"));
        assert!(prompted.contains("c"));
    }

    #[test]
    fn record_and_replace_history() {
        let mut state = SessionState::new();
        state.record_items(vec![
            ResponseInputItem::text_message("user", "hello".into()),
        ]);
        assert_eq!(state.history.len(), 1);

        state.replace_history(vec![
            ResponseInputItem::text_message("user", "new".into()),
        ]);
        assert_eq!(state.history.len(), 1);
    }

    #[test]
    fn server_reasoning_toggle() {
        let mut state = SessionState::new();
        assert!(!state.server_reasoning_included());
        state.set_server_reasoning_included(true);
        assert!(state.server_reasoning_included());
    }
}
