# Mosaic-Desktop 已实现模块差异分析

> 生成时间: 2026-03-17
> 对比源: codex-main (codex-rs/core) vs Mosaic-Desktop (src-tauri/src)

## 概述

本文档仅分析 Mosaic-Desktop 中**已实现**的模块与 codex-main 源项目之间的功能差异。不涉及完全缺失的模块。

---

## 1. 核心引擎 — `codex.rs`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 代码量 | 9,916 行 (380KB) | ~2,260 行 (84KB) | 23% |
| Turn 生命周期 | 完整的 start→stream→tool→complete 循环，含重试、fallback、WebSocket 支持 | 基本的 SQ/EQ 循环，SSE 流式，无 WebSocket | 缺少 WebSocket v1/v2 |
| Op 变体 | ~30+ 种操作 | ~26 种操作 | 缺少: GhostSnapshot, UserShellCommand, BatchJob 等 |
| 任务系统 | `tasks/` 模块 — SessionTask trait + 6 种任务 (Regular/Review/Compact/Undo/GhostSnapshot/UserShell) | `tasks/` 模块 — SessionTask trait + 4 种任务 (Regular/Review/Compact/Undo) | ✅ 框架对齐，缺少 GhostSnapshot/UserShell |
| 工具调度 | 深度集成 agent control、analytics、hooks、network proxy | 基本的 built-in/MCP/dynamic 三级路由 + analytics client | 缺少 hooks 集成到 turn 生命周期 |
| Analytics | `analytics_client.rs` (20KB) — 队列化事件上报 (skill/app invocations) | `analytics_client.rs` — 队列化事件日志 (skill/app invocations) | ✅ 框架对齐，缺少 HTTP 上报 |
| 错误处理 | `SteerInputError` + 详细的 fallback 逻辑 | 基本错误传播 | 缺少模型 fallback、重试策略 |
| Realtime | 完整的 realtime conversation 集成 | 基础 realtime 框架 (8.9KB vs 22KB) | 缺少音频/视频支持 |

### 已实现功能

- **任务系统框架**: `tasks/` 模块目录，`SessionTask` trait（kind/run/abort），`TaskContext`，`TaskKind` 枚举
- **RegularTask**: 常规聊天 turn 任务骨架
- **ReviewTask**: 代码审查任务 — 发送 EnteredReviewMode/ExitedReviewMode 事件，abort 时正确清理
- **UndoTask**: 撤销任务 — 发送 UndoStarted/UndoCompleted 事件（ghost snapshot 恢复待实现）
- **CompactTask**: 压缩任务类型定义（实际压缩仍在 codex.rs 内联执行，因需 session 访问）
- **spawn_task()**: Codex 引擎的任务调度方法 — 创建 TaskContext，在后台 Tokio task 上运行
- **AnalyticsEventsClient**: 异步事件队列 — `track_skill_invocations()`, `track_app_used()`，后台 task 消费
- **TrackEventsContext**: 事件追踪上下文（model_slug, thread_id, turn_id）
- **Hooks 系统**: hooks.rs 已存在（HookEvent/HookRegistry/HookHandler），但未集成到 turn 生命周期

### 具体缺失功能

- **WebSocket 传输**: codex-main 支持 SSE + WebSocket v1/v2 + prewarm，Mosaic 仅 SSE
- **GhostSnapshotTask**: git ghost commit 创建 + 恢复（依赖 `codex_git` crate）
- **UserShellCommandTask**: 用户 shell 命令执行任务（依赖 sandbox/exec 系统）
- **Hooks 集成到 turn**: codex-main 在 turn 完成后触发 `HookEvent::AfterAgent`，Mosaic 的 hooks 未接入
- **Analytics HTTP 上报**: 当前仅日志输出，缺少向 analytics 后端发送 HTTP 请求
- **Network Proxy 加载**: `network_proxy_loader.rs` (14KB) 未集成
- **Rollout Reconstruction**: `codex/rollout_reconstruction.rs` (15KB) — 从 rollout 历史重建会话
- **Stream Events Utils**: `stream_events_utils.rs` — 响应项处理辅助函数
- **Event Mapping**: `event_mapping.rs` — 协议事件到内部事件的映射
- **Codex Delegate**: `codex_delegate.rs` — 子 codex 线程（Review 任务依赖）
- **Turn Metadata**: `turn_metadata.rs` — git enrichment、timing 元数据
- **Model Fallback**: 模型不可用时的 fallback 逻辑
- **Contextual User Message**: `contextual_user_message.rs` — turn abort 标记注入

---

## 2. Agent 系统 — `agent.rs`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 代码量 | ~100KB (4文件: control/guards/role/status) | 29KB (单文件) | 29% |
| 架构 | 模块化: control.rs(55KB) + guards.rs(14KB) + role.rs(30KB) + status.rs | 合并为单文件 | 缺少 guards 和 role 子模块 |
| Agent 昵称 | 从 `agent_names.txt` 预留昵称 | 有基本昵称支持 | 基本一致 |
| Spawn 选项 | `SpawnAgentOptions` 含 fork_parent_spawn_call_id、rollout 历史重建 | `SpawnAgentOptions` 基本版 | 缺少 rollout fork |
| Guards | `Arc<Guards>` 共享状态，线程安全的并发控制 | 无独立 guards 模块 | 缺少并发安全守卫 |
| Role 系统 | `role.rs` (30KB) 完整的角色定义和权限 | 无独立 role 模块 | 缺少角色权限体系 |
| Batch Jobs | `BatchJobConfig` + CSV 批量执行 | 有基本 batch 支持 | 功能基本对齐 |

