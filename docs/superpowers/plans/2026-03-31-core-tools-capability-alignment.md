# Core Tools Capability Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align Mosaic's `core/tools` runtime with `codex-main` at the level of real executable capability, not just tool spec presence, with full backend, frontend, and end-to-end verification.

**Architecture:** Start by introducing a per-invocation tool runtime context so handlers can see the active turn, event sender, MCP/session services, and collaboration mode. Build all missing high-value capabilities on top of that context in dependency order: MCP delegation first, then user interaction and approvals, then MCP-adjacent utilities (`mcp_resource`, `search_tool_bm25`), then higher-level tools (`js_repl`, `update_plan`, `presentation_artifact`), and finally tighten spec/router/runtime parity and docs.

**Tech Stack:** Rust, Tokio, Tauri, Serde, existing `src-tauri/src/core/tools/*` runtime, MCP client manager, React, TypeScript, Zustand, Vitest, Playwright

---

## File Structure

### Core runtime files

- Modify: `src-tauri/src/core/tools/context.rs`
  - Add per-call runtime context shared by router and handlers.
- Modify: `src-tauri/src/core/tools/mod.rs`
  - Change `ToolHandler` / `ToolRegistry` dispatch signatures to accept invocation context.
- Modify: `src-tauri/src/core/tools/router.rs`
  - Pass invocation context through builtin, MCP, and dynamic routes.
- Modify: `src-tauri/src/core/session.rs`
  - Build router/runtime state with turn-aware context and keep default tool exposure stable.
- Modify: `src-tauri/src/core/codex.rs`
  - Pass turn-scoped context into router calls and handle new submission flows.
- Modify: `src-tauri/src/core/mcp_server.rs`
  - Update MCP-facing router call sites to the new routing contract.

### MCP and tool capability files

- Modify: `src-tauri/src/core/tools/handlers/mcp.rs`
  - Replace placeholder behavior with real delegation helper logic.
- Modify: `src-tauri/src/core/mcp_client/connection_manager.rs`
  - Add MCP resource accessors and any metadata helpers needed by BM25 search.
- Modify: `src-tauri/src/core/tools/handlers/mcp_resource.rs`
  - Wire list/read operations to the MCP connection manager.
- Modify: `src-tauri/src/core/tools/handlers/search_tool_bm25.rs`
  - Build a real searchable index from active MCP tools.

### User interaction, approval, and UI files

- Modify: `src-tauri/src/core/tools/handlers/request_user_input.rs`
  - Emit events and wait for `Op::UserInputAnswer`.
- Modify: `src-tauri/src/core/state/turn.rs`
  - Reuse existing pending-user-input state for request/response completion.
- Modify: `src-tauri/src/protocol/event.rs`
  - Reuse and/or extend event payloads as needed for plan and approval rendering.
- Modify: `src-tauri/src/protocol/submission.rs`
  - Keep `UserInputAnswer` contract as the backend completion path.
- Modify: `src-tauri/src/core/tools/network_approval.rs`
  - Replace stub service with real approval lifecycle.
- Modify: `src-tauri/src/exec/sandbox.rs`
  - Attach network approval context to exec approval requests and honor decisions.
- Modify: `src-tauri/src/core/network_policy_decision.rs`
  - Convert proxy/network payloads into approval and policy amendment data.
- Modify: `src/hooks/useCodexEvent.ts`
  - Consume `request_user_input`, `plan_update`, and richer approval events.
- Modify: `src/types/events.ts`
  - Add missing `Op` variants and event payload typing needed by the UI.
- Modify: `src/types/chat.ts`
  - Extend clarification and approval state for structured choices and network context.
- Modify: `src/stores/clarificationStore.ts`
  - Track active clarification requests and resolution state.
- Modify: `src/stores/approvalStore.ts`
  - Track richer approval payloads for exec/network requests.
- Modify: `src/components/chat/agent/ClarificationCard.tsx`
  - Submit structured `user_input_answer` responses from the UI.
- Modify: `src/components/chat/agent/ApprovalRequestCard.tsx`
  - Surface network approval host/protocol details and decision actions.

### Higher-level tool files

- Modify: `src-tauri/src/core/tools/js_repl/mod.rs`
  - Add persistent REPL runtime wrapper instead of re-exporting handler stubs.
- Modify: `src-tauri/src/core/tools/handlers/js_repl.rs`
  - Delegate to the persistent runtime and emit shell-like events.
- Modify: `src-tauri/src/core/tools/handlers/plan.rs`
  - Emit real `PlanUpdate` events and enforce mode constraints with turn context.
