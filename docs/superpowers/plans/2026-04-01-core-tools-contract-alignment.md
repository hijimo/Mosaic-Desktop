# Core Tools Contract Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align Mosaic's `core/tools` tool surface with `codex-main` at the contract layer only: tool names, tool types, parameter schemas, default exposure, output envelopes, and caller integration.

**Architecture:** Implement this in two layers. First, make `src-tauri/src/core/tools/spec.rs` and related handler signatures produce the same externally observable tool contracts as `codex-main`, including provider-native, freeform, function, MCP, and dynamic tools. Then adapt `router`, `session`, `codex`, and frontend consumers so the expanded tool surface can be collected, routed, and rendered without assuming Mosaic's older builtin-only contract.

**Tech Stack:** Rust, Tokio, Serde, Tauri, TypeScript, React, Zustand, Vitest, existing `src-tauri/src/core/tools/*`, `codex-main` reference source under `/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools`

---

## File Structure

### Contract source of truth

- Modify: `src-tauri/src/core/tools/spec.rs`
  - Expand `ToolsConfig`, add `codex-main`-compatible tool builders, and align default exposure rules.
- Modify: `src-tauri/src/core/tools/mod.rs`
  - Extend tool registry contracts where output/tool-type differences require richer plumbing.
- Modify: `src-tauri/src/core/tools/router.rs`
  - Align collected tool specs and routing behavior for builtin, MCP, dynamic, custom, and provider-native tools.
- Modify: `src-tauri/src/core/session.rs`
  - Build a `ToolsConfig` that exposes the same default tool surface as `codex-main` for this phase.

### Backend handler alignment

- Modify: `src-tauri/src/core/tools/handlers/apply_patch.rs`
  - Support the `codex-main` `apply_patch` contract chosen by `spec.rs`.
- Modify: `src-tauri/src/core/tools/handlers/shell.rs`
  - Align `shell` contract and any alias-facing behavior needed by callers.
- Modify: `src-tauri/src/core/tools/handlers/shell_command.rs`
  - Align `shell_command` schema/output contract with `codex-main`.
- Modify: `src-tauri/src/core/tools/handlers/unified_exec.rs`
  - Align `exec_command` / `write_stdin` parameter and output envelopes.
- Modify: `src-tauri/src/core/tools/handlers/list_dir.rs`
- Modify: `src-tauri/src/core/tools/handlers/read_file.rs`
- Modify: `src-tauri/src/core/tools/handlers/grep_files.rs`
- Modify: `src-tauri/src/core/tools/handlers/view_image.rs`
- Modify: `src-tauri/src/core/tools/handlers/test_sync.rs`
- Modify: `src-tauri/src/core/tools/handlers/plan.rs`
- Modify: `src-tauri/src/core/tools/handlers/request_user_input.rs`
- Modify: `src-tauri/src/core/tools/handlers/multi_agents.rs`
- Modify: `src-tauri/src/core/tools/handlers/agent_jobs.rs`
- Modify: `src-tauri/src/core/tools/handlers/mcp.rs`
- Modify: `src-tauri/src/core/tools/handlers/mcp_resource.rs`
- Modify: `src-tauri/src/core/tools/handlers/dynamic.rs`
- Modify: `src-tauri/src/core/tools/handlers/js_repl.rs`
- Modify: `src-tauri/src/core/tools/handlers/presentation_artifact.rs`
- Modify: `src-tauri/src/core/tools/handlers/search_tool_bm25.rs`

### Shared invocation and caller plumbing

- Modify: `src-tauri/src/core/tools/context.rs`
  - Preserve enough payload/output detail to distinguish function, custom, and MCP outputs.
- Modify: `src-tauri/src/core/codex.rs`
  - Adapt request/response item handling to the aligned tool surface.
- Modify: `src-tauri/src/core/mcp_server.rs`
  - Keep MCP-facing tool listing/calling aligned with the new registry behavior.
- Modify: `src/types/chat.ts`
- Modify: `src/types/events.ts`
- Modify: `src/stores/toolCallStore.ts`
- Modify: `src/components/chat/ToolCallDisplay.tsx`
- Modify: `src/components/chat/agent/CodeExecutionBlock.tsx`
- Modify: `src/components/chat/agent/McpToolCallCard.tsx`
- Modify: `src/components/chat/streaming/StreamingToolRegion.tsx`
  - Consume aligned tool call/output shapes without assuming the old subset.

