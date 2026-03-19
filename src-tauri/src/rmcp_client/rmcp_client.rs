use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use futures::Future;
use futures::future::BoxFuture;
use oauth2::TokenResponse;
use rmcp::model::CallToolRequestParams;
use rmcp::model::CallToolResult;
use rmcp::model::ClientNotification;
use rmcp::model::ClientRequest;
use rmcp::model::CreateElicitationRequestParams;
use rmcp::model::CreateElicitationResult;
use rmcp::model::CustomNotification;
use rmcp::model::CustomRequest;
use rmcp::model::Extensions;
use rmcp::model::InitializeRequestParams;
use rmcp::model::InitializeResult;
use rmcp::model::ListResourceTemplatesResult;
use rmcp::model::ListResourcesResult;
use rmcp::model::ListToolsResult;
use rmcp::model::PaginatedRequestParams;
use rmcp::model::ReadResourceRequestParams;
use rmcp::model::ReadResourceResult;
use rmcp::model::RequestId;
use rmcp::model::ServerResult;
use rmcp::model::Tool;
use rmcp::service::RoleClient;
use rmcp::service::RunningService;
use rmcp::service::{self};
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time;
use tracing::info;
use tracing::warn;

use crate::rmcp_client::logging_client_handler::LoggingClientHandler;
use crate::rmcp_client::oauth::load_oauth_tokens;
use crate::rmcp_client::program_resolver;
use crate::rmcp_client::utils::create_env_for_mcp_server;
use crate::rmcp_client::utils::run_with_timeout;

/// Type-erased future that produces a RunningService.
type ServiceFuture = Pin<
    Box<
        dyn Future<
                Output = Result<
                    RunningService<RoleClient, LoggingClientHandler>,
                    rmcp::service::ClientInitializeError,
                >,
            > + Send,
    >,
>;

/// A factory closure that, given a LoggingClientHandler, returns a ServiceFuture.
type TransportFactory =
    Box<dyn FnOnce(LoggingClientHandler) -> ServiceFuture + Send>;

enum ClientState {
    Connecting {
        factory: Option<TransportFactory>,
        process_group_guard: Option<ProcessGroupGuard>,
    },
    Ready {
        _process_group_guard: Option<ProcessGroupGuard>,
        service: Arc<RunningService<RoleClient, LoggingClientHandler>>,
    },
}

#[cfg(unix)]
const PROCESS_GROUP_TERM_GRACE_PERIOD: Duration = Duration::from_secs(2);

#[cfg(unix)]
struct ProcessGroupGuard {
    process_group_id: u32,
}

#[cfg(not(unix))]
struct ProcessGroupGuard;

impl ProcessGroupGuard {
    fn new(process_group_id: u32) -> Self {
        #[cfg(unix)]
        {
            Self { process_group_id }
        }
        #[cfg(not(unix))]
        {
            let _ = process_group_id;
            Self
        }
    }

    #[cfg(unix)]
    fn maybe_terminate_process_group(&self) {
        let pgid = self.process_group_id;
        unsafe {
            if libc::killpg(pgid as i32, libc::SIGTERM) == 0 {
                let grace = PROCESS_GROUP_TERM_GRACE_PERIOD;
                std::thread::spawn(move || {
                    std::thread::sleep(grace);
                    let _ = libc::killpg(pgid as i32, libc::SIGKILL);
                });
            }
        }
    }

    #[cfg(not(unix))]
    fn maybe_terminate_process_group(&self) {}
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        self.maybe_terminate_process_group();
    }
}

pub type Elicitation = CreateElicitationRequestParams;
pub type ElicitationResponse = CreateElicitationResult;

/// Interface for sending elicitation requests to the UI and awaiting a response.
pub type SendElicitation = Box<
    dyn Fn(RequestId, Elicitation) -> BoxFuture<'static, Result<ElicitationResponse>> + Send + Sync,
>;

pub struct ToolWithConnectorId {
    pub tool: Tool,
    pub connector_id: Option<String>,
    pub connector_name: Option<String>,
}

pub struct ListToolsWithConnectorIdResult {
    pub next_cursor: Option<String>,
    pub tools: Vec<ToolWithConnectorId>,
}

/// MCP client implemented on top of the official `rmcp` SDK.
/// Supports both stdio and streamable HTTP transports, with OAuth token support.
pub struct RmcpClient {
    state: Mutex<ClientState>,
}

