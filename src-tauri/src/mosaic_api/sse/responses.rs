use crate::mosaic_api::common::{ResponseEvent, ResponseStream};
use crate::mosaic_api::error::ApiError;
use crate::mosaic_api::rate_limits::parse_all_rate_limits;
use crate::mosaic_api::telemetry::SseTelemetry;
use crate::mosaic_client::{ByteStream, StreamResponse, TransportError};
use crate::protocol::types::{ResponseItem, TokenUsage};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{timeout, Instant};
use tracing::{debug, trace};

const OPENAI_MODEL_HEADER: &str = "openai-model";

pub fn spawn_response_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
) -> ResponseStream {
    let rate_limit_snapshots = parse_all_rate_limits(&stream_response.headers);
    let models_etag = stream_response
        .headers
        .get("X-Models-Etag")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let server_model = stream_response
        .headers
        .get(OPENAI_MODEL_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let reasoning_included = stream_response
        .headers
        .get("x-reasoning-included")
        .is_some();

    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(async move {
        if let Some(model) = server_model {
            let _ = tx_event.send(Ok(ResponseEvent::ServerModel(model))).await;
        }
        for snapshot in rate_limit_snapshots {
            let _ = tx_event.send(Ok(ResponseEvent::RateLimits(snapshot))).await;
        }
        if let Some(etag) = models_etag {
            let _ = tx_event.send(Ok(ResponseEvent::ModelsEtag(etag))).await;
        }
        if reasoning_included {
            let _ = tx_event
                .send(Ok(ResponseEvent::ServerReasoningIncluded(true)))
                .await;
        }
        process_sse(stream_response.bytes, tx_event, idle_timeout, telemetry).await;
    });

    ResponseStream { rx_event }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Error {
    r#type: Option<String>,
    code: Option<String>,
    message: Option<String>,
    plan_type: Option<String>,
    resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ResponseCompleted {
    id: String,
    #[serde(default)]
    usage: Option<ResponseCompletedUsage>,
}

#[derive(Debug, Deserialize)]
struct ResponseDone {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    usage: Option<ResponseCompletedUsage>,
}

#[derive(Debug, Deserialize)]
struct ResponseCompletedUsage {
    input_tokens: i64,
    input_tokens_details: Option<ResponseCompletedInputTokensDetails>,
    output_tokens: i64,
    output_tokens_details: Option<ResponseCompletedOutputTokensDetails>,
    total_tokens: i64,
}

impl From<ResponseCompletedUsage> for TokenUsage {
    fn from(val: ResponseCompletedUsage) -> Self {
        TokenUsage {
            input_tokens: val.input_tokens,
            cached_input_tokens: val
                .input_tokens_details
                .map(|d| d.cached_tokens)
                .unwrap_or(0),
            output_tokens: val.output_tokens,
            reasoning_output_tokens: val
                .output_tokens_details
                .map(|d| d.reasoning_tokens)
                .unwrap_or(0),
            total_tokens: val.total_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ResponseCompletedInputTokensDetails {
    cached_tokens: i64,
}

#[derive(Debug, Deserialize)]
struct ResponseCompletedOutputTokensDetails {
    reasoning_tokens: i64,
}

#[derive(Deserialize, Debug)]
struct ResponsesStreamEvent {
    #[serde(rename = "type")]
    kind: String,
    headers: Option<Value>,
    response: Option<Value>,
    item: Option<Value>,
    delta: Option<String>,
    summary_index: Option<i64>,
    content_index: Option<i64>,
}

impl ResponsesStreamEvent {
    fn response_model(&self) -> Option<String> {
        let response_headers_model = self
            .response
            .as_ref()
            .and_then(|response| response.get("headers"))
            .and_then(header_openai_model_value_from_json);

        match response_headers_model {
            Some(model) => Some(model),
            None => self
                .headers
                .as_ref()
                .and_then(header_openai_model_value_from_json),
        }
    }
}

fn header_openai_model_value_from_json(value: &Value) -> Option<String> {
    let headers = value.as_object()?;
    headers.iter().find_map(|(name, value)| {
        if name.eq_ignore_ascii_case("openai-model") || name.eq_ignore_ascii_case("x-openai-model")
        {
            json_value_as_string(value)
        } else {
            None
        }
    })
}

fn json_value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Array(items) => items.first().and_then(json_value_as_string),
        _ => None,
    }
}

#[derive(Debug)]
enum ResponsesEventError {
    Api(ApiError),
}

impl ResponsesEventError {
    fn into_api_error(self) -> ApiError {
        match self {
            Self::Api(error) => error,
        }
    }
}

fn process_responses_event(
    event: ResponsesStreamEvent,
) -> Result<Option<ResponseEvent>, ResponsesEventError> {
    match event.kind.as_str() {
        "response.output_item.done" => {
            if let Some(item_val) = event.item {
                if let Ok(item) = serde_json::from_value::<ResponseItem>(item_val) {
                    return Ok(Some(ResponseEvent::OutputItemDone(item)));
                }
                debug!("failed to parse ResponseItem from output_item.done");
            }
        }
        "response.output_text.delta" => {
            if let Some(delta) = event.delta {
                return Ok(Some(ResponseEvent::OutputTextDelta(delta)));
            }
        }
        "response.reasoning_summary_text.delta" => {
            if let (Some(delta), Some(summary_index)) = (event.delta, event.summary_index) {
                return Ok(Some(ResponseEvent::ReasoningSummaryDelta {
                    delta,
                    summary_index,
                }));
            }
        }
        "response.reasoning_text.delta" => {
            if let (Some(delta), Some(content_index)) = (event.delta, event.content_index) {
                return Ok(Some(ResponseEvent::ReasoningContentDelta {
                    delta,
                    content_index,
                }));
            }
        }
        "response.created" => {
            if event.response.is_some() {
                return Ok(Some(ResponseEvent::Created));
            }
        }
        "response.failed" => {
            if let Some(resp_val) = event.response {
                let mut response_error =
                    ApiError::Stream("response.failed event received".into());
                if let Some(error) = resp_val.get("error") {
                    if let Ok(error) = serde_json::from_value::<Error>(error.clone()) {
                        if is_context_window_error(&error) {
                            response_error = ApiError::ContextWindowExceeded;
                        } else if is_quota_exceeded_error(&error) {
                            response_error = ApiError::QuotaExceeded;
                        } else if is_usage_not_included(&error) {
                            response_error = ApiError::UsageNotIncluded;
                        } else if is_invalid_prompt_error(&error) {
                            let message = error
                                .message
                                .unwrap_or_else(|| "Invalid request.".to_string());
                            response_error = ApiError::InvalidRequest { message };
                        } else if is_server_overloaded_error(&error) {
                            response_error = ApiError::ServerOverloaded;
                        } else {
                            let delay = try_parse_retry_after(&error);
                            let message = error.message.unwrap_or_default();
                            response_error = ApiError::Retryable { message, delay };
                        }
                    }
                }
                return Err(ResponsesEventError::Api(response_error));
            }
            return Err(ResponsesEventError::Api(ApiError::Stream(
                "response.failed event received".into(),
            )));
        }
        "response.incomplete" => {
            let reason = event.response.as_ref().and_then(|response| {
                response
                    .get("incomplete_details")
                    .and_then(|details| details.get("reason"))
                    .and_then(Value::as_str)
            });
            let reason = reason.unwrap_or("unknown");
            let message = format!("Incomplete response returned, reason: {reason}");
            return Err(ResponsesEventError::Api(ApiError::Stream(message)));
        }
        "response.completed" => {
            if let Some(resp_val) = event.response {
                match serde_json::from_value::<ResponseCompleted>(resp_val) {
                    Ok(resp) => {
                        return Ok(Some(ResponseEvent::Completed {
                            response_id: resp.id,
                            token_usage: resp.usage.map(Into::into),
                            can_append: false,
                        }));
                    }
                    Err(err) => {
                        let error = format!("failed to parse ResponseCompleted: {err}");
                        debug!("{error}");
                        return Err(ResponsesEventError::Api(ApiError::Stream(error)));
                    }
                }
            }
        }
        "response.done" => {
            if let Some(resp_val) = event.response {
                match serde_json::from_value::<ResponseDone>(resp_val) {
                    Ok(resp) => {
                        return Ok(Some(ResponseEvent::Completed {
                            response_id: resp.id.unwrap_or_default(),
                            token_usage: resp.usage.map(Into::into),
                            can_append: true,
                        }));
                    }
                    Err(err) => {
                        let error = format!("failed to parse ResponseCompleted: {err}");
                        debug!("{error}");
                        return Err(ResponsesEventError::Api(ApiError::Stream(error)));
                    }
                }
            }
            debug!("response.done missing response payload");
            return Ok(Some(ResponseEvent::Completed {
                response_id: String::new(),
                token_usage: None,
                can_append: true,
            }));
        }
        "response.output_item.added" => {
            if let Some(item_val) = event.item {
                if let Ok(item) = serde_json::from_value::<ResponseItem>(item_val) {
                    return Ok(Some(ResponseEvent::OutputItemAdded(item)));
                }
                debug!("failed to parse ResponseItem from output_item.added");
            }
        }
        "response.reasoning_summary_part.added" => {
            if let Some(summary_index) = event.summary_index {
                return Ok(Some(ResponseEvent::ReasoningSummaryPartAdded {
                    summary_index,
                }));
            }
        }
        _ => {
            trace!("unhandled responses event: {}", event.kind);
        }
    }
    Ok(None)
}

pub async fn process_sse(
    stream: ByteStream,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
) {
    let mut stream = stream.eventsource();
    let mut response_error: Option<ApiError> = None;
    let mut last_server_model: Option<String> = None;

    loop {
        let start = Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        if let Some(t) = telemetry.as_ref() {
            t.on_sse_poll(&response, start.elapsed());
        }
        let sse = match response {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(e))) => {
                debug!("SSE Error: {e:#}");
                let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                return;
            }
            Ok(None) => {
                let error = response_error.unwrap_or(ApiError::Stream(
                    "stream closed before response.completed".into(),
                ));
                let _ = tx_event.send(Err(error)).await;
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream("idle timeout waiting for SSE".into())))
                    .await;
                return;
            }
        };

        trace!("SSE event: {}", &sse.data);

        let event: ResponsesStreamEvent = match serde_json::from_str(&sse.data) {
            Ok(event) => event,
            Err(e) => {
                debug!("Failed to parse SSE event: {e}, data: {}", &sse.data);
                continue;
            }
        };

        if let Some(model) = event.response_model() {
            if last_server_model.as_deref() != Some(model.as_str()) {
                if tx_event
                    .send(Ok(ResponseEvent::ServerModel(model.clone())))
                    .await
                    .is_err()
                {
                    return;
                }
                last_server_model = Some(model);
            }
        }

        match process_responses_event(event) {
            Ok(Some(event)) => {
                let is_completed = matches!(event, ResponseEvent::Completed { .. });
                if tx_event.send(Ok(event)).await.is_err() {
                    return;
                }
                if is_completed {
                    return;
                }
            }
            Ok(None) => {}
            Err(error) => {
                response_error = Some(error.into_api_error());
            }
        }
    }
}