### Tests

- Add: `src-tauri/tests/tool_contract_alignment_tests.rs`
  - Snapshot and compare contract-visible tool definitions against `codex-main`.
- Modify: `src-tauri/tests/tool_handler_tests.rs`
  - Add routing/output-envelope coverage for the aligned contracts.
- Modify: `src-tauri/tests/command_interface_tests.rs`
  - Ensure session-level tool exposure reflects the new defaults.
- Modify: `src/__tests__/unit/services/commands.test.ts`
- Modify: `src/__tests__/unit/components/streaming/StreamingTurnRoot.test.tsx`
- Modify: `src/__tests__/unit/components/Message.test.tsx`
  - Verify frontend callers and renderers survive the aligned contract shapes.

## Dependency Order

1. Lock the target contract with automated comparisons before changing production code.
2. Align `ToolsConfig` and `spec.rs` before touching callers, otherwise every downstream change will be unstable.
3. Fix tool-type-sensitive tools first: `apply_patch`, `js_repl`, `web_search`, MCP tools, dynamic tools.
4. Update `router` / `session` / `codex` after the spec layer settles.
5. Only then update frontend callers and snapshots to the new shapes.

## Task 1: Create a contract comparison harness and freeze the target surface

**Files:**
- Add: `src-tauri/tests/tool_contract_alignment_tests.rs`
- Modify: `src-tauri/Cargo.toml`
- Test: `src-tauri/tests/tool_contract_alignment_tests.rs`

- [ ] **Step 1: Add a failing Rust test that compares Mosaic's contract-visible tool names to the target subset from `codex-main`**

```rust
#[test]
fn tool_contract_names_match_codex_main_default_surface() {
    let config = tauri_app_lib::core::tools::spec::ToolsConfig::default();
    let assembled = tauri_app_lib::core::tools::spec::build_specs(&config, false);
    let mut actual: Vec<String> = assembled
        .configured_specs
        .iter()
        .map(|spec| spec.spec.name().to_string())
        .collect();
    actual.sort();

    let expected = vec![
        "apply_patch",
        "grep_files",
        "list_dir",
        "read_file",
        "shell",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();

    assert_eq!(actual, expected);
}
```

- [ ] **Step 2: Add a codex-main-backed comparison helper to load target tool metadata from the reference checkout**

```rust
fn codex_main_tools_root() -> std::path::PathBuf {
    std::path::PathBuf::from("/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools")
}

fn codex_main_spec_path() -> std::path::PathBuf {
    codex_main_tools_root().join("spec.rs")
}

#[test]
fn codex_main_reference_checkout_exists() {
    assert!(codex_main_spec_path().is_file());
}
```

- [ ] **Step 3: Add a table-driven failing test for tool presence by group**

```rust
#[test]
fn mosaic_is_missing_contract_groups_we_intend_to_add() {
    let expected_groups = [
        ("shell", vec!["shell", "shell_command", "exec_command", "write_stdin"]),
        ("collab", vec!["spawn_agent", "send_input", "resume_agent", "wait", "close_agent"]),
        ("mcp", vec!["list_mcp_resources", "list_mcp_resource_templates", "read_mcp_resource"]),
    ];

    let names = tauri_app_lib::core::tools::spec::build_specs(
        &tauri_app_lib::core::tools::spec::ToolsConfig::default(),
        false,
    )
    .configured_specs
    .into_iter()
    .map(|configured| configured.spec.name().to_string())
    .collect::<std::collections::BTreeSet<_>>();

    for (_group, group_names) in expected_groups {
        for tool_name in group_names {
            assert!(names.contains(tool_name), "missing tool: {tool_name}");
        }
    }
}
```

