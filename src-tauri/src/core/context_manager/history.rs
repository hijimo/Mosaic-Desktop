use crate::protocol::types::{ContentItem, ContentOrItems, FunctionCallOutputBody, FunctionCallOutputPayload, ResponseInputItem};

use super::normalize;

/// Transcript of thread history with token tracking.
#[derive(Debug, Clone, Default)]
pub struct ContextManager {
    /// Items ordered oldest → newest.
    items: Vec<ResponseInputItem>,
    /// Last known total token count from the API response.
    last_api_total_tokens: i64,
}

/// Breakdown of estimated token usage.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsageBreakdown {
    /// Total tokens reported by the last API response.
    pub last_api_total_tokens: i64,
    /// Estimated bytes of all history items.
    pub all_items_bytes: i64,
    /// Estimated tokens of items added since the last API response.
    pub pending_tokens: i64,
}

impl ContextManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record items into the history. Non-API items are silently skipped.
    pub fn record_items(&mut self, items: impl IntoIterator<Item = ResponseInputItem>) {
        for item in items {
            if !is_api_item(&item) {
                continue;
            }
            let processed = truncate_output_if_needed(item);
            self.items.push(processed);
        }
    }

    /// Return the history prepared for sending to the model.
    ///
    /// This normalizes the history (ensures call/output pairing) and
    /// returns a clone suitable for prompt construction.
    pub fn for_prompt(&self) -> Vec<ResponseInputItem> {
        let mut items = self.items.clone();
        normalize::ensure_call_outputs_present(&mut items);
        normalize::remove_orphan_outputs(&mut items);
        items
    }

    /// Raw items in insertion order.
    pub fn raw_items(&self) -> &[ResponseInputItem] {
        &self.items
    }

    /// Number of items in the history.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Remove the oldest item (and its corresponding call/output pair).
    pub fn remove_first(&mut self) {
        if !self.items.is_empty() {
            let removed = self.items.remove(0);
            normalize::remove_corresponding(&mut self.items, &removed);
        }
    }

    /// Remove the newest item (and its corresponding call/output pair).
    pub fn remove_last(&mut self) -> bool {
        if let Some(removed) = self.items.pop() {
            normalize::remove_corresponding(&mut self.items, &removed);
            true
        } else {
            false
        }
    }

    /// Replace the entire history.
    pub fn replace(&mut self, items: Vec<ResponseInputItem>) {
        self.items = items;
    }

    /// Drop the last `n` user turns from history.
    ///
    /// A "user turn" is a `Message` with `role == "user"`.
    /// Items before the first user message are preserved.
    pub fn drop_last_n_user_turns(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        let positions: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match item {
                ResponseInputItem::Message { role, .. } if role == "user" => Some(i),
                _ => None,
            })
            .collect();

        let Some(&first_user) = positions.first() else {
            return;
        };

        let n = n as usize;
        let cut = if n >= positions.len() {
            first_user
        } else {
            positions[positions.len() - n]
        };
        self.items.truncate(cut);
    }

    /// Update token info from an API response.
    pub fn update_token_usage(&mut self, total_tokens: i64) {
        self.last_api_total_tokens = total_tokens;
    }

    /// Estimate total token usage including pending items.
    pub fn estimate_total_tokens(&self) -> i64 {
        let pending = self
            .items_after_last_model_item()
            .iter()
            .map(estimate_item_tokens)
            .sum::<i64>();
        self.last_api_total_tokens.saturating_add(pending)
    }

    /// Detailed token usage breakdown.
    pub fn token_usage_breakdown(&self) -> TokenUsageBreakdown {
        TokenUsageBreakdown {
            last_api_total_tokens: self.last_api_total_tokens,
            all_items_bytes: self
                .items
                .iter()
                .map(estimate_item_bytes)
                .sum(),
            pending_tokens: self
                .items_after_last_model_item()
                .iter()
                .map(estimate_item_tokens)
                .sum(),
        }
    }

    /// Items added after the last model-generated item (assistant message or
    /// function call). These are not yet reflected in `last_api_total_tokens`.
    fn items_after_last_model_item(&self) -> &[ResponseInputItem] {
        let start = self
            .items
            .iter()
            .rposition(is_model_generated)
            .map_or(0, |i| i + 1);
        &self.items[start..]
    }
}

