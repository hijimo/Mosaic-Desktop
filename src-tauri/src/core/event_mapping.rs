//! Convert `ResponseItem` into `TurnItem` for UI display.
//!
//! Ported from codex-main `codex-rs/core/src/event_mapping.rs`.

use crate::protocol::items::{
    AgentMessageContent, AgentMessageItem, ReasoningItem, TurnItem, UserMessageItem, WebSearchItem,
};
use crate::protocol::types::{ContentItem, MessagePhase, ResponseItem, UserInput, WebSearchAction};
use uuid::Uuid;

// ── Contextual user message detection ────────────────────────────

/// Markers for contextual user messages that should be hidden from UI.
const CONTEXTUAL_FRAGMENTS: &[(&str, &str)] = &[
    ("# AGENTS.md instructions for ", "</INSTRUCTIONS>"),
    ("<environment_context>", "</environment_context>"),
    ("<skill>", "</skill>"),
    ("<user_shell_command>", "</user_shell_command>"),
    ("<turn_aborted>", "</turn_aborted>"),
    ("<subagent_notification>", "</subagent_notification>"),
];

fn is_contextual_user_fragment(item: &ContentItem) -> bool {
    let ContentItem::InputText { text } = item else {
        return false;
    };
    let trimmed_start = text.trim_start();
    let trimmed_end = text.trim_end();
    CONTEXTUAL_FRAGMENTS.iter().any(|(start, end)| {
        let starts = trimmed_start
            .get(..start.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(start));
        let ends = trimmed_end
            .get(trimmed_end.len().saturating_sub(end.len())..)
            .is_some_and(|s| s.eq_ignore_ascii_case(end));
        starts && ends
    })
}

pub(crate) fn is_contextual_user_message(content: &[ContentItem]) -> bool {
    content.iter().any(|c| is_contextual_user_fragment(c))
}

// ── Image tag detection ──────────────────────────────────────────

fn is_image_open_tag(text: &str) -> bool {
    text.starts_with("<image") && text.ends_with('>')
}

fn is_image_close_tag(text: &str) -> bool {
    text == "</image>"
}

// ── User message parsing ─────────────────────────────────────────

fn parse_user_message(content: &[ContentItem]) -> Option<UserMessageItem> {
    if is_contextual_user_message(content) {
        return None;
    }

    let mut inputs: Vec<UserInput> = Vec::new();
    for (idx, item) in content.iter().enumerate() {
        match item {
            ContentItem::InputText { text } => {
                // Skip image wrapper tags adjacent to InputImage items
                if (is_image_open_tag(text))
                    && matches!(content.get(idx + 1), Some(ContentItem::InputImage { .. }))
                {
                    continue;
                }
                if idx > 0
                    && is_image_close_tag(text)
                    && matches!(content.get(idx - 1), Some(ContentItem::InputImage { .. }))
                {
                    continue;
                }
                inputs.push(UserInput::Text {
                    text: text.clone(),
                    text_elements: Vec::new(),
                });
            }
            ContentItem::InputImage { image_url } => {
                inputs.push(UserInput::Image {
                    image_url: image_url.clone(),
                });
            }
            ContentItem::OutputText { .. } => {}
        }
    }
    Some(UserMessageItem::new(&inputs))
}

// ── Agent message parsing ────────────────────────────────────────

fn parse_agent_message(
    id: Option<&String>,
    content: &[ContentItem],
    phase: Option<MessagePhase>,
) -> AgentMessageItem {
    let mut texts: Vec<AgentMessageContent> = Vec::new();
    for item in content {
        if let ContentItem::OutputText { text } = item {
            texts.push(AgentMessageContent::Text { text: text.clone() });
        }
        // Also accept InputText from assistant (some models emit this)
        if let ContentItem::InputText { text } = item {
            texts.push(AgentMessageContent::Text { text: text.clone() });
        }
    }
    AgentMessageItem {
        id: id.cloned().unwrap_or_else(|| Uuid::new_v4().to_string()),
        content: texts,
        phase,
    }
}