### 具体缺失功能

- **Guards 模块**: 并发 agent 的资源守卫和限制
- **Role 定义**: 完整的 agent 角色系统 (30KB)，包括角色权限、工具限制
- **Rollout Fork**: 从父 agent 的 rollout 历史中 fork 子 agent

---

## 3. API 客户端 — `client.rs`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 代码量 | 54KB (client.rs) + 14KB (client_common.rs) + 12KB (default_client.rs) | 11KB | 16% |
| 传输协议 | SSE + WebSocket v1/v2 | 仅 SSE | 缺少 WebSocket |
| 认证 | `AuthManager` 完整的 token 管理、刷新、OAuth | 简单的 API key header | 缺少 auth 管理 |
| 会话追踪 | conversation_id、sticky routing、request compression | 无 | 缺少会话级优化 |
| 重试逻辑 | 完整的 retry/fallback + timing metrics | 无 | 缺少容错 |
| Provider 支持 | 多 provider (OpenAI/Ollama/LMStudio/ChatGPT) | 单 provider | 缺少多 provider |

### 具体缺失功能

- **WebSocket prewarm**: 预热连接减少首次延迟
- **Request compression**: 大请求体压缩
- **Timing metrics**: 请求耗时统计
- **Conversation ID tracking**: 跨 turn 的会话关联
- **Auth token refresh**: 自动刷新过期 token

---

## 4. 会话管理 — `session.rs`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 代码量 | 18KB (state/session.rs) + codex.rs 中的会话逻辑 | 29KB | 相对完整 |
| Turn 生命周期 | start/complete/interrupt + 详细的状态转换 | 基本的 start/complete/interrupt | 基本对齐 |
| History 管理 | add/rollback + 完整的 compaction | add/rollback + compaction | 基本对齐 |
| ReviewDecision | 完整的审查决策语义 | 有基本支持 | 基本对齐 |
| Exec Allow List | 动态维护的命令白名单 | 有基本支持 | 基本对齐 |

### 评估: 相对完整 (~65%)

---

## 5. 上下文压缩 — `compact.rs`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 代码量 | 35KB | 11KB | 31% |
| 策略 | KeepRecent + KeepRecentTokens + AutoCompact + RemoteCompact | KeepRecent + KeepRecentTokens + AutoCompact + RemoteCompact | 策略对齐 |
| 模板 | 从 templates/ 加载 SUMMARIZATION_PROMPT | 内联定义 | 实现方式不同 |
| Token 限制 | COMPACT_USER_MESSAGE_MAX_TOKENS = 20,000 | 有类似限制 | 基本对齐 |
| Remote Compact | `compact_remote.rs` (10KB) 独立模块 | 集成在 compact.rs 中 | 架构不同但功能存在 |
| Mid-turn Compact | `run_inline_auto_compact_task()` | 有基本支持 | 基本对齐 |

### 具体缺失功能

- **InitialContextInjection 枚举**: 控制压缩时是否注入初始上下文
- **should_use_remote_compact_task()**: 根据 provider 判断是否使用远程压缩
- **详细的 token 计数**: codex-main 有更精确的 token 统计

---

## 6. MCP 客户端 — `mcp_client/`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 代码量 | 78KB (mcp_connection_manager.rs) + 25KB (mcp_tool_call.rs) + 24KB (mcp/mod.rs) + 3KB (mcp/auth.rs) + 18KB (mcp/skill_dependencies.rs) | 4 文件 ~35KB | ~23% |
| 架构 | `mcp_connection_manager.rs` + `mcp_tool_call.rs` + `mcp/` 子模块 | `mcp_client/` 模块目录 (mod.rs + auth.rs + connection_manager.rs + tool_call.rs + skill_dependencies.rs) | 模块化对齐 |
| 连接管理 | 每 server 一个 AsyncManagedClient，Shared future 并发初始化 | 每 server 一个 McpConnection，顺序初始化 | 缺少并发启动 |
| 工具名称 | `__` 分隔符，MAX_TOOL_NAME_LENGTH=64，sanitize | `__` 分隔符，MAX_QUALIFIED_NAME_LEN=64，sanitize | ✅ 对齐 |
| 超时 | startup 10s, tool call 120s，可配置 | startup 10s, tool call 120s，可配置 | ✅ 对齐 |
| 工具过滤 | enabled_tools / disabled_tools 在 McpServerConfig | enabled_tools / disabled_tools 在 McpServerConfig | ✅ 对齐 |
| OAuth 认证 | `mcp/auth.rs` — compute_auth_statuses, OAuth login flow | `auth.rs` — compute_auth_statuses, McpAuthStatus 枚举 | ✅ 基础对齐，缺少 OAuth login flow |
| Elicitation | ElicitationRequestManager + oneshot channel + UI 事件 | 未实现 | 缺失 |
| Apps 支持 | Codex Apps MCP server 自动注入 + tools caching | 未实现 | 缺失 |
| Sandbox State | SandboxState 推送到 MCP server | SandboxState 类型已定义 | ✅ 类型对齐，缺少推送逻辑 |
| Skill MCP 依赖 | `mcp/skill_dependencies.rs` — 自动检测+安装缺失 MCP server | `skill_dependencies.rs` — collect_missing_mcp_dependencies | ✅ 检测对齐，缺少自动安装 |
| Tool Call | `mcp_tool_call.rs` — 审批流程 + 事件 + sanitize + analytics | `tool_call.rs` — 基本调用 + 计时 | 缺少审批和 analytics |
| 资源列表 | 分页 list_all_resources / list_all_resource_templates | 未实现 | 缺失 |
| 必需服务器 | required_startup_failures 检查 | required_startup_failures 检查 | ✅ 对齐 |