- Modify: `src-tauri/src/core/tools/handlers/presentation_artifact.rs`
  - Implement file-backed artifact operations with sandbox-aware path checks.
- Modify: `src-tauri/src/core/tools/spec.rs`
  - Keep tool exposure and descriptions aligned with newly completed runtime behavior.
- Modify: `docs/core-tools-codex-main-vs-mosaic-analysis.md`
  - Update capability status after implementation lands.

### Test files

- Modify: `src-tauri/tests/tool_handler_tests.rs`
  - Add integration coverage for router, MCP delegation, `mcp_resource`, BM25, and request/approval flows.
- Modify: `src/__tests__/unit/hooks/useCodexEvent.test.ts`
  - Cover `request_user_input`, `plan_update`, and enriched approval events.
- Modify: `src/__tests__/unit/components/streaming/StreamingTurnRoot.test.tsx`
  - Cover clarification and approval rendering.
- Add: `e2e/tests/core-tools-capability-alignment.spec.ts`
  - Verify UI round trips for clarification and approval flows.

## Dependency Order

1. `ToolInvocationContext` plumbing must land before any handler can use turn state, session services, or event emission.
2. MCP delegation must land before `mcp_resource` and `search_tool_bm25`, because both depend on real MCP runtime data.
3. `request_user_input` and network approval both depend on turn-scoped waiters plus frontend submission flow.
4. `js_repl`, `update_plan`, and `presentation_artifact` can be implemented after the runtime context exists, but they should not block MCP/user-interaction parity.

## Task 1: Add per-invocation tool runtime context

**Files:**
- Modify: `src-tauri/src/core/tools/context.rs`
- Modify: `src-tauri/src/core/tools/mod.rs`
- Modify: `src-tauri/src/core/tools/router.rs`
- Modify: `src-tauri/src/core/session.rs`
- Modify: `src-tauri/src/core/codex.rs`
- Modify: `src-tauri/src/core/mcp_server.rs`
- Test: `src-tauri/tests/tool_handler_tests.rs`

- [x] **Step 1: Write a failing integration test that proves handlers need turn context**

```rust
#[tokio::test]
async fn t07_router_passes_turn_context_to_builtin_handlers() {
    let seen = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));

    struct RecordingHandler {
        seen: std::sync::Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl ToolHandler for RecordingHandler {
        fn matches_kind(&self, kind: &ToolKind) -> bool {
            matches!(kind, ToolKind::Builtin(name) if name == "record")
        }

        fn kind(&self) -> ToolKind {
            ToolKind::Builtin("record".to_string())
        }

        async fn handle(
            &self,
            ctx: ToolInvocationContext,
            _args: serde_json::Value,
        ) -> Result<serde_json::Value, CodexError> {
            self.seen
                .lock()
                .await
                .push(format!("{}:{}", ctx.turn_id, ctx.mode.display_name()));
            Ok(serde_json::json!({"ok": true}))
        }
    }

    // Register handler, route once, then assert captured turn metadata.
}
```

- [x] **Step 2: Run the targeted Rust test and confirm the current signature blocks it**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t07_router_passes_turn_context_to_builtin_handlers -- --exact
```

Expected:

- FAIL because `ToolHandler::handle(...)` and `ToolRegistry::dispatch(...)` do not yet accept a per-call context.

- [x] **Step 3: Introduce `ToolInvocationContext` and thread it through registry/router/callers**

Add a turn-aware runtime context in `src-tauri/src/core/tools/context.rs`:

```rust
#[derive(Clone)]
pub struct ToolInvocationContext {
    pub turn_id: String,
    pub cwd: std::path::PathBuf,
    pub mode: crate::protocol::types::ModeKind,
    pub tx_event: async_channel::Sender<crate::protocol::event::Event>,
    pub active_turn: std::sync::Arc<tokio::sync::Mutex<crate::core::state::turn::TurnState>>,
    pub mcp_manager: std::sync::Arc<crate::core::mcp_client::McpConnectionManager>,
    pub network_approval: std::sync::Arc<crate::core::tools::network_approval::NetworkApprovalService>,
}
```

Update the runtime contract in `src-tauri/src/core/tools/mod.rs` and `src-tauri/src/core/tools/router.rs`:

```rust
async fn handle(
    &self,
    ctx: ToolInvocationContext,
    args: serde_json::Value,
) -> Result<serde_json::Value, CodexError>;

pub async fn dispatch(
    &self,
    kind: &ToolKind,
    ctx: ToolInvocationContext,
    args: serde_json::Value,
) -> Result<serde_json::Value, CodexError>;

