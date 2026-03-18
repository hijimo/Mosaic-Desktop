use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{ContextCompactedEvent, Event, EventMsg};
use crate::protocol::types::ResponseInputItem;

use super::truncation::{apply_truncation, TruncationPolicy};

/// Prompt template used to ask the model to summarize conversation history.
pub const SUMMARIZATION_PROMPT: &str = "\
You are a conversation summarizer. Given the following conversation history, \
produce a concise summary that preserves all important context, decisions, \
and action items. The summary should be short enough to fit in a single message \
but detailed enough that the conversation can continue without losing context.\n\n\
Conversation:\n{history}\n\nSummary:";

/// Result of a compaction operation.
pub struct CompactResult {
    pub history: Vec<ResponseInputItem>,
    pub changed: bool,
}

/// Compact conversation history using the given truncation policy.
pub async fn compact<F, Fut>(
    history: &[ResponseInputItem],
    policy: &TruncationPolicy,
    summarize_fn: Option<F>,
) -> Result<CompactResult, CodexError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<String, CodexError>>,
{
    match policy {
        TruncationPolicy::KeepRecent { max_items } => {
            if history.len() <= *max_items {
                return Ok(CompactResult {
                    history: history.to_vec(),
                    changed: false,
                });
            }
            Ok(CompactResult {
                history: apply_truncation(history, policy),
                changed: true,
            })
        }
        TruncationPolicy::KeepRecentTokens { .. } => {
            let truncated = apply_truncation(history, policy);
            let changed = truncated.len() < history.len();
            Ok(CompactResult {
                history: truncated,
                changed,
            })
        }

        TruncationPolicy::AutoCompact => {
            let summarize = summarize_fn.ok_or_else(|| {
                CodexError::new(
                    ErrorCode::InternalError,
                    "AutoCompact requires a summarize function",
                )
            })?;
            if history.len() <= 3 {
                return Ok(CompactResult {
                    history: history.to_vec(),
                    changed: false,
                });
            }
            let split_point = history.len() - 2;
            let older = &history[..split_point];
            let recent = &history[split_point..];
            let history_text = format_history_for_summary(older);
            let prompt = SUMMARIZATION_PROMPT.replace("{history}", &history_text);
            let summary = summarize(prompt).await?;
            let mut compacted = vec![ResponseInputItem::Message {
                role: "system".into(),
                content: format!("[Conversation Summary]\n{summary}"),
            }];
            compacted.extend_from_slice(recent);
            Ok(CompactResult {
                history: compacted,
                changed: true,
            })
        }
    }
}

/// Compact conversation history by calling a remote API endpoint.
pub async fn compact_remote(
    history: &[ResponseInputItem],
    endpoint: &str,
    model: &str,
) -> Result<CompactResult, CodexError> {
    if history.len() <= 3 {
        return Ok(CompactResult {
            history: history.to_vec(),
            changed: false,
        });
    }
    let split_point = history.len() - 2;
    let older = &history[..split_point];
    let recent = &history[split_point..];
    let history_text = format_history_for_summary(older);
    let prompt = SUMMARIZATION_PROMPT.replace("{history}", &history_text);
    let request_body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": "You are a conversation summarizer." },
            { "role": "user", "content": prompt }
        ],
        "max_tokens": 1024,
    });
    let client = reqwest::Client::new();
    let response = client
        .post(endpoint)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("compact_remote request failed: {e}"),
            )
        })?;
    if !response.status().is_success() {
        return Err(CodexError::new(
            ErrorCode::InternalError,
            format!("compact_remote returned status {}", response.status()),
        ));
    }
    let body: serde_json::Value = response.json().await.map_err(|e| {
        CodexError::new(
            ErrorCode::InternalError,
            format!("compact_remote response parse failed: {e}"),
        )
    })?;
    let summary = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("(summary unavailable)")
        .to_string();
    let mut compacted = vec![ResponseInputItem::Message {
        role: "system".into(),
        content: format!("[Conversation Summary]\n{summary}"),
    }];
    compacted.extend_from_slice(recent);
    Ok(CompactResult {
        history: compacted,
        changed: true,
    })
}

/// Emit a `ContextCompacted` event if the history was actually shortened.
pub async fn emit_compacted_if_changed(
    tx_event: &async_channel::Sender<Event>,
    result: &CompactResult,
) {
    if result.changed {
        let _ = tx_event
            .send(Event {
                id: uuid::Uuid::new_v4().to_string(),
                msg: EventMsg::ContextCompacted(ContextCompactedEvent),
            })
            .await;
    }
}