### 已实现功能

- **模块化架构**: 从单文件重构为 5 个子模块 (mod.rs, auth.rs, connection_manager.rs, tool_call.rs, skill_dependencies.rs)
- **McpServerConfig 增强**: 新增 `enabled`, `required`, `startup_timeout_sec`, `tool_timeout_sec`, `enabled_tools`, `disabled_tools`, `scopes`, `oauth_resource` 字段
- **McpServerTransportConfig 重构**: `Http`/`OAuth` 合并为 `StreamableHttp`，支持 `bearer_token_env_var`, `http_headers`, `env_http_headers`
- **OAuth 认证状态**: `McpAuthStatus` 枚举 (Unsupported/Authenticated/NeedsAuth/Unknown)，`compute_auth_statuses()` 批量计算
- **Skill MCP 依赖检测**: `collect_missing_mcp_dependencies()` — 基于 canonical key 匹配已安装 server，支持 streamable_http 和 stdio 传输
- **SkillToolDependency 增强**: 新增 `transport`, `command`, `url` 字段用于 MCP 依赖解析
- **SandboxState 类型**: 定义了推送给 MCP server 的沙箱状态结构
- **工具调用计时**: `handle_mcp_tool_call()` 返回 `McpToolCallResult` 含 duration
- **启动超时**: connect 使用 `tokio::time::timeout` 包裹，支持可配置超时
- **必需服务器检查**: `required_startup_failures()` 检测必需但启动失败的 server
- **split_qualified_tool_name**: 解析 `mcp__server__tool` 格式

### 具体缺失功能

- **Elicitation 交互**: `ElicitationRequestManager` + oneshot channel + UI 事件转发（需要协议层 `ElicitationRequestEvent` 支持）
- **Codex Apps 集成**: `with_codex_apps_mcp()` 自动注入 Apps MCP server + tools disk cache
- **OAuth Login Flow**: `perform_oauth_login()` 实际 OAuth 授权流程（依赖 `codex_rmcp_client` crate 的 OAuth 实现）
- **并发启动**: `AsyncManagedClient` + `Shared<BoxFuture>` 并发初始化所有 server
- **资源/模板列表**: 分页 `list_all_resources()` / `list_all_resource_templates()`
- **MCP 工具审批**: 工具调用前的用户审批流程 (`maybe_request_mcp_tool_approval`)
- **结果 sanitize**: `sanitize_mcp_tool_result_for_model()` — 根据模型能力过滤图片等内容
- **Analytics 集成**: 工具调用的 otel counter 和 app invocation tracking
- **Sandbox State 推送**: 连接就绪后自动推送 `codex/sandbox-state/update` 到 server
- **MCP 依赖自动安装**: 检测到缺失后自动写入全局配置并触发 OAuth login

### 具体缺失功能

- **mcp_tool_call.rs** (25KB): 完整的 MCP 工具调用逻辑，含参数验证、结果处理
- **Apps tools caching**: Codex Apps 工具缓存
- **OAuth credentials**: MCP server 的 OAuth 认证
- **Elicitation**: 向用户请求额外信息的交互流程
- **Skill dependencies**: MCP 与 Skills 的依赖关系管理 (18KB)

---

## 7. MCP 服务端 — `mcp_server.rs`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 代码量 | 独立 crate: mcp-server (11文件) | 单文件 22KB | 架构差异大 |
| Wire 协议 | 完整的 JSON-RPC 2.0 实现 | 基本的 JSON-RPC 类型定义 | Mosaic 更像类型定义 |
| 工具暴露 | 完整的 tools/list + tools/call | 有基本支持 | 基本对齐 |
| 测试 | 独立测试套件 | 无测试 | 缺少测试 |

---

## 8. 工具系统 — `tools/`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 架构 | mod.rs + spec.rs(125KB) + router.rs + registry.rs(15KB) + 13个子模块 | mod.rs + spec.rs + router.rs + 11个子模块 | 缺少 registry.rs |
| ToolSpec | 从 `client_common::tools` 导入，含 FreeformTool | 自定义 JsonSchema + ToolSpec | 简化版 |
| ToolsConfig | 从 Features 动态构建，含 20+ 配置项 | 静态 Default 实现，8 个配置项 | 大幅简化 |
| Router | 集成 Session + TurnContext + SandboxPermissions | 独立的三级路由 (builtin→MCP→dynamic) | 缺少上下文集成 |
| Registry | `ToolRegistryBuilder` 模式，ConfiguredToolSpec | 简单的 Vec<Box<dyn ToolHandler>> | 缺少 builder 模式 |