pub async fn route_tool_call(
    &self,
    ctx: ToolInvocationContext,
    tool_name: &str,
    args: serde_json::Value,
) -> RouteResult;
```

- [x] **Step 4: Update all router call sites to construct real invocation context**

Call the router with active turn data from `src-tauri/src/core/codex.rs` and `src-tauri/src/core/mcp_server.rs`:

```rust
let tool_ctx = ToolInvocationContext {
    turn_id: turn_id.to_string(),
    cwd: session.cwd().to_path_buf(),
    mode: mode_kind,
    tx_event: self.tx_event.clone(),
    active_turn: active_turn.turn_state.clone(),
    mcp_manager: session.services().mcp_manager.clone(),
    network_approval: session.services().network_approval.clone(),
};

router.route_tool_call(tool_ctx, tool_name, arguments.clone()).await
```

- [x] **Step 5: Re-run the targeted test and the existing router suite**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t07_router_passes_turn_context_to_builtin_handlers -- --exact
cargo test --manifest-path src-tauri/Cargo.toml tool_handler_tests
cargo test --manifest-path src-tauri/Cargo.toml core::tools::router::
```

Expected:

- PASS for the new context test.
- Existing router/tool handler tests still pass after the signature migration.

## Task 2: Replace MCP placeholder routing with real delegation

**Files:**
- Modify: `src-tauri/src/core/tools/router.rs`
- Modify: `src-tauri/src/core/tools/handlers/mcp.rs`
- Modify: `src-tauri/src/core/codex.rs`
- Test: `src-tauri/tests/tool_handler_tests.rs`

- [x] **Step 1: Add a failing test for MCP-qualified tool execution**

```rust
#[tokio::test]
async fn t08_router_delegates_mcp_qualified_tool_calls() {
    let router = make_router_with_fake_mcp_tool("filesystem", "read_file");

    let result = router
        .route_tool_call(
            fake_tool_context("turn-mcp"),
            "mcp__filesystem__read_file",
            serde_json::json!({"path": "/tmp/demo.txt"}),
        )
        .await;

    match result {
        RouteResult::Handled(Ok(value)) => {
            assert_eq!(value["server"], "filesystem");
            assert_eq!(value["tool"], "read_file");
        }
        other => panic!("expected MCP delegation success, got: {other:?}"),
    }
}
```

- [x] **Step 2: Run the targeted test and confirm the current stub error**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t08_router_delegates_mcp_qualified_tool_calls -- --exact
```

Expected:

- FAIL because the router still returns `call delegation is not yet implemented`.

- [x] **Step 3: Route MCP calls through the connection manager and emit MCP begin/end events**

Implement the real path in `src-tauri/src/core/tools/router.rs`:

```rust
if let Some((server, tool)) = parse_mcp_tool_name(tool_name) {
    let call_id = uuid::Uuid::new_v4().to_string();
    let invocation = crate::protocol::types::McpInvocation {
        server: server.to_string(),
        tool: tool.to_string(),
        arguments: args.clone(),
    };

    let _ = ctx.tx_event.send(Event {
        id: uuid::Uuid::new_v4().to_string(),
        msg: EventMsg::McpToolCallBegin(McpToolCallBeginEvent {
            call_id: call_id.clone(),
            invocation: invocation.clone(),
        }),
    }).await;

    let started = std::time::Instant::now();
    let result = ctx.mcp_manager.call_tool(server, tool, args).await;

    let _ = ctx.tx_event.send(Event {
        id: uuid::Uuid::new_v4().to_string(),
        msg: EventMsg::McpToolCallEnd(McpToolCallEndEvent {
            call_id,
            invocation,
            duration: started.elapsed(),
            result: result.clone().map(|value| CallToolResult {
                content: vec![],
                structured_content: Some(value),
            }).map_err(|err| err.message),
        }),
    }).await;

    return RouteResult::Handled(result);
}
```

- [x] **Step 4: Keep builtin-over-MCP priority and dynamic-tool behavior unchanged**

Add or update regression tests so these cases stay true:

```rust
assert!(matches!(router.route_tool_call(ctx.clone(), "read_file", json!({})).await, RouteResult::Handled(_)));
assert!(matches!(router.route_tool_call(ctx, "my_dynamic_tool", json!({})).await, RouteResult::DynamicTool(_)));
```

- [x] **Step 5: Re-run MCP and router coverage**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t08_router_delegates_mcp_qualified_tool_calls -- --exact
cargo test --manifest-path src-tauri/Cargo.toml core::tools::router::
cargo test --manifest-path src-tauri/Cargo.toml tool_handler_tests
```

Expected:

