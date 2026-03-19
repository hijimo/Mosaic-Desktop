use crate::mosaic_api::auth::{add_auth_headers, AuthProvider};
use crate::mosaic_api::provider::Provider;
use crate::mosaic_api::telemetry::run_with_request_telemetry;
use crate::mosaic_client::{
    HttpTransport, Request, RequestTelemetry, Response, StreamResponse, TransportError,
};
use http::{HeaderMap, Method};
use serde_json::Value;
use std::sync::Arc;

pub(crate) struct EndpointSession<T: HttpTransport, A: AuthProvider> {
    transport: T,
    provider: Provider,
    auth: A,
    request_telemetry: Option<Arc<dyn RequestTelemetry>>,
}

impl<T: HttpTransport, A: AuthProvider> EndpointSession<T, A> {
    pub(crate) fn new(transport: T, provider: Provider, auth: A) -> Self {
        Self {
            transport,
            provider,
            auth,
            request_telemetry: None,
        }
    }

    pub(crate) fn with_request_telemetry(
        mut self,
        request: Option<Arc<dyn RequestTelemetry>>,
    ) -> Self {
        self.request_telemetry = request;
        self
    }

    pub(crate) fn provider(&self) -> &Provider {
        &self.provider
    }

    fn make_request(
        &self,
        method: &Method,
        path: &str,
        extra_headers: &HeaderMap,
        body: Option<&Value>,
    ) -> Request {
        let mut req = self.provider.build_request(method.clone(), path);
        req.headers.extend(extra_headers.clone());
        if let Some(body) = body {
            req.body = Some(body.clone());
        }
        add_auth_headers(&self.auth, req)
    }

    pub(crate) async fn execute(
        &self,
        method: Method,
        path: &str,
        extra_headers: HeaderMap,
        body: Option<Value>,
    ) -> Result<Response, TransportError> {
        self.execute_with(method, path, extra_headers, body, |_| {})
            .await
    }

    pub(crate) async fn execute_with<C>(
        &self,
        method: Method,
        path: &str,
        extra_headers: HeaderMap,
        body: Option<Value>,
        configure: C,
    ) -> Result<Response, TransportError>
    where
        C: Fn(&mut Request),
    {
        let make_request = || {
            let mut req = self.make_request(&method, path, &extra_headers, body.as_ref());
            configure(&mut req);
            req
        };

        run_with_request_telemetry(
            self.provider.retry.to_policy(),
            self.request_telemetry.clone(),
            make_request,
            |req| self.transport.execute(req),
        )
        .await
    }

    pub(crate) async fn stream_with<C>(
        &self,
        method: Method,
        path: &str,
        extra_headers: HeaderMap,
        body: Option<Value>,
        configure: C,
    ) -> Result<StreamResponse, TransportError>
    where
        C: Fn(&mut Request),
    {
        let make_request = || {
            let mut req = self.make_request(&method, path, &extra_headers, body.as_ref());
            configure(&mut req);
            req
        };

        run_with_request_telemetry(
            self.provider.retry.to_policy(),
            self.request_telemetry.clone(),
            make_request,
            |req| self.transport.stream(req),
        )
        .await
    }
}