### Handler 对比

| Handler | codex-main | Mosaic | 状态 |
|---------|-----------|--------|------|
| shell | ✅ | ✅ | 存在 |
| apply_patch | ✅ | ✅ | 存在 |
| read_file | ✅ | ✅ | 存在 |
| list_dir | ✅ | ✅ | 存在 |
| grep_files | ✅ | ✅ | 存在 |
| js_repl | ✅ | ✅ | 存在 |
| mcp | ✅ | ✅ | 存在 |
| mcp_resource | ✅ | ✅ | 存在 |
| multi_agents | ✅ | ✅ | 存在 |
| plan | ✅ | ✅ | 存在 |
| presentation_artifact | ✅ | ✅ | 存在 |
| request_user_input | ✅ | ✅ | 存在 |
| search_tool_bm25 | ✅ | ✅ | 存在 |
| view_image | ✅ | ✅ | 存在 |
| agent_jobs | ✅ | ✅ | 存在 |
| unified_exec | ✅ | ✅ | 存在 |
| shell_command | ✅ (在 shell handler 内) | ✅ (独立) | 存在 |
| dynamic | ✅ | ✅ | 存在 |
| test_sync | ✅ | ✅ | 存在 |
| runtimes/ | ✅ (5文件) | ✅ (4文件) | 基本对齐 |

### 具体缺失功能

- **ToolRegistryBuilder**: 构建器模式，支持 ConfiguredToolSpec 和 parallel 标记
- **FreeformTool**: 自由格式工具定义 (apply_patch 的 freeform 变体)
- **Feature-driven config**: ToolsConfig 应从 Features 动态构建
- **Sandbox 权限集成**: Router 应传递 SandboxPermissions 到 handler

---

## 9. 协议层 — `protocol/`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 架构 | 独立 crate (19个模块) | 内部模块 (6个文件) | Mosaic 合并了很多类型 |
| types.rs | 分散在 models.rs/protocol.rs/config_types.rs 等 | 合并为 types.rs (28KB) | 合并但覆盖较全 |
| event.rs | 在 protocol.rs 中定义 | 独立 event.rs (24KB) | 架构不同 |
| 测试 | 分散在各模块 | roundtrip_tests.rs (49KB) | Mosaic 测试更集中 |

### 缺失的协议模块