- PASS for MCP execution and existing routing precedence tests.

## Task 3: Complete `request_user_input` end-to-end round trip

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/request_user_input.rs`
- Modify: `src-tauri/src/core/state/turn.rs`
- Modify: `src-tauri/src/core/codex.rs`
- Modify: `src-tauri/src/protocol/event.rs`
- Modify: `src-tauri/src/protocol/submission.rs`
- Modify: `src/hooks/useCodexEvent.ts`
- Modify: `src/types/events.ts`
- Modify: `src/types/chat.ts`
- Modify: `src/stores/clarificationStore.ts`
- Modify: `src/components/chat/agent/ClarificationCard.tsx`
- Test: `src-tauri/tests/tool_handler_tests.rs`
- Test: `src/__tests__/unit/hooks/useCodexEvent.test.ts`
- Test: `src/__tests__/unit/components/streaming/StreamingTurnRoot.test.tsx`
- Test: `e2e/tests/core-tools-capability-alignment.spec.ts`

- [x] **Step 1: Add failing backend and frontend tests for the clarification flow**

Rust integration target:

```rust
#[tokio::test]
async fn t09_request_user_input_emits_event_and_waits_for_answer() {
    // 1. invoke request_user_input
    // 2. assert EventMsg::RequestUserInput is emitted
    // 3. resolve with Op::UserInputAnswer { id, response }
    // 4. assert handler returns the submitted JSON response
}
```

Vitest target:

```ts
it('stores clarification requests from request_user_input events', () => {
  renderHook(() => useCodexEvent());
  emit('t1', {
    type: 'request_user_input',
    id: 'clarify-1',
    message: 'Choose one',
    schema: { questions: [{ text: 'Choose one', options: ['A', 'B'] }] },
  });

  expect(useClarificationStore.getState().requests.get('clarify-1')?.message).toBe('Choose one');
});
```

- [x] **Step 2: Run the targeted tests and confirm current behavior fails or is incomplete**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t09_request_user_input_emits_event_and_waits_for_answer -- --exact
pnpm vitest run src/__tests__/unit/hooks/useCodexEvent.test.ts src/__tests__/unit/components/streaming/StreamingTurnRoot.test.tsx
```

Expected:

- Rust test fails because the handler always returns the UI-integration stub error.
- Frontend test fails once it tries to submit/resolve an answer because `Op::user_input_answer` is not typed or sent.

- [x] **Step 3: Emit `RequestUserInputEvent`, register a pending waiter, and resolve from `Op::UserInputAnswer`**

Implement the backend contract:

```rust
let request_id = uuid::Uuid::new_v4().to_string();
let schema = serde_json::json!({ "questions": params.questions });
let (tx, rx) = tokio::sync::oneshot::channel();

ctx.active_turn
    .lock()
    .await
    .insert_pending_user_input(request_id.clone(), tx);

ctx.tx_event.send(Event {
    id: uuid::Uuid::new_v4().to_string(),
    msg: EventMsg::RequestUserInput(RequestUserInputEvent {
        id: request_id.clone(),
        message: "Additional user input required".to_string(),
        schema: Some(schema),
    }),
}).await?;

let response = rx.await.map_err(|_| CodexError::new(
    ErrorCode::ToolExecutionFailed,
    "request_user_input was cancelled before the UI responded",
))?;

Ok(response)
```

Handle submission in `src-tauri/src/core/codex.rs`:

```rust
Op::UserInputAnswer { id, response } => {
    if let Some(active_turn) = self.active_turn().await {
        let waiter = active_turn.turn_state.lock().await.remove_pending_user_input(&id);
        if let Some(tx) = waiter {
            let _ = tx.send(response);
        }
    }
}
```

- [x] **Step 4: Wire the UI to submit structured answers**

Extend the frontend types:

```ts
export type Op =
  | { type: 'user_input_answer'; id: string; response: unknown }
  | // existing variants...
```

Submit from `ClarificationCard.tsx` via `useSubmitOp()`:

```ts
await submitOp(threadId, {
  type: 'user_input_answer',
  id: request.id,
  response: {
    answers: selectedOptions,
    freeform_text: otherText || null,
  },
});
```

