# Tools Runtime Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Mosaic's hand-written `core/tools` registration path with a config-driven runtime assembly flow while preserving the current stable default tool surface.

**Architecture:** Introduce a phase-1 tool assembly layer in `core/tools/spec.rs` that owns `ToolsConfig`, `ConfiguredToolSpec`, `ToolRegistryBuilder`, and `build_specs(...)`. Update `ToolRouter` to be constructed from assembled runtime state, then refactor `Session` to derive tool configuration and build its router from that assembly path instead of calling `registry.register(...)` directly.

**Tech Stack:** Rust, Tokio, Tauri, Serde, existing `core/tools` handler modules, inline Rust unit tests

---

## File Structure

### Modified files

- `src-tauri/src/core/tools/spec.rs`
  - Add phase-1 tool assembly types and builder functions.
- `src-tauri/src/core/tools/router.rs`
  - Add router construction from assembled tool runtime.
- `src-tauri/src/core/tools/mod.rs`
  - Export the new assembly-facing types if needed by router/session.
- `src-tauri/src/core/session.rs`
  - Replace hard-coded tool registration with config-driven runtime assembly.

### Test locations

- `src-tauri/src/core/tools/spec.rs`
  - Add inline tests for stable tool assembly and conditional multi-agent inclusion.
- `src-tauri/src/core/tools/router.rs`
  - Add inline tests for `ToolRouter::from_config(...)` and spec collection.
- `src-tauri/src/core/session.rs`
  - Add inline tests that the session default runtime still exposes the same stable tool set.

### Files intentionally not touched in phase 1

- `src-tauri/src/core/tools/handlers/js_repl.rs`
- `src-tauri/src/core/tools/handlers/mcp.rs`
- `src-tauri/src/core/tools/handlers/mcp_resource.rs`
- `src-tauri/src/core/tools/handlers/request_user_input.rs`
- `src-tauri/src/core/tools/handlers/unified_exec.rs`
- `src-tauri/src/core/tools/handlers/view_image.rs`
- `src-tauri/src/core/tools/handlers/presentation_artifact.rs`

These handlers stay out of the default assembled runtime for this phase.

## Task 1: Add phase-1 tool assembly types to `spec.rs`

**Files:**
- Modify: `src-tauri/src/core/tools/spec.rs`
- Test: `src-tauri/src/core/tools/spec.rs`

- [ ] **Step 1: Write failing assembly tests for the stable runtime**

Add inline tests that define the phase-1 contract:

```rust
#[test]
fn build_specs_includes_stable_builtin_tools_without_collab() {
    let assembled = build_specs(&ToolsConfig::default(), false);
    let names: Vec<String> = assembled
        .configured_specs
        .iter()
        .map(|spec| spec.spec.name().to_string())
        .collect();

    assert!(names.contains(&"shell".to_string()));
    assert!(names.contains(&"apply_patch".to_string()));
    assert!(names.contains(&"list_dir".to_string()));
    assert!(names.contains(&"read_file".to_string()));
    assert!(names.contains(&"grep_files".to_string()));
    assert!(!names.contains(&"spawn_agent".to_string()));
}

#[test]
fn build_specs_adds_collab_tools_when_enabled_and_available() {
    let config = ToolsConfig {
        collab_tools: true,
        ..ToolsConfig::default()
    };
    let assembled = build_specs(&config, true);
    let names: Vec<String> = assembled
        .configured_specs
        .iter()
        .map(|spec| spec.spec.name().to_string())
        .collect();

    assert!(names.contains(&"spawn_agent".to_string()));
    assert!(names.contains(&"send_input".to_string()));
    assert!(names.contains(&"resume_agent".to_string()));
    assert!(names.contains(&"wait".to_string()));
    assert!(names.contains(&"close_agent".to_string()));
}
```

