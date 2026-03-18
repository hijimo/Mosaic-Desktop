//! Session- and turn-scoped helpers for talking to model provider APIs.
//!
//! `ModelClient` lives for the lifetime of a Codex session and holds stable
//! configuration (provider, conversation id, retry settings).
//!
//! `ModelClientSession` is created per turn and streams one or more Responses
//! API requests. It caches the `previous_response_id` so subsequent requests
//! within the same turn can chain responses.
//!
//! Retry logic uses exponential backoff with jitter. Retryable conditions:
//! - HTTP 429 (rate limit)
//! - HTTP 5xx (server error)
//! - Transport-level errors (connection reset, timeout)

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::protocol::error::{CodexError, ErrorCode};
use crate::provider::{ModelProviderInfo, Provider, RetryConfig};

// ── WebSocket version ────────────────────────────────────────────

/// WebSocket protocol version for the Responses API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsesWebsocketVersion {
    V1,
    V2,
}

/// Determine the WebSocket version from provider capabilities.
pub fn ws_version_from_provider(info: &ModelProviderInfo) -> Option<ResponsesWebsocketVersion> {
    if info.supports_websockets {
        Some(ResponsesWebsocketVersion::V1)
    } else {
        None
    }
}

// ── Model Fallback ───────────────────────────────────────────────

/// Configuration for model fallback behavior.
#[derive(Debug, Clone)]
pub struct ModelFallbackConfig {
    /// Primary model to use.
    pub primary_model: String,
    /// Ordered list of fallback models to try if the primary fails.
    pub fallback_models: Vec<String>,
}

/// Errors that trigger a fallback to the next model.
fn is_fallback_eligible(err: &CodexError) -> bool {
    let msg = err.message.to_lowercase();
    msg.contains("model_not_found")
        || msg.contains("model not found")
        || msg.contains("capacity")
        || msg.contains("overloaded")
        || (msg.contains("429") && msg.contains("rate"))
}

// ── Response Events ──────────────────────────────────────────────

/// Token usage information from a completed response.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: u64,
    pub reasoning_output_tokens: u64,
}

/// A parsed SSE event from the Responses API stream.
#[derive(Debug)]
pub enum ResponseEvent {
    /// The response has been created.
    Created,
    /// Streaming text delta from the assistant.
    OutputTextDelta { delta: String },
    /// A complete output item (message, function_call, etc.).
    OutputItemDone(crate::protocol::types::ResponseItem),
    /// A new output item has been added to the response.
    OutputItemAdded(crate::protocol::types::ResponseItem),
    /// A function call from the model requesting tool execution.
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    /// Reasoning summary text delta.
    ReasoningSummaryDelta { delta: String, summary_index: i64 },
    /// Reasoning content text delta.
    ReasoningContentDelta { delta: String, content_index: i64 },
    /// A new reasoning summary part has been added.
    ReasoningSummaryPartAdded { summary_index: i64 },
    /// The effective model reported by the server.
    ServerModel(String),
    /// Rate-limit snapshot from the server.
    RateLimits(crate::protocol::types::RateLimitSnapshot),
    /// The response is complete.
    Completed {
        response_id: String,
        token_usage: Option<TokenUsage>,
        /// Whether the conversation can be appended to (incremental turns).
        can_append: bool,
    },
    /// An error from the API.
    Failed { code: String, message: String },
}

// ── Request types ────────────────────────────────────────────────

/// Minimal request body for the Responses API.
#[derive(serde::Serialize, Clone, Debug)]
pub struct ResponsesApiRequest {
    pub model: String,
    pub input: Vec<Value>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Value>,
}

/// Prompt payload for a single model turn.
#[derive(Default, Debug, Clone)]
pub struct Prompt {
    pub input: Vec<Value>,
    pub instructions: Option<String>,
    pub tools: Option<Vec<Value>>,
    pub parallel_tool_calls: Option<bool>,
    pub output_schema: Option<Value>,
}

// ── ModelClient (session-scoped) ─────────────────────────────────

