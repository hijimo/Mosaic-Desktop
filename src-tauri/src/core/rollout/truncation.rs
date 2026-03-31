//! Rollout truncation helpers based on user-turn boundaries.

use super::policy::RolloutItem;
use crate::protocol::event::EventMsg;
use crate::protocol::types::ResponseInputItem;

/// Return indices of user message boundaries in a rollout.
///
/// Detects user messages from both `RolloutItem::ResponseItem` (new format)
/// and `EventMsg::UserMessage` (legacy format).
/// `ThreadRolledBack` markers are applied so indexing uses post-rollback history.
pub fn user_message_positions(items: &[RolloutItem]) -> Vec<usize> {
    let mut positions = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        match item {
            RolloutItem::ResponseItem(resp) => {
                if matches!(
                    resp.to_input_item().as_ref(),
                    Some(ResponseInputItem::Message { role, .. }) if role == "user"
                ) {
                    positions.push(idx);
                }
            }
            RolloutItem::EventMsg(EventMsg::UserMessage(_)) => {
                positions.push(idx);
            }
            RolloutItem::EventMsg(EventMsg::ThreadRolledBack(rollback)) => {
                let num_turns = rollback.num_turns as usize;
                let new_len = positions.len().saturating_sub(num_turns);
                positions.truncate(new_len);
            }
            _ => {}
        }
    }
    positions
}

/// Return a prefix of `items` by cutting before the nth user message (0-based).
///
/// - `n == usize::MAX` → returns the full rollout (no truncation).
/// - If fewer than `n` user messages exist → returns empty vec.
pub fn truncate_before_nth_user_message(
    items: &[RolloutItem],
    n_from_start: usize,
) -> Vec<RolloutItem> {
    if n_from_start == usize::MAX {
        return items.to_vec();
    }
    let positions = user_message_positions(items);
    if positions.len() <= n_from_start {
        return Vec::new();
    }
    let cut_idx = positions[n_from_start];
    items[..cut_idx].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::event::{AgentMessageEvent, ThreadRolledBackEvent, UserMessageEvent};

    fn user_item(text: &str) -> RolloutItem {
        RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: text.to_string(),
            images: None,
            local_images: Vec::new(),
            text_elements: Vec::new(),
        }))
    }

    fn agent_item(text: &str) -> RolloutItem {
        RolloutItem::EventMsg(EventMsg::AgentMessage(AgentMessageEvent {
            message: text.to_string(),
            phase: None,
        }))
    }

    #[test]
    fn truncate_before_second_user_message() {
        let items = vec![
            user_item("u1"),
            agent_item("a1"),
            user_item("u2"),
            agent_item("a2"),
        ];
        let truncated = truncate_before_nth_user_message(&items, 1);
        assert_eq!(truncated.len(), 2); // u1, a1
    }

    #[test]
    fn truncate_max_keeps_full() {
        let items = vec![user_item("u1"), agent_item("a1")];
        let truncated = truncate_before_nth_user_message(&items, usize::MAX);
        assert_eq!(truncated.len(), 2);
    }

    #[test]
    fn truncate_applies_rollback() {
        let items = vec![
            user_item("u1"),
            agent_item("a1"),
            user_item("u2"),
            agent_item("a2"),
            RolloutItem::EventMsg(EventMsg::ThreadRolledBack(ThreadRolledBackEvent {
                num_turns: 1,
            })),
            user_item("u3"),
            agent_item("a3"),
            user_item("u4"),
        ];
        // After rollback(1): effective users are u1, u3, u4
        // n=2 → cut before u4 (index 7)
        let truncated = truncate_before_nth_user_message(&items, 2);
        assert_eq!(truncated.len(), 7);
    }

    #[test]
    fn web_search_response_item_is_not_counted_as_user_boundary() {
        let items = vec![
            RolloutItem::ResponseItem(crate::protocol::types::ResponseItem::WebSearchCall {
                id: Some("ws1".to_string()),
                status: Some("completed".to_string()),
                action: Some(crate::protocol::types::WebSearchAction::Search {
                    query: Some("mosaic".to_string()),
                    queries: None,
                }),
            }),
            user_item("u1"),
        ];

        let positions = user_message_positions(&items);
        assert_eq!(positions, vec![1]);
    }
}
