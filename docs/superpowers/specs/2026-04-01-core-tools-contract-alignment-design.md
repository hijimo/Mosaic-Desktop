# Mosaic Core Tools Contract Alignment Design

**Date:** 2026-04-01

## Goal

第一阶段将 `Mosaic` 的 `core/tools` 工具定义完全追平 `codex-main`，范围包括：

- 工具名称
- 工具形态：`function` / `freeform` / provider-native `web_search`
- 入参 schema、required 字段、描述文案、默认暴露规则
- 出参形状与调用约定
- `router` / `session` / `caller` / 前端接收层对新契约的适配

本阶段不要求所有工具执行语义追平。未具备完整运行时的工具，允许在执行阶段返回兼容错误，但不能停留在旧 schema、旧 tool shape 或旧 caller 契约。

## Non-Goals

本阶段不包含以下目标：

- 完整补齐所有工具运行时能力
- 完全重构 `Mosaic` 工具运行时到底层架构与 `codex-main` 一致
- 一次性解决 approval、artifact subsystem、agent jobs backend、sandbox runtime 的所有实现差距
- 优化与本阶段无关的 UI 表现或文档结构

## Scope

范围以 `codex-main/codex-rs/core/src/tools` 当前会进入模型工具面的全部工具为准，而不是只对齐 `Mosaic` 已声明的 builtin 工具。

需要纳入本阶段的工具分组如下。

### Shell And Patch

- `shell`
- `local_shell`
- `exec_command`
- `write_stdin`
- `shell_command`
- `apply_patch`

### File And Local Utility

- `list_dir`
- `read_file`
- `grep_files`
- `view_image`
- `test_sync_tool`

### Collaboration

- `update_plan`
- `request_user_input`
- `spawn_agent`
- `send_input`
- `resume_agent`
- `wait`
- `close_agent`

### Agent Jobs

- `spawn_agents_on_csv`
- `report_agent_job_result`

### MCP And Dynamic

- `list_mcp_resources`
- `list_mcp_resource_templates`
- `read_mcp_resource`
- MCP tool spec 暴露
- dynamic tool spec 暴露

### Higher-Level Tools

- `js_repl`
- `js_repl_reset`
- `presentation_artifact`
- `search_tool_bm25`
- `web_search`

## Design Principles

### 1. 先追平契约，再追平语义

第一阶段只解决“模型看到了什么工具、如何调用、调用结果长什么样”这件事。  
真正的执行语义差距留给第二阶段处理。

### 2. 以 `codex-main` 的 spec 和调用链为单一真源

对齐时不再以 `Mosaic` 当前行为为基准做折中。若两边冲突，以 `codex-main` 当前 `core/tools` 实现为准。

### 3. 先固定工具面，再改调用方

只有在工具定义、默认暴露面、tool shape 固定后，`router`、`session`、`codex`、前端事件层的适配才有稳定目标。  
调用方不得继续依赖旧的 `Mosaic` 私有契约。

### 4. 错误可以保留，但位置必须后移

未实现能力允许失败，但失败必须发生在具体 handler/runtime 层，而不是：

- 工具未暴露
- schema 不兼容
- tool type 错误
- caller 无法理解返回值

## Architecture

第一阶段分成两层推进。

### Layer 1: Tool Contract Alignment

直接对齐 `spec.rs`、tool builder、handler 入参解析和出参封装。

这一层负责解决：

- 工具是否存在
- 工具是否默认暴露
- 是 `function` 还是 `freeform`
- schema 是否一致
- handler 接收和返回的数据形状是否一致

### Layer 2: Tool Invocation Alignment

在契约稳定后，改造 `router`、`session`、`codex`、前端事件接收层，使整个调用链能处理与 `codex-main` 一致的工具面。

这一层负责解决：

- model 返回不同类型 tool call 后如何路由
- MCP tool 和 dynamic tool 如何被加入 tool specs
- freeform/function/custom/provider-native tool 的 caller 适配
- 默认工具暴露面如何与 turn 配置联动

## Component Design

### `spec.rs`

`spec.rs` 是本阶段的中心。需要把 `Mosaic` 现有的简化 `ToolsConfig` 和 spec builder，调整为至少能表达以下信息：

- shell tool 类型选择
- `apply_patch` 的 tool type 选择
- `request_user_input` 默认模式开关
- `presentation_artifact` 开关
- `agent_jobs` / worker tools 开关
- `experimental_supported_tools`
- `web_search_mode`
- collab tools 开关
- MCP tools / app tools / dynamic tools 注入

是否完全复制 `codex-main` 的内部类型可以再斟酌，但外部行为必须一致。

### `router.rs`

`router` 需要从当前“内建 + 运行时兜底的 MCP + 动态工具表”的模式，提升到更接近 `codex-main` 的调用约定：

- 能区分 function call、custom/freeform call、provider-native tool
- 能处理 MCP tool spec 已暴露后的调用路径
- 能保证 dynamic tool 不只是 runtime 上可调用，也能进入模型工具面

第一阶段不强求完全复刻 `codex-main` 的内部类型，但要让行为结果与调用方观察到的契约一致。

### `session.rs`

`session` 需要从当前“默认只打开少数稳定工具”的策略，调整为与 `codex-main` 的默认暴露规则一致。  
这包括：

