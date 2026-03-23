# 辅助模块说明

## 配置系统 (`config/`)

分层配置架构，优先级从低到高：Default → User → Project → Session。

| 模块 | 说明 |
|------|------|
| `ConfigLayerStack` | 分层配置栈，支持动态添加/合并 |
| `schema.rs` | 配置 schema 定义 |
| `merge.rs` | 多层配置合并逻辑 |
| `toml_types.rs` | TOML 到 Rust 类型映射 |
| `permissions.rs` | 权限相关配置 |
| `diagnostics.rs` | 配置诊断和校验 |
| `edit.rs` | 配置编辑操作 |
| `fingerprint.rs` | 配置指纹（变更检测） |
| `constraint.rs` | 配置约束验证 |
| `overrides.rs` | 配置覆盖机制 |
| `service.rs` | 配置服务层 |
| `config_requirements.rs` | 配置依赖需求 |
| `layer_stack.rs` | 配置层栈实现 |

## 认证 (`auth/`)

管理 API 认证 token 的获取和存储。支持 API Key 和 ChatGPT 登录两种方式。

## 密钥管理 (`secrets/`)

| 模块 | 说明 |
|------|------|
| `manager.rs` | 密钥管理器 — 统一的密钥获取接口 |
| `backend.rs` | 存储后端 (keychain / 文件) |
| `sanitizer.rs` | 输出脱敏 — 防止密钥泄露到日志和输出 |

## 持久化状态 (`state/`)

| 模块 | 说明 |
|------|------|
| `db.rs` | SQLite 数据库封装 |
| `memories_db.rs` | 记忆专用数据库 |
| `memory.rs` | 内存状态管理 |
| `rollout.rs` | Rollout 状态持久化 |
| `migration_runner.rs` | 数据库迁移运行器 — 管理 SQLite schema 版本升级 (`SCHEMA_VERSION`) |

## Skills 系统 (`core/skills/`)

可扩展的技能系统，支持本地和远程 Skill。

| 模块 | 说明 |
|------|------|
| `loader.rs` | 从文件系统加载 Skill |
| `manager.rs` | `SkillsManager` — Skill 生命周期管理 |
| `injection.rs` | 将 Skill 指令注入到 system prompt |
| `permissions.rs` | Skill 权限控制 (文件访问、网络等) |
| `remote.rs` | 远程 Skill 下载和安装 |
| `model.rs` | Skill 数据模型 (`SkillMetadata`, `SkillInterface`) |
| `render.rs` | Skill 渲染 — 将 Skill 内容渲染为 prompt 片段 |
| `system.rs` | 系统内置 Skill 管理 |
| `env_var_dependencies.rs` | Skill 环境变量依赖收集 |
| `invocation_utils.rs` | Skill 调用辅助工具 |

## 记忆系统 (`core/memories/`)

跨会话的持久化记忆，分两阶段处理：

1. **Phase 1** — 收集原始记忆 (用户偏好、项目上下文)
2. **Phase 2** — 摘要和压缩记忆

| 模块 | 说明 |
|------|------|
| `storage.rs` | 记忆文件存储 |
| `prompts.rs` | 记忆相关的 prompt 模板 |
| `start.rs` | 启动时加载记忆 (`start_memories_startup_task`) |
| `phase1.rs` | Phase 1 — 原始记忆收集 |
| `phase2.rs` | Phase 2 — 记忆摘要和压缩 |
| `usage.rs` | 记忆使用统计 — 追踪记忆的使用频率和效果 |
| `citations.rs` | 记忆引用 — 管理记忆在对话中的引用关系 |

## 上下文管理 (`core/context_manager/`)

管理模型上下文窗口，防止超出 token 限制。

| 模块 | 说明 |
|------|------|
| `history.rs` | 历史记录管理和截断 |
| `normalize.rs` | 输入规范化 |
| `updates.rs` | 上下文增量更新 |

配合 `compact.rs`（上下文压缩）和 `truncation.rs`（截断策略）使用。

## 模型管理 (`core/models_manager/`)

| 模块 | 说明 |
|------|------|
| `manager.rs` | `ModelsManager` — 模型列表刷新和选择 |
| `model_info.rs` | `ModelDescriptor` — 模型元信息 |
| `cache.rs` | 模型列表缓存 |
| `collaboration_mode_presets.rs` | 协作模式预设 — 内置的协作模式配置 (`CollaborationModesConfig`) |
| `model_presets.rs` | 模型预设 — 预定义的模型配置 |

## Rollout 系统 (`core/rollout/`)

会话录制和回放，用于调试和审计。

| 模块 | 说明 |
|------|------|
| `recorder.rs` | `RolloutRecorder` — 录制器 |
| `policy.rs` | 录制策略 (何时录制, `EventPersistenceMode`, `SessionSource`) |
| `session_index.rs` | 会话索引 |
| `truncation.rs` | 录制数据截断 |
| `list.rs` | 会话列表查询 |
| `metadata.rs` | 会话元数据 |
| `error.rs` | Rollout 错误类型 |

## 其他辅助模块