- [ ] **Step 4: Run the new harness and confirm it fails on the current reduced Mosaic contract**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml tool_contract_names_match_codex_main_default_surface -- --exact
cargo test --manifest-path src-tauri/Cargo.toml mosaic_is_missing_contract_groups_we_intend_to_add -- --exact
```

Expected:

- The first test passes only for the current reduced default surface.
- The second test fails because the full `codex-main` contract is not yet exposed by default.

- [ ] **Step 5: Commit the test harness before widening the production surface**

```bash
git add src-tauri/tests/tool_contract_alignment_tests.rs src-tauri/Cargo.toml
git commit -m "test: add core tools contract alignment harness"
```

## Task 2: Expand `ToolsConfig` and `spec.rs` to match `codex-main` tool exposure rules

**Files:**
- Modify: `src-tauri/src/core/tools/spec.rs`
- Modify: `src-tauri/src/core/session.rs`
- Test: `src-tauri/tests/tool_contract_alignment_tests.rs`

- [ ] **Step 1: Add failing spec tests that describe the target default exposure**

```rust
#[test]
fn build_specs_exposes_standard_codex_main_tools_by_default() {
    let assembled = build_specs(&ToolsConfig::default(), false);
    let names = assembled
        .configured_specs
        .iter()
        .map(|configured| configured.spec.name().to_string())
        .collect::<std::collections::BTreeSet<_>>();

    for name in [
        "update_plan",
        "request_user_input",
        "view_image",
        "shell",
        "apply_patch",
    ] {
        assert!(names.contains(name), "missing default tool {name}");
    }
}
```

- [ ] **Step 2: Replace the reduced `ToolsConfig` shape with a codex-main-compatible contract surface**

Update `src-tauri/src/core/tools/spec.rs` to carry the toggles the contract layer needs:

```rust
pub struct ToolsConfig {
    pub shell_type: MosaicShellToolType,
    pub allow_login_shell: bool,
    pub apply_patch_tool_type: Option<MosaicApplyPatchToolType>,
    pub web_search_mode: Option<WebSearchMode>,
    pub search_tool: bool,
    pub request_permission_enabled: bool,
    pub js_repl_enabled: bool,
    pub js_repl_tools_only: bool,
    pub collab_tools: bool,
    pub presentation_artifact: bool,
    pub default_mode_request_user_input: bool,
    pub experimental_supported_tools: Vec<String>,
    pub agent_jobs_tools: bool,
    pub agent_jobs_worker_tools: bool,
}
```

- [ ] **Step 3: Rewrite `ToolsConfig::default()` and `Session::tools_config_from_resolved_config(...)` to expose the codex-main default surface**

Implement the default contract in `src-tauri/src/core/session.rs`:

```rust
crate::core::tools::ToolsConfig {
    shell_type: crate::core::tools::spec::MosaicShellToolType::Shell,
    allow_login_shell: true,
    apply_patch_tool_type: Some(crate::core::tools::spec::MosaicApplyPatchToolType::Function),
    web_search_mode,
    search_tool: false,
    request_permission_enabled: false,
    js_repl_enabled: true,
    js_repl_tools_only: false,
    collab_tools: true,
    presentation_artifact: true,
    default_mode_request_user_input: false,
    experimental_supported_tools: vec![
        "grep_files".to_string(),
        "read_file".to_string(),
        "list_dir".to_string(),
        "test_sync_tool".to_string(),
    ],
    agent_jobs_tools: true,
    agent_jobs_worker_tools: false,
}
```

- [ ] **Step 4: Make `build_specs(...)` include the same top-level families that codex-main includes**

Refactor `build_specs(...)` so it pushes:

```rust
builder.push_spec(create_update_plan_tool());
builder.push_spec(create_request_user_input_tool(default_mode_request_user_input));
builder.push_spec_with_parallel_support(create_view_image_tool(), true);
```

and conditionally includes:

```rust
create_shell_tool(...)
create_shell_command_tool(...)
create_exec_command_tool(...)
create_write_stdin_tool()
create_apply_patch_tool(...)
create_js_repl_tool()
create_js_repl_reset_tool()
create_presentation_artifact_tool()
create_spawn_agent_tool(...)
create_spawn_agents_on_csv_tool()
```

- [ ] **Step 5: Re-run the spec tests until the default tool set matches the target contract**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml build_specs_exposes_standard_codex_main_tools_by_default -- --exact
cargo test --manifest-path src-tauri/Cargo.toml tool_contract_alignment_tests -- --nocapture
```