fn try_parse_retry_after(err: &Error) -> Option<Duration> {
    if err.code.as_deref() != Some("rate_limit_exceeded") {
        return None;
    }

    let message = err.message.as_ref()?;
    let re = rate_limit_regex();
    let captures = re.captures(message)?;
    let value = captures.get(1)?;
    let unit = captures.get(2)?;
    let value = value.as_str().parse::<f64>().ok()?;
    let unit = unit.as_str().to_ascii_lowercase();

    if unit == "s" || unit.starts_with("second") {
        Some(Duration::from_secs_f64(value))
    } else if unit == "ms" {
        Some(Duration::from_millis(value as u64))
    } else {
        None
    }
}

fn is_context_window_error(error: &Error) -> bool {
    error.code.as_deref() == Some("context_length_exceeded")
}

fn is_quota_exceeded_error(error: &Error) -> bool {
    error.code.as_deref() == Some("insufficient_quota")
}

fn is_usage_not_included(error: &Error) -> bool {
    error.code.as_deref() == Some("usage_not_included")
}

fn is_invalid_prompt_error(error: &Error) -> bool {
    error.code.as_deref() == Some("invalid_prompt")
}

fn is_server_overloaded_error(error: &Error) -> bool {
    error.code.as_deref() == Some("server_is_overloaded")
        || error.code.as_deref() == Some("slow_down")
}

fn rate_limit_regex() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?i)try again in\s*(\d+(?:\.\d+)?)\s*(s|ms|seconds?)").unwrap()
    })
}
