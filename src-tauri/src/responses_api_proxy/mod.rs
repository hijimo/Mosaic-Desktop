//! Minimal Responses API proxy.
//!
//! Listens on a local port and forwards `POST /v1/responses` requests to an upstream
//! OpenAI-compatible endpoint, injecting the Authorization header read from the caller.

pub mod process_hardening;
pub mod read_api_key;

use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, HOST};
use reqwest::Url;
use serde::Serialize;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

/// Configuration for the proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub port: Option<u16>,
    pub server_info_path: Option<PathBuf>,
    pub http_shutdown: bool,
    pub upstream_url: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            port: None,
            server_info_path: None,
            http_shutdown: false,
            upstream_url: "https://api.openai.com/v1/responses".to_string(),
        }
    }
}

#[derive(Serialize)]
struct ServerInfo {
    port: u16,
    pid: u32,
}

struct ForwardConfig {
    upstream_url: Url,
    host_header: HeaderValue,
}

/// Start the proxy server. `auth_header` should be a full `Bearer <token>` string.
/// This function blocks until the server stops.
pub fn run_proxy(config: ProxyConfig, auth_header: &'static str) -> Result<()> {
    let upstream_url = Url::parse(&config.upstream_url).context("parsing upstream URL")?;
    let host = match (upstream_url.host_str(), upstream_url.port()) {
        (Some(h), Some(p)) => format!("{h}:{p}"),
        (Some(h), None) => h.to_string(),
        _ => return Err(anyhow!("upstream URL must include a host")),
    };
    let host_header = HeaderValue::from_str(&host).context("constructing Host header")?;

    let forward_config = Arc::new(ForwardConfig {
        upstream_url,
        host_header,
    });

    let (listener, bound_addr) = bind_listener(config.port)?;
    if let Some(path) = config.server_info_path.as_ref() {
        write_server_info(path, bound_addr.port())?;
    }
    let server = Server::from_listener(listener, None)
        .map_err(|err| anyhow!("creating HTTP server: {err}"))?;
    let client = Arc::new(
        Client::builder()
            .timeout(None::<Duration>)
            .build()
            .context("building reqwest client")?,
    );

    let http_shutdown = config.http_shutdown;
    for request in server.incoming_requests() {
        let client = client.clone();
        let fwd = forward_config.clone();
        std::thread::spawn(move || {
            if http_shutdown && request.method() == &Method::Get && request.url() == "/shutdown" {
                let _ = request.respond(Response::new_empty(StatusCode(200)));
                std::process::exit(0);
            }
            if let Err(e) = forward_request(&client, auth_header, &fwd, request) {
                eprintln!("forwarding error: {e}");
            }
        });
    }

    Err(anyhow!("server stopped unexpectedly"))
}

fn bind_listener(port: Option<u16>) -> Result<(TcpListener, SocketAddr)> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port.unwrap_or(0)));
    let listener = TcpListener::bind(addr).with_context(|| format!("failed to bind {addr}"))?;
    let bound = listener.local_addr().context("failed to read local_addr")?;
    Ok((listener, bound))
}

fn write_server_info(path: &Path, port: u16) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let info = ServerInfo {
        port,
        pid: std::process::id(),
    };
    let mut data = serde_json::to_string(&info)?;
    data.push('\n');
    let mut f = std::fs::File::create(path)?;
    f.write_all(data.as_bytes())?;
    Ok(())
}

fn forward_request(
    client: &Client,
    auth_header: &'static str,
    config: &ForwardConfig,
    mut req: Request,
) -> Result<()> {
    let method = req.method().clone();
    let url_path = req.url().to_string();
    if !(method == Method::Post && url_path == "/v1/responses") {
        let _ = req.respond(Response::new_empty(StatusCode(403)));
        return Ok(());
    }

    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut req.as_reader(), &mut body)?;

    let mut headers = HeaderMap::new();
    for header in req.headers() {
        let lower = header.field.as_str().to_ascii_lowercase();
        if lower == "authorization" || lower == "host" {
            continue;
        }
        let Ok(name) = HeaderName::from_bytes(lower.as_bytes()) else {
            continue;
        };
        if let Ok(value) = HeaderValue::from_bytes(header.value.as_bytes()) {
            headers.append(name, value);
        }
    }

    let mut auth_value = HeaderValue::from_static(auth_header);
    auth_value.set_sensitive(true);
    headers.insert(AUTHORIZATION, auth_value);
    headers.insert(HOST, config.host_header.clone());

    let upstream_resp = client
        .post(config.upstream_url.clone())
        .headers(headers)
        .body(body)
        .send()
        .context("forwarding request to upstream")?;

    let status = upstream_resp.status();
    let mut response_headers = Vec::new();
    for (name, value) in upstream_resp.headers().iter() {
        if matches!(
            name.as_str(),
            "content-length" | "transfer-encoding" | "connection" | "trailer" | "upgrade"
        ) {
            continue;
        }
        if let Ok(h) = Header::from_bytes(name.as_str().as_bytes(), value.as_bytes()) {
            response_headers.push(h);
        }
    }

    let content_length = upstream_resp.content_length().and_then(|len| {
        if len <= usize::MAX as u64 {
            Some(len as usize)
        } else {
            None
        }
    });

    let response = Response::new(
        StatusCode(status.as_u16()),
        response_headers,
        upstream_resp,
        content_length,
        None,
    );
    let _ = req.respond(response);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_config_default_upstream() {
        let cfg = ProxyConfig::default();
        assert_eq!(cfg.upstream_url, "https://api.openai.com/v1/responses");
        assert_eq!(cfg.port, None);
    }

    #[test]
    fn bind_listener_ephemeral_port() {
        let (listener, addr) = bind_listener(None).expect("bind ok");
        assert_ne!(addr.port(), 0);
        drop(listener);
    }

    #[test]
    fn write_server_info_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("info.json");
        write_server_info(&path, 12345).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let info: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(info["port"], 12345);
    }
}