Expected:

- The default-contract test passes.
- The harness still fails on tool shape and schema differences that later tasks will fix.

- [ ] **Step 6: Commit the config/spec expansion**

```bash
git add src-tauri/src/core/tools/spec.rs src-tauri/src/core/session.rs src-tauri/tests/tool_contract_alignment_tests.rs
git commit -m "feat: align core tools default exposure with codex main"
```

## Task 3: Align shell-family and patch tool contracts, including tool type differences

**Files:**
- Modify: `src-tauri/src/core/tools/spec.rs`
- Modify: `src-tauri/src/core/tools/handlers/shell.rs`
- Modify: `src-tauri/src/core/tools/handlers/shell_command.rs`
- Modify: `src-tauri/src/core/tools/handlers/unified_exec.rs`
- Modify: `src-tauri/src/core/tools/handlers/apply_patch.rs`
- Modify: `src-tauri/src/core/tools/context.rs`
- Modify: `src-tauri/tests/tool_contract_alignment_tests.rs`

- [ ] **Step 1: Add failing tests for shell, exec, and `apply_patch` tool-type contracts**

```rust
#[test]
fn apply_patch_contract_uses_configured_tool_type() {
    let config = ToolsConfig {
        apply_patch_tool_type: Some(MosaicApplyPatchToolType::Freeform),
        ..ToolsConfig::default()
    };
    let assembled = build_specs(&config, false);
    let spec = assembled
        .configured_specs
        .iter()
        .find(|configured| configured.spec.name() == "apply_patch")
        .expect("apply_patch spec");
    assert!(matches!(spec.spec, ToolSpec::Custom { .. }));
}
```

- [ ] **Step 2: Add codex-main-compatible builders for `shell`, `shell_command`, `exec_command`, `write_stdin`, and `apply_patch`**

Implement shape-aware builders in `src-tauri/src/core/tools/spec.rs`:

```rust
match apply_patch_tool_type {
    MosaicApplyPatchToolType::Freeform => builder.push_spec(create_apply_patch_freeform_tool()),
    MosaicApplyPatchToolType::Function => builder.push_spec(create_apply_patch_json_tool()),
}
```

and:

```rust
match shell_type {
    MosaicShellToolType::Shell => builder.push_spec_with_parallel_support(create_shell_tool(...), true),
    MosaicShellToolType::ShellCommand => builder.push_spec_with_parallel_support(create_shell_command_tool(...), true),
    MosaicShellToolType::UnifiedExec => {
        builder.push_spec_with_parallel_support(create_exec_command_tool(...), true);
        builder.push_spec(create_write_stdin_tool());
    }
}
```

- [ ] **Step 3: Ensure handlers accept the aligned payload shapes and return stable output envelopes**

Normalize outputs in handlers to the same structured keys:

```rust
serde_json::json!({
    "exit_code": exit_code,
    "stdout": stdout,
    "stderr": stderr,
    "aggregated_output": aggregated_output,
    "duration_ms": duration_ms,
    "timed_out": timed_out,
})
```

and allow `apply_patch` to accept either JSON args or freeform content chosen by `spec.rs`.

