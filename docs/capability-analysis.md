# Mosaic-Desktop 项目能力分析

> 生成时间: 2026-03-18

## 一、整体架构状态

项目是一个基于 Tauri v2 + React 的桌面 AI 助手应用，参考了 OpenAI Codex CLI 的架构。后端 Rust 代码量很大（`src-tauri/src/core/` 有 ~40+ 个模块），但前端仍然是 Tauri 模板的初始状态。

## 二、AI 对话能力

| 能力 | 状态 | 说明 |
|------|------|------|
| 核心引擎 (Codex) | ✅ 已实现 | SQ/EQ 消息循环、Session 管理、Turn 生命周期 |
| SSE 流式对话 | ✅ 已实现 | `client.rs` (27KB) 支持 SSE 流式响应 |
| 消息历史 | ✅ 已实现 | `message_history.rs` + `context_manager/` |
| 上下文压缩 | ✅ 已实现 | `compact.rs` (11KB) |
| 配置系统 | ✅ 已实现 | `config/` 模块完整（层叠配置、TOML 解析、诊断） |
| WebSocket 传输 | ✅ 已实现 | `tokio-tungstenite` WebSocket 连接，自动 fallback 到 SSE |
| 多 Provider 支持 | ✅ 已实现 | `ProviderRegistry` 管理 openai/chatgpt/ollama/lmstudio + 用户自定义 |
| Auth 管理 | ✅ 已实现 | `AuthManager` 支持 ApiKey/ChatGPT 模式，token 刷新，文件持久化 |
| 模型 Fallback | ✅ 已实现 | `ModelFallbackConfig` 主模型失败自动尝试备选模型 |
| **前端对话 UI** | ❌ 完全缺失 | `App.tsx` 仍是模板的 greet 示例 |

## 三、Agent 系统

| 能力 | 状态 | 说明 |
|------|------|------|
| AgentInstance 生命周期 | ✅ 已实现 | `agent.rs` (29KB) — spawn/pause/resume/complete |
| Multi-Agent 调度 | ✅ 已实现 | `multi_agents.rs` handler |
| Batch Jobs | ✅ 已实现 | `agent_jobs.rs` handler |
| External Agent Config | ✅ 已实现 | `external_agent_config.rs` (25KB) — 检测/导入 Claude 配置 |
| Guards (并发守卫) | ❌ 缺失 | 源项目有 14KB 的 guards 模块 |
| Role 系统 | ❌ 缺失 | 源项目有 30KB 的角色权限体系 |

## 四、Skills 系统

| 能力 | 状态 | 说明 |
|------|------|------|
| Skill 模型定义 | ✅ 完整 | `model.rs` — SkillMetadata/SkillScope |
| Skill 加载器 | ✅ 核心完整 | `loader.rs` (46KB) — 多 scope 加载、符号链接、缓存 |
| Skill 管理器 | ✅ 核心完整 | `manager.rs` — 加载/查询/注入 |
| Skill 注入 | ✅ 核心完整 | `injection.rs` (21KB) — system prompt 注入 |
| Skill 权限 | ✅ 简化版 | `permissions.rs` — 字符串映射（非结构化 PermissionProfile） |
| 远程 Skill | ✅ 完整 | `remote.rs` (10KB) |
| 隐式调用检测 | ✅ 核心完整 | `invocation_utils.rs` |
| 环境变量依赖 | ✅ 完全对齐 | `env_var_dependencies.rs` |

## 五、Tools 系统

| Tool Handler | 状态 | 说明 |
|------|------|------|
| Shell 执行 | ✅ | `shell.rs` + `shell_command.rs` |
| Unified Exec (PTY) | ✅ | `unified_exec/` 完整模块 |
| 文件读取 | ✅ | `read_file.rs` |
| 目录列表 | ✅ | `list_dir.rs` |
| Grep 搜索 | ✅ | `grep_files.rs` |
| Apply Patch | ✅ | `apply_patch.rs` |
| MCP 工具调用 | ✅ | `mcp.rs` + `mcp_resource.rs` |
| MCP 连接管理 | ✅ | `mcp_client/connection_manager.rs` (22KB) |
| MCP Server | ✅ | `mcp_server.rs` (22KB) — JSON-RPC 2.0 |
| Dynamic Tools | ✅ | `dynamic.rs` (11KB) |
| JS REPL | ✅ | `js_repl.rs` |
| 图片查看 | ✅ | `view_image.rs` |
| BM25 搜索 | ✅ | `search_tool_bm25.rs` |
| Plan 工具 | ✅ | `plan.rs` |
| 用户输入请求 | ✅ | `request_user_input.rs` |
| Tool Router | ✅ | 三级路由: Built-in → MCP → Dynamic |

## 六、其他基础设施

| 模块 | 状态 |
|------|------|
| 协议层 (protocol/) | ✅ types/event/submission/error 完整 |
| 执行策略 (exec_policy/) | ✅ 命令审批/启发式分析 |
| Seatbelt 沙箱 | ✅ macOS 沙箱支持 |
| Git 信息 | ✅ `git_info.rs` (24KB) |
| Rollout 记录 | ✅ 完整的 rollout 子系统 |
| Memories | ✅ 两阶段记忆系统 |
| Hooks | ⚠️ 框架存在但未集成到 turn 生命周期 |
| File Watcher | ✅ `file_watcher.rs` |
| Secrets 管理 | ✅ `secrets/` 模块 |
| PTY | ✅ `pty/` 模块 |

## 七、核心结论

**后端能力（Rust）: ~70% 完成度** — AI 对话引擎、Agent、Skills、Tools 的核心框架都已搭建，622 个测试全部通过。主要缺失的是 WebSocket 传输、多 Provider、Auth 管理、Guards/Role 等高级功能。

**前端能力（React）: ~0% 完成度** — 这是最大的瓶颈。`App.tsx` 仍然是 Tauri 模板的 greet 示例，没有任何对话 UI、Agent 管理界面、Skills 配置界面。后端的所有能力目前无法通过 UI 使用。

**Tauri IPC 桥接: 最小化** — 只暴露了 5 个命令（`submit_op`, `poll_events`, `get_config`, `update_config`, `get_cwd`），虽然这个设计是正确的（通过 SQ/EQ 消息队列通信），但前端没有消费这些接口。

## 八、要让项目"可用"的优先级排序

1. **前端对话 UI** — 这是 0→1 的关键，需要聊天界面、消息渲染、流式显示
2. **前端 SQ/EQ 集成** — 用 `submit_op` 发送用户消息，用 `poll_events` 接收 AI 响应
3. **API Key 配置界面** — 让用户能配置 Provider 和密钥
4. **Tool 审批 UI** — 当 AI 要执行命令时，前端需要展示审批弹窗