- [x] **Step 5: Re-run unit and end-to-end verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t09_request_user_input_emits_event_and_waits_for_answer -- --exact
pnpm vitest run src/__tests__/unit/hooks/useCodexEvent.test.ts src/__tests__/unit/components/streaming/StreamingTurnRoot.test.tsx
pnpm test:e2e
```

Expected:

- Rust flow passes with event emission plus answer resolution.
- UI tests pass for clarification rendering/submission.
- E2E covers at least one full clarification round trip.

## Task 4: Implement real network approval flow

**Files:**
- Modify: `src-tauri/src/core/tools/network_approval.rs`
- Modify: `src-tauri/src/exec/sandbox.rs`
- Modify: `src-tauri/src/core/network_policy_decision.rs`
- Modify: `src/hooks/useCodexEvent.ts`
- Modify: `src/types/events.ts`
- Modify: `src/types/chat.ts`
- Modify: `src/stores/approvalStore.ts`
- Modify: `src/components/chat/agent/ApprovalRequestCard.tsx`
- Test: `src-tauri/tests/tool_handler_tests.rs`
- Test: `src/__tests__/unit/hooks/useCodexEvent.test.ts`

- [x] **Step 1: Add a failing test for approval requests that carry network context**

```rust
#[tokio::test]
async fn t10_exec_approval_request_includes_network_context_when_proxy_blocks_host() {
    // Arrange a blocked request payload for api.openai.com over HTTPS.
    // Execute the command path.
    // Assert ExecApprovalRequestEvent.network_approval_context == Some(...)
}
```

- [ ] **Step 2: Run the targeted test and confirm the service is still a no-op**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t10_exec_approval_request_includes_network_context_when_proxy_blocks_host -- --exact
```

Expected:

- FAIL because `NetworkApprovalService::begin(...)` always returns `None`.

- [x] **Step 3: Replace the stub service with pending-approval registration and finalization**

Implement a real service in `src-tauri/src/core/tools/network_approval.rs`:

```rust
pub struct NetworkApprovalService {
    next_registration_id: std::sync::atomic::AtomicU64,
    pending: tokio::sync::Mutex<std::collections::HashMap<String, DeferredNetworkApproval>>,
}

pub async fn begin(&self, spec: &NetworkApprovalSpec) -> Option<DeferredNetworkApproval> {
    if spec.hosts.is_empty() {
        return None;
    }
    let id = format!(
        "net_{}",
        self.next_registration_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let approval = DeferredNetworkApproval {
        registration_id: id.clone(),
        hosts: spec.hosts.clone(),
    };
    self.pending.lock().await.insert(id, approval.clone());
    Some(approval)
}
```

- [x] **Step 4: Attach network context to approval events and surface it in the UI**

Populate the approval event from `src-tauri/src/exec/sandbox.rs` and render it:

```rust
network_approval_context: Some(NetworkApprovalContext {
    host: blocked_host.to_string(),
    protocol: NetworkApprovalProtocol::Https,
}),
proposed_network_policy_amendments: Some(vec![NetworkPolicyAmendment {
    action: NetworkPolicyRuleAction::Allow,
    host: blocked_host.to_string(),
}]),
```

```ts
addApproval({
  callId: msg.call_id,
  turnId: msg.turn_id,
  type: 'exec',
  networkContext: msg.network_approval_context,
  proposedNetworkPolicyAmendments: msg.proposed_network_policy_amendments,
  command: msg.command,
  cwd: msg.cwd,
  reason: msg.reason,
});
```

- [x] **Step 5: Re-run approval coverage**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t10_exec_approval_request_includes_network_context_when_proxy_blocks_host -- --exact
pnpm vitest run src/__tests__/unit/hooks/useCodexEvent.test.ts
```

Expected:

- Backend approval event now carries host/protocol data.
- Frontend store/UI tests pass with the richer payload.

## Task 5: Finish `mcp_resource` and `search_tool_bm25`

**Files:**
- Modify: `src-tauri/src/core/mcp_client/connection_manager.rs`
- Modify: `src-tauri/src/core/tools/handlers/mcp_resource.rs`
- Modify: `src-tauri/src/core/tools/handlers/search_tool_bm25.rs`
- Test: `src-tauri/tests/tool_handler_tests.rs`

- [x] **Step 1: Add failing integration tests for MCP resource listing/reading and BM25 search**

```rust
#[tokio::test]
async fn t11_list_mcp_resources_aggregates_resources_by_server() {
    // Arrange two fake MCP servers with resources.
    // Assert the handler returns a stable, sorted aggregate payload.
}

#[tokio::test]
async fn t12_search_tool_bm25_returns_ranked_tool_matches() {
    // Seed active MCP tools.
    // Search for a keyword and assert ranked results contain the expected server/tool.
}
```

- [ ] **Step 2: Run the targeted tests and confirm current handlers return empty/stub payloads**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t11_list_mcp_resources_aggregates_resources_by_server -- --exact
cargo test --manifest-path src-tauri/Cargo.toml t12_search_tool_bm25_returns_ranked_tool_matches -- --exact
```