- [ ] **Step 4: Re-run the shell-family tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml apply_patch_contract_uses_configured_tool_type -- --exact
cargo test --manifest-path src-tauri/Cargo.toml tool_handler_tests -- --nocapture
```

Expected:

- Contract tests pass for tool type and output envelope shape.
- Existing shell-family tests continue to pass.

- [ ] **Step 5: Commit the shell-family contract alignment**

```bash
git add src-tauri/src/core/tools/spec.rs src-tauri/src/core/tools/handlers/shell.rs src-tauri/src/core/tools/handlers/shell_command.rs src-tauri/src/core/tools/handlers/unified_exec.rs src-tauri/src/core/tools/handlers/apply_patch.rs src-tauri/src/core/tools/context.rs src-tauri/tests/tool_contract_alignment_tests.rs
git commit -m "feat: align shell and patch tool contracts"
```

## Task 4: Align file and local utility tool schemas and outputs

**Files:**
- Modify: `src-tauri/src/core/tools/spec.rs`
- Modify: `src-tauri/src/core/tools/handlers/list_dir.rs`
- Modify: `src-tauri/src/core/tools/handlers/read_file.rs`
- Modify: `src-tauri/src/core/tools/handlers/grep_files.rs`
- Modify: `src-tauri/src/core/tools/handlers/view_image.rs`
- Modify: `src-tauri/src/core/tools/handlers/test_sync.rs`
- Modify: `src-tauri/tests/tool_contract_alignment_tests.rs`

- [ ] **Step 1: Add failing schema tests for utility tools**

```rust
#[test]
fn view_image_is_parallel_and_has_single_path_parameter() {
    let assembled = build_specs(&ToolsConfig::default(), false);
    let spec = assembled
        .configured_specs
        .iter()
        .find(|configured| configured.spec.name() == "view_image")
        .expect("view_image spec");
    assert!(spec.supports_parallel_tool_calls);
}
```

- [ ] **Step 2: Align utility tool builders to codex-main parameter descriptions and required fields**

Update these builders in `src-tauri/src/core/tools/spec.rs`:

```rust
create_list_dir_tool()
create_read_file_tool()
create_grep_files_tool()
create_view_image_tool()
create_test_sync_tool()
```

so they preserve:

- the same field names
- the same `required` list
- the same `supports_parallel_tool_calls`
- the same description strings where feasible

- [ ] **Step 3: Normalize utility handler outputs to the same high-level envelopes**

Examples:

```rust
Ok(serde_json::json!({
    "content": [
        {
            "type": "input_image",
            "image_url": image_url,
        }
    ],
    "size_bytes": data.len(),
}))
```

and:

```rust
Ok(serde_json::json!({
    "entries": entries,
}))
```

where `codex-main` returns structured content rather than Mosaic-specific ad hoc keys.

- [ ] **Step 4: Run the utility contract tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml view_image_is_parallel_and_has_single_path_parameter -- --exact
cargo test --manifest-path src-tauri/Cargo.toml tool_contract_alignment_tests -- --nocapture
```

Expected:

- The utility tool contract checks pass.
- The remaining failures are now concentrated in collab, MCP, dynamic, and higher-level tools.

- [ ] **Step 5: Commit the utility-tool contract alignment**

```bash
git add src-tauri/src/core/tools/spec.rs src-tauri/src/core/tools/handlers/list_dir.rs src-tauri/src/core/tools/handlers/read_file.rs src-tauri/src/core/tools/handlers/grep_files.rs src-tauri/src/core/tools/handlers/view_image.rs src-tauri/src/core/tools/handlers/test_sync.rs src-tauri/tests/tool_contract_alignment_tests.rs
git commit -m "feat: align file and utility tool contracts"
```

## Task 5: Align collaboration and agent-job tool contracts without requiring full runtime parity

**Files:**
- Modify: `src-tauri/src/core/tools/spec.rs`
- Modify: `src-tauri/src/core/tools/handlers/plan.rs`
- Modify: `src-tauri/src/core/tools/handlers/request_user_input.rs`
- Modify: `src-tauri/src/core/tools/handlers/multi_agents.rs`
- Modify: `src-tauri/src/core/tools/handlers/agent_jobs.rs`
- Modify: `src-tauri/tests/tool_contract_alignment_tests.rs`

- [ ] **Step 1: Add failing tests for collab tool presence and schemas**

```rust
#[test]
fn collab_tools_match_codex_main_names_when_enabled() {
    let config = ToolsConfig {
        collab_tools: true,
        ..ToolsConfig::default()
    };
    let assembled = build_specs(&config, true);
    let names = assembled
        .configured_specs
        .iter()
        .map(|configured| configured.spec.name().to_string())
        .collect::<std::collections::BTreeSet<_>>();

    for name in ["spawn_agent", "send_input", "resume_agent", "wait", "close_agent"] {
        assert!(names.contains(name), "missing collab tool {name}");
    }
}
```

- [ ] **Step 2: Align `update_plan` and `request_user_input` descriptions and required schemas**

Implement `codex-main`-compatible builders:

```rust
builder.push_spec(create_update_plan_tool());
builder.push_spec(create_request_user_input_tool(default_mode_request_user_input));
```