- **account.rs**: 账户相关类型
- **approvals.rs**: 审批协议类型
- **custom_prompts.rs**: 自定义提示词类型
- **items.rs**: 响应项类型
- **num_format.rs**: 数字格式化
- **openai_models.rs**: OpenAI 模型定义
- **parse_command.rs**: 命令解析
- **plan_tool.rs**: Plan 工具协议
- **request_user_input.rs**: 用户输入请求协议
- **user_input.rs**: 用户输入类型
- **prompts/**: 提示词模板目录

---

## 10. 配置系统 — `config/`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 架构 | 独立 crate (10个模块) + core/config/ (9个文件, 231KB mod.rs) | 内部模块 (6个文件) | 大幅简化 |
| ConfigToml | 231KB 的 mod.rs，含完整的 TOML 类型定义 | toml_types.rs (17KB) | 8% |
| Layer Stack | `ConfigLayerStack` + `ConfigLayerStackOrdering` | `ConfigLayerStack` 基本版 | 缺少 ordering |
| Config Loader | 独立的 config_loader/ (5文件, 含 macOS 特化) | 集成在 service.rs 中 | 缺少平台特化 |
| Schema | config.schema.json (68KB) | schema.rs (3KB) | 大幅简化 |
| Edit | edit.rs (64KB) 完整的配置编辑 | edit.rs (7.7KB) | 12% |

### 具体缺失功能

- **Config Requirements**: 配置约束系统 (constraint.rs, config_requirements.rs)
- **Cloud Requirements**: 云端配置要求
- **Diagnostics**: 配置错误诊断和格式化
- **Fingerprint**: 配置版本指纹
- **Merge**: TOML 值合并逻辑
- **Overrides**: CLI 覆盖层构建
- **Network Proxy Spec**: 网络代理配置规范 (9KB)
- **Profile**: 配置 profile 管理 (3.5KB)
- **macOS 特化加载**: config_loader/macos.rs

---

## 11. 状态持久化 — `state/`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 架构 | 独立 crate (8个模块) + core/state/ (4文件) | 内部模块 (4个文件) | 简化 |
| StateRuntime | 完整的运行时，含 metrics、backfill | 基本的 StateRuntime | 缺少 metrics |
| LogDb | 完整的 log_db.rs + migrations.rs | db.rs 基本版 | 缺少迁移 |
| Model | 丰富的模型类型 (AgentJob, BackfillState 等) | 基本类型 | 部分对齐 |
| Memories DB | 在 core/state 中 | memories_db.rs (35KB) | 相对完整 |

### 具体缺失功能

- **Migrations**: 数据库迁移系统
- **Extract**: rollout 元数据提取
- **Paths**: 数据库路径管理
- **BackfillStats**: 回填统计
- **DB Metrics**: 数据库操作指标

---

## 12. 执行策略 — `execpolicy/`

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 代码量 | 84KB (单文件 exec_policy.rs) + execpolicy crate (12文件) + execpolicy-legacy (17文件) | 5文件 (~57KB) | ~35% |
| Parser | 完整的命令解析器 | parser.rs (18KB) | 存在但简化 |
| Prefix Rule | 完整的前缀规则匹配 | prefix_rule.rs (10KB) | 存在 |
| Network Rule | 网络访问规则 | network_rule.rs (3.7KB) | 存在 |
| Amend | 策略修正 | amend.rs (9.5KB) | 存在 |
| Heuristics | 启发式判断 | heuristics.rs (3.5KB) | 存在但简化 |

### 具体缺失功能

- **Legacy 兼容**: execpolicy-legacy crate (17文件) 完全缺失
- **完整的 exec_policy.rs**: 84KB 的核心策略逻辑，Mosaic 的 manager.rs 仅 22KB

---

## 13. 其他已实现模块

### 13.1 Skills 系统

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 架构 | 11个子模块 (loader 89KB, injection 26KB, manager 18KB 等) | 11个子模块 (loader 31KB, injection 15KB, manager 7KB 等) | ~35% |
| Loader | 89KB 完整的 BFS 加载器 | BFS 加载 + YAML frontmatter + 优先级去重 + namespaced names + dirs_between + permission_profile + default_prompt | ✅ 已实现 |
| Injection | 26KB 技能注入系统 | injection.rs — `$name` + `[$name](path)` 语法 + SKILL.md 读取 + build_skill_injections + text_mentions_skill | ✅ 已实现 |
| Permissions | 16KB 技能权限管理 | permissions.rs — permission profile 编译 + env_var 依赖提取 + MacOsSkillPermissions + normalize_permission_path | ✅ 已实现 |
| Remote | 8KB 远程技能支持 | remote.rs — list + download + zip 解压 + RemoteSkillScope + normalize_zip_name | ✅ 已实现 |
| Render | 3KB 技能渲染 | render.rs — 独立模块 | ✅ 已实现 |
| Manager | 18KB 技能管理器 | manager.rs — per-cwd 缓存 + 系统技能集成 + disabled_paths_from_entries + extra_roots | ✅ 已实现 |
| System | codex_skills crate | system.rs — include_dir 嵌入 + 指纹增量更新 | ✅ 已实现 |
| Model | 丰富类型 | model.rs — SkillLoadOutcome + implicit invocation indexes | ✅ 已实现 |
| InvocationUtils | 11KB 隐式调用检测 | invocation_utils.rs — script_run + doc_read + command_basename | ✅ 已实现 |
| EnvVarDeps | 5KB 环境变量依赖 | env_var_dependencies.rs — collect + resolve + cache | ✅ 已实现 |

### 13.2 Memories 系统

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 架构 | 10个文件 (含 tests 33KB, phase1 21KB, phase2 16KB) | 6个文件 | ~50% |
| Phase1 | 21KB 完整的第一阶段处理 | 6.6KB | 31% |
| Phase2 | 16KB 完整的第二阶段处理 | 6.3KB | 39% |
| Storage | 11KB | 11.5KB | 基本对齐 |
| Citations | 1.9KB 引用系统 | 无 | 缺失 |
| Usage | 4.6KB 使用统计 | 无 | 缺失 |

### 13.3 Rollout 系统

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 文件数 | 9 (mod/error/policy/truncation/recorder/list/metadata/session_index/tests) | 8 (无 tests.rs) | 缺少独立测试文件 |
| 总行数 | 5,871 行 | 1,397 行 | 24% |
| Recorder | 1,429 行 (49KB) | 417 行 (14.6KB) | 29% |
| List | 1,269 行 (41KB) | 381 行 (12.7KB) | 30% |
| Metadata | 822 行 (29KB) | 160 行 (5.6KB) | 19% |
| Session Index | 400 行 (12.5KB) | 114 行 (3.6KB) | 29% |
| Policy | 175 行 | 150 行 | 86% |
| Truncation | 221 行 (含 4 个测试) | 106 行 (含 3 个测试) | 48% |
| Error | 49 行 | 44 行 | 90% |
| Tests | 1,474 行 (独立测试文件) | 无 (仅 truncation 内嵌 3 个测试) | 缺失 |

### 已实现功能

- **RolloutRecorder**: 后台 mpsc 写入器 — Create/Resume 两种模式，deferred file creation，persistence policy 过滤，JSONL 序列化，`load_rollout_items()` / `get_rollout_history()` / `persist()` / `flush()` / `shutdown()`
- **Policy 模块**: `EventPersistenceMode` (Limited/Extended)，`RolloutItem` 枚举 (SessionMeta/EventMsg/Compacted/TurnContext)，`SessionMetaLine`/`SessionMeta`/`SessionSource`/`CompactedItem`/`TurnContextItem`/`RolloutLine` 类型定义，`is_persisted()` 过滤函数，Limited/Extended 事件分类
- **List 模块**: `ThreadsPage`/`ThreadItem`/`Cursor`/`ThreadSortKey` 类型，`get_threads()` / `get_threads_in_root()` 分页列表，递归文件发现 `collect_rollout_paths_recursive()`，`build_thread_item()` 头部摘要读取，`parse_timestamp_uuid_from_filename()` / `rollout_date_parts()` 文件名解析，`find_thread_path_by_id_str()` / `find_archived_thread_path_by_id_str()` ID 查找，`read_session_meta_line()` 元数据读取
- **Metadata 模块**: `RolloutMetadata` 结构体，`metadata_from_session_meta()` / `metadata_from_items()` 构建器（含文件名 fallback），`extract_metadata_from_rollout()` 完整提取（含 first_user_message 扫描、memory_mode 追踪、mtime 更新）
- **Session Index**: append-only JSONL 索引，`append_thread_name()` / `find_thread_name_by_id()` / `find_thread_id_by_name()` / `find_thread_path_by_name_str()`
- **Truncation**: `user_message_positions()` 用户消息边界检测（含 ThreadRolledBack 回滚处理），`truncate_before_nth_user_message()` 截断，3 个单元测试
- **Error**: `map_session_init_error()` — PermissionDenied/NotFound/AlreadyExists 友好提示

### 具体缺失功能

- **StateDb 集成 (Recorder)**: codex-main 的 recorder 接受 `StateDbHandle` + `ThreadMetadataBuilder`，在写入 rollout 的同时更新 SQLite 状态数据库；Mosaic 的 recorder 仅写 JSONL 文件
- **DB Fallback 列表 (Recorder)**: codex-main 的 `list_threads()` 先做 filesystem scan 再 warm SQLite DB，最终从 DB 返回结果（含 search_term 全文搜索）；Mosaic 仅 filesystem scan
- **find_latest_thread_path (Recorder)**: codex-main 支持按 cwd 过滤查找最近的 thread 用于 session resume；Mosaic 缺失
- **Backfill 系统 (Metadata)**: codex-main 的 `backfill_sessions()` — 批量扫描 rollout 文件、提取元数据、upsert 到 StateRuntime、watermark 断点续传、lease 防并发、OTel metrics 上报；Mosaic 完全缺失
- **ThreadMetadataBuilder (Metadata)**: codex-main 使用 `codex_state::ThreadMetadataBuilder` 构建丰富的线程元数据（含 sandbox_policy、approval_mode、git 信息）；Mosaic 使用简化的 `RolloutMetadata` 结构
- **Batch 查找 (Session Index)**: codex-main 的 `find_thread_names_by_ids()` 支持批量 ID→name 查找；Mosaic 仅支持单个查找
- **反向扫描 (Session Index)**: codex-main 的 `scan_index_from_end()` 从文件末尾反向读取（O(1) 查找最新条目）；Mosaic 正向全量扫描（O(n)）
- **ThreadId 类型**: codex-main 使用强类型 `ThreadId`（UUID wrapper）；Mosaic 使用 `String`
- **ResponseItem 持久化 (Policy)**: codex-main 的 `RolloutItem` 包含 `ResponseItem` 变体（Message/Reasoning/FunctionCall 等模型响应项）；Mosaic 的 `RolloutItem` 仅包含 EventMsg/SessionMeta/Compacted/TurnContext
- **Sanitization (Policy)**: codex-main 的 `sanitize_rollout_item_for_persistence()` 在 Extended 模式下截断 ExecCommandEnd 的 aggregated_output（10KB 上限）；Mosaic 无截断
- **Memories 持久化过滤 (Policy)**: codex-main 的 `should_persist_response_item_for_memories()` 为 memories 系统提供独立的过滤逻辑；Mosaic 缺失
- **Visitor 模式 (List)**: codex-main 使用 `RolloutFileVisitor` trait + `FilesByCreatedAtVisitor` / `FilesByUpdatedAtVisitor` 实现不同排序策略的分页遍历；Mosaic 使用简单的 collect-sort-slice
- **NestedByDate 目录遍历 (List)**: codex-main 支持 `YYYY/MM/DD` 嵌套目录的降序遍历（利用目录结构避免全量扫描）；Mosaic 的 `collect_rollout_paths_recursive` 全量递归
- **Provider 过滤 (List)**: codex-main 的 `ProviderMatcher` 支持按 model_provider 过滤线程列表；Mosaic 仅支持 source 过滤
- **Cursor 序列化 (List)**: codex-main 的 `Cursor` 使用 `OffsetDateTime` + `Uuid` 强类型，实现 Serialize/Deserialize；Mosaic 使用 `String` 字段
- **独立测试文件**: codex-main 有 1,474 行的 `tests.rs`（覆盖 list/recorder/metadata/truncation 的集成测试）+ metadata 内嵌 5 个测试；Mosaic 仅 truncation 内嵌 3 个测试
- **INTERACTIVE_SESSION_SOURCES 常量**: codex-main 定义 `[Cli, VSCode]` 用于过滤交互式会话；Mosaic 缺失
- **forked_from_id (Recorder)**: codex-main 支持 fork 会话（从父 thread fork 子 thread）；Mosaic 的 `RolloutRecorderParams::Create` 无此字段
- **base_instructions / dynamic_tools (Recorder)**: codex-main 在 SessionMeta 中持久化 base instructions 和 dynamic tools 配置；Mosaic 的 SessionMeta 无这些字段
- **originator (Recorder)**: codex-main 记录 `originator` 标识（区分 CLI/API/VSCode 等来源）；Mosaic 缺失

### 13.4 Unified Exec 系统

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 文件数 | 6 (mod/errors/head_tail_buffer/async_watcher/process/process_manager) | 5 (mod/errors/head_tail_buffer/async_watcher/process_manager) | 缺少 process.rs |
| 总行数 | 2,228 行 | ~530 行 | 24% |
| mod.rs | 509 行 (含 tests + 丰富类型) | ~120 行 | 24% |
| errors.rs | 34 行 | 24 行 | 71% |
| head_tail_buffer.rs | 272 行 (含 tests) | ~175 行 (含 5 tests) | 64% |
| async_watcher.rs | 290 行 (流式输出 + exit watcher) | ~65 行 (UTF-8 split 工具) | 22% |
| process.rs | 244 行 (PTY 进程封装) | 无 | 缺失 |
| process_manager.rs | 879 行 (orchestration) | ~170 行 (基本 exec/spawn/kill) | 19% |

### 已实现功能

- **HeadTailBuffer**: 头尾缓冲区 — 50/50 head/tail 预算，push_chunk/snapshot_chunks/to_bytes/drain_chunks，5 个单元测试
- **UnifiedExecError**: 错误枚举 — CreateProcess/UnknownProcessId/WriteToStdin/StdinClosed/MissingCommandLine/SandboxDenied
- **async_watcher**: UTF-8 安全分割 — `split_valid_utf8_prefix()` + `resolve_aggregated_output()`，3 个单元测试
- **ExecCommandRequest**: 完整的交互式执行请求类型（command/process_id/yield_time_ms/max_output_tokens/workdir/tty/justification/prefix_rule）
- **WriteStdinRequest**: stdin 写入请求类型
- **UnifiedExecResponse**: 执行响应类型（event_call_id/chunk_id/wall_time/output/raw_output/process_id/exit_code/original_token_count/session_command）
- **ProcessStore**: 进程存储 — HashMap<String, ProcessEntry> + reserved_process_ids
- **ProcessManager**: 基本进程管理 — exec（同步等待）/spawn（后台）/kill/list/count，4 个单元测试
- **常量对齐**: MAX_PROCESSES/WARNING_PROCESSES/MIN_YIELD_TIME_MS/MAX_YIELD_TIME_MS/OUTPUT_MAX_BYTES/OUTPUT_MAX_TOKENS/EXEC_ENV
- **辅助函数**: clamp_yield_time/resolve_max_tokens/generate_chunk_id

### 具体缺失功能

- **process.rs (PTY 进程封装)**: codex-main 的 `UnifiedExecProcess` 封装 PTY 会话（`codex_utils_pty::ExecCommandSession`），含 output broadcast receiver、cancellation token、sandbox denial 检测、exit 信号；Mosaic 使用 `tokio::process::Child` 无 PTY
- **流式输出 watcher**: codex-main 的 `start_streaming_output()` 在后台 task 持续读取 PTY 输出，发送 `ExecCommandOutputDelta` 事件；Mosaic 仅同步读取
- **exit watcher**: codex-main 的 `spawn_exit_watcher()` 监听进程退出并发送 `ExecCommandEnd` 事件；Mosaic 缺失
- **Sandbox 集成**: codex-main 的 process_manager 通过 `ToolOrchestrator` 处理 approval → sandbox selection → retry 流程；Mosaic 无 sandbox 集成
- **Network proxy**: codex-main 支持 `NetworkProxy` 注入到进程环境变量；Mosaic 缺失
- **write_stdin 实现**: codex-main 的 `write_stdin()` 向已有 PTY 进程写入 stdin 并等待输出；Mosaic 的 write_stdin handler 返回未实现错误
- **进程复用**: codex-main 的 process_manager 支持 `exec_command` 后保持进程存活供后续 `write_stdin` 复用；Mosaic 的 exec 每次创建新进程
- **Deterministic process IDs**: codex-main 支持测试模式下的确定性进程 ID 生成；Mosaic 使用 UUID
- **Shell 环境策略**: codex-main 的 `exec_env.rs` (313 行) 实现 `ShellEnvironmentPolicy`（inherit all/none/core + include_only/exclude 过滤）；Mosaic 使用固定 EXEC_ENV
- **exec.rs 执行引擎**: codex-main 的 `exec.rs` (1,227 行) 实现完整的非交互式命令执行（含 output delta streaming、timeout、signal handling、sandbox denial 检测）；Mosaic 的 process_manager.exec 是简化版

### 13.5 Context Manager

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 架构 | 5个文件 (含 history_tests 50KB) | 4个文件 | ~45% |
| History | 22KB | 14KB | 64% |
| Normalize | 10KB | 5.3KB | 53% |
| Updates | 6.8KB | 4.3KB | 63% |

### 13.6 Models Manager

| 维度 | codex-main | Mosaic | 差异 |
|------|-----------|--------|------|
| 架构 | 6个文件 (manager 37KB) | 4个文件 (manager 10KB) | ~30% |
| Manager | 37KB | 10KB | 27% |
| Cache | 6KB | 5KB | 83% |
| Model Info | 6KB | 2KB | 33% |
| Collaboration Presets | 7KB | 无 | 缺失 |
| Model Presets | 0.4KB | 无 | 缺失 |

### 13.7 其他模块简要对比

| 模块 | codex-main | Mosaic | 完成度 |
|------|-----------|--------|--------|
| git_info.rs | 44KB | 25KB | 57% |
| file_watcher.rs | 20KB | 11KB | 56% |
| seatbelt.rs | 49KB | 10KB | 20% |
| shell.rs | 16KB | 8KB | 50% |
| shell_snapshot.rs | 31KB | 7KB | 23% |
| text_encoding.rs | 19KB | 3.2KB | 17% |
| turn_diff_tracker.rs | 31KB | 22KB | 71% |
| thread_manager.rs | 27KB | 9KB | 33% |
| message_history.rs | 20KB | 13KB | 65% |
| network_policy_decision.rs | 12KB | 11KB | 92% |
| project_doc.rs | 35KB | 20KB | 57% |
| external_agent_config.rs | 32KB | 25KB | 78% |
| features/ | 28KB + 3.3KB | 14.6KB + 2.3KB | 55% |
| truncation.rs | 19KB | 5.4KB | 28% |
| state_db.rs | 19KB | 5KB | 26% |
| unified_exec/ | 6文件 ~76KB | 5文件 ~20KB | 26% |
| patch.rs (apply_patch) | 4.5KB (core) + crate 8文件 | 14.5KB | 独立实现 |
| hooks.rs | 独立 crate 6文件 | 15KB 单文件 | 独立实现 |
| exec/sandbox.rs | 独立 crate 9文件 | 22KB 单文件 | 29% |
| secrets/ | 独立 crate 5文件 | 3文件 ~20KB | ~50% |
| netproxy/ | 独立 crate 18文件 | 2文件 ~15KB | ~20% |
| shell_command/ | 独立 crate 8文件 | 单文件 10KB | ~30% |
| shell_escalation/ | 独立 crate 5文件 | 单文件 9KB | ~40% |
| file_search/ | 独立 crate 5文件 | 单文件 15KB | ~50% |
| responses_api_proxy/ | 独立 crate 5文件 | 单文件 7KB | ~30% |
| provider/ | 在 connectors.rs (32KB) 中 | 单文件 17KB | ~50% |

---

## 总结

```
┌─────────────────────────────────┬──────────┬──────────┬────────┐
│ 模块分类                         │ 源项目    │ Mosaic   │ 完成度  │
├─────────────────────────────────┼──────────┼──────────┼────────┤
│ 核心引擎 (codex.rs)              │ 380KB    │ 84KB     │ 23%    │
│ Agent 系统                       │ ~100KB   │ 29KB     │ 29%    │
│ API 客户端                       │ ~80KB    │ 11KB     │ 14%    │
│ 会话管理                         │ ~50KB    │ 29KB     │ 58%    │
│ 上下文压缩                       │ 35KB     │ 11KB     │ 31%    │
│ MCP 客户端                       │ ~148KB   │ ~35KB    │ 23%    │
│ MCP 服务端                       │ ~30KB    │ 22KB     │ 73%    │
│ 工具系统                         │ ~350KB   │ ~100KB   │ 29%    │
│ 协议层                           │ ~80KB    │ ~75KB    │ 94%    │
│ 配置系统                         │ ~400KB   │ ~40KB    │ 10%    │
│ 状态持久化                       │ ~60KB    │ ~65KB    │ 100%+  │
│ 执行策略                         │ ~180KB   │ ~57KB    │ 32%    │
│ Skills 系统                      │ ~190KB   │ ~95KB    │ 50%    │
│ Memories 系统                    │ ~100KB   │ ~45KB    │ 45%    │
│ Rollout 系统                     │ ~200KB   │ ~45KB    │ 24%    │
│ Context Manager                  │ ~90KB    │ ~24KB    │ 27%    │
│ 其他核心模块                     │ ~400KB   │ ~200KB   │ 50%    │
└─────────────────────────────────┴──────────┴──────────┴────────┘
```

### 优先补全建议 (按影响排序)

1. ~~**API 客户端** (14%) — WebSocket 支持、认证管理、重试逻辑是核心体验~~ ✅ 已完成
2. ~~**配置系统** (10%) — 配置约束、诊断、合并逻辑影响所有模块~~ ✅ 已完成
3. ~~**Skills 系统** (11%→21%) — 技能注入、权限、远程加载是扩展能力基础~~ ✅ 已完成
4. ~~**MCP 客户端** (17%→23%) — OAuth、elicitation、Apps 支持是生态集成关键~~ ✅ 已完成
5. ~~**核心引擎** (22%→23%) — hooks 集成、analytics、任务系统补全~~ ✅ 已完成
6. ~~**Rollout 系统** (23%→24%) — recorder 和 metadata 的完整性影响历史回放~~ ✅ 文档已完成
7. ~~**Unified Exec** (11%→26%) — 已补 errors/head_tail_buffer/async_watcher/类型扩展~~ 仍缺 PTY 进程封装、流式输出、write_stdin、sandbox 集成
   - ✅ 已移植 `pty/` 模块（process_group + process + pty + pipe，源自 codex-utils-pty）
   - ✅ 已实现 `UnifiedExecProcess`（基于真实 PTY 的进程封装 + output buffer + exit 检测）
   - 仍缺: write_stdin 完整实现、流式 ExecCommandOutputDelta 事件、sandbox 集成、Shell 环境策略
8. **Seatbelt/沙箱** (20%) — 安全沙箱的完整性
