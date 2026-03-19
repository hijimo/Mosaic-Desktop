use crate::mosaic_client::Request;
use http::HeaderMap;
use http::HeaderValue;

/// Provides bearer and account identity information for API requests.
pub trait AuthProvider: Send + Sync {
    fn bearer_token(&self) -> Option<String>;
    fn account_id(&self) -> Option<String> {
        None
    }
}

pub(crate) fn add_auth_headers_to_header_map<A: AuthProvider>(auth: &A, headers: &mut HeaderMap) {
    if let Some(token) = auth.bearer_token() {
        if let Ok(header) = HeaderValue::from_str(&format!("Bearer {token}")) {
            let _ = headers.insert(http::header::AUTHORIZATION, header);
        }
    }
    if let Some(account_id) = auth.account_id() {
        if let Ok(header) = HeaderValue::from_str(&account_id) {
            let _ = headers.insert("ChatGPT-Account-ID", header);
        }
    }
}

pub(crate) fn add_auth_headers<A: AuthProvider>(auth: &A, mut req: Request) -> Request {
    add_auth_headers_to_header_map(auth, &mut req.headers);
    req
}