/// Shared state across all turns within a session.
#[derive(Debug)]
struct ModelClientState {
    provider: Provider,
    api_key: String,
    extra_headers: HashMap<String, String>,
    conversation_id: String,
    /// Retry config from provider.
    retry: RetryConfig,
    /// Stream idle timeout.
    stream_idle_timeout: Duration,
    /// Last response_id for chaining (shared across turns).
    last_response_id: StdMutex<Option<String>>,
    /// WebSocket version to use (None = SSE only).
    ws_version: Option<ResponsesWebsocketVersion>,
    /// Whether WebSocket has been disabled due to connection failures.
    ws_disabled: std::sync::atomic::AtomicBool,
    /// Fallback config (optional).
    fallback_config: Option<ModelFallbackConfig>,
}

/// A session-scoped client for model-provider API calls.
///
/// Holds configuration and state shared across turns: provider, API key,
/// conversation ID, retry settings, and the last response_id for chaining.
#[derive(Debug, Clone)]
pub struct ModelClient {
    state: Arc<ModelClientState>,
}

impl ModelClientState {
    /// Clone all fields into a new owned state (needed for Arc::try_unwrap fallback).
    fn clone_state(&self) -> Self {
        Self {
            provider: self.provider.clone(),
            api_key: self.api_key.clone(),
            extra_headers: self.extra_headers.clone(),
            conversation_id: self.conversation_id.clone(),
            retry: self.retry.clone(),
            stream_idle_timeout: self.stream_idle_timeout,
            last_response_id: StdMutex::new(
                self.last_response_id
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone(),
            ),
            ws_version: self.ws_version,
            ws_disabled: std::sync::atomic::AtomicBool::new(
                self.ws_disabled
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            fallback_config: self.fallback_config.clone(),
        }
    }
}

impl ModelClient {
    pub fn new(
        provider: Provider,
        api_key: String,
        extra_headers: HashMap<String, String>,
        conversation_id: String,
    ) -> Self {
        let retry = provider.retry.clone();
        let stream_idle_timeout = provider.stream_idle_timeout;
        Self {
            state: Arc::new(ModelClientState {
                provider,
                api_key,
                extra_headers,
                conversation_id,
                retry,
                stream_idle_timeout,
                last_response_id: StdMutex::new(None),
                ws_version: None,
                ws_disabled: std::sync::atomic::AtomicBool::new(false),
                fallback_config: None,
            }),
        }
    }

    /// Creates a new session-scoped client from a `ModelProviderInfo`.
    pub fn from_provider_info(
        info: &ModelProviderInfo,
        api_key: String,
        conversation_id: String,
    ) -> Self {
        let provider = info.to_provider();
        let extra_headers = info.resolved_headers();
        let ws_version = ws_version_from_provider(info);
        let retry = provider.retry.clone();
        let stream_idle_timeout = provider.stream_idle_timeout;
        Self {
            state: Arc::new(ModelClientState {
                provider,
                api_key,
                extra_headers,
                conversation_id,
                retry,
                stream_idle_timeout,
                last_response_id: StdMutex::new(None),
                ws_version,
                ws_disabled: std::sync::atomic::AtomicBool::new(false),
                fallback_config: None,
            }),
        }
    }

    /// Creates a client with fallback model configuration.
    pub fn with_fallback(self, config: ModelFallbackConfig) -> Self {
        // We need to rebuild the Arc to set fallback_config
        let state = Arc::try_unwrap(self.state).unwrap_or_else(|arc| (*arc).clone_state());
        Self {
            state: Arc::new(ModelClientState {
                fallback_config: Some(config),
                ..state
            }),
        }
    }

