# codex-main vs Mosaic `core/tools` 对比分析

> 更新日期: 2026-03-31  
> 对比范围:
> - `codex-main`: `/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools`
> - `Mosaic`: `/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools`

## 结论摘要

`Mosaic` 的 `core/tools` 现在已经不是早期那种“只有少量 builtin tools 的最小骨架”。从目录结构、`build_specs(...)` 装配链、工具 spec 覆盖率来看，它已经明显向 `codex-main` 靠拢，并且 `web_search` 的 provider-native spec 也已经接到当前运行时里了。

但如果按“真实可用能力”衡量，`Mosaic` 仍然明显落后于 `codex-main`。差距主要不在有没有文件、有没有 tool spec，而在于：

1. `codex-main` 的工具系统是完整的平台级运行时，`build_specs(...)` 会同时装配 config、MCP tools、app tools、dynamic tools，并通过完整的 registry/router/context/runtime 分发。[codex-main spec.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/spec.rs#L1703) [codex-main router.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/router.rs#L39)
2. `Mosaic` 已经补上了大量工具 spec 和开关，但当前 `Session` 默认只打开基础 builtin tools；很多高级工具仍然需要显式配置才会暴露给模型。[Mosaic spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs#L88) [Mosaic session.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/session.rs#L325)
3. `Mosaic` 里一批 handler 仍是 stub 或半成品，尤其是 `request_user_input`、`js_repl`、`mcp_resource`、`search_tool_bm25`、network approval、MCP tool delegation。
4. 之前“`Mosaic` 没有接通 `web_search`”的结论已经过期。当前代码里，`web_search` 已经能按 config 进入当前 turn 的 `tool_specs`，并被发往 Responses API；它缺的不是 spec 暴露，而是 `codex-main` 那套更完整的整体验证、约束来源整合与外围运行时成熟度。[Mosaic spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs#L301) [Mosaic session.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/session.rs#L697)

## 1. 目录结构对比

两边共同拥有的大类基本一致：

- `context.rs`
- `events.rs`
- `handlers/*`
- `js_repl/*`
- `network_approval.rs`
- `orchestrator.rs`
- `parallel.rs`
- `router.rs`
- `sandboxing.rs`
- `spec.rs`

文件级差异：

### `codex-main` 独有

- `registry.rs`
- `runtimes/mod.rs`
- `runtimes/apply_patch.rs`
- `runtimes/shell.rs`
- `runtimes/shell/unix_escalation.rs`
- `runtimes/unified_exec.rs`
- `js_repl/kernel.js`
- `js_repl/meriyah.umd.min.js`

### `Mosaic` 独有

- `handlers/runtimes/mod.rs`
- `handlers/runtimes/apply_patch.rs`
- `handlers/runtimes/shell.rs`
- `handlers/runtimes/unified_exec.rs`
- `handlers/shell_command.rs`

这说明 `Mosaic` 仍然保留了 `codex-main` 的总体目录影子，但把部分 runtime 实现挪进了 `handlers/runtimes/*`，同时单独拆出了 `shell_command.rs`。

## 2. 架构差异

### `codex-main`

`codex-main` 的工具系统是完整装配式架构：

- `ToolsConfig` 由 model、features、session source 等因素共同生成
- `build_specs(...)` 同时消费 `mcp_tools`、`app_tools`、`dynamic_tools`
- `ToolRouter` 同时承担 tool spec 暴露、payload 解析、dispatch 入口
- `ToolRegistry` 分发时携带 `Session`、`TurnContext`、`ToolPayload`、`TurnDiffTracker`、hooks、telemetry、mutating tool gate 等上下文

关键文件：

- [spec.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/spec.rs)
- [router.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/router.rs)
- [registry.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/registry.rs)
- [context.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/context.rs)

### `Mosaic`

`Mosaic` 目前是轻量装配版：

- `ToolsConfig` 仍然是布尔开关 + `web_search_mode` 的简化模型
- `build_specs(...)` 已能装配大量工具，但主要只看本地 config 和 `has_agent_control`
- `ToolRegistry` 仍是“匹配 `ToolKind` 后传入 JSON 参数”的简化派发层
- `ToolRouter` 能管理 builtin / MCP-qualified / dynamic 三类名字，但 MCP tool delegation 尚未真正接通

关键文件：

- [spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs)
- [router.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/router.rs)
- [mod.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/mod.rs)
- [context.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/context.rs)

一句话概括：

- `codex-main`：完整工具平台
- `Mosaic`：工具 spec 覆盖面已显著扩张，但运行时仍是简化实现

## 3. handler 文件集合对比

两边 `handlers` 文件集合几乎一一对应，都有：

- `agent_jobs`
- `apply_patch`
- `dynamic`
- `grep_files`
- `js_repl`
- `list_dir`
- `mcp`
- `mcp_resource`
- `multi_agents`
- `plan`
- `presentation_artifact`
- `read_file`
- `request_user_input`
- `search_tool_bm25`
- `shell`
- `test_sync`
- `unified_exec`
- `view_image`

`Mosaic` 额外有：

- `shell_command`

因此现在的核心问题已经不是“有没有对应文件”，而是：

- 是否注册进运行时
- 是否默认暴露给模型
- 是否接通真实 subsystem
- 是否只是返回占位结果或错误

## 4. 工具装配与默认工具面

### `codex-main`

`codex-main` 的 `build_specs(...)` 会根据 shell 模式、feature 开关、MCP tools、dynamic tools、collab mode 等条件装配完整工具集，并显式注册：

- `shell`
- `container.exec`
- `local_shell`
- `shell_command`
- `exec_command`
- `write_stdin`
- `update_plan`
- `request_user_input`
- `apply_patch`
- `grep_files`
- `read_file`
- `list_dir`
- `test_sync_tool`
- `view_image`
- `presentation_artifact`
- `js_repl`
- `js_repl_reset`
- `list_mcp_resources`
- `list_mcp_resource_templates`
- `read_mcp_resource`
- `web_search`
- `spawn_agent`
- `send_input`
- `resume_agent`
- `wait`
- `close_agent`
- `spawn_agents_on_csv`
- `report_agent_job_result`
- 动态转换后的 MCP tools
- 动态 tools

见 [codex-main spec.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/spec.rs#L1750)。

### `Mosaic`

`Mosaic` 现在的 `build_specs(...)` 也已经能装配以下工具：

- `shell`
- `shell_command`
- `apply_patch`
- `list_dir`
- `read_file`
- `grep_files`
- `list_mcp_resources`
- `list_mcp_resource_templates`
- `read_mcp_resource`
- `exec_command`
- `write_stdin`
- `update_plan`
- `view_image`
- `request_user_input`
- `js_repl`
- `js_repl_reset`
- `test_sync_tool`
- `spawn_agents_on_csv`
- `report_agent_job_result`
- `presentation_artifact`
- `search_tool_bm25`
- `web_search`
- `spawn_agent`
- `send_input`
- `resume_agent`
- `wait`
- `close_agent`

见 [Mosaic spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs#L199)。

但默认 `ToolsConfig` 只打开了少量稳定工具：

- `shell`
- `apply_patch`
- `list_dir`
- `read_file`
- `grep_files`

其他能力默认关闭，包括 `shell_command`、`exec_command`、`update_plan`、`request_user_input`、`js_repl`、`view_image`、`mcp_resource`、`search_tool_bm25`、`presentation_artifact`、`agent_jobs`、`web_search`。[Mosaic spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs#L88)

另外，多代理工具只有在 `config.collab_tools && has_agent_control` 时才会进入 spec 列表；handler 则在 `Session` 创建后按 `agent_control` 注入 registry。[Mosaic spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs#L311) [Mosaic session.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/session.rs#L363)

## 5. 运行时成熟度差异

下面这些模块最能体现两边的差距。

### `request_user_input`

`Mosaic`：

- 已有 tool spec 和 handler
- 仍然固定走 `ModeKind::Default`
- 还没有真实 UI 事件发射和等待返回
- 当前会报错 `request_user_input requires UI integration to emit events and await responses`

见 [request_user_input.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/request_user_input.rs#L83)。

`codex-main`：

- 已在真实工具调用流里接通
- 能和 collaboration mode / event 系统整合

### `js_repl`

`Mosaic`：

- 已有 tool spec 和 handler
- 明确写了 `TODO: wire to actual js_repl runtime`
- `js_repl` / `js_repl_reset` 当前都会直接返回 runtime 缺失错误

见 [js_repl.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/js_repl.rs#L130)。

`codex-main`：

- 有完整 JS REPL runtime
- 使用 freeform tool 形态
- 配套 `kernel.js` / `meriyah.umd.min.js`

### `mcp_resource`

`Mosaic`：

- `list_mcp_resources` 只返回空结果
- `list_mcp_resource_templates` 只返回空结果
- `read_mcp_resource` 仍然直接报依赖 MCP connection manager

见 [mcp_resource.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/mcp_resource.rs#L220)。

`codex-main`：

- 已真正接到 MCP manager
- 能读取资源与模板

### `search_tool_bm25`

`Mosaic`：

- 这是“工具元数据搜索”，不是网页搜索
- 当前仍固定返回空工具列表
- 注释明确写着还没接 BM25 和 MCP tools

见 [search_tool_bm25.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/search_tool_bm25.rs#L91)。

`codex-main`：

- 会收集 MCP tools
- 构建 BM25 索引
- 返回匹配结果并更新工具选择状态

### `update_plan`

`Mosaic`：

- 已有 tool spec 和参数校验
- 当前只返回 `{"status":"Plan updated"}`
- 还没有真正发送 plan update event

见 [plan.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/plan.rs#L77)。

`codex-main`：

- 会发送 `PlanUpdate` 事件
- 与真实 collaboration mode 集成

### MCP tool dispatch

`Mosaic`：

- router 能识别 `mcp__{server}__{tool}` 名字
- 但如果连接存在而没有真实 registry entry，会直接返回 `call delegation is not yet implemented`

见 [router.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/router.rs#L74)。

`codex-main`：

- 会把 MCP tools 转成实际 tool specs
- 会注册真实 handler 并进入调用链

见 [codex-main spec.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/spec.rs#L1914)。

### network approval

`Mosaic`：

- `NetworkApprovalService` 仍然是 stub
- `begin()` 固定返回 `None`
- `finish()` 是 no-op

见 [network_approval.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/network_approval.rs#L29)。

`codex-main`：

- 有完整的 deferred / immediate approval 流程
- 会记录 blocked requests、用户选择、session-level 允许/拒绝状态

见 [codex-main network_approval.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/network_approval.rs#L161)。

## 6. Web Search 对比

### `codex-main`

`codex-main` 在 `tools/spec.rs` 中原生支持 `ToolSpec::WebSearch`：

- `cached` 对应 `external_web_access = false`
- `live` 对应 `external_web_access = true`

见 [codex-main spec.rs](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/spec.rs#L1868)。

### `Mosaic`

这部分是这次需要更新的重点。当前 `Mosaic` 已经不只是“有配置和事件模型”，而是已经把 `web_search` 接到了当前运行时的 tool spec 装配链里：

- `build_specs(...)` 会按 `web_search_mode` 装配 `web_search` spec
- `Session::tools_config_from_resolved_config(...)` 会把顶层 `web_search` 或 legacy `[tools].enable-web-search` 转成 `web_search_mode`
- `collect_tool_specs_for_current_turn()` 会在 turn 级别根据 sandbox policy 和 config requirements 再次解析并调整 `web_search`
- 最终 `tool_specs` 会被传给 Responses API

关键位置：

- [Mosaic spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs#L301)
- [Mosaic session.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/session.rs#L325)
- [Mosaic session.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/session.rs#L697)

当前语义：

- 顶层 `web_search = "cached" | "live"` 是 canonical 配置
- `[tools].enable-web-search = true` 只是 legacy toggle，在未显式设置顶层 `web_search` 时映射为 `live`
- `cached` 会暴露 `{"type":"web_search","external_web_access":false}`
- `live` 会暴露 `{"type":"web_search","external_web_access":true}`
- 如果 requirements 或 turn 级约束只允许 `disabled`，当前 turn 会直接移除 `web_search`

因此，更新后的结论应该是：

- `codex-main`：有成熟的 provider-native `web_search` tool spec 装配能力
- `Mosaic`：现在也已经有可工作的 provider-native `web_search` spec 暴露链路，模型在本轮确实可以拿到 `web_search`

但需要注意两点：

1. `web_search` 在两边都不是普通 builtin handler；它走的是 provider-native tool spec 路径，不在 `Mosaic` 的 builtin handler registry 里。
2. `Mosaic` 当前补齐的是“配置到 tool spec 暴露”这条链路，不等于所有外围运行时能力都已和 `codex-main` 等价，例如更完整的 requirements 来源整合、外围约束体系、整体成熟度仍有差距。

换句话说，旧结论“`Mosaic` 没有接通 `web_search`”已经不成立；更准确的说法应该是“`Mosaic` 已接通 `web_search` 的 provider-native spec 暴露链，但整体工具运行时成熟度仍低于 `codex-main`”。

## 7. 关键差异清单

### `codex-main` 明显更完整的部分

- 完整的 `ToolRegistry` / `ToolInvocation` / `ToolPayload` / `ToolOutput` 体系
- 完整的 mutating tool gate、telemetry、hooks、取消与中断处理
- MCP tool 动态转 tool spec 并参与调用
- 完整的 network approval
- 完整的 `request_user_input` UI 闭环
- 完整的 JS REPL runtime
- 完整的 MCP resource 读取
- 完整的 BM25 工具搜索
- 更成熟的 `exec_command` / `write_stdin` / shell 运行时集成
- 更成熟的 artifact / agent jobs 能力

### `Mosaic` 当前相对完整的部分

- `shell`
- `apply_patch`
- `list_dir`
- `read_file`
- `grep_files`
- `web_search` 的 provider-native spec 暴露链
- 多代理工具的基本装配与 registry 注入
- `dynamic_tools` 的基本注册与列举

### `Mosaic` 当前仍明显未完成的部分

- MCP tool 真正 delegation
- `request_user_input` 的真实交互闭环
- `js_repl` / `js_repl_reset` 的真实 Node runtime
- `mcp_resource` 与真实 MCP manager 的集成
- `search_tool_bm25` 与真实 MCP tools/BM25 的集成
- network approval
- `update_plan` 的真实事件链
- `presentation_artifact` 的 artifact subsystem
- `agent_jobs` 的真实 job orchestration

## 8. 对迁移工作的意义

如果目标是让 `Mosaic` 的 `core/tools` 继续向 `codex-main` 靠齐，优先级建议如下。

### 高优先级

- 接通 MCP tool dispatch
- 接通 `request_user_input`
- 补齐 `mcp_resource`
- 补齐 `search_tool_bm25`
- 补齐 network approval

### 中优先级

- 补齐 `js_repl`
- 补齐 `update_plan` 的真实事件链
- 补齐 `presentation_artifact`
- 提升 `exec_command` / `write_stdin` / `shell_command` 的上下文与权限集成

### 低优先级

- 补齐 `agent_jobs`
- 继续缩小 `ToolRegistry` / `ToolRouter` / `ToolCallRuntime` 与 `codex-main` 的上下文差距

## 9. 一句话总结

`Mosaic` 的 `core/tools` 现在更像“已接通大部分工具装配骨架、但运行时仍然偏轻量”的实现，而不再是只有少量 builtin tools 的最小骨架。特别是 `web_search` 这条结论需要更新：它已经接到当前运行时的 provider-native tool spec 暴露链里了；真正的差距主要集中在 MCP、UI 交互、runtime、approval 和上下文系统这些更深的部分。
