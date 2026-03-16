use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::{oneshot, Mutex};

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{DynamicToolCallResponseEvent, Event, EventMsg};
use crate::protocol::types::{DynamicToolCallRequest, DynamicToolResponse, DynamicToolSpec};

/// Manages dynamically registered tools.
///
/// When a dynamic tool is called:
/// 1. A `DynamicToolCallRequest` event is sent on the EQ.
/// 2. The handler waits for a matching `DynamicToolResponse` (keyed by `call_id`).
/// 3. The response content is returned as the tool result.
///
/// All fields use interior mutability so the handler can be shared via `Arc`
/// without an outer `Mutex`, avoiding deadlocks between `invoke` (which awaits
/// a oneshot) and `resolve_call` (which completes the oneshot).
pub struct DynamicToolHandler {
    /// Registered tool specs. Uses `std::sync::Mutex` so `matches_kind` (sync) can access it.
    specs: std::sync::Mutex<HashMap<String, DynamicToolSpec>>,
    tx_event: async_channel::Sender<Event>,
    /// Pending call waiters: call_id → oneshot sender for the response.
    pending: Mutex<HashMap<String, oneshot::Sender<DynamicToolResponse>>>,
    /// Counter for generating unique call IDs.
    next_call_id: Mutex<u64>,
}

impl DynamicToolHandler {
    pub fn new(tx_event: async_channel::Sender<Event>) -> Self {
        Self {
            specs: std::sync::Mutex::new(HashMap::new()),
            tx_event,
            pending: Mutex::new(HashMap::new()),
            next_call_id: Mutex::new(0),
        }
    }

    /// Register a dynamic tool spec. Immediately available for invocation.
    pub fn register_tool(&self, spec: DynamicToolSpec) {
        let mut specs = self.specs.lock().unwrap();
        specs.insert(spec.name.clone(), spec);
    }

    /// Remove a registered dynamic tool.
    pub fn unregister_tool(&self, name: &str) -> Option<DynamicToolSpec> {
        let mut specs = self.specs.lock().unwrap();
        specs.remove(name)
    }

    /// Get a registered tool spec by name (returns a clone).
    pub fn get_spec(&self, name: &str) -> Option<DynamicToolSpec> {
        let specs = self.specs.lock().unwrap();
        specs.get(name).cloned()
    }

    /// List all registered dynamic tool specs (returns clones).
    pub fn registered_tools(&self) -> Vec<DynamicToolSpec> {
        let specs = self.specs.lock().unwrap();
        specs.values().cloned().collect()
    }

    /// Check if a tool with the given name is registered.
    pub fn has_tool(&self, name: &str) -> bool {
        let specs = self.specs.lock().unwrap();
        specs.contains_key(name)
    }

    /// Invoke a dynamic tool: send the request event and wait for the response.
    pub async fn invoke(
        &self,
        tool_name: &str,
        turn_id: &str,
        args: serde_json::Value,
    ) -> Result<DynamicToolResponse, CodexError> {
        {
            let specs = self.specs.lock().unwrap();
            if !specs.contains_key(tool_name) {
                return Err(CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    format!("dynamic tool not registered: {tool_name}"),
                ));
            }
        }

        let call_id = {
            let mut counter = self.next_call_id.lock().await;
            *counter += 1;
            format!("dyn_call_{counter}")
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(call_id.clone(), tx);
        }

        // Send DynamicToolCallRequest event on the EQ
        let request = DynamicToolCallRequest {
            call_id: call_id.clone(),
            turn_id: turn_id.to_string(),
            tool: tool_name.to_string(),
            arguments: args,
        };

        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            msg: EventMsg::DynamicToolCallRequest(request),
        };

        self.tx_event.send(event).await.map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to send DynamicToolCallRequest event: {e}"),
            )
        })?;

        // Wait for the matching response
        rx.await.map_err(|_| {
            CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("dynamic tool call '{call_id}' was cancelled or timed out"),
            )
        })
    }

    /// Resolve a pending dynamic tool call with the given response.
    /// Called when `Op::DynamicToolResponse` is received on the SQ.
    pub async fn resolve_call(
        &self,
        call_id: &str,
        response: DynamicToolResponse,
    ) -> Result<(), CodexError> {
        let sender = {
            let mut pending = self.pending.lock().await;
            pending.remove(call_id)
        };

        match sender {
            Some(tx) => {
                // Ignore send error — receiver may have been dropped (timeout).
                let _ = tx.send(response);
                Ok(())
            }
            None => Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("no pending dynamic tool call with id: {call_id}"),
            )),
        }
    }

    /// Send a DynamicToolCallResponse event on the EQ (for logging/UI).
    pub async fn send_response_event(
        &self,
        call_id: &str,
        turn_id: &str,
        tool_name: &str,
        args: serde_json::Value,
        response: &DynamicToolResponse,
    ) -> Result<(), CodexError> {
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            msg: EventMsg::DynamicToolCallResponse(DynamicToolCallResponseEvent {
                call_id: call_id.to_string(),
                turn_id: turn_id.to_string(),
                tool: tool_name.to_string(),
                arguments: args,
                content_items: response.content_items.clone(),
                success: response.success,
                error: None,
                duration: None,
            }),
        };

        self.tx_event.send(event).await.map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to send DynamicToolCallResponse event: {e}"),
            )
        })
    }
}