    /// Check if WebSocket transport is available and not disabled.
    pub fn ws_available(&self) -> bool {
        self.state.ws_version.is_some()
            && !self
                .state
                .ws_disabled
                .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Disable WebSocket transport (e.g., after connection failure).
    pub fn disable_ws(&self) {
        self.state
            .ws_disabled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Creates a fresh turn-scoped streaming session.
    pub fn new_session(&self, model: String) -> ModelClientSession {
        let previous_response_id = self
            .state
            .last_response_id
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        ModelClientSession {
            client: self.clone(),
            model,
            previous_response_id,
        }
    }

    fn store_response_id(&self, id: String) {
        *self
            .state
            .last_response_id
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(id);
    }

    pub fn conversation_id(&self) -> &str {
        &self.state.conversation_id
    }

    pub fn provider(&self) -> &Provider {
        &self.state.provider
    }
}

// ── ModelClientSession (turn-scoped) ─────────────────────────────

/// A turn-scoped streaming session created from a `ModelClient`.
///
/// Caches the `previous_response_id` so subsequent requests within the
/// same turn can chain responses.
pub struct ModelClientSession {
    client: ModelClient,
    model: String,
    previous_response_id: Option<String>,
}

impl ModelClientSession {
    /// Streams a single model request within the current turn.
    ///
    /// Transport selection: WebSocket (if available) → SSE fallback.
    /// Model fallback: if configured, tries fallback models on eligible errors.
    pub async fn stream(
        &mut self,
        prompt: &Prompt,
    ) -> Result<tokio_stream::wrappers::ReceiverStream<Result<ResponseEvent, CodexError>>, CodexError> {
        let state = &self.client.state;

        // If fallback is configured, try models in order
        if let Some(ref fallback) = state.fallback_config {
            let models: Vec<String> = std::iter::once(fallback.primary_model.clone())
                .chain(fallback.fallback_models.iter().cloned())
                .collect();

            let mut last_err = None;
            for model in &models {
                match self.stream_single(prompt, model).await {
                    Ok(stream) => return Ok(stream),
                    Err(e) if is_fallback_eligible(&e) => {
                        tracing::warn!(model, "model failed, trying fallback: {}", e.message);
                        last_err = Some(e);
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            return Err(last_err.unwrap_or_else(|| {
                CodexError::new(
                    ErrorCode::InternalError,
                    "all fallback models exhausted".to_string(),
                )
            }));
        }

        self.stream_single(prompt, &self.model.clone()).await
    }

    /// Stream using a specific model, with WebSocket → SSE fallback.
    async fn stream_single(
        &mut self,
        prompt: &Prompt,
        model: &str,
    ) -> Result<tokio_stream::wrappers::ReceiverStream<Result<ResponseEvent, CodexError>>, CodexError> {
        let state = &self.client.state;

        // Try WebSocket first if available
        if self.client.ws_available() {
            match self.stream_via_ws(prompt, model).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    tracing::warn!("WebSocket stream failed, falling back to SSE: {}", e.message);
                    self.client.disable_ws();
                }
            }
        }

        // SSE path
        self.stream_via_sse(prompt, model).await
    }

    /// Stream via SSE (existing logic).
    async fn stream_via_sse(
        &mut self,
        prompt: &Prompt,
        model: &str,
    ) -> Result<tokio_stream::wrappers::ReceiverStream<Result<ResponseEvent, CodexError>>, CodexError> {
        let request = ResponsesApiRequest {
            model: model.to_string(),
            input: prompt.input.clone(),
            stream: true,
            instructions: prompt.instructions.clone(),
            previous_response_id: self.previous_response_id.clone(),
            tool_choice: Some("auto".into()),
            tools: prompt.tools.clone(),
            parallel_tool_calls: prompt.parallel_tool_calls,
            text: build_text_param(&prompt.output_schema),
        };

        let state = &self.client.state;
        let url = state.provider.url_for_path("responses");

        let stream = stream_with_retry(
            &url,
            &state.api_key,
            &state.extra_headers,
            &request,
            &state.retry,
            state.stream_idle_timeout,
        )
        .await?;

        // Wrap to capture response_id for chaining
        let client = self.client.clone();
        let (tx, rx) = mpsc::channel::<Result<ResponseEvent, CodexError>>(256);

        tokio::spawn(async move {
            let mut inner = std::pin::pin!(stream);
            while let Some(ev) = inner.next().await {
                match &ev {
                    Ok(ResponseEvent::Completed { response_id, .. }) => {
                        if !response_id.is_empty() {
                            client.store_response_id(response_id.clone());
                        }
                        let _ = tx.send(ev).await;
                        return;
                    }
                    _ => {
                        if tx.send(ev).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
    }

    /// Stream via WebSocket transport.
    async fn stream_via_ws(
        &mut self,
        prompt: &Prompt,
        model: &str,
    ) -> Result<tokio_stream::wrappers::ReceiverStream<Result<ResponseEvent, CodexError>>, CodexError> {
        use tokio_tungstenite::tungstenite::Message;

        let state = &self.client.state;
        let ws_url = state
            .provider
            .websocket_url_for_path("responses")
            .map_err(|e| CodexError::new(ErrorCode::InternalError, e))?;

        // Build WebSocket request with auth headers
        let ws_url_with_auth = format!(
            "{}{}",
            ws_url,
            if ws_url.contains('?') { "&" } else { "?" }
        );

        // tokio-tungstenite uses http::Request for custom headers
        let mut request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&ws_url)
            .header("Authorization", format!("Bearer {}", state.api_key))
            .header("OpenAI-Beta", "responses_websockets=2026-02-04");

        for (k, v) in &state.extra_headers {
            request = request.header(k.as_str(), v.as_str());
        }

        let request = request
            .body(())
            .map_err(|e| CodexError::new(ErrorCode::InternalError, format!("ws request build: {e}")))?;

        let (ws_stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("WebSocket connection failed: {e}"),
                )
            })?;

        let (mut write, mut read) = futures::StreamExt::split(ws_stream);

        // Build the request payload
        let request = serde_json::json!({
            "type": "response.create",
            "response": {
                "model": model,
                "input": prompt.input,
                "instructions": prompt.instructions,
                "tools": prompt.tools,
                "tool_choice": "auto",
                "parallel_tool_calls": prompt.parallel_tool_calls,
                "previous_response_id": self.previous_response_id,
            }
        });

        use futures::SinkExt;
        write
            .send(Message::Text(request.to_string().into()))
            .await
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("WebSocket send failed: {e}"),
                )
            })?;

        // Spawn reader task
        let client = self.client.clone();
        let idle_timeout = state.stream_idle_timeout;
        let (tx, rx) = mpsc::channel::<Result<ResponseEvent, CodexError>>(256);

        tokio::spawn(async move {
            loop {
                let next = tokio::time::timeout(idle_timeout, read.next()).await;
                match next {
                    Err(_) => {
                        let _ = tx
                            .send(Err(CodexError::new(
                                ErrorCode::InternalError,
                                "WebSocket idle timeout".to_string(),
                            )))
                            .await;
                        return;
                    }
                    Ok(None) => return,
                    Ok(Some(Err(e))) => {
                        let _ = tx
                            .send(Err(CodexError::new(
                                ErrorCode::InternalError,
                                format!("WebSocket error: {e}"),
                            )))
                            .await;
                        return;
                    }
                    Ok(Some(Ok(msg))) => {
                        let text = match msg {
                            Message::Text(t) => t.to_string(),
                            Message::Close(_) => return,
                            _ => continue,
                        };

                        let json: Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        let kind = json.get("type").and_then(Value::as_str).unwrap_or("");
                        let parsed = parse_ws_event(kind, &json);

                        if let Some(ev) = parsed {
                            let is_completed = matches!(&ev, Ok(ResponseEvent::Completed { .. }));
                            if let Ok(ResponseEvent::Completed { response_id, .. }) = &ev {
                                if !response_id.is_empty() {
                                    client.store_response_id(response_id.clone());
                                }
                            }
                            if tx.send(ev).await.is_err() {
                                return;
                            }
                            if is_completed {
                                return;
                            }
                        }
                    }
                }
            }
        });

        Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
    }

    /// Make a non-streaming Responses API call with JSON structured output.
    pub async fn complete_structured(
        &self,
        input: Vec<Value>,
        instructions: Option<&str>,
        output_schema: &Value,
    ) -> Result<String, CodexError> {
        complete_structured(
            &self.client.state.provider.url_for_path("responses"),
            &self.client.state.api_key,
            &self.client.state.extra_headers,
            &self.model,
            instructions,
            input,
            output_schema,
        )
        .await
    }
}

// ── Retry logic ──────────────────────────────────────────────────

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn backoff_delay(attempt: u32, base: Duration) -> Duration {
    let exp = base.mul_f64(2.0_f64.powi(attempt as i32));
    let jitter = Duration::from_millis(rand_jitter_ms());
    exp + jitter
}

/// Simple jitter: 0-100ms using a basic hash of the current time.
fn rand_jitter_ms() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (t % 100) as u64
}

async fn stream_with_retry(
    url: &str,
    api_key: &str,
    extra_headers: &HashMap<String, String>,
    request: &ResponsesApiRequest,
    retry: &RetryConfig,
    idle_timeout: Duration,
) -> Result<impl futures::Stream<Item = Result<ResponseEvent, CodexError>>, CodexError> {
    let max_attempts = retry.max_attempts.max(1);
    let mut last_err = None;

    for attempt in 0..max_attempts {
        if attempt > 0 {
            let delay = backoff_delay(attempt as u32 - 1, retry.base_delay);
            tokio::time::sleep(delay).await;
        }

        match try_stream_request(url, api_key, extra_headers, request, idle_timeout).await {
            Ok(stream) => return Ok(stream),
            Err(RetryableError::Retryable(e)) => {
                tracing::warn!(attempt, "retryable API error: {}", e.message);
                last_err = Some(e);
                continue;
            }
            Err(RetryableError::Fatal(e)) => return Err(e),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        CodexError::new(ErrorCode::InternalError, "all retry attempts exhausted".to_string())
    }))
}

enum RetryableError {
    Retryable(CodexError),
    Fatal(CodexError),
}

async fn try_stream_request(
    url: &str,
    api_key: &str,
    extra_headers: &HashMap<String, String>,
    request: &ResponsesApiRequest,
    idle_timeout: Duration,
) -> Result<impl futures::Stream<Item = Result<ResponseEvent, CodexError>>, RetryableError> {
    let mut req = reqwest::Client::builder()
        .timeout(Duration::from_secs(0)) // no overall timeout for streaming
        .build()
        .map_err(|e| RetryableError::Fatal(CodexError::new(
            ErrorCode::InternalError,
            format!("failed to build HTTP client: {e}"),
        )))?
        .post(url)
        .bearer_auth(api_key)
        .json(request);

    for (k, v) in extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let resp = req.send().await.map_err(|e| {
        if e.is_connect() || e.is_timeout() {
            RetryableError::Retryable(CodexError::new(
                ErrorCode::InternalError,
                format!("transport error: {e}"),
            ))
        } else {
            RetryableError::Fatal(CodexError::new(
                ErrorCode::InternalError,
                format!("HTTP request failed: {e}"),
            ))
        }
    })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let err = CodexError::new(
            ErrorCode::InternalError,
            format!("API returned {status}: {body}"),
        );
        return if is_retryable_status(status) {
            Err(RetryableError::Retryable(err))
        } else {
            Err(RetryableError::Fatal(err))
        };
    }

