//! Reconstruct conversation history from persisted rollout items.
//!
//! Implements a two-phase algorithm (modelled after codex-main):
//! 1. **Reverse scan** — newest-to-oldest to find metadata boundaries
//!    (replacement-history checkpoint, previous turn settings, rollback counts).
//! 2. **Forward replay** — replay the surviving suffix to rebuild exact history.

use crate::core::context_manager::history::{ContextManager, ItemTruncationPolicy};
use crate::core::rollout::policy::{CompactedItem, RolloutItem, TurnContextItem};
use crate::protocol::event::EventMsg;
use crate::protocol::types::{ResponseInputItem, TokenUsageInfo};

// ── Public types ─────────────────────────────────────────────────

/// Result of reconstructing history from a rollout.
#[derive(Debug)]
pub struct RolloutReconstruction {
    /// Rebuilt conversation history items.
    pub history: Vec<ResponseInputItem>,
    /// Turn settings from the last surviving user turn.
    pub previous_turn_settings: Option<PreviousTurnSettings>,
    /// The last surviving turn context baseline (for diff-based updates).
    pub reference_context_item: Option<TurnContextItem>,
    /// Full token usage info from the rollout (for UI display on resume).
    pub last_token_info: Option<TokenUsageInfo>,
}

/// Settings captured from the last surviving turn context.
#[derive(Debug, Clone)]
pub struct PreviousTurnSettings {
    pub model: String,
    pub realtime_active: Option<bool>,
}

// ── Internal types ───────────────────────────────────────────────

#[derive(Debug, Default)]
enum TurnReferenceContextItem {
    #[default]
    NeverSet,
    Cleared,
    Latest(Box<TurnContextItem>),
}

#[derive(Debug, Default)]
struct ActiveReplaySegment<'a> {
    turn_id: Option<String>,
    counts_as_user_turn: bool,
    previous_turn_settings: Option<PreviousTurnSettings>,
    reference_context_item: TurnReferenceContextItem,
    base_replacement_history: Option<&'a [ResponseInputItem]>,
}

fn turn_ids_are_compatible(active: Option<&str>, item: Option<&str>) -> bool {
    active.is_none() || item.is_none() || active == item
}

fn finalize_active_segment<'a>(
    segment: ActiveReplaySegment<'a>,
    base_replacement_history: &mut Option<&'a [ResponseInputItem]>,
    previous_turn_settings: &mut Option<PreviousTurnSettings>,
    reference_context_item: &mut TurnReferenceContextItem,
    pending_rollback_turns: &mut usize,
) {
    if *pending_rollback_turns > 0 {
        if segment.counts_as_user_turn {
            *pending_rollback_turns -= 1;
        }
        return;
    }

    if base_replacement_history.is_none() {
        if let Some(base) = segment.base_replacement_history {
            *base_replacement_history = Some(base);
        }
    }

    if previous_turn_settings.is_none() && segment.counts_as_user_turn {
        *previous_turn_settings = segment.previous_turn_settings;
    }

    if matches!(reference_context_item, TurnReferenceContextItem::NeverSet)
        && (segment.counts_as_user_turn
            || matches!(segment.reference_context_item, TurnReferenceContextItem::Cleared))
    {
        *reference_context_item = segment.reference_context_item;
    }
}

// ── Core algorithm ───────────────────────────────────────────────