and keep `request_user_input_tool_description(...)` identical in allowed-mode wording.

- [ ] **Step 3: Align collab and agent-job builders with codex-main names and argument shapes**

Update the tool builders for:

```rust
create_spawn_agent_tool(...)
create_send_input_tool()
create_resume_agent_tool()
create_wait_tool()
create_close_agent_tool()
create_spawn_agents_on_csv_tool()
create_report_agent_job_result_tool()
```

so they match `codex-main` field names, descriptions, and optional/required behavior.

- [ ] **Step 4: Keep handler errors compatible for not-yet-implemented runtime paths**

Return explicit execution-stage failures like:

```rust
Err(CodexError::new(
    ErrorCode::ToolExecutionFailed,
    "report_agent_job_result requires the agent subsystem",
))
```

instead of failing earlier from missing tool registration or mismatched args.

- [ ] **Step 5: Run the collaboration contract tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml collab_tools_match_codex_main_names_when_enabled -- --exact
cargo test --manifest-path src-tauri/Cargo.toml tool_contract_alignment_tests -- --nocapture
```

Expected:

- Collaboration and agent-job tool schemas now match the target contract.
- Runtime gaps remain execution-time only.

- [ ] **Step 6: Commit the collaboration contract work**

```bash
git add src-tauri/src/core/tools/spec.rs src-tauri/src/core/tools/handlers/plan.rs src-tauri/src/core/tools/handlers/request_user_input.rs src-tauri/src/core/tools/handlers/multi_agents.rs src-tauri/src/core/tools/handlers/agent_jobs.rs src-tauri/tests/tool_contract_alignment_tests.rs
git commit -m "feat: align collaboration and agent job tool contracts"
```

## Task 6: Expose MCP tools, MCP resources, dynamic tools, and higher-level tool contracts

**Files:**
- Modify: `src-tauri/src/core/tools/spec.rs`
- Modify: `src-tauri/src/core/tools/router.rs`
- Modify: `src-tauri/src/core/tools/handlers/mcp.rs`
- Modify: `src-tauri/src/core/tools/handlers/mcp_resource.rs`
- Modify: `src-tauri/src/core/tools/handlers/dynamic.rs`
- Modify: `src-tauri/src/core/tools/handlers/js_repl.rs`
- Modify: `src-tauri/src/core/tools/handlers/presentation_artifact.rs`
- Modify: `src-tauri/src/core/tools/handlers/search_tool_bm25.rs`
- Modify: `src-tauri/src/core/session.rs`
- Modify: `src-tauri/tests/tool_contract_alignment_tests.rs`

- [ ] **Step 1: Add failing tests for MCP and dynamic spec exposure**

```rust
#[test]
fn collect_tool_specs_includes_dynamic_and_mcp_specs() {
    let mut router = tauri_app_lib::core::tools::router::ToolRouter::from_config(
        ToolsConfig::default(),
        true,
    );
    router.register_dynamic_tool(tauri_app_lib::protocol::types::DynamicToolSpec {
        name: "dyn_echo".to_string(),
        description: "dynamic echo".to_string(),
        input_schema: serde_json::json!({"type":"object","properties":{}}),
    });

    let specs = router.collect_tool_specs();
    assert!(specs.iter().any(|value| value["name"] == "dyn_echo"));
}
```

- [ ] **Step 2: Extend `build_specs(...)` to accept MCP/app/dynamic tool inputs and emit matching specs**

Refactor the signature in `src-tauri/src/core/tools/spec.rs`:

```rust
pub fn build_specs(
    config: &ToolsConfig,
    mcp_tools: Option<std::collections::HashMap<String, rmcp::model::Tool>>,
    app_tools: Option<std::collections::HashMap<String, crate::core::mcp_client::ToolInfo>>,
    dynamic_tools: &[crate::protocol::types::DynamicToolSpec],
) -> AssembledToolRuntime
```

and convert external tool definitions into collected specs just as `codex-main` does.

- [ ] **Step 3: Align `web_search`, `js_repl`, `presentation_artifact`, and `search_tool_bm25` builders**

Ensure `spec.rs` can emit:

```rust
ToolSpec::WebSearch { external_web_access: Some(false) }
ToolSpec::WebSearch { external_web_access: Some(true) }
create_js_repl_tool()
create_js_repl_reset_tool()
create_presentation_artifact_tool()
create_search_tool_bm25_tool(...)
```

with the same type and description behavior as `codex-main`.

- [ ] **Step 4: Update `router.collect_tool_specs()` and `session.collect_tool_specs_for_current_turn()` to aggregate the aligned external tool surface**

Use collection logic shaped like:

```rust
let mut specs = self
    .configured_specs
    .iter()
    .map(|configured| tool_spec_to_json(&configured.spec))
    .collect::<Vec<_>>();

