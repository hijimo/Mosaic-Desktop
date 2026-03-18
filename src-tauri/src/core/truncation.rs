use crate::protocol::types::ResponseInputItem;

/// Policy controlling how conversation history is truncated.
#[derive(Debug, Clone, PartialEq)]
pub enum TruncationPolicy {
    /// Keep only the most recent `max_items` messages.
    KeepRecent { max_items: usize },
    /// Keep messages from the end whose cumulative token count does not exceed `max_tokens`.
    KeepRecentTokens { max_tokens: usize },
    /// Automatically compact history by calling a model to generate a summary.
    AutoCompact,
}

/// Rough token estimation: split on whitespace, count words.
/// This is intentionally simple — a production system would use a proper tokenizer.
fn estimate_tokens(item: &ResponseInputItem) -> usize {
    let text = match item {
        ResponseInputItem::Message { content, .. } => content.as_str(),
        ResponseInputItem::FunctionCall { arguments, .. } => arguments.as_str(),
        ResponseInputItem::FunctionCallOutput { output, .. } => match &output.body {
            crate::protocol::types::FunctionCallOutputBody::Text(s) => s.as_str(),
            crate::protocol::types::FunctionCallOutputBody::ContentItems(_) => return 10,
        },
        ResponseInputItem::McpToolCallOutput { .. }
        | ResponseInputItem::CustomToolCallOutput { .. } => return 10,
    };
    // ~4 chars per token is a common rough heuristic
    text.len().div_ceil(4)
}

/// Apply a truncation policy to a history slice, returning the truncated result.
///
/// For `KeepRecent`, returns the last `max_items` entries.
/// For `KeepRecentTokens`, walks backwards accumulating tokens and keeps entries
/// whose cumulative count stays within `max_tokens`.
/// For `AutoCompact`, returns the history unchanged — the caller must invoke
/// the compact functions separately.
pub fn apply_truncation(
    history: &[ResponseInputItem],
    policy: &TruncationPolicy,
) -> Vec<ResponseInputItem> {
    match policy {
        TruncationPolicy::KeepRecent { max_items } => {
            if history.len() <= *max_items {
                return history.to_vec();
            }
            history[history.len() - max_items..].to_vec()
        }
        TruncationPolicy::KeepRecentTokens { max_tokens } => {
            let mut cumulative = 0usize;
            let mut start_index = history.len();
            for (i, item) in history.iter().enumerate().rev() {
                let tokens = estimate_tokens(item);
                if cumulative + tokens > *max_tokens {
                    break;
                }
                cumulative += tokens;
                start_index = i;
            }
            history[start_index..].to_vec()
        }
        TruncationPolicy::AutoCompact => {
            // AutoCompact does not truncate in-place; the caller should use
            // compact() or compact_remote() instead.
            history.to_vec()
        }
    }
}

/// Approximate token count for a raw string (~4 chars per token).
pub fn approx_token_count(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Truncate text to fit within a token budget, appending a notice if truncated.
pub fn formatted_truncate_text(text: &str, policy: TruncationPolicy) -> String {
    let max_tokens = match policy {
        TruncationPolicy::KeepRecentTokens { max_tokens } => max_tokens,
        _ => return text.to_string(),
    };
    let max_chars = max_tokens * 4;
    if text.len() <= max_chars {
        return text.to_string();
    }
    let mut end = max_chars;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n\n[... truncated {} tokens]",
        &text[..end],
        approx_token_count(&text[end..])
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::ResponseInputItem;

    fn msg(content: &str) -> ResponseInputItem {
        ResponseInputItem::Message {
            role: "user".into(),
            content: content.into(),
        }
    }

    #[test]
    fn keep_recent_truncates_to_max() {
        let history: Vec<_> = (0..10).map(|i| msg(&format!("msg-{i}"))).collect();
        let result = apply_truncation(&history, &TruncationPolicy::KeepRecent { max_items: 3 });
        assert_eq!(result.len(), 3);
        assert!(
            matches!(&result[0], ResponseInputItem::Message { content, .. } if content == "msg-7")
        );
        assert!(
            matches!(&result[2], ResponseInputItem::Message { content, .. } if content == "msg-9")
        );
    }

    #[test]
    fn keep_recent_noop_when_under_limit() {
        let history = vec![msg("a"), msg("b")];
        let result = apply_truncation(&history, &TruncationPolicy::KeepRecent { max_items: 5 });
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn keep_recent_empty_history() {
        let result = apply_truncation(&[], &TruncationPolicy::KeepRecent { max_items: 3 });
        assert!(result.is_empty());
    }

    #[test]
    fn keep_recent_tokens_respects_limit() {
        // Each "hello world" is ~11 chars → ~3 tokens
        let history: Vec<_> = (0..10).map(|_| msg("hello world")).collect();
        let result = apply_truncation(
            &history,
            &TruncationPolicy::KeepRecentTokens { max_tokens: 6 },
        );
        // Should keep ~2 items (3 tokens each = 6 total)
        assert!(result.len() <= 3);
        assert!(!result.is_empty());
    }

    #[test]
    fn keep_recent_tokens_keeps_all_when_under_limit() {
        let history = vec![msg("hi"), msg("ok")];
        let result = apply_truncation(
            &history,
            &TruncationPolicy::KeepRecentTokens { max_tokens: 10000 },
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn keep_recent_tokens_empty_history() {
        let result = apply_truncation(&[], &TruncationPolicy::KeepRecentTokens { max_tokens: 100 });
        assert!(result.is_empty());
    }

    #[test]
    fn auto_compact_returns_unchanged() {
        let history = vec![msg("a"), msg("b"), msg("c")];
        let result = apply_truncation(&history, &TruncationPolicy::AutoCompact);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn estimate_tokens_basic() {
        // "hello world" = 11 chars → (11+3)/4 = 3 tokens
        assert_eq!(estimate_tokens(&msg("hello world")), 3);
        // empty string → 0 tokens
        assert_eq!(estimate_tokens(&msg("")), 0);
    }
}