Expected:

- `mcp_resource` tests fail because list/read still return empty/stub results.
- BM25 test fails because the handler always reports `total_tools = 0`.

- [x] **Step 3: Add resource accessors to the MCP connection manager**

Expose real helpers in `src-tauri/src/core/mcp_client/connection_manager.rs`:

```rust
pub async fn list_resources(
    &self,
    server: &str,
    cursor: Option<String>,
) -> Result<ListResourcesResult, CodexError>;

pub async fn list_resource_templates(
    &self,
    server: &str,
    cursor: Option<String>,
) -> Result<ListResourceTemplatesResult, CodexError>;

pub async fn read_resource(
    &self,
    server: &str,
    uri: &str,
) -> Result<serde_json::Value, CodexError>;
```

- [x] **Step 4: Wire both handlers to active MCP data**

Implement real handler logic:

```rust
let resources = if let Some(server) = params.server.as_deref() {
    let result = ctx.mcp_manager.list_resources(server, params.cursor.clone()).await?;
    ListResourcesPayload::from_single_server(server.to_string(), map_resources(server, result.resources), result.next_cursor)
} else {
    let mut by_server = std::collections::HashMap::new();
    for server in ctx.mcp_manager.connected_servers().await {
        let result = ctx.mcp_manager.list_resources(&server, None).await?;
        by_server.insert(server.clone(), map_resources(&server, result.resources));
    }
    ListResourcesPayload::from_all_servers(by_server)
};
```

```rust
let docs: Vec<String> = ctx
    .mcp_manager
    .all_tools()
    .await
    .into_iter()
    .map(|tool| ToolEntry::build_search_text(
        &tool.name,
        &tool.server_name,
        tool.title.as_deref(),
        tool.description.as_deref(),
        &tool.input_keys,
    ))
    .collect();
```

- [x] **Step 5: Re-run MCP utility verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t11_list_mcp_resources_aggregates_resources_by_server -- --exact
cargo test --manifest-path src-tauri/Cargo.toml t12_search_tool_bm25_returns_ranked_tool_matches -- --exact
cargo test --manifest-path src-tauri/Cargo.toml tool_handler_tests
```

Expected:

- `mcp_resource` returns real data.
- BM25 returns ranked active MCP tool matches instead of the stub envelope.

## Task 6: Implement persistent `js_repl`

**Files:**
- Modify: `src-tauri/src/core/tools/js_repl/mod.rs`
- Modify: `src-tauri/src/core/tools/handlers/js_repl.rs`
- Test: `src-tauri/tests/tool_handler_tests.rs`

- [ ] **Step 1: Add failing tests for REPL persistence and reset**

```rust
#[tokio::test]
async fn t13_js_repl_persists_state_between_calls() {
    // First call: globalThis.answer = 41;
    // Second call: globalThis.answer + 1;
    // Expect 42.
}

#[tokio::test]
async fn t14_js_repl_reset_clears_persistent_state() {
    // Seed a global, reset, then assert the next call no longer sees it.
}
```

- [ ] **Step 2: Run the targeted tests and confirm the current handler still returns the runtime-missing error**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t13_js_repl_persists_state_between_calls -- --exact
cargo test --manifest-path src-tauri/Cargo.toml t14_js_repl_reset_clears_persistent_state -- --exact
```

Expected:

- FAIL because both handlers still return the REPL runtime stub error.

- [ ] **Step 3: Add a persistent child-process runtime in `src-tauri/src/core/tools/js_repl/mod.rs`**

Create a small manager that owns one Node.js child process per session:

```rust
pub struct JsReplRuntime {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
}

impl JsReplRuntime {
    pub async fn execute(&mut self, code: &str, timeout_ms: Option<u64>) -> Result<serde_json::Value, CodexError> {
        // send request JSON, await a framed response, preserve process state
    }

    pub async fn reset(&mut self) -> Result<(), CodexError> {
        // kill child and spawn a clean one
    }
}
```

- [ ] **Step 4: Emit shell-like begin/end events and preserve pragma parsing**

Use the new runtime from `src-tauri/src/core/tools/handlers/js_repl.rs`:

```rust
emit_js_repl_exec_begin(&ctx, &call_id).await;
let result = runtime.execute(&clean_code, params.timeout_ms).await?;
emit_js_repl_exec_end(&ctx, &call_id, &result.stdout, result.stderr.as_deref(), elapsed).await;
Ok(build_js_repl_exec_output(&result.stdout, result.stderr.as_deref(), elapsed))
```