- [ ] **Step 2: Run the spec tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml build_specs_includes_stable_builtin_tools_without_collab
```

Expected:

- FAIL because `build_specs`, `ConfiguredToolSpec`, and the assembled runtime type do not exist yet.

- [ ] **Step 3: Add `ConfiguredToolSpec`, `ToolRegistryBuilder`, and assembled runtime output**

Update `src-tauri/src/core/tools/spec.rs` with the new phase-1 assembly types:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ConfiguredToolSpec {
    pub spec: ToolSpec,
    pub supports_parallel_tool_calls: bool,
}

impl ConfiguredToolSpec {
    pub fn new(spec: ToolSpec, supports_parallel_tool_calls: bool) -> Self {
        Self {
            spec,
            supports_parallel_tool_calls,
        }
    }
}

pub struct AssembledToolRuntime {
    pub configured_specs: Vec<ConfiguredToolSpec>,
    pub registry: crate::core::tools::ToolRegistry,
}

pub struct ToolRegistryBuilder {
    configured_specs: Vec<ConfiguredToolSpec>,
    handlers: Vec<Box<dyn crate::core::tools::ToolHandler>>,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self {
            configured_specs: Vec::new(),
            handlers: Vec::new(),
        }
    }

    pub fn push_spec(&mut self, spec: ToolSpec) {
        self.push_spec_with_parallel_support(spec, false);
    }

    pub fn push_spec_with_parallel_support(
        &mut self,
        spec: ToolSpec,
        supports_parallel_tool_calls: bool,
    ) {
        self.configured_specs
            .push(ConfiguredToolSpec::new(spec, supports_parallel_tool_calls));
    }

    pub fn register_handler(&mut self, handler: Box<dyn crate::core::tools::ToolHandler>) {
        self.handlers.push(handler);
    }

    pub fn build(self) -> AssembledToolRuntime {
        let mut registry = crate::core::tools::ToolRegistry::new();
        for handler in self.handlers {
            registry.register(handler);
        }
        AssembledToolRuntime {
            configured_specs: self.configured_specs,
            registry,
        }
    }
}
```

- [ ] **Step 4: Implement phase-1 `build_specs(...)`**

Still in `src-tauri/src/core/tools/spec.rs`, add a builder entry point that assembles only the approved phase-1 tools:

```rust
pub fn build_specs(config: &ToolsConfig, has_agent_control: bool) -> AssembledToolRuntime {
    use crate::core::tools::handlers::{
        ApplyPatchHandler, GrepFilesHandler, ListDirHandler, ReadFileHandler, ShellHandler,
    };

    let mut builder = ToolRegistryBuilder::new();

    if config.shell_enabled {
        builder.push_spec_with_parallel_support(create_shell_tool(), true);
        builder.register_handler(Box::new(ShellHandler));
    }

    if config.apply_patch_enabled {
        builder.push_spec(create_apply_patch_tool());
        builder.register_handler(Box::new(ApplyPatchHandler));
    }

    if config.list_dir_enabled {
        builder.push_spec_with_parallel_support(create_list_dir_tool(), true);
        builder.register_handler(Box::new(ListDirHandler));
    }

    if config.read_file_enabled {
        builder.push_spec_with_parallel_support(create_read_file_tool(), true);
        builder.register_handler(Box::new(ReadFileHandler));
    }

    if config.grep_files_enabled {
        builder.push_spec_with_parallel_support(create_grep_files_tool(), true);
        builder.register_handler(Box::new(GrepFilesHandler));
    }

    if config.collab_tools && has_agent_control {
        // Phase 1 keeps spec assembly separate from session-specific handler injection.
        builder.push_spec(create_spawn_agent_tool());
        builder.push_spec(create_send_input_tool());
        builder.push_spec(create_resume_agent_tool());
        builder.push_spec(create_wait_tool());
        builder.push_spec(create_close_agent_tool());
    }

    builder.build()
}
```

Also add small local helper constructors such as `create_shell_tool()`, `create_apply_patch_tool()`, `create_list_dir_tool()`, `create_read_file_tool()`, and `create_grep_files_tool()` that emit the current JSON function specs already expected by Mosaic.

- [ ] **Step 5: Run the spec tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml build_specs_
```

Expected:

- PASS for the new `build_specs_*` tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/tools/spec.rs
git commit -m "refactor: add phase-1 tool runtime assembly types"
```

