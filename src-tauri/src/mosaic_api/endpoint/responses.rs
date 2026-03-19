use crate::mosaic_api::auth::AuthProvider;
use crate::mosaic_api::common::{ResponseStream, ResponsesApiRequest};
use crate::mosaic_api::endpoint::session::EndpointSession;
use crate::mosaic_api::error::ApiError;
use crate::mosaic_api::provider::Provider;
use crate::mosaic_api::requests::headers::build_conversation_headers;
use crate::mosaic_api::requests::responses::{attach_item_ids, Compression};
use crate::mosaic_api::sse::responses::spawn_response_stream;
use crate::mosaic_api::telemetry::SseTelemetry;
use crate::mosaic_client::{HttpTransport, RequestCompression, RequestTelemetry};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::Value;
use std::sync::Arc;

pub struct ResponsesClient<T: HttpTransport, A: AuthProvider> {
    session: EndpointSession<T, A>,
    sse_telemetry: Option<Arc<dyn SseTelemetry>>,
}

#[derive(Default)]
pub struct ResponsesOptions {
    pub conversation_id: Option<String>,
    pub extra_headers: HeaderMap,
    pub compression: Compression,
}

impl<T: HttpTransport, A: AuthProvider> ResponsesClient<T, A> {
    pub fn new(transport: T, provider: Provider, auth: A) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
            sse_telemetry: None,
        }
    }

    pub fn with_telemetry(
        self,
        request: Option<Arc<dyn RequestTelemetry>>,
        sse: Option<Arc<dyn SseTelemetry>>,
    ) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
            sse_telemetry: sse,
        }
    }

    pub async fn stream_request(
        &self,
        request: ResponsesApiRequest,
        options: ResponsesOptions,
    ) -> Result<ResponseStream, ApiError> {
        let ResponsesOptions {
            conversation_id,
            extra_headers,
            compression,
        } = options;

        let mut body = serde_json::to_value(&request)
            .map_err(|e| ApiError::Stream(format!("failed to encode responses request: {e}")))?;
        if request.store && self.session.provider().is_azure_responses_endpoint() {
            attach_item_ids(&mut body, &request.input);
        }

        let mut headers = extra_headers;
        headers.extend(build_conversation_headers(conversation_id));

        self.stream(body, headers, compression).await
    }

    fn path() -> &'static str {
        "responses"
    }

    pub async fn stream(
        &self,
        body: Value,
        extra_headers: HeaderMap,
        compression: Compression,
    ) -> Result<ResponseStream, ApiError> {
        let request_compression = match compression {
            Compression::None => RequestCompression::None,
            Compression::Zstd => RequestCompression::Zstd,
        };

        let stream_response = self
            .session
            .stream_with(
                Method::POST,
                Self::path(),
                extra_headers,
                Some(body),
                |req| {
                    req.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                    req.compression = request_compression;
                },
            )
            .await
            .map_err(ApiError::Transport)?;

        Ok(spawn_response_stream(
            stream_response,
            self.session.provider().stream_idle_timeout,
            self.sse_telemetry.clone(),
        ))
    }
}