for spec in self.dynamic_tools.values() {
    specs.push(serde_json::json!({
        "type": "function",
        "name": spec.name,
        "description": spec.description,
        "parameters": spec.input_schema,
    }));
}
```

and ensure session-level web-search constraints still post-process the collected list.

- [ ] **Step 5: Run the MCP/dynamic/high-level contract tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml collect_tool_specs_includes_dynamic_and_mcp_specs -- --exact
cargo test --manifest-path src-tauri/Cargo.toml tool_contract_alignment_tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml command_interface_tests -- --nocapture
```

Expected:

- MCP and dynamic specs now appear in collected tool specs.
- `web_search` remains provider-native and post-processed by session constraints.

- [ ] **Step 6: Commit the external-tool contract alignment**

```bash
git add src-tauri/src/core/tools/spec.rs src-tauri/src/core/tools/router.rs src-tauri/src/core/tools/handlers/mcp.rs src-tauri/src/core/tools/handlers/mcp_resource.rs src-tauri/src/core/tools/handlers/dynamic.rs src-tauri/src/core/tools/handlers/js_repl.rs src-tauri/src/core/tools/handlers/presentation_artifact.rs src-tauri/src/core/tools/handlers/search_tool_bm25.rs src-tauri/src/core/session.rs src-tauri/tests/tool_contract_alignment_tests.rs
git commit -m "feat: expose mcp dynamic and higher-level tool contracts"
```

## Task 7: Adapt backend callers and frontend consumers to the expanded aligned tool surface

**Files:**
- Modify: `src-tauri/src/core/codex.rs`
- Modify: `src-tauri/src/core/mcp_server.rs`
- Modify: `src/types/chat.ts`
- Modify: `src/types/events.ts`
- Modify: `src/stores/toolCallStore.ts`
- Modify: `src/components/chat/ToolCallDisplay.tsx`
- Modify: `src/components/chat/agent/CodeExecutionBlock.tsx`
- Modify: `src/components/chat/agent/McpToolCallCard.tsx`
- Modify: `src/components/chat/streaming/StreamingToolRegion.tsx`
- Modify: `src/__tests__/unit/services/commands.test.ts`
- Modify: `src/__tests__/unit/components/streaming/StreamingTurnRoot.test.tsx`
- Modify: `src/__tests__/unit/components/Message.test.tsx`

- [ ] **Step 1: Add failing frontend tests that prove the old builtin-only assumptions break**

```tsx
it("renders dynamic tool calls using generic tool metadata", () => {
  renderStreamingTurn({
    toolCalls: [
      {
        name: "dyn_echo",
        status: "completed",
        input: { value: "hi" },
        output: { echoed: "hi" },
      },
    ],
  });

  expect(screen.getByText("dyn_echo")).toBeInTheDocument();
});
```

- [ ] **Step 2: Update backend caller plumbing to preserve aligned output envelopes**

In `src-tauri/src/core/codex.rs`, adapt tool call handling so it preserves function/custom/provider-native distinctions rather than flattening them into Mosaic-specific JSON.

```rust
match route_result {
    RouteResult::Handled(result) => result,
    RouteResult::DynamicTool(name) => { /* existing dynamic protocol */ }
    RouteResult::NotFound(name) => Err(CodexError::new(
        ErrorCode::ToolExecutionFailed,
        format!("no handler found for tool: {name}"),
    )),
}
```

- [ ] **Step 3: Update frontend stores and renderers to accept arbitrary aligned tool names and envelopes**

Keep rendering generic where possible:

```ts
export type ToolCallOutput =
  | { kind: "json"; value: unknown }
  | { kind: "text"; content: string }
  | { kind: "content_items"; items: Array<unknown> };
```