/// Reconstruct conversation history from rollout items.
///
/// This is the Mosaic equivalent of codex-main's
/// `Session::reconstruct_history_from_rollout`.
pub fn reconstruct_history_from_rollout(
    rollout_items: &[RolloutItem],
    item_truncation: ItemTruncationPolicy,
) -> RolloutReconstruction {
    // ── Phase 1: Reverse scan ────────────────────────────────────
    let mut base_replacement_history: Option<&[ResponseInputItem]> = None;
    let mut previous_turn_settings: Option<PreviousTurnSettings> = None;
    let mut reference_context_item = TurnReferenceContextItem::NeverSet;
    let mut pending_rollback_turns = 0usize;
    let mut rollout_suffix = rollout_items;
    let mut active_segment: Option<ActiveReplaySegment<'_>> = None;
    let mut last_token_info: Option<TokenUsageInfo> = None;

    for (index, item) in rollout_items.iter().enumerate().rev() {
        // Extract last token info (only need the first one found in reverse)
        if last_token_info.is_none() {
            if let RolloutItem::EventMsg(EventMsg::TokenCount(tc)) = item {
                if let Some(ref info) = tc.info {
                    last_token_info = Some(info.clone());
                }
            }
        }

        match item {
            RolloutItem::Compacted(compacted) => {
                let seg = active_segment.get_or_insert_with(ActiveReplaySegment::default);
                // Compaction clears any older baseline
                if matches!(seg.reference_context_item, TurnReferenceContextItem::NeverSet) {
                    seg.reference_context_item = TurnReferenceContextItem::Cleared;
                }
                if seg.base_replacement_history.is_none() {
                    if let Some(ref replacement) = compacted.replacement_history {
                        seg.base_replacement_history = Some(replacement.as_slice());
                        rollout_suffix = &rollout_items[index + 1..];
                    }
                }
            }
            RolloutItem::EventMsg(EventMsg::ThreadRolledBack(rollback)) => {
                pending_rollback_turns = pending_rollback_turns
                    .saturating_add(rollback.num_turns as usize);
            }
            RolloutItem::EventMsg(EventMsg::TurnComplete(event)) => {
                let seg = active_segment.get_or_insert_with(ActiveReplaySegment::default);
                if seg.turn_id.is_none() {
                    seg.turn_id = Some(event.turn_id.clone());
                }
            }
            RolloutItem::EventMsg(EventMsg::TurnAborted(event)) => {
                if let Some(seg) = active_segment.as_mut() {
                    if seg.turn_id.is_none() {
                        if let Some(tid) = &event.turn_id {
                            seg.turn_id = Some(tid.clone());
                        }
                    }
                } else if let Some(tid) = &event.turn_id {
                    active_segment = Some(ActiveReplaySegment {
                        turn_id: Some(tid.clone()),
                        ..Default::default()
                    });
                }
            }
            RolloutItem::EventMsg(EventMsg::UserMessage(_)) => {
                let seg = active_segment.get_or_insert_with(ActiveReplaySegment::default);
                seg.counts_as_user_turn = true;
            }
            RolloutItem::ResponseItem(resp) => {
                let input: ResponseInputItem = resp.clone().into();
                if matches!(&input, ResponseInputItem::Message { role, .. } if role == "user") {
                    let seg = active_segment.get_or_insert_with(ActiveReplaySegment::default);
                    seg.counts_as_user_turn = true;
                }
            }
            RolloutItem::TurnContext(ctx) => {
                let seg = active_segment.get_or_insert_with(ActiveReplaySegment::default);
                if seg.turn_id.is_none() {
                    seg.turn_id = ctx.turn_id.clone();
                }
                if turn_ids_are_compatible(
                    seg.turn_id.as_deref(),
                    ctx.turn_id.as_deref(),
                ) {
                    seg.previous_turn_settings = Some(PreviousTurnSettings {
                        model: ctx.model.clone(),
                        realtime_active: ctx.realtime_active,
                    });
                    if matches!(seg.reference_context_item, TurnReferenceContextItem::NeverSet) {
                        seg.reference_context_item =
                            TurnReferenceContextItem::Latest(Box::new(ctx.clone()));
                    }
                }
            }
            RolloutItem::EventMsg(EventMsg::TurnStarted(event)) => {
                if active_segment.as_ref().is_some_and(|seg| {
                    turn_ids_are_compatible(
                        seg.turn_id.as_deref(),
                        Some(event.turn_id.as_str()),
                    )
                }) {
                    if let Some(seg) = active_segment.take() {
                        finalize_active_segment(
                            seg,
                            &mut base_replacement_history,
                            &mut previous_turn_settings,
                            &mut reference_context_item,
                            &mut pending_rollback_turns,
                        );
                    }
                }
            }
            RolloutItem::EventMsg(_)
            | RolloutItem::SessionMeta(_) => {}
        }

        // Early exit once we have all metadata.
        if base_replacement_history.is_some()
            && previous_turn_settings.is_some()
            && !matches!(reference_context_item, TurnReferenceContextItem::NeverSet)
        {
            break;
        }
    }

    // Finalize any remaining segment.
    if let Some(seg) = active_segment.take() {
        finalize_active_segment(
            seg,
            &mut base_replacement_history,
            &mut previous_turn_settings,
            &mut reference_context_item,
            &mut pending_rollback_turns,
        );
    }

    // ── Phase 2: Forward replay ──────────────────────────────────
    let mut history = ContextManager::new();
    let mut saw_legacy_compaction_without_replacement_history = false;

    if let Some(base) = base_replacement_history {
        history.replace(base.to_vec());
    }

    for item in rollout_suffix {
        match item {
            RolloutItem::ResponseItem(response_item) => {
                let input_item: ResponseInputItem = response_item.clone().into();
                history.record_items_with_policy(std::iter::once(input_item), item_truncation);
            }
            RolloutItem::Compacted(compacted) => {
                if let Some(ref replacement) = compacted.replacement_history {
                    history.replace(replacement.clone());
                } else {
                    saw_legacy_compaction_without_replacement_history = true;
                    // Legacy compaction: rebuild with recent user messages + summary.
                    let user_messages: Vec<String> = history.raw_items().iter()
                        .filter_map(|item| match item {
                            ResponseInputItem::Message { role, content, .. } if role == "user" => {
                                content.iter().find_map(|c| match c {
                                    crate::protocol::types::ContentItem::InputText { text }
                                    | crate::protocol::types::ContentItem::OutputText { text } => Some(text.clone()),
                                    _ => None,
                                })
                            }
                            _ => None,
                        })
                        .collect();

                    let mut rebuilt = Vec::new();
                    // Keep recent user messages within ~1000 token budget
                    let mut remaining_tokens = 1000usize;
                    let mut selected: Vec<String> = Vec::new();
                    for msg in user_messages.iter().rev() {
                        let tokens = msg.len() / 4;
                        if tokens <= remaining_tokens {
                            selected.push(msg.clone());
                            remaining_tokens = remaining_tokens.saturating_sub(tokens);
                        } else {
                            break;
                        }
                    }
                    selected.reverse();
                    for msg in &selected {
                        rebuilt.push(ResponseInputItem::text_message("user", msg.clone()));
                    }
                    // Append summary as a user message (matching codex-main convention)
                    let summary = if compacted.message.is_empty() {
                        "(no summary available)".to_string()
                    } else {
                        format!("[Previous conversation summary]\n{}", compacted.message)
                    };
                    rebuilt.push(ResponseInputItem::text_message("user", summary));
                    history.replace(rebuilt);
                }
            }
            RolloutItem::EventMsg(EventMsg::ThreadRolledBack(rollback)) => {
                history.drop_last_n_user_turns(rollback.num_turns);
            }
            _ => {}
        }
    }

    // Resolve reference_context_item, clearing it if legacy compaction was seen
    // (matching codex-main: forces full context reinjection on next turn).
    let resolved_reference = match reference_context_item {
        TurnReferenceContextItem::NeverSet | TurnReferenceContextItem::Cleared => None,
        TurnReferenceContextItem::Latest(ctx) => Some(*ctx),
    };
    let resolved_reference = if saw_legacy_compaction_without_replacement_history {
        None
    } else {
        resolved_reference
    };

    RolloutReconstruction {
        history: history.raw_items().to_vec(),
        previous_turn_settings,
        reference_context_item: resolved_reference,
        last_token_info,
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::event::{
        AgentMessageEvent, ThreadRolledBackEvent, TurnCompleteEvent,
        TurnStartedEvent, UserMessageEvent,
    };
    use crate::protocol::types::{ContentItem, ModeKind, ResponseItem};

    fn user_item(text: &str) -> RolloutItem {
        RolloutItem::ResponseItem(ResponseItem::Message {
            id: None,
            role: "user".into(),
            content: vec![ContentItem::InputText { text: text.into() }],
            end_turn: None,
            phase: None,
        })
    }

    fn agent_item(text: &str) -> RolloutItem {
        RolloutItem::ResponseItem(ResponseItem::Message {
            id: None,
            role: "assistant".into(),
            content: vec![ContentItem::OutputText { text: text.into() }],
            end_turn: None,
            phase: None,
        })
    }

    fn turn_started(id: &str) -> RolloutItem {
        RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: id.to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }))
    }

    fn turn_complete(id: &str) -> RolloutItem {
        RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: id.to_string(),
            last_agent_message: None,
        }))
    }

    fn rollback(n: u32) -> RolloutItem {
        RolloutItem::EventMsg(EventMsg::ThreadRolledBack(ThreadRolledBackEvent {
            num_turns: n,
        }))
    }

    fn turn_context(turn_id: &str, model: &str) -> RolloutItem {
        RolloutItem::TurnContext(TurnContextItem {
            turn_id: Some(turn_id.to_string()),
            cwd: std::path::PathBuf::from("/tmp"),
            model: model.to_string(),
            realtime_active: None,
        })
    }

    fn compacted_legacy(msg: &str) -> RolloutItem {
        RolloutItem::Compacted(CompactedItem {
            message: msg.to_string(),
            replacement_history: None,
        })
    }

    fn compacted_with_replacement(msg: &str, items: Vec<ResponseInputItem>) -> RolloutItem {
        RolloutItem::Compacted(CompactedItem {
            message: msg.to_string(),
            replacement_history: Some(items),
        })
    }

    // ── Test: basic user + agent reconstruction ──────────────────

    #[test]
    fn basic_user_agent_reconstruction() {
        let items = vec![
            turn_started("t1"),
            user_item("hello"),
            agent_item("hi there"),
            turn_complete("t1"),
            turn_started("t2"),
            user_item("how are you?"),
            agent_item("I'm fine"),
            turn_complete("t2"),
        ];
        let result = reconstruct_history_from_rollout(&items, Default::default());
        assert_eq!(result.history.len(), 4);
        assert_eq!(result.history[0].message_text().unwrap(), "hello");
        assert_eq!(result.history[1].message_text().unwrap(), "hi there");
        assert_eq!(result.history[2].message_text().unwrap(), "how are you?");
        assert_eq!(result.history[3].message_text().unwrap(), "I'm fine");
    }

    // ── Test: rollback skips correct turns ───────────────────────

    #[test]
    fn rollback_skips_rolled_back_turns() {
        let items = vec![
            turn_started("t1"),
            user_item("u1"),
            agent_item("a1"),
            turn_complete("t1"),
            turn_started("t2"),
            user_item("u2"),
            agent_item("a2"),
            turn_complete("t2"),
            rollback(1), // undo t2
            turn_started("t3"),
            user_item("u3"),
            agent_item("a3"),
            turn_complete("t3"),
        ];
        let result = reconstruct_history_from_rollout(&items, Default::default());
        // After rollback(1): u2/a2 dropped, then u3/a3 added
        // Forward replay: u1, a1, rollback drops u2/a2, then u3, a3
        assert_eq!(result.history.len(), 4);
        assert_eq!(result.history[0].message_text().unwrap(), "u1");
        assert_eq!(result.history[1].message_text().unwrap(), "a1");
        assert_eq!(result.history[2].message_text().unwrap(), "u3");
        assert_eq!(result.history[3].message_text().unwrap(), "a3");
    }

    // ── Test: compacted with replacement_history ─────────────────

    #[test]
    fn compacted_with_replacement_replaces_history() {
        let replacement = vec![
            ResponseInputItem::text_message("user", "summary-u".to_string()),
            ResponseInputItem::text_message("assistant", "summary-a".to_string()),
        ];
        let items = vec![
            turn_started("t1"),
            user_item("old1"),
            agent_item("old-a1"),
            turn_complete("t1"),
            compacted_with_replacement("compacted", replacement),
            turn_started("t2"),
            user_item("new1"),
            agent_item("new-a1"),
            turn_complete("t2"),
        ];
        let result = reconstruct_history_from_rollout(&items, Default::default());
        // replacement_history replaces everything before it, then new items appended
        assert_eq!(result.history.len(), 4);
        assert_eq!(result.history[0].message_text().unwrap(), "summary-u");
        assert_eq!(result.history[1].message_text().unwrap(), "summary-a");
        assert_eq!(result.history[2].message_text().unwrap(), "new1");
        assert_eq!(result.history[3].message_text().unwrap(), "new-a1");
    }

    // ── Test: legacy compacted (no replacement_history) ──────────

    #[test]
    fn legacy_compacted_uses_message() {
        let items = vec![
            turn_started("t1"),
            user_item("old"),
            agent_item("old-a"),
            turn_complete("t1"),
            compacted_legacy("This is a summary"),
            turn_started("t2"),
            user_item("new"),
            agent_item("new-a"),
            turn_complete("t2"),
        ];
        let result = reconstruct_history_from_rollout(&items, Default::default());
        // Legacy compaction: keeps recent user messages + summary, then new turn items
        assert_eq!(result.history.len(), 4);
        // First: preserved user message from before compaction
        assert_eq!(result.history[0].message_text().unwrap(), "old");
        // Second: summary
        assert!(result.history[1].message_text().unwrap().contains("summary"));
        // Third+Fourth: new turn
        assert_eq!(result.history[2].message_text().unwrap(), "new");
        assert_eq!(result.history[3].message_text().unwrap(), "new-a");
    }

    // ── Test: TurnContext restores previous_turn_settings ────────

    #[test]
    fn turn_context_restores_settings() {
        let items = vec![
            turn_started("t1"),
            turn_context("t1", "gpt-4"),
            user_item("hello"),
            agent_item("hi"),
            turn_complete("t1"),
            turn_started("t2"),
            turn_context("t2", "o3-mini"),
            user_item("bye"),
            agent_item("goodbye"),
            turn_complete("t2"),
        ];
        let result = reconstruct_history_from_rollout(&items, Default::default());
        let settings = result.previous_turn_settings.unwrap();
        // Should capture the newest surviving user turn's settings (t2)
        assert_eq!(settings.model, "o3-mini");
    }

    // ── Test: empty rollout ──────────────────────────────────────

    #[test]
    fn empty_rollout_returns_empty() {
        let result = reconstruct_history_from_rollout(&[], Default::default());
        assert!(result.history.is_empty());
        assert!(result.previous_turn_settings.is_none());
    }

    // ── Test: ResponseItem variant in rollout ────────────────────

    #[test]
    fn response_item_variant_recorded() {
        let items = vec![
            RolloutItem::ResponseItem(crate::protocol::types::ResponseItem::Message {
                id: Some("m1".to_string()),
                role: "assistant".to_string(),
                content: vec![crate::protocol::types::ContentItem::OutputText {
                    text: "from response item".to_string(),
                }],
                end_turn: None,
                phase: None,
            }),
        ];
        let result = reconstruct_history_from_rollout(&items, Default::default());
        assert_eq!(result.history.len(), 1);
        assert_eq!(result.history[0].message_text().unwrap(), "from response item");
    }

    // ── Test: reference_context_item extracted ───────────────────

    #[test]
    fn reference_context_item_extracted() {
        let items = vec![
            turn_started("t1"),
            turn_context("t1", "gpt-4"),
            user_item("hello"),
            agent_item("hi"),
            turn_complete("t1"),
        ];
        let result = reconstruct_history_from_rollout(&items, Default::default());
        let ctx = result.reference_context_item.unwrap();
        assert_eq!(ctx.model, "gpt-4");
        assert_eq!(ctx.turn_id.as_deref(), Some("t1"));
    }

    // ── Test: token count extracted from rollout ─────────────────

    #[test]
    fn last_token_info_extracted() {
        use crate::protocol::event::TokenCountEvent;
        use crate::protocol::types::{TokenUsage, TokenUsageInfo};

        let tc = RolloutItem::EventMsg(EventMsg::TokenCount(TokenCountEvent {
            info: Some(TokenUsageInfo {
                total_token_usage: TokenUsage {
                    input_tokens: 100,
                    cached_input_tokens: 0,
                    output_tokens: 50,
                    reasoning_output_tokens: 0,
                    total_tokens: 150,
                },
                last_token_usage: TokenUsage {
                    input_tokens: 0,
                    cached_input_tokens: 0,
                    output_tokens: 0,
                    reasoning_output_tokens: 0,
                    total_tokens: 0,
                },
                model_context_window: None,
            }),
            rate_limits: None,
        }));

        let items = vec![
            turn_started("t1"),
            user_item("hello"),
            agent_item("hi"),
            tc,
            turn_complete("t1"),
        ];
        let result = reconstruct_history_from_rollout(&items, Default::default());
        let info = result.last_token_info.unwrap();
        assert_eq!(info.total_token_usage.total_tokens, 150);
    }

    // ── Test: legacy compaction clears reference_context_item ────

    #[test]
    fn legacy_compaction_clears_reference_context_item() {
        let items = vec![
            turn_started("t1"),
            turn_context("t1", "gpt-4"),
            user_item("old"),
            agent_item("old-a"),
            turn_complete("t1"),
            compacted_legacy("Summary of conversation"),
            turn_started("t2"),
            user_item("new"),
            agent_item("new-a"),
            turn_complete("t2"),
        ];
        let result = reconstruct_history_from_rollout(&items, Default::default());
        // Legacy compaction should clear reference_context_item
        // (forces full context reinjection on next turn).
        assert!(result.reference_context_item.is_none());
    }
}
