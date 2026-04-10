# Core Tools Runtime Parity Remaining Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完成 `Mosaic-Desktop` 剩余 `core/tools` 运行时语义对拍，使清单中尚未完成的工具都具备明确的对拍证据、必要的代码修正和回归验证。

**Architecture:** 继续沿用“先补失败测试，再做最小实现修正，再更新清单”的 TDD 节奏。优先收敛可观察 contract 差异，其次收敛上下文解析、错误传播和 manager/runtime 行为，最后处理仍明显落后的 runtime 级工具。

**Tech Stack:** Rust, Tokio, Tauri, async-channel, serde/serde_json, cargo test

---

### Task 1: 收尾 MCP 搜索与动态路由工具

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/search_tool_bm25.rs`
- Modify: `src-tauri/src/core/tools/router.rs`
- Modify: `src-tauri/src/core/tools/handlers/dynamic.rs`
- Modify: `src-tauri/src/core/tools/handlers/mcp.rs`
- Modify: `src-tauri/tests/tool_handler_tests.rs`
- Modify: `docs/core-tools-runtime-parity-checklist.md`

- [ ] **Step 1: 为 `search_tool_bm25` 写失败测试**
  目标：
  - parse error 前缀必须是 `failed to parse function arguments: ...`
  - 空 manager、空结果、limit=0、空 query 的返回与错误语义要有测试
  - 若存在 active selection 合并语义，补一条回归测试

- [ ] **Step 2: 跑 `search_tool_bm25` 定点测试确认红灯**

  Run: `cargo test search_tool_bm25 --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 3: 最小修正 `search_tool_bm25`**
  约束：
  - 不引入与 codex-main 无关的新字段
  - 仅收敛 parse error / 空结果 / selection merge 的可观察语义

- [ ] **Step 4: 为 `mcp__<server>__<tool>` 与 `dynamic tools` 写失败测试**
  目标：
  - MCP 动态工具：server/tool 不存在、MCP 错误透传、路由命中、返回 envelope
  - Dynamic tools：未注册、重复 resolve、无 handler 场景、router `DynamicTool(...)` 行为

- [ ] **Step 5: 跑路由与动态工具测试确认红灯**

  Run: `cargo test dynamic --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test mcp__ --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 6: 最小修正 router / dynamic / MCP route 行为**
  约束：
  - 优先修正错误消息、路由 fallback、response 包装
  - 不做大规模 router 重构

- [ ] **Step 7: 回归验证**

  Run: `cargo test search_tool_bm25 --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test dynamic --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test mcp_resource --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 8: 更新清单**
  将 `search_tool_bm25`、`mcp__<server>__<tool>`、`dynamic tools` 的状态与证据写回 `docs/core-tools-runtime-parity-checklist.md`。

### Task 2: 收尾 Multi-Agent 工具族

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/multi_agents.rs`
- Modify: `src-tauri/src/core/codex.rs`
- Modify: `src-tauri/src/protocol/types.rs`
- Modify: `src-tauri/tests/tool_handler_tests.rs`
- Modify: `docs/core-tools-runtime-parity-checklist.md`

- [ ] **Step 1: 为 `spawn_agent` / `send_input` / `resume_agent` / `wait` / `close_agent` 写失败测试**
  目标：
  - 基本成功路径
  - 不存在 target 的错误
  - `interrupt`、`fork_context`、`items/message` 的兼容行为
  - `wait` 的 timeout/空结果/完成态包装

- [ ] **Step 2: 跑 Multi-Agent 定点测试确认红灯**

  Run: `cargo test multi_agents --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 3: 最小修正工具语义**
  约束：
  - 先对齐错误消息、返回 envelope、空值处理
  - 不在这一步大改 agent 子系统生命周期

- [ ] **Step 4: 回归验证**

  Run: `cargo test multi_agents --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 5: 更新清单**

### Task 3: 评估并修正 Agent Jobs 明显落后项

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/agent_jobs.rs`
- Modify: `src-tauri/tests/tool_handler_tests.rs`
- Modify: `docs/core-tools-runtime-parity-checklist.md`