// ── Web search detail ────────────────────────────────────────────

fn web_search_action_detail(action: &WebSearchAction) -> String {
    match action {
        WebSearchAction::Search { query, queries } => {
            if let Some(q) = query {
                return q.clone();
            }
            if let Some(qs) = queries {
                return qs.join(", ");
            }
            String::new()
        }
        WebSearchAction::OpenPage { url } => url.clone().unwrap_or_default(),
        WebSearchAction::FindInPage { url, pattern } => match (pattern, url) {
            (Some(p), Some(u)) => format!("'{p}' in {u}"),
            (Some(p), None) => format!("'{p}'"),
            (None, Some(u)) => u.clone(),
            (None, None) => String::new(),
        },
        WebSearchAction::Other => String::new(),
    }
}

// ── Main entry point ─────────────────────────────────────────────

/// Convert a `ResponseItem` into a `TurnItem` for UI display.
///
/// Returns `None` for items that should not be shown in the UI
/// (function calls, tool outputs, system messages, contextual user messages).
pub fn parse_turn_item(item: &ResponseItem) -> Option<TurnItem> {
    match item {
        ResponseItem::Message {
            role,
            content,
            id,
            phase,
            ..
        } => match role.as_str() {
            "user" => parse_user_message(content).map(TurnItem::UserMessage),
            "assistant" => {
                let msg = parse_agent_message(id.as_ref(), content, phase.clone());
                if msg.content.is_empty() {
                    None
                } else {
                    Some(TurnItem::AgentMessage(msg))
                }
            }
            _ => None,
        },
        ResponseItem::Reasoning {
            id,
            summary,
            content,
            ..
        } => {
            use crate::protocol::types::{ReasoningContentItem, ReasoningSummaryItem};
            let summary_text = summary
                .iter()
                .map(|s| match s {
                    ReasoningSummaryItem::SummaryText { text } => text.clone(),
                })
                .collect();
            let raw_content = content
                .as_ref()
                .map(|items| {
                    items
                        .iter()
                        .map(|c| match c {
                            ReasoningContentItem::ReasoningText { text }
                            | ReasoningContentItem::Text { text } => text.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(TurnItem::Reasoning(ReasoningItem {
                id: id.clone(),
                summary_text,
                raw_content,
            }))
        }
        ResponseItem::WebSearchCall { id, action, .. } => {
            let (action_val, query) = match action {
                Some(a) => (a.clone(), web_search_action_detail(a)),
                None => (WebSearchAction::Other, String::new()),
            };
            Some(TurnItem::WebSearch(WebSearchItem {
                id: id.clone().unwrap_or_default(),
                query,
                action: Some(action_val),
            }))
        }
        // FunctionCall, FunctionCallOutput, CustomToolCall, CustomToolCallOutput,
        // LocalShellCall, GhostSnapshot, Compaction, Other — not shown in UI
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_text_message() {
        let item = ResponseItem::Message {
            id: Some("u1".into()),
            role: "user".into(),
            content: vec![ContentItem::InputText {
                text: "hello".into(),
            }],
            end_turn: None,
            phase: None,
        };
        let ti = parse_turn_item(&item).unwrap();
        match ti {
            TurnItem::UserMessage(m) => {
                assert_eq!(m.content.len(), 1);
                match &m.content[0] {
                    UserInput::Text { text, .. } => assert_eq!(text, "hello"),
                    other => panic!("expected Text, got {other:?}"),
                }
            }
            other => panic!("expected UserMessage, got {other:?}"),
        }
    }

    #[test]
    fn parses_user_message_with_image() {
        let item = ResponseItem::Message {
            id: None,
            role: "user".into(),
            content: vec![
                ContentItem::InputText {
                    text: "look at this".into(),
                },
                ContentItem::InputImage {
                    image_url: "https://img.png".into(),
                },
            ],
            end_turn: None,
            phase: None,
        };
        let ti = parse_turn_item(&item).unwrap();
        match ti {
            TurnItem::UserMessage(m) => assert_eq!(m.content.len(), 2),
            other => panic!("expected UserMessage, got {other:?}"),
        }
    }

    #[test]
    fn skips_contextual_user_messages() {
        let cases = vec![
            "<environment_context>test</environment_context>",
            "<skill>\n<name>demo</name>\nbody\n</skill>",
            "# AGENTS.md instructions for dir\n\n<INSTRUCTIONS>\nbody\n</INSTRUCTIONS>",
            "<user_shell_command>echo 42</user_shell_command>",
        ];
        for text in cases {
            let item = ResponseItem::Message {
                id: None,
                role: "user".into(),
                content: vec![ContentItem::InputText { text: text.into() }],
                end_turn: None,
                phase: None,
            };
            assert!(parse_turn_item(&item).is_none(), "should skip: {text}");
        }
    }

    #[test]
    fn skips_image_wrapper_tags() {
        let item = ResponseItem::Message {
            id: None,
            role: "user".into(),
            content: vec![
                ContentItem::InputText {
                    text: "<image>".into(),
                },
                ContentItem::InputImage {
                    image_url: "data:image/png;base64,abc".into(),
                },
                ContentItem::InputText {
                    text: "</image>".into(),
                },
                ContentItem::InputText {
                    text: "describe this".into(),
                },
            ],
            end_turn: None,
            phase: None,
        };
        let ti = parse_turn_item(&item).unwrap();
        match ti {
            TurnItem::UserMessage(m) => {
                assert_eq!(m.content.len(), 2); // image + text, no wrapper tags
            }
            other => panic!("expected UserMessage, got {other:?}"),
        }
    }

    #[test]
    fn parses_assistant_message() {
        let item = ResponseItem::Message {
            id: Some("a1".into()),
            role: "assistant".into(),
            content: vec![ContentItem::OutputText { text: "hi".into() }],
            end_turn: None,
            phase: None,
        };
        let ti = parse_turn_item(&item).unwrap();
        match ti {
            TurnItem::AgentMessage(m) => {
                assert_eq!(m.id, "a1");
                match &m.content[0] {
                    AgentMessageContent::Text { text } => assert_eq!(text, "hi"),
                }
            }
            other => panic!("expected AgentMessage, got {other:?}"),
        }
    }

    #[test]
    fn skips_system_messages() {
        let item = ResponseItem::Message {
            id: None,
            role: "system".into(),
            content: vec![ContentItem::InputText { text: "sys".into() }],
            end_turn: None,
            phase: None,
        };
        assert!(parse_turn_item(&item).is_none());
    }

    #[test]
    fn skips_function_calls() {
        let item = ResponseItem::FunctionCall {
            id: Some("fc1".into()),
            name: "shell".into(),
            arguments: "{}".into(),
            call_id: "c1".into(),
        };
        assert!(parse_turn_item(&item).is_none());
    }

    #[test]
    fn parses_web_search() {
        let item = ResponseItem::WebSearchCall {
            id: Some("ws1".into()),
            status: Some("completed".into()),
            action: Some(WebSearchAction::Search {
                query: Some("rust".into()),
                queries: None,
            }),
        };
        let ti = parse_turn_item(&item).unwrap();
        match ti {
            TurnItem::WebSearch(s) => {
                assert_eq!(s.query, "rust");
            }
            other => panic!("expected WebSearch, got {other:?}"),
        }
    }

    #[test]
    fn empty_assistant_message_returns_none() {
        let item = ResponseItem::Message {
            id: Some("a2".into()),
            role: "assistant".into(),
            content: vec![],
            end_turn: None,
            phase: None,
        };
        assert!(parse_turn_item(&item).is_none());
    }
}