    let (tx, rx) = mpsc::channel::<Result<ResponseEvent, CodexError>>(256);

    tokio::spawn(async move {
        let mut stream = resp.bytes_stream().eventsource();
        let idle = idle_timeout;

        loop {
            let next = tokio::time::timeout(idle, stream.next()).await;
            match next {
                Err(_elapsed) => {
                    let _ = tx
                        .send(Err(CodexError::new(
                            ErrorCode::InternalError,
                            format!("stream idle timeout after {}s", idle.as_secs()),
                        )))
                        .await;
                    return;
                }
                Ok(None) => {
                    let _ = tx
                        .send(Err(CodexError::new(
                            ErrorCode::InternalError,
                            "stream closed before response.completed".to_string(),
                        )))
                        .await;
                    return;
                }
                Ok(Some(Err(e))) => {
                    let _ = tx
                        .send(Err(CodexError::new(
                            ErrorCode::InternalError,
                            format!("SSE stream error: {e}"),
                        )))
                        .await;
                    return;
                }
                Ok(Some(Ok(event))) => {
                    if event.data.is_empty() || event.data == "[DONE]" {
                        continue;
                    }

                    let json: Value = match serde_json::from_str(&event.data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let kind = json.get("type").and_then(Value::as_str).unwrap_or("");

                    let parsed = match kind {
                        "response.completed" | "response.done" => {
                            // These terminate the stream, handle specially
                            if let Some(result) = parse_ws_event(kind, &json) {
                                let _ = tx.send(result).await;
                            }
                            return;
                        }
                        _ => parse_ws_event(kind, &json),
                    };

                    if let Some(ev) = parsed {
                        if tx.send(ev).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }
    });

    Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
}

fn parse_ws_event(kind: &str, json: &Value) -> Option<Result<ResponseEvent, CodexError>> {
    match kind {
        "response.output_text.delta" => {
            let delta = json
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Some(Ok(ResponseEvent::OutputTextDelta { delta }))
        }
        "response.output_item.done" => {
            let item_val = json.get("item").cloned().unwrap_or(Value::Null);
            // Check if this is a function_call item
            if item_val.get("type").and_then(Value::as_str) == Some("function_call") {
                let call_id = item_val.get("call_id").and_then(Value::as_str).unwrap_or("").to_string();
                let name = item_val.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                let arguments = item_val.get("arguments").and_then(Value::as_str).unwrap_or("{}").to_string();
                return Some(Ok(ResponseEvent::FunctionCall { call_id, name, arguments }));
            }
            if let Ok(item) = serde_json::from_value::<crate::protocol::types::ResponseItem>(item_val) {
                return Some(Ok(ResponseEvent::OutputItemDone(item)));
            }
            None
        }
        "response.output_item.added" => {
            let item_val = json.get("item").cloned()?;
            let item = serde_json::from_value::<crate::protocol::types::ResponseItem>(item_val).ok()?;
            Some(Ok(ResponseEvent::OutputItemAdded(item)))
        }
        "response.reasoning_summary_text.delta" => {
            let delta = json.get("delta").and_then(Value::as_str).unwrap_or("").to_string();
            let summary_index = json.get("summary_index").and_then(Value::as_i64).unwrap_or(0);
            Some(Ok(ResponseEvent::ReasoningSummaryDelta { delta, summary_index }))
        }
        "response.reasoning_text.delta" => {
            let delta = json.get("delta").and_then(Value::as_str).unwrap_or("").to_string();
            let content_index = json.get("content_index").and_then(Value::as_i64).unwrap_or(0);
            Some(Ok(ResponseEvent::ReasoningContentDelta { delta, content_index }))
        }
        "response.reasoning_summary_part.added" => {
            let summary_index = json.get("summary_index").and_then(Value::as_i64)?;
            Some(Ok(ResponseEvent::ReasoningSummaryPartAdded { summary_index }))
        }
        "response.created" => {
            if json.get("response").is_some() {
                return Some(Ok(ResponseEvent::Created));
            }
            None
        }
        "response.completed" | "response.done" => {
            let can_append = kind == "response.done";
            let resp_obj = json.get("response");
            let response_id = resp_obj
                .and_then(|r| r.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let token_usage = resp_obj
                .and_then(|r| r.get("usage"))
                .map(parse_token_usage);
            Some(Ok(ResponseEvent::Completed {
                response_id,
                token_usage,
                can_append,
            }))
        }
        "response.incomplete" => {
            let reason = json.get("response")
                .and_then(|r| r.get("incomplete_details"))
                .and_then(|d| d.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let message = format!("Incomplete response, reason: {reason}");
            Some(Err(CodexError::new(ErrorCode::InternalError, message)))
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
        "error" => {
            let message = json
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("WebSocket error")
                .to_string();
            Some(Err(CodexError::new(ErrorCode::InternalError, message)))
        }
        _ => None,
    }
}

fn parse_token_usage(usage: &Value) -> TokenUsage {
    TokenUsage {
        input_tokens: usage
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: usage
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        total_tokens: usage
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cached_input_tokens: usage
            .get("input_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        reasoning_output_tokens: usage
            .get("output_tokens_details")
            .and_then(|d| d.get("reasoning_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
    }
}

fn build_text_param(output_schema: &Option<Value>) -> Option<Value> {
    let schema = output_schema.as_ref()?;
    Some(serde_json::json!({
        "format": {
            "type": "json_schema",
            "name": "codex_output_schema",
            "schema": schema,
            "strict": true,
        }
    }))
}

// ── History conversion ───────────────────────────────────────────

/// Convert a `ResponseInputItem` from session history into a Responses API input item.
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
        crate::protocol::types::ResponseInputItem::FunctionCallOutput { call_id, output } => {
            let content = match &output.body {
                crate::protocol::types::FunctionCallOutputBody::Text(s) => s.clone(),
                crate::protocol::types::FunctionCallOutputBody::ContentItems(items) => items
                    .iter()
                    .filter_map(|i| match i {
                        crate::protocol::types::FunctionCallOutputContentItem::InputText { text } => Some(text.clone()),
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
        crate::protocol::types::ResponseInputItem::McpToolCallOutput { call_id, result } => {
            let output = match result {
                Ok(r) => serde_json::to_string(r).unwrap_or_default(),
                Err(e) => e.clone(),
            };
            serde_json::json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": output
            })
        }
        crate::protocol::types::ResponseInputItem::CustomToolCallOutput { call_id, output } => {
            let content = output.text_content().unwrap_or_default().to_string();
            serde_json::json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": content
            })
        }
    }
}

// ── Non-streaming structured output ──────────────────────────────

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
    #[derive(serde::Serialize)]
    struct StructuredRequest<'a> {
        model: &'a str,
        input: Vec<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        instructions: Option<&'a str>,
        stream: bool,
        text: Value,
    }

    let body = StructuredRequest {
        model,
        input,
        instructions,
        stream: false,
        text: serde_json::json!({
            "format": {
                "type": "json_schema",
                "name": "memory_extraction",
                "schema": output_schema,
                "strict": true,
            }
        }),
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

// ── Legacy free-function API (backward compat) ───────────────────

/// Stream events from the Responses API for a single turn (legacy API).
///
/// Prefer `ModelClient::new_session().stream()` for new code.
pub async fn stream_response(
    url: &str,
    api_key: &str,
    extra_headers: &HashMap<String, String>,
    model: &str,
    instructions: Option<&str>,
    history: Vec<Value>,
    previous_response_id: Option<&str>,
    tools: Option<Vec<Value>>,
) -> Result<impl futures::Stream<Item = Result<ResponseEvent, CodexError>>, CodexError> {
    let request = ResponsesApiRequest {
        model: model.into(),
        input: history,
        stream: true,
        instructions: instructions.map(String::from),
        previous_response_id: previous_response_id.map(String::from),
        tool_choice: if tools.is_some() { Some("auto".into()) } else { None },
        tools,
        parallel_tool_calls: None,
        text: None,
    };

    let retry = RetryConfig {
        max_attempts: 4,
        base_delay: Duration::from_millis(200),
        retry_429: false,
        retry_5xx: true,
        retry_transport: true,
    };

    stream_with_retry(
        url,
        api_key,
        extra_headers,
        &request,
        &retry,
        Duration::from_secs(300),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_token_usage_extracts_fields() {
        let usage = serde_json::json!({
            "input_tokens": 100,
            "output_tokens": 50,
            "total_tokens": 150,
            "input_tokens_details": {"cached_tokens": 30},
            "output_tokens_details": {"reasoning_tokens": 10}
        });
        let parsed = parse_token_usage(&usage);
        assert_eq!(parsed.input_tokens, 100);
        assert_eq!(parsed.output_tokens, 50);
        assert_eq!(parsed.total_tokens, 150);
        assert_eq!(parsed.cached_input_tokens, 30);
        assert_eq!(parsed.reasoning_output_tokens, 10);
    }

    #[test]
    fn parse_token_usage_handles_missing_fields() {
        let usage = serde_json::json!({});
        let parsed = parse_token_usage(&usage);
        assert_eq!(parsed.input_tokens, 0);
        assert_eq!(parsed.total_tokens, 0);
    }

    #[test]
    fn build_text_param_returns_none_for_none() {
        assert!(build_text_param(&None).is_none());
    }

    #[test]
    fn build_text_param_wraps_schema() {
        let schema = serde_json::json!({"type": "object"});
        let result = build_text_param(&Some(schema.clone())).unwrap();
        assert_eq!(
            result["format"]["type"].as_str(),
            Some("json_schema")
        );
        assert_eq!(result["format"]["schema"], schema);
    }

    #[test]
    fn backoff_delay_increases_exponentially() {
        let base = Duration::from_millis(100);
        let d0 = backoff_delay(0, base);
        let d1 = backoff_delay(1, base);
        let d2 = backoff_delay(2, base);
        // d0 ≈ 100ms + jitter, d1 ≈ 200ms + jitter, d2 ≈ 400ms + jitter
        assert!(d1 > d0);
        assert!(d2 > d1);
    }

    #[test]
    fn is_retryable_status_checks_correctly() {
        assert!(is_retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retryable_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(!is_retryable_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(reqwest::StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(reqwest::StatusCode::NOT_FOUND));
    }

    #[test]
    fn history_item_to_api_message() {
        let item = crate::protocol::types::ResponseInputItem::Message {
            role: "user".into(),
            content: "hello".into(),
        };
        let v = history_item_to_api(&item);
        assert_eq!(v["type"], "message");
        assert_eq!(v["role"], "user");
    }

    #[test]
    fn history_item_to_api_function_call() {
        let item = crate::protocol::types::ResponseInputItem::FunctionCall {
            call_id: "c1".into(),
            name: "shell".into(),
            arguments: "{}".into(),
        };
        let v = history_item_to_api(&item);
        assert_eq!(v["type"], "function_call");
        assert_eq!(v["name"], "shell");
    }

    #[test]
    fn model_client_stores_and_retrieves_response_id() {
        let info = crate::provider::ModelProviderInfo::create_oss("http://localhost:8080/v1");
        let client = ModelClient::from_provider_info(&info, "test-key".into(), "conv-1".into());
        assert_eq!(client.conversation_id(), "conv-1");

        client.store_response_id("resp-123".into());
        let session = client.new_session("gpt-test".into());
        assert_eq!(session.previous_response_id.as_deref(), Some("resp-123"));
    }

    #[test]
    fn ws_version_from_provider_returns_v1_when_supported() {
        let mut info = crate::provider::ModelProviderInfo::create_openai();
        info.supports_websockets = true;
        assert_eq!(
            ws_version_from_provider(&info),
            Some(ResponsesWebsocketVersion::V1)
        );
    }

    #[test]
    fn ws_version_from_provider_returns_none_when_not_supported() {
        let info = crate::provider::ModelProviderInfo::create_oss("http://localhost:8080/v1");
        assert_eq!(ws_version_from_provider(&info), None);
    }

    #[test]
    fn ws_available_respects_provider_and_disable() {
        let mut info = crate::provider::ModelProviderInfo::create_openai();
        info.supports_websockets = true;
        let client = ModelClient::from_provider_info(&info, "key".into(), "conv".into());
        assert!(client.ws_available());

        client.disable_ws();
        assert!(!client.ws_available());
    }

    #[test]
    fn ws_not_available_for_oss_provider() {
        let info = crate::provider::ModelProviderInfo::create_oss("http://localhost:8080/v1");
        let client = ModelClient::from_provider_info(&info, "key".into(), "conv".into());
        assert!(!client.ws_available());
    }

    #[test]
    fn is_fallback_eligible_detects_model_not_found() {
        let err = CodexError::new(ErrorCode::InternalError, "model_not_found");
        assert!(is_fallback_eligible(&err));
    }

    #[test]
    fn is_fallback_eligible_detects_capacity() {
        let err = CodexError::new(ErrorCode::InternalError, "server overloaded");
        assert!(is_fallback_eligible(&err));
    }

    #[test]
    fn is_fallback_eligible_detects_rate_limit() {
        let err = CodexError::new(ErrorCode::InternalError, "429 rate limit exceeded");
        assert!(is_fallback_eligible(&err));
    }

    #[test]
    fn is_fallback_eligible_rejects_auth_error() {
        let err = CodexError::new(ErrorCode::InternalError, "unauthorized");
        assert!(!is_fallback_eligible(&err));
    }

    #[test]
    fn parse_ws_event_output_text_delta() {
        let json = serde_json::json!({"type": "response.output_text.delta", "delta": "hello"});
        let ev = parse_ws_event("response.output_text.delta", &json).unwrap().unwrap();
        match ev {
            ResponseEvent::OutputTextDelta { delta } => assert_eq!(delta, "hello"),
            _ => panic!("expected OutputTextDelta"),
        }
    }

    #[test]
    fn parse_ws_event_completed() {
        let json = serde_json::json!({
            "type": "response.completed",
            "response": {"id": "resp-1", "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15}}
        });
        let ev = parse_ws_event("response.completed", &json).unwrap().unwrap();
        match ev {
            ResponseEvent::Completed { response_id, token_usage, can_append } => {
                assert_eq!(response_id, "resp-1");
                assert_eq!(token_usage.unwrap().total_tokens, 15);
                assert!(!can_append);
            }
            _ => panic!("expected Completed"),
        }
    }

    #[test]
    fn parse_ws_event_error() {
        let json = serde_json::json!({"type": "error", "error": {"message": "bad request"}});
        let ev = parse_ws_event("error", &json).unwrap();
        assert!(ev.is_err());
        assert!(ev.unwrap_err().message.contains("bad request"));
    }

    #[test]
    fn parse_ws_event_unknown_returns_none() {
        let json = serde_json::json!({"type": "unknown.event"});
        assert!(parse_ws_event("unknown.event", &json).is_none());
    }

    #[test]
    fn model_fallback_config_creation() {
        let config = ModelFallbackConfig {
            primary_model: "gpt-4o".into(),
            fallback_models: vec!["gpt-4o-mini".into(), "gpt-3.5-turbo".into()],
        };
        assert_eq!(config.primary_model, "gpt-4o");
        assert_eq!(config.fallback_models.len(), 2);
    }
}