## Task 2: Make `ToolRouter` constructible from assembled runtime

**Files:**
- Modify: `src-tauri/src/core/tools/router.rs`
- Test: `src-tauri/src/core/tools/router.rs`

- [ ] **Step 1: Write failing router construction tests**

Add inline tests that verify the router can be built from the phase-1 assembly path:

```rust
#[test]
fn from_config_collects_stable_specs() {
    let config = crate::core::tools::spec::ToolsConfig::default();
    let router = ToolRouter::from_config(config, false);
    let names: Vec<String> = router
        .configured_specs()
        .iter()
        .map(|spec| spec.spec.name().to_string())
        .collect();

    assert!(names.contains(&"shell".to_string()));
    assert!(names.contains(&"apply_patch".to_string()));
    assert!(names.contains(&"list_dir".to_string()));
    assert!(names.contains(&"read_file".to_string()));
    assert!(names.contains(&"grep_files".to_string()));
}

#[tokio::test]
async fn from_config_router_still_routes_builtin_tools() {
    let router = ToolRouter::from_config(crate::core::tools::spec::ToolsConfig::default(), false);
    let result = router
        .route_tool_call("read_file", serde_json::json!({"file_path": "/tmp/missing"}))
        .await;

    assert!(matches!(result, RouteResult::Handled(_)));
}
```

- [ ] **Step 2: Run the router tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml from_config_collects_stable_specs
```

Expected:

- FAIL because `ToolRouter::from_config(...)` and `configured_specs()` do not exist.

- [ ] **Step 3: Extend `ToolRouter` to store configured specs**

Update `src-tauri/src/core/tools/router.rs` so the router owns both the registry and the assembled spec metadata:

```rust
pub struct ToolRouter {
    registry: ToolRegistry,
    mcp_manager: McpConnectionManager,
    dynamic_tools: HashMap<String, DynamicToolSpec>,
    configured_specs: Vec<crate::core::tools::spec::ConfiguredToolSpec>,
}

impl ToolRouter {
    pub fn new(
        registry: ToolRegistry,
        mcp_manager: McpConnectionManager,
        configured_specs: Vec<crate::core::tools::spec::ConfiguredToolSpec>,
    ) -> Self {
        Self {
            registry,
            mcp_manager,
            dynamic_tools: HashMap::new(),
            configured_specs,
        }
    }

    pub fn from_config(
        config: crate::core::tools::spec::ToolsConfig,
        has_agent_control: bool,
    ) -> Self {
        let assembled = crate::core::tools::spec::build_specs(&config, has_agent_control);
        Self::new(
            assembled.registry,
            McpConnectionManager::new(),
            assembled.configured_specs,
        )
    }

    pub fn configured_specs(
        &self,
    ) -> &[crate::core::tools::spec::ConfiguredToolSpec] {
        &self.configured_specs
    }
}
```

- [ ] **Step 4: Update spec collection to read from configured specs**

Still in `router.rs`, replace the current `collect_tool_specs()` body with one driven by assembled specs:

```rust
pub fn collect_tool_specs(&self) -> Vec<serde_json::Value> {
    let mut specs: Vec<serde_json::Value> = self
        .configured_specs
        .iter()
        .map(|configured| match &configured.spec {
            crate::core::tools::spec::ToolSpec::Function {
                name,
                description,
                strict,
                parameters,
            } => serde_json::json!({
                "type": "function",
                "name": name,
                "description": description,
                "strict": strict,
                "parameters": parameters,
            }),
        })
        .collect();

    for spec in self.dynamic_tools.values() {
        specs.push(serde_json::json!({
            "type": "function",
            "name": spec.name,
            "description": spec.description,
            "parameters": spec.input_schema,
        }));
    }

    specs
}
```

Keep the existing builtin/MCP/dynamic dispatch logic unchanged in phase 1.

- [ ] **Step 5: Run the router tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml from_config_
```

Expected:

- PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/tools/router.rs
git commit -m "refactor: build tool router from assembled runtime"
```

## Task 3: Export the new runtime assembly types cleanly

**Files:**
- Modify: `src-tauri/src/core/tools/mod.rs`
- Test: `src-tauri/src/core/tools/mod.rs` and downstream compile checks in router/session tests

- [ ] **Step 1: Write the failing compile-oriented usage test**

Add or extend a simple test that exercises the new public exports:

```rust
#[test]
fn tools_module_exposes_runtime_assembly_types() {
    let _config = crate::core::tools::spec::ToolsConfig::default();
    let assembled = crate::core::tools::spec::build_specs(&_config, false);
    assert!(!assembled.configured_specs.is_empty());
}
```

- [ ] **Step 2: Run the focused test to verify any missing exports fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml tools_module_exposes_runtime_assembly_types
```

Expected:

- FAIL if `mod.rs` does not expose the needed spec-layer types for router/session usage.

- [ ] **Step 3: Re-export the phase-1 assembly types from `mod.rs`**

Update `src-tauri/src/core/tools/mod.rs` with explicit re-exports:

```rust
pub use spec::AssembledToolRuntime;
pub use spec::ConfiguredToolSpec;
pub use spec::ToolRegistryBuilder;
pub use spec::ToolsConfig;
pub use spec::build_specs;
```

Do not remove the existing `ToolRegistry`, `ToolHandler`, or `ToolKind` definitions in phase 1.

- [ ] **Step 4: Run the focused test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml tools_module_exposes_runtime_assembly_types
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/tools/mod.rs
git commit -m "refactor: re-export tool runtime assembly interfaces"
```

## Task 4: Replace session hand-registration with config-driven assembly

**Files:**
- Modify: `src-tauri/src/core/session.rs`
- Test: `src-tauri/src/core/session.rs`

- [ ] **Step 1: Write failing session bootstrap tests**

Add inline tests that lock down the stable default runtime surface:

```rust
#[test]
fn session_default_router_contains_stable_tools_only() {
    let session = Session::new(
        std::path::PathBuf::from("."),
        ConfigLayerStack::new(),
        async_channel::unbounded().0,
    );

    let router = tokio_test::block_on(session.tool_router());
    let names: Vec<String> = router
        .configured_specs()
        .iter()
        .map(|spec| spec.spec.name().to_string())
        .collect();

    assert!(names.contains(&"shell".to_string()));
    assert!(names.contains(&"apply_patch".to_string()));
    assert!(names.contains(&"list_dir".to_string()));
    assert!(names.contains(&"read_file".to_string()));
    assert!(names.contains(&"grep_files".to_string()));
    assert!(!names.contains(&"request_user_input".to_string()));
    assert!(!names.contains(&"exec_command".to_string()));
}
```

Add a second test for multi-agent assembly:

```rust
#[test]
fn session_with_agent_control_adds_collab_specs() {
    let tx = async_channel::unbounded().0;
    let ctrl = std::sync::Arc::new(crate::core::agent::control::AgentControl::new(
        3,
        std::path::PathBuf::from("."),
        crate::protocol::types::SandboxPolicy::DangerFullAccess,
        tx.clone(),
    ));

    let session = Session::new_with_agent_control(
        std::path::PathBuf::from("."),
        ConfigLayerStack::new(),
        tx,
        Some(ctrl),
    );

    let router = tokio_test::block_on(session.tool_router());
    let names: Vec<String> = router
        .configured_specs()
        .iter()
        .map(|spec| spec.spec.name().to_string())
        .collect();

    assert!(names.contains(&"spawn_agent".to_string()));
    assert!(names.contains(&"send_input".to_string()));
}
```

- [ ] **Step 2: Run the session tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml session_default_router_contains_stable_tools_only
```

Expected:

- FAIL because `Session` still constructs the router from manual `registry.register(...)` calls.

- [ ] **Step 3: Replace the hard-coded registration block**

Update `Session::new_with_agent_control()` in `src-tauri/src/core/session.rs` so the constructor derives phase-1 config and builds the router through the new assembly path:

```rust
let tools_config = crate::core::tools::ToolsConfig {
    shell_enabled: true,
    apply_patch_enabled: true,
    list_dir_enabled: true,
    read_file_enabled: true,
    grep_files_enabled: true,
    collab_tools: agent_control.is_some(),
    ..Default::default()
};

let mut router = crate::core::tools::router::ToolRouter::from_config(
    tools_config,
    agent_control.is_some(),
);

if let Some(ctrl) = agent_control {
    router
        .registry_mut()
        .register(Box::new(
            crate::core::tools::handlers::multi_agents::MultiAgentHandler::new(ctrl, 0),
        ));
}
```

Then store that router instead of constructing one from a manually populated `ToolRegistry`.

This preserves phase-1 behavior:

- builtin specs come from assembly,
- multi-agent specs come from assembly when enabled,
- the session still injects the concrete `MultiAgentHandler` because it requires runtime `AgentControl`.

- [ ] **Step 4: Run the session tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml session_default_router_contains_stable_tools_only
cargo test --manifest-path src-tauri/Cargo.toml session_with_agent_control_adds_collab_specs
```

Expected:

- PASS

- [ ] **Step 5: Run a broader non-regression test slice**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml core::tools::
cargo test --manifest-path src-tauri/Cargo.toml core::session::
```

Expected:

- PASS for the affected `core/tools` and `core/session` test slices.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/session.rs
git commit -m "refactor: assemble session tools from runtime config"
```

## Task 5: Final verification and documentation sync

**Files:**
- Modify: `docs/core-tools-codex-main-vs-mosaic-analysis.md`
- Modify: `docs/superpowers/specs/2026-03-29-tools-runtime-skeleton-design.md` only if implementation forced a design clarification
- Test: repository test commands only

- [ ] **Step 1: Re-read the analysis and spec documents against the implemented shape**

Verify these statements remain true:

- Session no longer hand-registers builtin tools.
- Tool assembly now has a single source of truth.
- Only stable phase-1 tools are exposed by default.
- Incomplete handlers remain excluded from the default runtime.

- [ ] **Step 2: Update the analysis doc if the implementation changes wording**

If needed, adjust `docs/core-tools-codex-main-vs-mosaic-analysis.md` so the “Mosaic 当前默认注册工具” section reflects the new config-driven assembly path instead of direct hard-coded registration.

Suggested replacement text:

```md
`Mosaic` 当前默认运行时仍只暴露基础稳定工具，但其装配方式已从 `Session` 内部硬编码注册迁移为 `core/tools/spec.rs` 驱动的统一构建链路。
```

- [ ] **Step 3: Run the full targeted verification commands**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml build_specs_
cargo test --manifest-path src-tauri/Cargo.toml from_config_
cargo test --manifest-path src-tauri/Cargo.toml session_default_router_contains_stable_tools_only
cargo test --manifest-path src-tauri/Cargo.toml session_with_agent_control_adds_collab_specs
```

Expected:

- PASS

- [ ] **Step 4: Run formatting if needed**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all
```

Expected:

- Exit code 0

- [ ] **Step 5: Commit**

```bash
git add docs/core-tools-codex-main-vs-mosaic-analysis.md docs/superpowers/specs/2026-03-29-tools-runtime-skeleton-design.md src-tauri/src/core/tools/spec.rs src-tauri/src/core/tools/router.rs src-tauri/src/core/tools/mod.rs src-tauri/src/core/session.rs
git commit -m "refactor: add phase-1 tools runtime skeleton"
```

## Self-Review Checklist

- Spec coverage: this plan covers the approved phase-1 scope only: `spec + registry + router + session`.
- Placeholder scan: no task relies on “implement later” behavior; unfinished handlers are explicitly excluded from phase-1 runtime assembly.
- Type consistency: the plan uses one assembly vocabulary throughout:
  - `ToolsConfig`
  - `ConfiguredToolSpec`
  - `ToolRegistryBuilder`
  - `AssembledToolRuntime`
  - `ToolRouter::from_config(...)`