fn format_history_for_summary(items: &[ResponseInputItem]) -> String {
    items
        .iter()
        .map(|item| match item {
            ResponseInputItem::Message { role, content } => format!("[{role}]: {content}"),
            ResponseInputItem::FunctionCall {
                name, arguments, ..
            } => {
                format!("[function_call:{name}]: {arguments}")
            }
            ResponseInputItem::FunctionCallOutput { output, .. } => {
                let text = match &output.body {
                    crate::protocol::types::FunctionCallOutputBody::Text(s) => s.clone(),
                    crate::protocol::types::FunctionCallOutputBody::ContentItems(items) => {
                        format!("({} items)", items.len())
                    }
                };
                format!("[function_output]: {text}")
            }
            ResponseInputItem::McpToolCallOutput { call_id, .. } => {
                format!("[mcp_tool_output:{call_id}]")
            }
            ResponseInputItem::CustomToolCallOutput { call_id, .. } => {
                format!("[custom_tool_output:{call_id}]")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ResponseInputItem {
        ResponseInputItem::Message {
            role: role.into(),
            content: content.into(),
        }
    }

    #[tokio::test]
    async fn compact_keep_recent_under_limit_is_noop() {
        let history = vec![msg("user", "a"), msg("assistant", "b")];
        let result = compact::<fn(String) -> std::future::Ready<Result<String, CodexError>>, _>(
            &history,
            &TruncationPolicy::KeepRecent { max_items: 5 },
            None,
        )
        .await
        .unwrap();
        assert!(!result.changed);
        assert_eq!(result.history.len(), 2);
    }

    #[tokio::test]
    async fn compact_keep_recent_truncates() {
        let history: Vec<_> = (0..10).map(|i| msg("user", &format!("msg-{i}"))).collect();
        let result = compact::<fn(String) -> std::future::Ready<Result<String, CodexError>>, _>(
            &history,
            &TruncationPolicy::KeepRecent { max_items: 3 },
            None,
        )
        .await
        .unwrap();
        assert!(result.changed);
        assert_eq!(result.history.len(), 3);
    }

    #[tokio::test]
    async fn compact_keep_recent_idempotent() {
        let history = vec![msg("user", "a"), msg("assistant", "b")];
        let first = compact::<fn(String) -> std::future::Ready<Result<String, CodexError>>, _>(
            &history,
            &TruncationPolicy::KeepRecent { max_items: 5 },
            None,
        )
        .await
        .unwrap();
        let second = compact::<fn(String) -> std::future::Ready<Result<String, CodexError>>, _>(
            &first.history,
            &TruncationPolicy::KeepRecent { max_items: 5 },
            None,
        )
        .await
        .unwrap();
        assert!(!second.changed);
    }

    #[tokio::test]
    async fn compact_auto_without_fn_errors() {
        let history = vec![
            msg("user", "a"),
            msg("a", "b"),
            msg("u", "c"),
            msg("a", "d"),
        ];
        let result = compact::<fn(String) -> std::future::Ready<Result<String, CodexError>>, _>(
            &history,
            &TruncationPolicy::AutoCompact,
            None,
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn compact_auto_with_summarizer() {
        let history = vec![
            msg("user", "hello"),
            msg("assistant", "hi"),
            msg("user", "how are you"),
            msg("assistant", "good"),
            msg("user", "latest"),
            msg("assistant", "answer"),
        ];
        let summarize = |_prompt: String| async { Ok("This is a summary.".to_string()) };
        let result = compact(&history, &TruncationPolicy::AutoCompact, Some(summarize))
            .await
            .unwrap();
        assert!(result.changed);
        assert_eq!(result.history.len(), 3);
        assert!(
            matches!(&result.history[0], ResponseInputItem::Message { role, content }
            if role == "system" && content.contains("summary"))
        );
    }

    #[tokio::test]
    async fn compact_auto_short_history_is_noop() {
        let history = vec![msg("user", "a"), msg("assistant", "b")];
        let summarize = |_prompt: String| async { Ok("summary".to_string()) };
        let result = compact(&history, &TruncationPolicy::AutoCompact, Some(summarize))
            .await
            .unwrap();
        assert!(!result.changed);
        assert_eq!(result.history.len(), 2);
    }

    #[tokio::test]
    async fn emit_compacted_sends_event_when_changed() {
        let (tx, rx) = async_channel::unbounded();
        let result = CompactResult {
            history: vec![],
            changed: true,
        };
        emit_compacted_if_changed(&tx, &result).await;
        let event = rx.try_recv().unwrap();
        assert!(matches!(event.msg, EventMsg::ContextCompacted(_)));
    }

    #[tokio::test]
    async fn emit_compacted_silent_when_unchanged() {
        let (tx, rx) = async_channel::unbounded();
        let result = CompactResult {
            history: vec![],
            changed: false,
        };
        emit_compacted_if_changed(&tx, &result).await;
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn format_history_formats_messages() {
        let items = vec![msg("user", "hello"), msg("assistant", "hi")];
        let text = format_history_for_summary(&items);
        assert!(text.contains("[user]: hello"));
        assert!(text.contains("[assistant]: hi"));
    }
}