- [ ] **Step 1: 先确认 `spawn_agents_on_csv` / `report_agent_job_result` 的当前真实能力**
  目标：
  - 明确哪些是 contract-only
  - 哪些是 TODO runtime 缺失

- [ ] **Step 2: 为当前能对齐的 parse error / envelope / not-found 行为写失败测试**

- [ ] **Step 3: 做最小修正**
  约束：
  - 如果 runtime 确实缺失且无法在本轮补完，至少把错误语义、禁用语义和清单状态说清楚
  - 若能补齐简单 runtime，优先补齐

- [ ] **Step 4: 跑定点回归**

  Run: `cargo test agent_jobs --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 5: 更新清单**

### Task 4: 收尾 `js_repl` / `js_repl_reset` / `presentation_artifact`

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/js_repl.rs`
- Modify: `src-tauri/src/core/tools/handlers/presentation_artifact.rs`
- Modify: `src-tauri/tests/tool_handler_tests.rs`
- Modify: `docs/core-tools-runtime-parity-checklist.md`

- [ ] **Step 1: 先补失败测试，确认当前与 codex-main 的最小差异**
  目标：
  - `js_repl` / `js_repl_reset` 的 parse error、reset 行为、session 缺失行为
  - `presentation_artifact` 的输入校验与错误 envelope

- [ ] **Step 2: 跑定点测试确认红灯**

  Run: `cargo test js_repl --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test presentation_artifact --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 3: 做最小语义修正**
  约束：
  - 不在没有必要时扩张功能面
  - 先收敛 parse error / reset / not-found / feature gate 的外显行为

- [ ] **Step 4: 跑回归**

  Run: `cargo test js_repl --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test presentation_artifact --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 5: 更新清单**

### Task 5: 收尾 `test_sync_tool` / `view_image` / `web_search`

**Files:**
- Modify: `src-tauri/src/core/tools/handlers/test_sync.rs`
- Modify: `src-tauri/src/core/tools/handlers/view_image.rs`
- Modify: `src-tauri/src/core/session.rs`
- Modify: `src-tauri/src/core/tools/spec.rs`
- Modify: `src-tauri/tests/tool_handler_tests.rs`
- Modify: `src-tauri/tests/tool_contract_alignment_tests.rs`
- Modify: `docs/core-tools-runtime-parity-checklist.md`

- [ ] **Step 1: 为 `test_sync_tool` / `view_image` 补失败测试**
  目标：
  - barrier 行为
  - 路径不存在 / 非图片 / 返回结构

- [ ] **Step 2: 为 `web_search` 暴露条件写失败测试**
  目标：
  - `Cached/Live/Disabled`
  - `external_web_access`
  - session 配置优先级

- [ ] **Step 3: 跑测试确认红灯**

  Run: `cargo test test_sync --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test view_image --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test web_search --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 4: 最小修正**

- [ ] **Step 5: 回归验证**

  Run: `cargo test test_sync --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test view_image --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test web_search --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 6: 更新清单**

### Task 6: 最终总回归与清单收口

**Files:**
- Modify: `docs/core-tools-runtime-parity-checklist.md`

- [ ] **Step 1: 跑剩余工具全集关键过滤测试**

  Run: `cargo test search_tool_bm25 --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test mcp_resource --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test dynamic --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test multi_agents --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test js_repl --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test presentation_artifact --manifest-path src-tauri/Cargo.toml -- --nocapture`
  Run: `cargo test view_image --manifest-path src-tauri/Cargo.toml -- --nocapture`

- [ ] **Step 2: 扫描清单中的 `[ ]` / `[!]`**

  Run: `rg -n "^- \\[( |!)]" docs/core-tools-runtime-parity-checklist.md`
  Expected: 只剩确实无法在本轮补齐且已明确注明原因的项；若还能修，继续修到收口。

- [ ] **Step 3: 最终更新清单结论**
  目标：
  - 对每个工具明确写出“已验证什么”
  - 只保留真实无法本轮追平的差距
  - 给出当前 `Mosaic` 与 `codex-main` 的严谨结论