and stop hard-coding the assumption that only a fixed builtin subset can appear.

- [ ] **Step 4: Run the frontend and integration tests**

Run:

```bash
npm test -- src/__tests__/unit/services/commands.test.ts
npm test -- src/__tests__/unit/components/streaming/StreamingTurnRoot.test.tsx
npm test -- src/__tests__/unit/components/Message.test.tsx
```

Expected:

- Frontend tests pass with dynamic, MCP, and expanded builtin tool metadata.

- [ ] **Step 5: Commit the caller-consumer alignment**

```bash
git add src-tauri/src/core/codex.rs src-tauri/src/core/mcp_server.rs src/types/chat.ts src/types/events.ts src/stores/toolCallStore.ts src/components/chat/ToolCallDisplay.tsx src/components/chat/agent/CodeExecutionBlock.tsx src/components/chat/agent/McpToolCallCard.tsx src/components/chat/streaming/StreamingToolRegion.tsx src/__tests__/unit/services/commands.test.ts src/__tests__/unit/components/streaming/StreamingTurnRoot.test.tsx src/__tests__/unit/components/Message.test.tsx
git commit -m "feat: adapt callers to aligned core tools contract"
```

## Task 8: Final verification, snapshot refresh, and documentation

**Files:**
- Modify: `src-tauri/tests/tool_contract_alignment_tests.rs`
- Modify: `docs/core-tools-codex-main-vs-mosaic-analysis.md`
- Modify: `docs/superpowers/specs/2026-04-01-core-tools-contract-alignment-design.md`

- [ ] **Step 1: Tighten the contract tests from incremental checks into a full regression suite**

Add assertions for:

```rust
assert_eq!(actual_tool_names, expected_tool_names);
assert_eq!(actual_required_fields("request_user_input"), expected_required_fields("request_user_input"));
assert_eq!(actual_tool_type("web_search"), "web_search");
assert_eq!(actual_tool_type("js_repl"), "custom");
```

- [ ] **Step 2: Run the complete verification suite**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml tool_contract_alignment_tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml tool_handler_tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml command_interface_tests -- --nocapture
npm test -- src/__tests__/unit/services/commands.test.ts
npm test -- src/__tests__/unit/components/streaming/StreamingTurnRoot.test.tsx
npm test -- src/__tests__/unit/components/Message.test.tsx
```

Expected:

- All contract-layer tests pass.
- Remaining failures, if any, are execution-semantic gaps rather than schema or exposure mismatches.

- [ ] **Step 3: Update the analysis docs to reflect the new first-phase status**

In `docs/core-tools-codex-main-vs-mosaic-analysis.md`, replace old “missing/limited” statements with:

```md
- Tool surface, tool type, and default exposure are now contract-aligned.
- Remaining gaps are execution-time runtime differences only.
```

- [ ] **Step 4: Commit the verification and docs update**

```bash
git add src-tauri/tests/tool_contract_alignment_tests.rs docs/core-tools-codex-main-vs-mosaic-analysis.md docs/superpowers/specs/2026-04-01-core-tools-contract-alignment-design.md
git commit -m "docs: record core tools contract alignment phase one"
```

## Self-Review

### Spec coverage

- 工具清单已按 shell、utility、collab、agent jobs、MCP/dynamic、higher-level 六组覆盖。
- spec 中要求的默认暴露面、tool shape、schema、caller 适配、MCP/dynamic 暴露、错误后移，均有对应任务。
- 未纳入本计划的只有第二阶段运行时语义追平项，符合 spec 的非目标定义。

### Placeholder scan

- 计划中没有 `TODO`、`TBD`、`implement later` 等占位文本。
- 每个任务都包含明确文件、代码片段、命令和预期结果。

### Type consistency

- 统一使用 `ToolsConfig`、`build_specs(...)`、`ToolRouter::collect_tool_specs()`、`DynamicToolSpec`、`web_search`、`apply_patch` 等同一命名。
- 第一阶段内部统一将 `apply_patch` 视为可配置的 function/freeform 工具，避免与其他任务冲突。

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-01-core-tools-contract-alignment.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