#[async_trait]
impl ToolHandler for DynamicToolHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        match kind {
            ToolKind::Dynamic(name) => {
                let specs = self.specs.lock().unwrap();
                specs.contains_key(name)
            }
            _ => false,
        }
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Dynamic("__dynamic_handler__".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Direct handle() is not the primary invocation path for dynamic tools.
        // The caller should use `invoke()` which manages the request/response lifecycle.
        // This fallback extracts tool_name from args if present.
        let tool_name = args
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!(
                "dynamic tool '{tool_name}' must be invoked via DynamicToolHandler::invoke(), not handle()"
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn make_handler() -> (DynamicToolHandler, async_channel::Receiver<Event>) {
        let (tx, rx) = async_channel::unbounded();
        let handler = DynamicToolHandler::new(tx);
        (handler, rx)
    }

    #[test]
    fn register_and_query() {
        let (handler, _rx) = make_handler();
        let spec = DynamicToolSpec {
            name: "my_tool".to_string(),
            description: "test".to_string(),
            input_schema: serde_json::json!({}),
        };
        handler.register_tool(spec);
        assert!(handler.has_tool("my_tool"));
        assert!(!handler.has_tool("other"));
        assert_eq!(handler.registered_tools().len(), 1);
    }

    #[test]
    fn unregister_tool() {
        let (handler, _rx) = make_handler();
        handler.register_tool(DynamicToolSpec {
            name: "tmp".to_string(),
            description: String::new(),
            input_schema: serde_json::Value::Null,
        });
        assert!(handler.has_tool("tmp"));
        let removed = handler.unregister_tool("tmp");
        assert!(removed.is_some());
        assert!(!handler.has_tool("tmp"));
    }

    #[test]
    fn matches_kind_only_dynamic() {
        let (handler, _rx) = make_handler();
        handler.register_tool(DynamicToolSpec {
            name: "dyn1".to_string(),
            description: String::new(),
            input_schema: serde_json::Value::Null,
        });
        assert!(handler.matches_kind(&ToolKind::Dynamic("dyn1".to_string())));
        assert!(!handler.matches_kind(&ToolKind::Dynamic("dyn2".to_string())));
        assert!(!handler.matches_kind(&ToolKind::Builtin("dyn1".to_string())));
    }

    #[tokio::test]
    async fn invoke_unregistered_tool_returns_error() {
        let (handler, _rx) = make_handler();
        let result = handler
            .invoke("nonexistent", "turn_1", serde_json::Value::Null)
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ToolExecutionFailed);
    }

    #[tokio::test]
    async fn invoke_sends_request_event_and_resolve_completes() {
        let (handler, rx) = make_handler();
        handler.register_tool(DynamicToolSpec {
            name: "echo".to_string(),
            description: "echo tool".to_string(),
            input_schema: serde_json::json!({}),
        });

        let handler = Arc::new(handler);
        let handler_clone = handler.clone();

        // Spawn the invoke in a background task
        let invoke_handle = tokio::spawn(async move {
            handler_clone
                .invoke("echo", "turn_1", serde_json::json!({"input": "hi"}))
                .await
        });

        // Wait for the request event
        let event = rx.recv().await.unwrap();
        let call_id = match &event.msg {
            EventMsg::DynamicToolCallRequest(req) => {
                assert_eq!(req.tool, "echo");
                assert_eq!(req.turn_id, "turn_1");
                req.call_id.clone()
            }
            other => panic!("expected DynamicToolCallRequest, got: {other:?}"),
        };

        // Resolve the call
        let response = DynamicToolResponse {
            content_items: vec![],
            success: true,
        };
        handler.resolve_call(&call_id, response).await.unwrap();

        // The invoke should complete successfully
        let result = invoke_handle.await.unwrap().unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn resolve_unknown_call_id_returns_error() {
        let (handler, _rx) = make_handler();
        let result = handler
            .resolve_call(
                "nonexistent",
                DynamicToolResponse {
                    content_items: vec![],
                    success: true,
                },
            )
            .await;
        assert!(result.is_err());
    }
}