impl RmcpClient {
    pub async fn new_stdio_client(
        program: OsString,
        args: Vec<OsString>,
        env: Option<HashMap<String, String>>,
        env_vars: &[String],
        cwd: Option<PathBuf>,
    ) -> io::Result<Self> {
        let program_name = program.to_string_lossy().into_owned();
        let envs = create_env_for_mcp_server(env, env_vars);
        let resolved_program = program_resolver::resolve(program, &envs)?;

        let mut command = Command::new(resolved_program);
        command
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .env_clear()
            .envs(envs)
            .args(&args);
        #[cfg(unix)]
        command.process_group(0);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }

        let (transport, stderr) = TokioChildProcess::builder(command)
            .stderr(Stdio::piped())
            .spawn()?;
        let process_group_guard = transport.id().map(ProcessGroupGuard::new);

        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                loop {
                    match reader.next_line().await {
                        Ok(Some(line)) => {
                            info!("MCP server stderr ({program_name}): {line}");
                        }
                        Ok(None) => break,
                        Err(error) => {
                            warn!("Failed to read MCP server stderr ({program_name}): {error}");
                            break;
                        }
                    }
                }
            });
        }

        let factory: TransportFactory = Box::new(move |handler| {
            Box::pin(service::serve_client(handler, transport))
        });

        Ok(Self {
            state: Mutex::new(ClientState::Connecting {
                factory: Some(factory),
                process_group_guard,
            }),
        })
    }

    /// Create a streamable HTTP client. If no bearer_token is provided and no
    /// Authorization header is in http_headers, attempts to load stored OAuth tokens.
    #[allow(clippy::too_many_arguments)]
    pub async fn new_streamable_http_client(
        server_name: &str,
        url: &str,
        bearer_token: Option<String>,
        http_headers: Option<HashMap<String, String>>,
        _env_http_headers: Option<HashMap<String, String>>,
    ) -> Result<Self> {
        let mut config = StreamableHttpClientTransportConfig::with_uri(url.to_string());

        // Determine auth header
        let auth_header = if let Some(token) = bearer_token {
            Some(token)
        } else if let Some(ref headers) = http_headers {
            headers
                .get("Authorization")
                .or_else(|| headers.get("authorization"))
                .cloned()
        } else {
            // Try loading stored OAuth tokens
            match load_oauth_tokens(server_name, url) {
                Ok(Some(tokens)) => {
                    let access_token = tokens
                        .token_response
                        .0
                        .access_token()
                        .secret()
                        .to_string();
                    Some(format!("Bearer {access_token}"))
                }
                Ok(None) => None,
                Err(err) => {
                    warn!("failed to read tokens for server `{server_name}`: {err}");
                    None
                }
            }
        };

        if let Some(auth) = auth_header {
            config = config.auth_header(auth);
        }

        let transport = StreamableHttpClientTransport::from_config(config);

        let factory: TransportFactory = Box::new(move |handler| {
            Box::pin(service::serve_client(handler, transport))
        });

        Ok(Self {
            state: Mutex::new(ClientState::Connecting {
                factory: Some(factory),
                process_group_guard: None,
            }),
        })
    }

    /// Perform the initialization handshake with the MCP server.
    pub async fn initialize(
        &self,
        params: InitializeRequestParams,
        timeout: Option<Duration>,
        send_elicitation: SendElicitation,
    ) -> Result<InitializeResult> {
        let client_handler = LoggingClientHandler::new(params.clone(), send_elicitation);

        let (transport_fut, process_group_guard) = {
            let mut guard = self.state.lock().await;
            match &mut *guard {
                ClientState::Connecting {
                    factory,
                    process_group_guard,
                } => match factory.take() {
                    Some(f) => (f(client_handler), process_group_guard.take()),
                    None => return Err(anyhow!("client already initializing")),
                },
                ClientState::Ready { .. } => return Err(anyhow!("client already initialized")),
            }
        };

        let service = match timeout {
            Some(duration) => time::timeout(duration, transport_fut)
                .await
                .map_err(|_| anyhow!("timed out handshaking with MCP server after {duration:?}"))?
                .map_err(|err| anyhow!("handshaking with MCP server failed: {err}"))?,
            None => transport_fut
                .await
                .map_err(|err| anyhow!("handshaking with MCP server failed: {err}"))?,
        };

        let initialize_result = service
            .peer()
            .peer_info()
            .ok_or_else(|| anyhow!("handshake succeeded but server info was missing"))?
            .clone();

        {
            let mut guard = self.state.lock().await;
            *guard = ClientState::Ready {
                _process_group_guard: process_group_guard,
                service: Arc::new(service),
            };
        }

        Ok(initialize_result)
    }

    pub async fn list_tools(
        &self,
        params: Option<PaginatedRequestParams>,
        timeout: Option<Duration>,
    ) -> Result<ListToolsResult> {
        let service = self.service().await?;
        let fut = service.list_tools(params);
        run_with_timeout(fut, timeout, "tools/list").await
    }

    pub async fn list_tools_with_connector_ids(
        &self,
        params: Option<PaginatedRequestParams>,
        timeout: Option<Duration>,
    ) -> Result<ListToolsWithConnectorIdResult> {
        let service = self.service().await?;
        let fut = service.list_tools(params);
        let result = run_with_timeout(fut, timeout, "tools/list").await?;
        let tools = result
            .tools
            .into_iter()
            .map(|tool| {
                let meta = tool.meta.as_ref();
                let connector_id = Self::meta_string(meta, "connector_id");
                let connector_name = Self::meta_string(meta, "connector_name")
                    .or_else(|| Self::meta_string(meta, "connector_display_name"));
                ToolWithConnectorId {
                    tool,
                    connector_id,
                    connector_name,
                }
            })
            .collect();
        Ok(ListToolsWithConnectorIdResult {
            next_cursor: result.next_cursor,
            tools,
        })
    }

    fn meta_string(meta: Option<&rmcp::model::Meta>, key: &str) -> Option<String> {
        meta.and_then(|meta| meta.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    pub async fn list_resources(
        &self,
        params: Option<PaginatedRequestParams>,
        timeout: Option<Duration>,
    ) -> Result<ListResourcesResult> {
        let service = self.service().await?;
        let fut = service.list_resources(params);
        run_with_timeout(fut, timeout, "resources/list").await
    }

    pub async fn list_resource_templates(
        &self,
        params: Option<PaginatedRequestParams>,
        timeout: Option<Duration>,
    ) -> Result<ListResourceTemplatesResult> {
        let service = self.service().await?;
        let fut = service.list_resource_templates(params);
        run_with_timeout(fut, timeout, "resources/templates/list").await
    }

    pub async fn read_resource(
        &self,
        params: ReadResourceRequestParams,
        timeout: Option<Duration>,
    ) -> Result<ReadResourceResult> {
        let service = self.service().await?;
        let fut = service.read_resource(params);
        run_with_timeout(fut, timeout, "resources/read").await
    }

    pub async fn call_tool(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<CallToolResult> {
        let service = self.service().await?;
        let arguments = match arguments {
            Some(Value::Object(map)) => Some(map),
            Some(other) => {
                return Err(anyhow!(
                    "MCP tool arguments must be a JSON object, got {other}"
                ));
            }
            None => None,
        };
        let rmcp_params = CallToolRequestParams::new(name);
        let rmcp_params = if let Some(args) = arguments {
            rmcp_params.with_arguments(args)
        } else {
            rmcp_params
        };
        let fut = service.call_tool(rmcp_params);
        run_with_timeout(fut, timeout, "tools/call").await
    }

    pub async fn send_custom_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<()> {
        let service = self.service().await?;
        service
            .send_notification(ClientNotification::CustomNotification(CustomNotification {
                method: method.to_string(),
                params,
                extensions: Extensions::new(),
            }))
            .await?;
        Ok(())
    }

    pub async fn send_custom_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<ServerResult> {
        let service = self.service().await?;
        let response = service
            .send_request(ClientRequest::CustomRequest(CustomRequest::new(
                method, params,
            )))
            .await?;
        Ok(response)
    }

    async fn service(&self) -> Result<Arc<RunningService<RoleClient, LoggingClientHandler>>> {
        let guard = self.state.lock().await;
        match &*guard {
            ClientState::Ready { service, .. } => Ok(Arc::clone(service)),
            ClientState::Connecting { .. } => Err(anyhow!("MCP client not initialized")),
        }
    }
}
