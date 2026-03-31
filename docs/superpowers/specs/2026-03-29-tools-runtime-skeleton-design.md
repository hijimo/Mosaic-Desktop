# Tools Runtime Skeleton Design

## Overview

This spec defines phase 1 of the `core/tools` retrofit for Mosaic.

The goal of this phase is architectural, not feature-complete parity. Mosaic currently exposes tools through a hand-built registration path in `core/session.rs`, while `codex-main` uses a composable tool assembly pipeline centered on `tools/spec.rs`, `tools/registry.rs`, and `tools/router.rs`. That difference is now the main source of friction: adding or enabling tools requires touching session bootstrap code directly, feature gating is inconsistent, and many existing handlers cannot be cleanly introduced without more refactoring.

Phase 1 replaces the ad hoc registration path with a configurable tool runtime skeleton. It does not attempt to make all existing handlers usable. Instead, it introduces the structure needed so later phases can enable more tools without reworking session startup again.

## Goals

- Replace the current hand-written tool registration path with a config-driven assembly flow.
- Introduce a Mosaic equivalent of `ConfiguredToolSpec` and `ToolRegistryBuilder`.
- Move tool spec construction responsibility into `core/tools/spec.rs`.
- Make `ToolRouter` constructible from a single assembled tool configuration instead of a manually populated registry.
- Preserve the currently stable tool surface:
  - `shell`
  - `apply_patch`
  - `list_dir`
  - `read_file`
  - `grep_files`
  - multi-agent tools when agent control is available
- Keep room for future phases to wire:
  - `exec_command` / `write_stdin`
  - MCP tools
  - `request_user_input`
  - `view_image`
  - `js_repl`
  - `presentation_artifact`

## Non-Goals

- No attempt in phase 1 to make all existing handlers executable.
- No MCP tool delegation rewrite in this phase.
- No UI event loop work for `request_user_input`.
- No JavaScript REPL runtime integration.
- No artifact subsystem implementation.
- No web search implementation.
- No user-visible expansion of the default tool surface beyond what Mosaic already supports reliably.

## Current Problems

### 1. Session bootstrap is the registration source of truth

`Session::new_with_agent_control()` directly registers builtin handlers in place. This makes the session constructor responsible for:

- tool availability,
- handler selection,
- future feature rollout,
- conditional multi-agent wiring.

That is workable for five tools but does not scale to the current handler set.

### 2. Tool specs and runtime registration are disconnected

Mosaic has tool spec helpers and handler modules, but there is no single assembly point that says:

- which tools exist,
- which specs are exposed,
- which handlers back them,
- which tools support parallel execution,
- which tools are disabled by configuration.

As a result, the codebase contains many handler implementations that are not part of the runtime story.

### 3. Router capabilities are too narrow

The current `ToolRouter` is a thin wrapper over:

- a simple builtin registry,
- a partially implemented MCP path,
- a dynamic tools map.

It is missing the richer assembly boundary present in `codex-main`, where the router is built from an already configured runtime package.

### 4. Future handler rollout would cause repeated churn

If Mosaic starts enabling `exec_command`, MCP resources, `request_user_input`, or `view_image` on the current structure, each tool will require custom session bootstrap edits. That would cause repeated changes to constructor code instead of extending a stable tool assembly layer.

## Evaluated Approaches

### Approach A: Minimal patch on `Session`

Keep the current registry design and only move the hard-coded registration lines into a helper function.

Pros:

- smallest diff,
- lowest short-term risk.

Cons:

- preserves the same architectural bottleneck,
- does not create a durable extension point,
- still mixes session construction with tool policy.

This approach was rejected because it would not materially improve the migration path.

### Approach B: Full skeleton retrofit without enabling unfinished tools

Introduce the codex-main-like assembly structure now:

- `ToolsConfig`
- `ConfiguredToolSpec`
- `ToolRegistryBuilder`
- `build_specs(...)`
- `ToolRouter::from_config(...)`

Then only register the tools Mosaic already supports well.

Pros:

- fixes the architecture first,
- keeps user-visible behavior stable,
- avoids prematurely exposing stub tools,
- makes later migration phases incremental.

Cons:

- larger initial refactor than a helper extraction,
- some structure lands before every consumer exists.

This is the recommended and approved approach.

### Approach C: Register every existing handler immediately

Build the new assembly layer and register all handler files, even if some still return placeholder errors.

Pros:

- fastest path to apparent parity.

Cons:

- increases user-visible inconsistency,
- exposes many tools that are not actually ready,
- creates testing ambiguity about what is intentionally disabled vs accidentally incomplete.

This approach was rejected for phase 1.

## Approved Design

Phase 1 will implement a new runtime assembly layer for Mosaic tools and migrate session startup to use it.

The runtime will be capable of describing:

- the exposed tool specs,
- the backing handlers,
- parallel-call support metadata,
- conditional inclusion for stable tool groups.

However, only the currently stable tools will be enabled by default.

## Architecture

### 1. `ToolsConfig` becomes the phase-1 assembly contract

Mosaic will define a local `ToolsConfig` in `core/tools/spec.rs` or a nearby tool assembly module.

Its purpose in phase 1 is narrower than `codex-main`:

- describe which stable tool groups should be assembled,
- carry collaboration-mode relevant flags for multi-agent tools,
- leave explicit extension points for future additions.

Phase 1 does not need to mirror every codex-main field. It only needs enough structure to stop hard-coding registrations in `Session`.

Suggested phase-1 fields:

- `shell_enabled`
- `apply_patch_enabled`
- `read_file_enabled`
- `grep_files_enabled`
- `list_dir_enabled`
- `collab_tools`

Optional future-compatible fields may be added if they are cheap and do not complicate behavior:

- `request_user_input_enabled`
- `view_image_enabled`
- `mcp_resources_enabled`
- `unified_exec_enabled`

These future-facing fields may exist without being enabled or fully wired in phase 1.

### 2. Introduce `ConfiguredToolSpec`

Mosaic should gain a type equivalent in purpose to codex-main’s configured spec wrapper:

- one field for the tool spec payload sent to the model,
- one field for whether the tool supports parallel calls.

This solves two problems:

- spec collection no longer depends on walking handler objects,
- parallel support becomes declarative instead of implied.

### 3. Introduce `ToolRegistryBuilder`

Instead of registering handlers directly into `ToolRegistry` during session construction, Mosaic will use a builder that accumulates:

- specs,
- handler registrations,
- per-tool parallel metadata.

The builder will then produce:

- the finalized `ToolRegistry`,
- the finalized configured specs list.

This keeps runtime assembly in one place and makes `ToolRouter` construction deterministic.

### 4. `build_specs(...)` becomes the new assembly entry point

Mosaic should add a phase-1 builder function, conceptually similar to codex-main:

- input: `ToolsConfig`, plus optional session capabilities such as agent control presence,
- output: assembled specs plus registry.

This function is where phase-1 enablement policy lives.

It should:

- register stable builtin tools,
- optionally register multi-agent tools when agent control exists,
- leave unfinished tools out of the assembled runtime.

This function becomes the single source of truth for default tool exposure.

### 5. `ToolRouter` should be constructible from assembled runtime state

Phase 1 should add a construction path similar in intent to `ToolRouter::from_config(...)`.

Mosaic does not need byte-for-byte parity with codex-main, but it should reach the same separation of concerns:

- `spec.rs` assembles runtime contents,
- `router.rs` owns dispatch,
- `session.rs` only asks for a configured router.

The resulting session bootstrap should stop manually calling `registry.register(...)`.

### 6. Session bootstrap becomes thin

After the refactor, `Session::new_with_agent_control()` should only:

- derive the phase-1 `ToolsConfig`,
- invoke the runtime assembly path,
- create the router from that assembled runtime.

This keeps session construction focused on session state instead of tool policy.

## Phase-1 Tool Exposure

### Included