| 模块 | 说明 |
|------|------|
| `hooks.rs` | Hook 系统 — 事件钩子注册和触发 |
| `git_info.rs` | Git 仓库信息收集 (分支、commit、diff、remote URLs) |
| `project_doc.rs` | 项目文档发现 (AGENTS.md 等) |
| `file_watcher.rs` | 文件变更监听 |
| `realtime.rs` | 实时对话 (语音/文本)，`RealtimeConversationManager` |
| `patch.rs` | Unified diff 补丁应用器，`PatchApplicator` |
| `seatbelt.rs` | macOS Seatbelt 沙箱 (仅 macOS) |
| `analytics_client.rs` | 分析事件上报，`AnalyticsEventsClient` |
| `netproxy/` | SOCKS5/HTTP 网络代理 |
| `file_search/` | BM25 文件搜索 |
| `shell_escalation/` | Shell 权限提升 |
| `message_history.rs` | 对话历史持久化 (`HistoryEntry`, `HistoryPersistence`) |
| `external_agent_config.rs` | 外部 Agent 配置检测与迁移 (`ExternalAgentConfigService`) |
| `network_policy_decision.rs` | 网络策略决策 (`NetworkPolicyDecision`, `BlockedRequest`) |
| `shell.rs` | Shell 检测与管理 (`Shell`, `ShellType`, `detect_shell_type`) |
| `shell_snapshot.rs` | Shell 环境快照 (`ShellSnapshot`) |
| `state_db.rs` | 状态数据库封装 (`StateDb`) |
| `turn_diff_tracker.rs` | Turn 变更追踪 (`TurnDiffTracker`) |
| `text_encoding.rs` | 文本编码处理 (`bytes_to_string_smart`) |
| `truncation.rs` | 截断策略 (`TruncationPolicy`) |
| `core/state/` | 会话状态分层管理 — service（长期服务）、session（会话状态）、turn（Turn 状态） |
| `core/review_prompts.rs` | 代码审查 prompt 模板 (`ResolvedReviewRequest`, `review_prompt`) |
| `core/review_format.rs` | 代码审查输出格式化 (`render_review_output_text`) |
| `core/custom_prompts.rs` | 自定义 prompt 管理 — 加载和列出用户自定义 prompt |
| `core/compact.rs` | 上下文压缩 — 压缩对话历史以节省 token (`compact`, `compact_remote`) |
| `responses_api_proxy/` | API 反向代理 — 含进程加固 (`process_hardening.rs`) 和 API Key 读取 (`read_api_key.rs`) |

## 流式文本解析 (`stream_parser/`)

处理 AI 模型流式输出的文本解析，支持多种内容格式的实时提取。

| 模块 | 说明 |
|------|------|
| `utf8_stream.rs` | UTF-8 流解析器 — 处理不完整的 UTF-8 字节序列 (`Utf8StreamParser`) |
| `assistant_text.rs` | 助手文本解析 — 提取助手消息文本块 (`AssistantTextStreamParser`) |
| `citation.rs` | 引用解析 — 从流式文本中提取和剥离引用标记 (`CitationStreamParser`) |
| `inline_hidden_tag.rs` | 内联隐藏标签 — 解析流中的隐藏标签（如工具调用标记）(`InlineHiddenTagParser`) |
| `proposed_plan.rs` | 计划解析 — 提取 AI 提出的执行计划 (`ProposedPlanParser`) |
| `tagged_line_parser.rs` | 标签行解析 — 按标签分类解析输出行 |
| `stream_text.rs` | 流式文本 — 基础文本流处理 (`StreamTextParser`) |

## Mosaic API 层 (`mosaic_api/`)

对 OpenAI Responses API 的高层抽象，提供类型安全的 API 客户端。

| 模块 | 说明 |
|------|------|
| `endpoint/responses.rs` | Responses API 客户端 (`ResponsesClient`) — SSE/WebSocket 流式请求 |
| `endpoint/compact.rs` | Compact API 客户端 (`CompactClient`) — 上下文压缩 |
| `endpoint/memories.rs` | Memories API 客户端 (`MemoriesClient`) — 记忆摘要 |
| `endpoint/models.rs` | Models API 客户端 (`ModelsClient`) — 模型列表 |
| `sse/` | SSE 流处理 |
| `requests/` | 请求构建（含 headers 构建） |
| `common.rs` | 共享类型 (`ResponseEvent`, `ResponseStream`, `CompactionInput` 等) |
| `rate_limits.rs` | 速率限制解析和追踪 |
| `telemetry.rs` | 请求遥测 (`SseTelemetry`, `WebsocketTelemetry`) |
| `provider.rs` | Provider 抽象 — 支持 OpenAI 和 Azure |
| `auth.rs` | 认证提供者 (`AuthProvider`) |
| `error.rs` | API 错误类型 (`ApiError`) |

## Mosaic HTTP 客户端 (`mosaic_client/`)

底层 HTTP 传输层，为 `mosaic_api/` 提供网络通信能力。

| 模块 | 说明 |
|------|------|
| `transport.rs` | HTTP 传输抽象 (`HttpTransport`, `ReqwestTransport`) — 支持流式响应 |
| `sse.rs` | SSE 流解析 — 将 HTTP 响应转换为 SSE 事件流 |
| `request.rs` | 请求/响应类型 (`Request`, `Response`, `RequestCompression`) |
| `retry.rs` | 重试策略 (`RetryPolicy`, `RetryOn`) — 指数退避重试 |
| `error.rs` | 错误类型 (`StreamError`, `TransportError`) |
| `telemetry.rs` | 请求遥测 (`RequestTelemetry`) |