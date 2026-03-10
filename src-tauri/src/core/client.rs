//! Minimal OpenAI Responses API client.
//!
//! Sends a POST to `/v1/responses` with `stream: true` and yields parsed SSE events.
//! Only the event types needed to drive `run_turn()` are handled; everything else is ignored.

use std::collections::HashMap;

use eventsource_stream::Eventsource;
use futures::StreamExt;
use futures::TryStreamExt;
use serde::Deserialize;
use serde_json::Value;

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
    /// Intermediate event to ignore (response.created, response.in_progress, etc.).
    Skip,
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
///
/// `history` is the full conversation history as Responses API input items.
/// Returns a stream of `ResponseEvent`s.
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
        CodexError::new(
            ErrorCode::InternalError,
            format!("HTTP request failed: {e}"),
        )
    })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(CodexError::new(
            ErrorCode::InternalError,
            format!("API returned {status}: {body}"),
        ));
    }

    let byte_stream = resp.bytes_stream();

    // Debug: log raw bytes
    let byte_stream = byte_stream.inspect(|chunk| {
        if let Ok(bytes) = chunk {
            eprintln!("[RAW] {} bytes: {}", bytes.len(), String::from_utf8_lossy(&bytes[..bytes.len().min(300)]));
        }
    });

    let event_stream = byte_stream.eventsource();

    let parsed = event_stream.map(|result| match result {
        Err(e) => Err(CodexError::new(
            ErrorCode::InternalError,
            format!("SSE stream error: {e}"),
        )),
        Ok(event) => parse_sse_event(&event.event, &event.data),
    });

    Ok(parsed)
}

fn parse_sse_event(_event_type: &str, data: &str) -> Result<ResponseEvent, CodexError> {
    if data.is_empty() || data == "[DONE]" {
        return Ok(ResponseEvent::Done { response_id: None });
    }

    eprintln!("[SSE] event_type={:?} data={}", _event_type, &data[..data.len().min(200)]);

    let json: Value = serde_json::from_str(data).map_err(|e| {
        CodexError::new(ErrorCode::InternalError, format!("Failed to parse SSE data: {e}"))
    })?;

    // Event type is in the JSON `type` field (Azure omits the SSE `event:` line)
    let kind = json.get("type").and_then(Value::as_str).unwrap_or("");
    eprintln!("[SSE] parsed kind={:?}", kind);

    match kind {
        "response.output_text.delta" => {
            let delta = json.get("delta").and_then(Value::as_str).unwrap_or("").to_string();
            Ok(ResponseEvent::OutputTextDelta { delta })
        }
        "response.output_item.done" => {
            let item = json.get("item").cloned().unwrap_or(Value::Null);
            Ok(ResponseEvent::OutputItemDone { item })
        }
        "response.completed" | "response.done" => {
            let response_id = json
                .get("response")
                .and_then(|r| r.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            Ok(ResponseEvent::Done { response_id })
        }
        "response.failed" => {
            let error = json.get("response").and_then(|r| r.get("error"));
            let code = error.and_then(|e| e.get("code")).and_then(Value::as_str).unwrap_or("unknown").to_string();
            let message = error.and_then(|e| e.get("message")).and_then(Value::as_str).unwrap_or("unknown error").to_string();
            Ok(ResponseEvent::Failed { code, message })
        }
        _ => Ok(ResponseEvent::Skip),
    }
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
