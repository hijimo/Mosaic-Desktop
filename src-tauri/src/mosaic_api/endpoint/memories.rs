use crate::mosaic_api::auth::AuthProvider;
use crate::mosaic_api::common::{MemorySummarizeInput, MemorySummarizeOutput};
use crate::mosaic_api::endpoint::session::EndpointSession;
use crate::mosaic_api::error::ApiError;
use crate::mosaic_api::provider::Provider;
use crate::mosaic_client::{HttpTransport, RequestTelemetry};
use http::{HeaderMap, Method};
use serde::Deserialize;
use serde_json::to_value;
use std::sync::Arc;

pub struct MemoriesClient<T: HttpTransport, A: AuthProvider> {
    session: EndpointSession<T, A>,
}

impl<T: HttpTransport, A: AuthProvider> MemoriesClient<T, A> {
    pub fn new(transport: T, provider: Provider, auth: A) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
        }
    }

    pub fn with_telemetry(self, request: Option<Arc<dyn RequestTelemetry>>) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
        }
    }

    fn path() -> &'static str {
        "memories/trace_summarize"
    }

    pub async fn summarize(
        &self,
        body: serde_json::Value,
        extra_headers: HeaderMap,
    ) -> Result<Vec<MemorySummarizeOutput>, ApiError> {
        let resp = self
            .session
            .execute(Method::POST, Self::path(), extra_headers, Some(body))
            .await
            .map_err(ApiError::Transport)?;
        let parsed: SummarizeResponse =
            serde_json::from_slice(&resp.body).map_err(|e| ApiError::Stream(e.to_string()))?;
        Ok(parsed.output)
    }

    pub async fn summarize_input(
        &self,
        input: &MemorySummarizeInput,
        extra_headers: HeaderMap,
    ) -> Result<Vec<MemorySummarizeOutput>, ApiError> {
        let body = to_value(input).map_err(|e| {
            ApiError::Stream(format!("failed to encode memory summarize input: {e}"))
        })?;
        self.summarize(body, extra_headers).await
    }
}

#[derive(Debug, Deserialize)]
struct SummarizeResponse {
    output: Vec<MemorySummarizeOutput>,
}