- [ ] **Step 5: Re-run REPL verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t13_js_repl_persists_state_between_calls -- --exact
cargo test --manifest-path src-tauri/Cargo.toml t14_js_repl_reset_clears_persistent_state -- --exact
```

Expected:

- PASS for persistence and reset semantics.

## Task 7: Make `update_plan` produce real UI-visible plan updates

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/plan.rs`
- Modify: `src/hooks/useCodexEvent.ts`
- Modify: `src/types/events.ts`
- Modify: `src/stores/messageStore.ts`
- Test: `src-tauri/tests/tool_handler_tests.rs`
- Test: `src/__tests__/unit/hooks/useCodexEvent.test.ts`

- [ ] **Step 1: Add failing backend/frontend tests for `plan_update`**

```rust
#[tokio::test]
async fn t15_update_plan_emits_plan_update_event() {
    // Invoke update_plan, then assert EventMsg::PlanUpdate carries the submitted JSON.
}
```

```ts
it('applies plan_update events to the streaming plan view', () => {
  renderHook(() => useCodexEvent());
  emit('t1', { type: 'plan_update', plan: [{ step: 'Add runtime context', status: 'completed' }] });
  // assert the plan is visible in messageStore / streaming view
});
```

- [ ] **Step 2: Run the targeted tests and confirm the current handler only returns `{status: "Plan updated"}`**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t15_update_plan_emits_plan_update_event -- --exact
pnpm vitest run src/__tests__/unit/hooks/useCodexEvent.test.ts
```

Expected:

- FAIL because no event is emitted and the frontend ignores `plan_update`.

- [ ] **Step 3: Emit `EventMsg::PlanUpdate` from the handler**

```rust
ctx.tx_event.send(Event {
    id: uuid::Uuid::new_v4().to_string(),
    msg: EventMsg::PlanUpdate(PlanUpdateEvent {
        plan: serde_json::json!({
            "explanation": params.explanation,
            "plan": params.plan,
        }),
    }),
}).await?;
```

- [ ] **Step 4: Render the plan update in the frontend**

Handle it in `useCodexEvent.ts` and `messageStore.ts`:

```ts
case 'plan_update':
  useMessageStore.getState().replacePlanFromEvent(msg.plan);
  break;
```

```ts
replacePlanFromEvent: (plan) =>
  set((state) => ({
    streamingView: state.streamingView
      ? materializePlanEvent(state.streamingView, plan)
      : state.streamingView,
  })),
```

- [ ] **Step 5: Re-run plan update verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t15_update_plan_emits_plan_update_event -- --exact
pnpm vitest run src/__tests__/unit/hooks/useCodexEvent.test.ts
```

Expected:

- Backend emits the event.
- Frontend renders the latest plan state.

## Task 8: Implement file-backed `presentation_artifact`

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/presentation_artifact.rs`
- Test: `src-tauri/tests/tool_handler_tests.rs`

- [ ] **Step 1: Add failing tests for artifact read/write/list within cwd**

```rust
#[tokio::test]
async fn t16_presentation_artifact_can_write_and_read_within_cwd() {
    // write content, read it back, assert sandbox-safe file access
}

#[tokio::test]
async fn t17_presentation_artifact_rejects_parent_traversal() {
    // action=write, path=../escape.txt -> expect error
}
```

- [ ] **Step 2: Run the targeted tests and confirm execution still stops at the stub error**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t16_presentation_artifact_can_write_and_read_within_cwd -- --exact
cargo test --manifest-path src-tauri/Cargo.toml t17_presentation_artifact_rejects_parent_traversal -- --exact
```

Expected:

- FAIL because the handler still returns `presentation_artifact execution requires the artifact subsystem`.

- [ ] **Step 3: Implement a minimal file-backed artifact executor**

Support the actions already implied by the handler input:

```rust
match params.action.as_deref().unwrap_or("write") {
    "write" | "create" | "update" => {
        std::fs::write(&resolved_path, params.content.unwrap_or_default())?;
        Ok(json!({ "path": resolved_path, "action": "write" }))
    }
    "read" => {
        let content = std::fs::read_to_string(&resolved_path)?;
        Ok(json!({ "path": resolved_path, "content": content }))
    }
    "list" => {
        let entries = std::fs::read_dir(&resolved_path)?
            .filter_map(Result::ok)
            .map(|entry| entry.path().display().to_string())
            .collect::<Vec<_>>();
        Ok(json!({ "path": resolved_path, "entries": entries }))
    }
    _ => Err(CodexError::new(ErrorCode::InvalidInput, "unsupported presentation_artifact action")),
}
```