// ── helpers ──────────────────────────────────────────────────────

fn is_api_item(item: &ResponseInputItem) -> bool {
    // All current variants are API items.
    matches!(
        item,
        ResponseInputItem::Message { .. }
            | ResponseInputItem::FunctionCall { .. }
            | ResponseInputItem::FunctionCallOutput { .. }
            | ResponseInputItem::McpToolCallOutput { .. }
            | ResponseInputItem::CustomToolCallOutput { .. }
    )
}

fn is_model_generated(item: &ResponseInputItem) -> bool {
    matches!(
        item,
        ResponseInputItem::Message { role, .. } if role == "assistant"
    ) || matches!(item, ResponseInputItem::FunctionCall { .. })
}

/// Rough byte estimate for one item (used for token estimation).
fn estimate_item_bytes(item: &ResponseInputItem) -> i64 {
    let text_len = match item {
        ResponseInputItem::Message { content, role } => {
            let text_len: usize = content.iter().map(|c| match c {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => text.len(),
                ContentItem::InputImage { .. } => 7373,
            }).sum();
            text_len + role.len()
        },
        ResponseInputItem::FunctionCall {
            name, arguments, ..
        } => name.len() + arguments.len(),
        ResponseInputItem::FunctionCallOutput { output, .. } => match &output.body {
            FunctionCallOutputBody::Text(s) => s.len(),
            FunctionCallOutputBody::ContentItems(items) => items
                .iter()
                .map(|ci| match ci {
                    crate::protocol::types::FunctionCallOutputContentItem::InputText { text } => text.len(),
                    crate::protocol::types::FunctionCallOutputContentItem::InputImage { .. } => 7373,
                })
                .sum(),
        },
        ResponseInputItem::McpToolCallOutput { .. }
        | ResponseInputItem::CustomToolCallOutput { .. } => 100,
    };
    text_len as i64
}

/// Approximate token count: ~4 bytes per token.
fn estimate_item_tokens(item: &ResponseInputItem) -> i64 {
    let bytes = estimate_item_bytes(item);
    (bytes + 3) / 4 // ceiling division
}

const MAX_OUTPUT_BYTES: usize = 128 * 1024; // 128 KiB

/// Truncate oversized function outputs to keep context manageable.
fn truncate_output_if_needed(item: ResponseInputItem) -> ResponseInputItem {
    match item {
        ResponseInputItem::FunctionCallOutput { call_id, output } => {
            let truncated = match &output.body {
                FunctionCallOutputBody::Text(s) if s.len() > MAX_OUTPUT_BYTES => {
                    FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(truncate_str(s, MAX_OUTPUT_BYTES)),
                        success: None,
                    }
                }
                _ => output,
            };
            ResponseInputItem::FunctionCallOutput {
                call_id,
                output: truncated,
            }
        }
        other => other,
    }
}

