//! Minimal OpenAI Responses API client.
//!
//! Sends a POST to `/responses` with `stream: true` and yields parsed SSE events.
//! Only the event types needed to drive `run_turn()` are handled; everything else is ignored.

use std::collections::HashMap;

use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::protocol::error::CodexError;
use crate::protocol::error::ErrorCode;

/// A single parsed SSE event from the Responses API stream.
#[derive(Debug)]
pub enum ResponseEvent {
    /// Streaming text delta from the assistant.
    OutputTextDelta { delta: String },
    /// A complete output item (message, function_call, etc.).
    OutputItemDone { item: Value },
    /// The response is complete.
    Done { response_id: Option<String> },
    /// An error from the API.
    Failed { code: String, message: String },
}

/// Minimal request body for the Responses API.
#[derive(serde::Serialize)]
struct CreateResponseRequest<'a> {
    model: &'a str,
    input: Vec<Value>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_response_id: Option<&'a str>,
}

/// Stream events from the Responses API for a single turn.
pub async fn stream_response(
    url: &str,
    api_key: &str,
    extra_headers: &HashMap<String, String>,
    model: &str,
    instructions: Option<&str>,
    history: Vec<Value>,
    previous_response_id: Option<&str>,
) -> Result<impl futures::Stream<Item = Result<ResponseEvent, CodexError>>, CodexError> {
    let body = CreateResponseRequest {
        model,
        input: history,
        stream: true,
        instructions,
        previous_response_id,
    };

    let mut req = reqwest::Client::new()
        .post(url)
        .bearer_auth(api_key)
        .json(&body);

    for (k, v) in extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let resp = req.send().await.map_err(|e| {
        CodexError::new(ErrorCode::InternalError, format!("HTTP request failed: {e}"))
    })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(CodexError::new(
            ErrorCode::InternalError,
            format!("API returned {status}: {body}"),
        ));
    }

    // Use channel + spawn (like the reference impl) so SSE parse errors don't
    // silently terminate the stream — we get an explicit error event instead.
    let (tx, rx) = mpsc::channel::<Result<ResponseEvent, CodexError>>(256);

    tokio::spawn(async move {
        let mut stream = resp.bytes_stream().eventsource();

        loop {
            match stream.next().await {
                None => {
                    // Stream closed before response.completed — send error so
                    // the consumer knows the turn ended unexpectedly.
                    let _ = tx
                        .send(Err(CodexError::new(
                            ErrorCode::InternalError,
                            "stream closed before response.completed".to_string(),
                        )))
                        .await;
                    return;
                }
                Some(Err(e)) => {
                    let _ = tx
                        .send(Err(CodexError::new(
                            ErrorCode::InternalError,
                            format!("SSE stream error: {e}"),
                        )))
                        .await;
                    return;
                }
                Some(Ok(event)) => {
                    eprintln!(
                        "[SSE] event={:?} data={}",
                        event.event,
                        &event.data[..event.data.len().min(300)]
                    );

                    if event.data.is_empty() || event.data == "[DONE]" {
                        continue;
                    }

                    let json: Value = match serde_json::from_str(&event.data) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("[SSE] parse error: {e}");
                            continue;
                        }
                    };

                    let kind = json.get("type").and_then(Value::as_str).unwrap_or("");
                    eprintln!("[SSE] kind={kind:?}");

                    let parsed = match kind {
                        "response.output_text.delta" => {
                            let delta = json
                                .get("delta")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            Some(Ok(ResponseEvent::OutputTextDelta { delta }))
                        }
                        "response.output_item.done" => {
                            let item = json.get("item").cloned().unwrap_or(Value::Null);
                            Some(Ok(ResponseEvent::OutputItemDone { item }))
                        }
                        "response.completed" | "response.done" => {
                            let response_id = json
                                .get("response")
                                .and_then(|r| r.get("id"))
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            let ev = ResponseEvent::Done { response_id };
                            let _ = tx.send(Ok(ev)).await;
                            return; // stream complete, exit task
                        }
                        "response.failed" => {
                            let error = json.get("response").and_then(|r| r.get("error"));
                            let code = error
                                .and_then(|e| e.get("code"))
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .to_string();
                            let message = error
                                .and_then(|e| e.get("message"))
                                .and_then(Value::as_str)
                                .unwrap_or("unknown error")
                                .to_string();
                            Some(Ok(ResponseEvent::Failed { code, message }))
                        }
                        _ => None, // ignore intermediate events
                    };

                    if let Some(ev) = parsed {
                        if tx.send(ev).await.is_err() {
                            return; // receiver dropped
                        }
                    }
                }
            }
        }
    });

    Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
}

/// Convert a `ResponseInputItem` from the session history into a Responses API input item.
pub fn history_item_to_api(item: &crate::protocol::types::ResponseInputItem) -> Value {
    match item {
        crate::protocol::types::ResponseInputItem::Message { role, content } => {
            serde_json::json!({
                "type": "message",
                "role": role,
                "content": [{"type": "input_text", "text": content}]
            })
        }
        crate::protocol::types::ResponseInputItem::FunctionCall {
            call_id,
            name,
            arguments,
        } => {
            serde_json::json!({
                "type": "function_call",
                "call_id": call_id,
                "name": name,
                "arguments": arguments
            })
        }
        crate::protocol::types::ResponseInputItem::FunctionOutput { call_id, output } => {
            let content = match &output.content {
                crate::protocol::types::ContentOrItems::String(s) => s.clone(),
                crate::protocol::types::ContentOrItems::Items(items) => items
                    .iter()
                    .filter_map(|i| match i {
                        crate::protocol::types::ContentItem::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            serde_json::json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": content
            })
        }
    }
}

// ── Non-streaming structured output call ─────────────────────────

/// Request body for a non-streaming Responses API call with structured output.
#[derive(serde::Serialize)]
struct StructuredRequest<'a> {
    model: &'a str,
    input: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<&'a str>,
    stream: bool,
    text: TextFormat<'a>,
}

#[derive(serde::Serialize)]
struct TextFormat<'a> {
    format: FormatSpec<'a>,
}

#[derive(serde::Serialize)]
struct FormatSpec<'a> {
    r#type: &'a str,
    name: &'a str,
    schema: &'a Value,
    strict: bool,
}

/// Make a non-streaming Responses API call with JSON structured output.
///
/// Returns the parsed text content from the first output message.
pub async fn complete_structured(
    url: &str,
    api_key: &str,
    extra_headers: &HashMap<String, String>,
    model: &str,
    instructions: Option<&str>,
    input: Vec<Value>,
    output_schema: &Value,
) -> Result<String, CodexError> {
    let body = StructuredRequest {
        model,
        input,
        instructions,
        stream: false,
        text: TextFormat {
            format: FormatSpec {
                r#type: "json_schema",
                name: "memory_extraction",
                schema: output_schema,
                strict: true,
            },
        },
    };

    let mut req = reqwest::Client::new()
        .post(url)
        .bearer_auth(api_key)
        .json(&body);

    for (k, v) in extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let resp = req.send().await.map_err(|e| {
        CodexError::new(ErrorCode::InternalError, format!("HTTP request failed: {e}"))
    })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(CodexError::new(
            ErrorCode::InternalError,
            format!("API returned {status}: {body}"),
        ));
    }

    let json: Value = resp.json().await.map_err(|e| {
        CodexError::new(ErrorCode::InternalError, format!("JSON parse error: {e}"))
    })?;

    // Extract text from output[0].content[0].text
    json.get("output")
        .and_then(|o| o.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .map(String::from)
        .ok_or_else(|| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("unexpected response structure: {json}"),
            )
        })
}