- [ ] **Step 4: Keep sandbox/path checks strict**

Before any read/write:

```rust
authorize_path_access(path, access_kind)?;
let resolved_path = effective_path(std::path::Path::new(path), access_kind);
if !resolved_path.starts_with(std::env::current_dir()?) {
    return Err(CodexError::new(ErrorCode::ToolExecutionFailed, "artifact path is outside cwd"));
}
```

- [ ] **Step 5: Re-run artifact verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml t16_presentation_artifact_can_write_and_read_within_cwd -- --exact
cargo test --manifest-path src-tauri/Cargo.toml t17_presentation_artifact_rejects_parent_traversal -- --exact
```

Expected:

- Safe in-cwd artifact operations pass.
- Traversal and out-of-root paths are rejected.

## Task 9: Final spec/router parity, docs sync, and full regression run

**Files:**
- Modify: `src-tauri/src/core/tools/spec.rs`
- Modify: `src-tauri/src/core/session.rs`
- Modify: `docs/core-tools-codex-main-vs-mosaic-analysis.md`
- Test: `src-tauri/tests/tool_handler_tests.rs`
- Test: `src/__tests__/unit/hooks/useCodexEvent.test.ts`
- Test: `e2e/tests/core-tools-capability-alignment.spec.ts`

- [ ] **Step 1: Add failing coverage that the completed capabilities are actually exposed when enabled**

```rust
#[test]
fn build_specs_exposes_completed_optional_tools_when_configured() {
    let assembled = build_specs(
        &ToolsConfig {
            request_user_input_enabled: true,
            mcp_resources_enabled: true,
            search_tool: true,
            presentation_artifact: true,
            js_repl_enabled: true,
            update_plan_enabled: true,
            ..ToolsConfig::default()
        },
        true,
    );

    let names: Vec<_> = assembled.configured_specs.iter().map(|spec| spec.spec.name().to_string()).collect();
    assert!(names.contains(&"request_user_input".to_string()));
    assert!(names.contains(&"list_mcp_resources".to_string()));
    assert!(names.contains(&"search_tool_bm25".to_string()));
    assert!(names.contains(&"presentation_artifact".to_string()));
    assert!(names.contains(&"js_repl".to_string()));
    assert!(names.contains(&"update_plan".to_string()));
}
```

- [ ] **Step 2: Reconcile tool descriptions/default config with the completed runtime**

Update `src-tauri/src/core/tools/spec.rs` and `src-tauri/src/core/session.rs` so that:

```rust
let config = ToolsConfig {
    request_user_input_enabled: resolved.default_mode_request_user_input,
    mcp_resources_enabled: resolved.enable_mcp_resources,
    search_tool: resolved.enable_search_tool,
    presentation_artifact: resolved.enable_artifact,
    js_repl_enabled: resolved.enable_js_repl,
    update_plan_enabled: resolved.enable_update_plan,
    ..ToolsConfig::default()
};
```

- [ ] **Step 3: Update the analysis doc from “stub/half-wired” to the new runtime status**

Refresh `docs/core-tools-codex-main-vs-mosaic-analysis.md` so the status lines explicitly reflect:

```md
- MCP delegation: completed
- request_user_input: completed end-to-end
- network approval: completed for exec approval flow
- mcp_resource / search_tool_bm25: completed against active MCP connections
- js_repl / update_plan / presentation_artifact: completed with Mosaic's current runtime scope
```

- [ ] **Step 4: Run full regression verification**

Run:

```bash
pnpm typecheck
pnpm vitest run
pnpm test:e2e
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected:

- All TypeScript checks pass.
- All unit tests pass.
- Playwright passes.
- Full Rust suite passes.

- [ ] **Step 5: Create a final integration commit**

Run:

```bash
git add docs/core-tools-codex-main-vs-mosaic-analysis.md \
  docs/superpowers/plans/2026-03-31-core-tools-capability-alignment.md \
  src-tauri/src/core/tools \
  src-tauri/src/core/mcp_client/connection_manager.rs \
  src-tauri/src/core/network_policy_decision.rs \
  src-tauri/src/core/codex.rs \
  src-tauri/src/core/mcp_server.rs \
  src-tauri/src/core/session.rs \
  src-tauri/src/core/state/turn.rs \
  src-tauri/src/exec/sandbox.rs \
  src-tauri/src/protocol \
  src \
  e2e/tests/core-tools-capability-alignment.spec.ts
git commit -m "feat: align core tools runtime with codex main capabilities"
```

Expected:

- One integration commit that includes the completed runtime, frontend wiring, tests, and updated analysis docs.
