# JsRepl Port And Tool Contract Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port `codex-main`'s full `js_repl` runtime into Mosaic, then align Mosaic's tool invocation contracts enough to preserve the imported functionality, including `codex.tool(...)`, without regressing existing built-in, MCP, or dynamic tool behavior.

**Architecture:** Implement this in two phases. Phase 1 copies the `js_repl` kernel assets, runtime manager, parser behavior, and test expectations from `codex-main`, then adapts only the minimum Rust seams needed for Mosaic to compile and execute it. Phase 2 shrinks the remaining runtime interface gaps by extending Mosaic's tool context/output model so nested `codex.tool(...)` calls can flow through the existing `ToolRouter`, `DynamicToolHandler`, and turn-scoped state without losing output fidelity.

**Tech Stack:** Rust, Tokio, Tauri, Serde, Node.js, inline Rust tests, `src-tauri/tests/tool_handler_tests.rs`

---

## Execution Status (2026-04-01)

- [x] 已复制 `codex-main` 的 `kernel.js` 与 `meriyah.umd.min.js`
- [x] 已将 `js_repl` handler 输入语义切到 freeform-first，并支持 `// codex-js-repl:` pragma
- [x] 已接通 persistent Node kernel、top-level await、`codex.tool(...)`
- [x] 已接通 builtin nested tool 调用
- [x] 已接通 dynamic nested tool 调用
- [x] 已修复 runtime 作用域，按 `turn_id + cwd` 隔离 kernel，避免跨 turn/测试串扰
- [x] 已修复 `js_repl` 错误路径 deadlock：失败后 reset 不再在持锁状态下重入同一 runtime
- [x] 已更新 `project_doc` / `tool spec` 中的 `js_repl` 文案
- [x] 已补并跑通 handler 级解析测试、`tool_handler_tests` 中的 `js_repl` 集成测试
- [x] 已补递归自调用拒绝测试
- [x] 已补 timeout 后 reset/recovery 测试
- [x] 已整理剩余与 `codex-main` 的接口差异清单

## File Structure

### Modified files

- `src-tauri/src/core/tools/handlers/js_repl.rs`
  - Replace placeholder/freeform parsing and event logic with `codex-main`-compatible handler behavior adapted to Mosaic.
- `src-tauri/src/core/tools/js_repl/mod.rs`
  - Replace the current lightweight VM bridge with the full persistent kernel manager and nested tool-call bridge.
- `src-tauri/src/core/tools/context.rs`
  - Extend invocation/output modeling so nested tool calls invoked from `js_repl` can be represented without lossy ad hoc JSON.
- `src-tauri/src/core/tools/mod.rs`
  - Adjust registry/handler contracts only where required by the imported runtime.
- `src-tauri/src/core/tools/router.rs`
  - Keep built-in, MCP, and dynamic routing compatible with nested `js_repl` tool calls.
- `src-tauri/src/core/tools/handlers/dynamic.rs`
  - Reuse the existing dynamic tool lifecycle from nested calls and fill any missing request/response glue.
- `src-tauri/src/core/codex.rs`
  - Reuse existing turn-scoped tool invocation entry points from `js_repl` nested calls and preserve event emission.
- `src-tauri/src/core/project_doc.rs`
  - Ensure `js_repl` documentation matches the actual imported contract.
- `src-tauri/src/core/tools/spec.rs`
  - Remove placeholder wording once the runtime is real.
- `src-tauri/Cargo.toml`
  - Add any crates required by the imported runtime.
- `src-tauri/Cargo.lock`
  - Lock added dependencies.

### Added files

- `src-tauri/src/core/tools/js_repl/kernel.js`
  - Imported Node kernel source from `codex-main`, with only minimal path/protocol adjustments required by Mosaic.
- `src-tauri/src/core/tools/js_repl/meriyah.umd.min.js`
  - Imported parser dependency used by the kernel.

### Test locations

- `src-tauri/src/core/tools/handlers/js_repl.rs`
  - Add handler-level parsing and event tests.
- `src-tauri/src/core/tools/js_repl/mod.rs`
  - Add runtime tests for persistence, timeout recovery, module resolution, and nested tool calling.
- `src-tauri/src/core/tools/router.rs`
  - Extend router tests if nested calls require interface changes.
- `src-tauri/tests/tool_handler_tests.rs`
  - Add integration coverage that exercises the ported runtime through Mosaic's public tool path.

### Files intentionally not touched unless required by a failing test

- `src-tauri/src/core/tools/handlers/shell.rs`
- `src-tauri/src/core/tools/handlers/mcp.rs`
- `src-tauri/src/core/tools/handlers/mcp_resource.rs`
- `src-tauri/src/core/tools/handlers/request_user_input.rs`
- `src-tauri/src/core/tools/handlers/presentation_artifact.rs`

These stay behaviorally stable unless the imported `js_repl` contract exposes a concrete incompatibility.