- `shell`
- `apply_patch`
- `list_dir`
- `read_file`
- `grep_files`
- `spawn_agent`
- `send_input`
- `resume_agent`
- `wait`
- `close_agent`

### Explicitly excluded for now

- `exec_command`
- `write_stdin`
- `shell_command`
- `list_mcp_resources`
- `list_mcp_resource_templates`
- `read_mcp_resource`
- `update_plan`
- `request_user_input`
- `search_tool_bm25`
- `js_repl`
- `js_repl_reset`
- `view_image`
- `presentation_artifact`
- `spawn_agents_on_csv`
- `report_agent_job_result`
- `test_sync_tool`
- direct MCP tool registration

Excluded here means “not assembled into the default runtime in phase 1,” not “delete the files.”

## File-Level Design

### Files to modify

- `src-tauri/src/core/tools/spec.rs`
  - add phase-1 `ToolsConfig`
  - add `ConfiguredToolSpec`
  - add `ToolRegistryBuilder`
  - add `build_specs(...)`

- `src-tauri/src/core/tools/mod.rs`
  - keep shared tool types or move builder-facing types if needed
  - adjust interfaces so spec collection is driven by configured specs rather than handler-only discovery

- `src-tauri/src/core/tools/router.rs`
  - add assembled-runtime construction path
  - keep builtin + MCP + dynamic routing behavior consistent with current Mosaic behavior

- `src-tauri/src/core/session.rs`
  - replace hard-coded `registry.register(...)` calls with assembled router creation

### Files likely touched lightly

- `src-tauri/src/core/codex.rs`
  - only if session construction or dynamic tool integration needs a small constructor signature change

- `src-tauri/src/core/tools/parallel.rs`
  - only if parallel metadata source changes from ad hoc tool lists to configured specs

### Files intentionally out of scope

- all incomplete handler runtime implementations,
- MCP dispatch internals,
- UI interaction loop code,
- JS REPL runtime assets,
- artifact subsystem.

## Error Handling

Phase 1 should keep error behavior conservative:

- if a tool is not assembled, it should not appear in specs and should not route,
- if a multi-agent tool is requested without agent control, it should not be assembled,
- router behavior for unfinished MCP delegation should remain unchanged in this phase.

The key principle is to avoid exposing tools that are not actually ready.

## Testing Strategy

Phase 1 needs focused assembly tests, not broad handler behavior tests.

Required coverage:

- `build_specs(...)` assembles the stable builtin tools by default
- multi-agent tools are only assembled when agent control is available
- excluded tools are absent from assembled specs in phase 1
- `ToolRouter` constructed from assembled specs still routes:
  - `shell`
  - `apply_patch`
  - `list_dir`
  - `read_file`
  - `grep_files`
- dynamic tool registration behavior still works after router construction changes
- spec collection still returns the assembled tool specs expected by the model-facing code

Testing emphasis should be on:

- assembly correctness,
- registration correctness,
- non-regression of current default behavior.

## Risks

### 1. Silent spec/runtime mismatch

If tool specs and handler registration are assembled through different code paths, Mosaic could expose tools the router cannot dispatch. The design avoids this by making one builder own both.

### 2. Over-abstracting too early

If phase 1 copies every codex-main config flag mechanically, Mosaic will inherit complexity it does not yet use. The design avoids this by introducing only the fields needed for the stable runtime skeleton.

### 3. Breaking current tool availability

Because session bootstrap changes, there is risk of accidentally dropping currently working tools. This is why phase-1 tests must assert the exact stable tool set.

## Rollout Guidance

Phase 1 should land before any attempt to enable:

- unified exec,
- MCP resources,
- user input requests,
- image viewing,
- JS REPL.

Those later migrations should build on the new assembly layer instead of editing session bootstrap again.

## Success Criteria

Phase 1 is complete when:

- `Session` no longer hard-codes builtin tool registration lines,
- tool assembly has a single source of truth,
- the current stable tool surface remains available,
- unfinished tools are not exposed by default,
- future tool enablement can be added by extending the assembly layer instead of rewriting session construction.