- `collect_tool_specs_for_current_turn()` 的输出集合
- `web_search_mode` 的注入与约束
- collab tools 默认暴露
- `request_user_input` / `view_image` / `update_plan` 等标准工具的默认可见性

### `codex.rs` And Frontend Callers

调用方需要适配新的工具形态：

- `apply_patch` 可能不再只是 JSON function tool
- `js_repl` 必须按 freeform/custom tool 处理
- `view_image`、`request_user_input`、MCP resource、agent jobs 的输出形状要与 `codex-main` 一致
- MCP/dynamic tool 出现在工具面后，前后端不能再假设工具列表只来自 builtin 集合

### MCP And Dynamic Tool Exposure

这是当前 `Mosaic` 与 `codex-main` 最关键的契约差距之一。  
第一阶段必须补齐：

- MCP tools 从 manager/tool info 转换成模型可见 spec
- dynamic tools 进入模型工具面
- `list_all_tools()` 与 `collect_tool_specs()` 在 builtin/MCP/dynamic 三类工具上行为一致

不要求第一阶段就补齐所有 MCP tool 执行语义细节，但至少要让“模型能看到并发起调用”这条链路对齐。

## Data Flow

### Tool Spec Collection

目标流向：

1. `session` 根据当前配置、features、turn 约束生成 tools config
2. `build_specs(...)` 同时装配 builtin、MCP tools、dynamic tools
3. `router` 保留配置后的 spec 列表
4. `collect_tool_specs_for_current_turn()` 输出与 `codex-main` 对齐的工具面

### Tool Invocation

目标流向：

1. 模型按对齐后的 tool shape 发起调用
2. `codex` / `router` 能识别 function、freeform/custom、provider-native tool
3. 调用进入对应 handler
4. handler 返回与 `codex-main` 对齐的结构
5. 前端或后续 caller 能消费该结构，即使运行时最终返回“未实现”

## Error Handling

本阶段采用两级错误策略。

### Contract Errors

这类错误必须消除：

- 缺少工具
- schema 不兼容
- required 字段不一致
- tool type 错误
- caller 不认识返回类型

### Runtime Gaps

这类错误允许保留，但必须标准化：

- 工具已暴露，但执行时返回 `runtime not implemented`
- 工具已接入 caller，但底层 subsystem 不存在
- approval/artifact/agent_jobs 等高层能力尚未实装

这些错误要在 handler 或 runtime 层显式返回，且返回格式要与 `codex-main` 的调用约定兼容。

## Testing Strategy

第一阶段测试重点不是业务成功，而是契约成功。

### Rust Tests

- 对比 spec 集合中工具名是否齐全
- 对比每个工具的 `type`
- 对比每个工具的 parameters schema、required 字段、描述
- 对比默认暴露规则
- 对比 MCP/dynamic tool 是否进入 spec 列表
- 对比 router 是否能处理新的 tool call 形态

### Frontend And Integration Tests

- 验证前端能消费新的工具输出形状
- 验证 freeform/custom tool 渲染与提交链路不再依赖旧契约
- 验证默认暴露工具面变化后，现有对话流不会因未知工具类型崩掉

### Contract Snapshot

建议增加一类契约快照测试：

- 从 `codex-main` 导出目标工具契约
- 对 `Mosaic` 当前生成结果做逐项比对

这样可以防止后续再次偏离。

## Rollout Plan

第一阶段按以下顺序实施：

1. 建立工具对齐矩阵，列出全部工具及其目标契约
2. 重写 `spec.rs` 及 config builder，使默认暴露面与 `codex-main` 一致
3. 处理 tool shape 差异最大的工具：`apply_patch`、`js_repl`、MCP/dynamic tools、`web_search`
4. 处理标准 function tools 的 schema 与出参封装
5. 调整 `router` / `session` / `codex` / 前端 caller
6. 增加契约测试和快照测试
7. 最后保留运行时未实现项，并把它们降级为执行期兼容错误

## Acceptance Criteria

第一阶段完成时，必须满足以下条件：

1. 在相同配置前提下，`Mosaic` 与 `codex-main` 的工具名集合一致
2. 每个工具的 `type`、schema、required 字段、描述文案、默认暴露规则一致，或仅保留明确记录的最小兼容差异
3. MCP tools 与 dynamic tools 能进入模型工具面
4. caller 能处理 `function`、`freeform/custom`、provider-native `web_search`
5. 未实装能力不再表现为“工具缺失/契约不兼容”，而是执行期错误

## Risks

### 1. 默认暴露面同步会冲击现有调用方

这是本阶段最大风险。很多旧代码默认假设：

- 只有少数 builtin 会出现
- 所有工具都是 JSON function tool
- MCP/dynamic tools 不会出现在工具面

因此调用方适配必须与 spec 对齐同一期完成，不能后置。

### 2. `apply_patch` 与 `js_repl` 的 tool shape 差异会带来较大改动

这两个工具不是简单 schema 改动，而是调用类型与出参通道都不同，必须优先处理。

### 3. MCP tool 注入会暴露更多历史假设

一旦 MCP tools 进入模型工具面，`list_all_tools()`、路由、前端展示、测试夹具都可能需要同步修改。

## Open Follow-Up

第二阶段再处理的内容包括：

- 各工具真实执行语义追平
- artifact subsystem 真正接入
- agent jobs 真正接入
- approval/runtime/sandbox 深度追平
- `registry` / `runtimes` / invocation model 是否整体向 `codex-main` 内部结构靠拢