## Task 1: Lock down the target contract with failing tests

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/js_repl.rs`
- Modify: `src-tauri/src/core/tools/js_repl/mod.rs`
- Modify: `src-tauri/tests/tool_handler_tests.rs`

- [ ] **Step 1: Add handler tests for `codex-main` freeform parsing semantics**

Cover these cases inline in `src-tauri/src/core/tools/handlers/js_repl.rs`:

```rust
#[test]
fn parse_freeform_args_without_pragma() {
    let args = parse_freeform_args("console.log('ok');").expect("parse args");
    assert_eq!(args.code, "console.log('ok');");
    assert_eq!(args.timeout_ms, None);
}

#[test]
fn parse_freeform_args_with_pragma() {
    let input = "// codex-js-repl: timeout_ms=15000\nconsole.log('ok');";
    let args = parse_freeform_args(input).expect("parse args");
    assert_eq!(args.code, "console.log('ok');");
    assert_eq!(args.timeout_ms, Some(15_000));
}

#[test]
fn parse_freeform_args_rejects_json_wrapped_code() {
    let err = parse_freeform_args(r#"{"code":"await doThing()"}"#).expect_err("expected error");
    assert!(err.to_string().contains("js_repl is a freeform tool"));
}
```

- [ ] **Step 2: Add integration tests that capture the imported runtime contract**

In `src-tauri/tests/tool_handler_tests.rs`, add tests for:

```rust
#[tokio::test]
async fn js_repl_persists_bindings_across_calls() {
    // call 1: let x = await Promise.resolve(41)
    // call 2: console.log(x + 1)
    // assert second output contains 42
}

#[tokio::test]
async fn js_repl_rejects_recursive_invocation() {
    // js_repl running `await codex.tool("js_repl", "...")` should fail
}
```

- [ ] **Step 3: Add runtime tests for timeout recovery and unawaited nested tools**

In `src-tauri/src/core/tools/js_repl/mod.rs`, add targeted tests for:

```rust
#[tokio::test]
async fn js_repl_timeout_recovers_on_next_exec() { /* timeout, reset, rerun */ }

#[tokio::test]
async fn js_repl_waits_for_unawaited_tool_calls_before_completion() { /* shell marker file */ }
```

- [ ] **Step 4: Run the targeted tests to verify they fail against the current implementation**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml parse_freeform_args_with_pragma -- --exact
cargo test --manifest-path src-tauri/Cargo.toml js_repl_persists_bindings_across_calls -- --exact
cargo test --manifest-path src-tauri/Cargo.toml js_repl_timeout_recovers_on_next_exec -- --exact
```

Expected:

- The parser tests fail because Mosaic still accepts `{ code }` JSON semantics.
- The runtime tests fail because the current lightweight bridge has no imported kernel behavior.

## Task 2: Port the `codex-main` `js_repl` runtime assets and manager

**Files:**
- Add: `src-tauri/src/core/tools/js_repl/kernel.js`
- Add: `src-tauri/src/core/tools/js_repl/meriyah.umd.min.js`
- Modify: `src-tauri/src/core/tools/js_repl/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

- [ ] **Step 1: Copy kernel assets and wire them with `include_str!`**

Import `kernel.js` and `meriyah.umd.min.js` from `codex-main`, then update `mod.rs` to load:

```rust
const KERNEL_SOURCE: &str = include_str!("kernel.js");
const MERIYAH_UMD: &str = include_str!("meriyah.umd.min.js");
```

- [ ] **Step 2: Replace the lightweight bridge with the imported manager structure**

Port these units from `codex-main` into `src-tauri/src/core/tools/js_repl/mod.rs`, adapting names only where Mosaic lacks the exact type:

```rust
pub struct JsReplManager { /* kernel state, exec lock, tmp dir */ }
struct KernelState { /* child, stdin, stderr tail, pending exec map */ }
struct ExecContext { /* Mosaic turn context bridge */ }

impl JsReplManager {
    async fn execute(/* ... */) -> Result<JsExecResult, CodexError> { /* ... */ }
    async fn reset(&self) -> Result<(), CodexError> { /* ... */ }
}
```

- [ ] **Step 3: Add crate dependencies required by the imported runtime**

Update `src-tauri/Cargo.toml` with any missing dependencies used by the imported manager, for example:

```toml
tempfile = "3"
```

Do not add dependencies already present in Mosaic.

- [ ] **Step 4: Run the runtime-focused tests and compile until the imported manager is live**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml js_repl_timeout_recovers_on_next_exec -- --exact
cargo test --manifest-path src-tauri/Cargo.toml js_repl_waits_for_unawaited_tool_calls_before_completion -- --exact
```

Expected:

- Tests pass or fail only on still-missing nested tool bridge behavior.

## Task 3: Adapt the imported nested `codex.tool(...)` bridge to Mosaic's tool runtime

**Files:**
- Modify: `src-tauri/src/core/tools/context.rs`
- Modify: `src-tauri/src/core/tools/mod.rs`
- Modify: `src-tauri/src/core/tools/router.rs`
- Modify: `src-tauri/src/core/tools/handlers/dynamic.rs`
- Modify: `src-tauri/src/core/codex.rs`
- Modify: `src-tauri/src/core/tools/js_repl/mod.rs`

- [ ] **Step 1: Add a failing test for built-in nested tool invocation**

Add a test in `src-tauri/tests/tool_handler_tests.rs`:

```rust
#[tokio::test]
async fn js_repl_can_invoke_builtin_tools() {
    // run `await codex.tool("list_dir", { dir_path: ... })`
    // assert nested call returns structured output
}
```

- [ ] **Step 2: Add a failing test for dynamic nested tool invocation**

Add a test in `src-tauri/tests/tool_handler_tests.rs`:

```rust
#[tokio::test]
async fn js_repl_can_invoke_dynamic_tools() {
    // register a dynamic tool
    // invoke it through `codex.tool(...)`
    // assert DynamicToolCallRequest/Response cycle completes
}
```

- [ ] **Step 3: Extend Mosaic's invocation/output model only where the imported bridge requires it**

Update `src-tauri/src/core/tools/context.rs` so nested calls can express both input kind and structured output:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToolPayload {
    Function { arguments: String },
    Custom { input: String },
    Shell { command: Vec<String> },
    Mcp { server: String, tool: String, raw_arguments: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToolOutput {
    Text { content: String, success: bool },
    Json { value: serde_json::Value, success: bool },
}
```

If this is still insufficient for imported response fidelity, add narrowly-scoped helpers rather than rewriting the entire Mosaic tool stack at once.

- [ ] **Step 4: Route nested `codex.tool(...)` calls through `ToolRouter` and `DynamicToolHandler`**

In `src-tauri/src/core/tools/js_repl/mod.rs`, adapt the imported nested tool call path to:

```rust
match router.route_tool_call_with_context(ctx.clone(), tool_name, args).await {
    RouteResult::Handled(result) => { /* serialize for kernel */ }
    RouteResult::DynamicTool(_) => { /* invoke DynamicToolHandler and serialize response */ }
    RouteResult::NotFound(name) => { /* return structured error */ }
}
```

- [ ] **Step 5: Guard against recursive `js_repl` invocation**

Preserve the imported contract:

```rust
fn is_js_repl_internal_tool(name: &str) -> bool {
    matches!(name, "js_repl" | "js_repl_reset")
}
```

- [ ] **Step 6: Run the nested tool tests until both built-in and dynamic calls pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml js_repl_can_invoke_builtin_tools -- --exact
cargo test --manifest-path src-tauri/Cargo.toml js_repl_can_invoke_dynamic_tools -- --exact
cargo test --manifest-path src-tauri/Cargo.toml js_repl_rejects_recursive_invocation -- --exact
```

Expected:

- All three tests pass.

## Task 4: Align external tool contract text and remove placeholder behavior

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/js_repl.rs`
- Modify: `src-tauri/src/core/project_doc.rs`
- Modify: `src-tauri/src/core/tools/spec.rs`
- Modify: `src-tauri/src/core/tools/router.rs`

- [ ] **Step 1: Change `js_repl` handler parsing to prefer freeform `codex-main` semantics**

Keep function payload compatibility only if required by an existing Mosaic caller, but make custom/freeform the canonical path:

```rust
match payload_kind {
    Custom => parse_freeform_args(input)?,
    Function => parse_json_args(arguments)?,
}
```

- [ ] **Step 2: Remove placeholder wording from tool specs**

Update `src-tauri/src/core/tools/spec.rs`:

```rust
"Execute JavaScript in a persistent Node-backed REPL with top-level await support and codex.tool(...) nested tool access."
```

- [ ] **Step 3: Keep project docs aligned with actual shipped behavior**

Reconcile `src-tauri/src/core/project_doc.rs` with the imported runtime so the docs only mention helpers and constraints that are actually implemented.

- [ ] **Step 4: Run the handler and router regression tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml parse_freeform_args_without_pragma -- --exact
cargo test --manifest-path src-tauri/Cargo.toml core::tools::router:: --lib
cargo test --manifest-path src-tauri/Cargo.toml from_config_router_routes_js_repl_tools_when_enabled -- --exact
```

Expected:

- Handler parsing and router exposure remain stable.

## Task 5: Full regression verification for the port

**Files:**
- Test: `src-tauri/src/core/tools/handlers/js_repl.rs`
- Test: `src-tauri/src/core/tools/js_repl/mod.rs`
- Test: `src-tauri/tests/tool_handler_tests.rs`

- [ ] **Step 1: Run the focused `js_repl` test set**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml js_repl --lib
cargo test --manifest-path src-tauri/Cargo.toml js_repl --test tool_handler_tests
```

Expected:

- All `js_repl` unit and integration tests pass.

- [ ] **Step 2: Run broader tool runtime regression tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml tool_handler_tests
cargo test --manifest-path src-tauri/Cargo.toml core::tools::router:: --lib
```

Expected:

- No regression in existing built-in, MCP, or dynamic tool routing.

- [ ] **Step 3: Run an end-to-end compile gate**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Expected:

- Library tests compile and pass.

- [ ] **Step 4: Document any remaining intentional interface deltas**

If Mosaic still keeps a deliberate difference from `codex-main`, record it in:

```text
docs/core-tools-codex-main-vs-mosaic-analysis.md
```

Only keep deltas that are intentional and covered by tests.