fn truncate_str(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // Find a valid UTF-8 boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut result = s[..end].to_string();
    result.push_str("\n... [truncated]");
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::{ContentOrItems, FunctionCallOutputPayload, ResponseInputItem};

    fn user_msg(text: &str) -> ResponseInputItem {
        ResponseInputItem::text_message("user", text.to_string())
    }

    fn assistant_msg(text: &str) -> ResponseInputItem {
        ResponseInputItem::text_message("assistant", text.to_string())
    }

    fn func_call(call_id: &str) -> ResponseInputItem {
        ResponseInputItem::FunctionCall {
            call_id: call_id.into(),
            name: "test_tool".into(),
            arguments: "{}".into(),
        }
    }

    fn func_output(call_id: &str, text: &str) -> ResponseInputItem {
        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.into(),
            output: FunctionCallOutputPayload::from_text(text.into()),
        }
    }

    #[test]
    fn empty_context_manager() {
        let cm = ContextManager::new();
        assert!(cm.is_empty());
        assert_eq!(cm.len(), 0);
        assert!(cm.for_prompt().is_empty());
    }

    #[test]
    fn record_and_retrieve() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![user_msg("hello"), assistant_msg("hi")]);
        assert_eq!(cm.len(), 2);
        assert_eq!(cm.raw_items().len(), 2);
    }

    #[test]
    fn for_prompt_normalizes() {
        let mut cm = ContextManager::new();
        // Add a function call without output
        cm.record_items(vec![
            user_msg("run tool"),
            func_call("c1"),
        ]);
        let prompt = cm.for_prompt();
        // Should have 3 items: user msg, func call, synthetic output
        assert_eq!(prompt.len(), 3);
        assert!(matches!(&prompt[2], ResponseInputItem::FunctionCallOutput { call_id, .. } if call_id == "c1"));
    }

    #[test]
    fn for_prompt_removes_orphan_outputs() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![
            user_msg("hello"),
            func_output("orphan", "result"),
        ]);
        let prompt = cm.for_prompt();
        // Orphan output should be removed
        assert_eq!(prompt.len(), 1);
    }

    #[test]
    fn remove_first_and_last() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![user_msg("a"), user_msg("b"), user_msg("c")]);
        cm.remove_first();
        assert_eq!(cm.len(), 2);
        assert!(cm.raw_items()[0].message_text().as_deref() == Some("b"));

        cm.remove_last();
        assert_eq!(cm.len(), 1);
        assert!(cm.raw_items()[0].message_text().as_deref() == Some("b"));
    }

    #[test]
    fn drop_last_n_user_turns() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![
            user_msg("u1"),
            assistant_msg("a1"),
            user_msg("u2"),
            assistant_msg("a2"),
            user_msg("u3"),
            assistant_msg("a3"),
        ]);
        cm.drop_last_n_user_turns(2);
        // Should keep: u1, a1
        assert_eq!(cm.len(), 2);
    }

    #[test]
    fn drop_last_n_user_turns_zero_is_noop() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![user_msg("u1")]);
        cm.drop_last_n_user_turns(0);
        assert_eq!(cm.len(), 1);
    }

    #[test]
    fn drop_last_n_user_turns_exceeds_count() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![
            user_msg("u1"),
            assistant_msg("a1"),
            user_msg("u2"),
        ]);
        cm.drop_last_n_user_turns(100);
        // Should keep nothing before first user msg → 0 items
        assert_eq!(cm.len(), 0);
    }

    #[test]
    fn token_estimation() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![user_msg("hello world")]); // ~11 bytes → ~3 tokens
        let tokens = cm.estimate_total_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn token_usage_after_api_response() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![user_msg("q"), assistant_msg("a")]);
        cm.update_token_usage(100);
        // Add a new user message after the API response
        cm.record_items(vec![user_msg("follow-up")]);
        let total = cm.estimate_total_tokens();
        assert!(total > 100); // 100 from API + pending tokens
    }

    #[test]
    fn replace_history() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![user_msg("old")]);
        cm.replace(vec![user_msg("new")]);
        assert_eq!(cm.len(), 1);
        assert!(cm.raw_items()[0].message_text().as_deref() == Some("new"));
    }

    #[test]
    fn truncate_large_output() {
        let big = "x".repeat(200_000);
        let item = ResponseInputItem::FunctionCallOutput {
            call_id: "c1".into(),
            output: FunctionCallOutputPayload::from_text(big),
        };
        let mut cm = ContextManager::new();
        cm.record_items(vec![item]);
        match &cm.raw_items()[0] {
            ResponseInputItem::FunctionCallOutput { output, .. } => match &output.body {
                FunctionCallOutputBody::Text(s) => {
                    assert!(s.len() < 200_000);
                    assert!(s.ends_with("[truncated]"));
                }
                _ => panic!("expected string content"),
            },
            _ => panic!("expected function output"),
        }
    }

    #[test]
    fn breakdown_reports_bytes() {
        let mut cm = ContextManager::new();
        cm.record_items(vec![user_msg("hello")]);
        let bd = cm.token_usage_breakdown();
        assert!(bd.all_items_bytes > 0);
    }
}
