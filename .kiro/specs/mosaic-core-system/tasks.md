# Implementation Plan: Mosaic Core System

## Overview

基于依赖关系顺序实现 Mosaic Core System 的 Rust 后端核心。从基础协议层开始，逐步构建配置、状态存储、执行策略、沙箱执行、核心引擎、工具系统、MCP 集成、技能系统、Agent 系统等模块，最终通过 Tauri Command 暴露给前端。所有模块在 `src-tauri/src/` 下作为子模块组织。

测试策略：proptest 属性测试（100 次迭代）、tokio::test 异步测试、wiremock HTTP mock、tempfile 文件隔离、insta 快照测试、pretty_assertions 增强断言。

## Tasks

- [x] 1. 项目基础设施与错误处理
  - [x] 1.1 配置 Cargo.toml 依赖和模块结构
    - 添加 serde, serde_json, async_channel, tokio, rusqlite, proptest, wiremock, tempfile, insta, pretty_assertions, chrono, uuid, async-trait, sha1, toml 等依赖
    - 创建 `src-tauri/src/` 下的模块目录结构：protocol/, core/, exec/, execpolicy/, config/, state/, netproxy/, shell_command/, secrets/
    - 在 lib.rs 中声明所有子模块
    - _Requirements: 全局_

  - [x] 1.2 实现 CodexError 和 ErrorCode（protocol/error.rs）
    - 定义 ErrorCode 枚举（InvalidInput, ToolExecutionFailed, McpServerUnavailable, ConfigurationError, SandboxViolation, ApprovalDenied, SessionError, InternalError）
    - 定义 CodexError 结构体（code, message, details），派生 Serialize/Deserialize，使用 camelCase
    - 实现 std::fmt::Display 和 std::error::Error trait
    - _Requirements: 29.1, 29.2, 29.3, 29.4_

  - [x] 1.3 编写 CodexError JSON round-trip 属性测试
    - **Property 5 (部分): CodexError round-trip**
    - 为 ErrorCode 和 CodexError 实现 Arbitrary
    - 验证序列化/反序列化 round-trip
    - **Validates: Requirements 29.5**

- [x] 2. 协议层 — 核心类型定义（mosaic_protocol）
  - [x] 2.1 实现附加类型（protocol/types.rs）
    - 定义 SandboxPolicy 枚举（ReadOnly, WorkspaceWriteOnly, DangerFullAccess），使用 tag="type" + camelCase
    - 定义 AskForApproval 枚举（Never, OnFailure, UnlessAllowListed, Always）
    - 定义 ReviewDecision 结构体（approved, always_approve, custom_instructions）
    - 定义 TurnStatus, Effort, ServiceTier, CollaborationMode 枚举
    - 定义 Personality, RealtimeConfig 结构体
    - 定义 ContentItem 枚举（Text, Image, InputAudio）
    - 定义 UserInput 结构体（content_items: Vec<ContentItem>）
    - 定义 ResponseInputItem 枚举（Message, FunctionCall, FunctionOutput）
    - 定义 FunctionCallOutputPayload 和 ContentOrItems（untagged 枚举）
    - 定义 DynamicToolSpec 和 DynamicToolCallRequest 结构体
    - 所有类型派生 Serialize/Deserialize + camelCase
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.9, 4.10, 4.11, 4.12_

  - [x] 2.2 实现 Op 操作枚举（protocol/submission.rs）
    - 定义 Submission 结构体（id: String, op: Op）
    - 定义 Op 枚举，使用 tag="type" + camelCase，包含 20+ 变体
    - 核心对话：UserTurn, UserInput, UserInputAnswer, Interrupt, Shutdown
    - 审批操作：ExecApproval, PatchApproval, ResolveElicitation
    - 上下文覆盖：OverrideTurnContext
    - 历史管理：AddToHistory
    - MCP 管理：ListMcpTools, RefreshMcpServers
    - 动态工具：DynamicToolResponse
    - 配置与技能：ReloadUserConfig, ListSkills, ListCustomPrompts
    - 实时对话：RealtimeConversationStart, RealtimeConversationStop, RealtimeConversationSendAudio
    - 后台管理：CleanBackgroundTerminals
    - _Requirements: 1.1, 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8, 2.9_

  - [x] 2.3 实现 EventMsg 事件枚举（protocol/event.rs）
    - 定义 Event 结构体（id: String, msg: EventMsg）
    - 定义 EventMsg 枚举，使用 tag="type" + camelCase，包含 25+ 变体
    - 会话生命周期：SessionConfigured, TurnStarted, TurnComplete, TurnAborted
    - Agent 消息流：AgentMessage, AgentMessageDelta, ReasoningDelta, PlanDelta
    - 结构化项目：ItemStarted, ItemCompleted, RawResponseItem
    - 命令执行：ExecCommandBegin, ExecCommandEnd, ExecCommandOutputDelta
    - 补丁应用：PatchApplyBegin, PatchApplyEnd
    - MCP 工具调用：McpToolCallBegin, McpToolCallEnd, McpStartupUpdate, McpStartupComplete
    - Token 统计：TokenUsageUpdate, TokenUsageSummary
    - 历史压缩：Compacted
    - 警告与错误：Warning, Error
    - _Requirements: 1.2, 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8, 3.9, 3.10_

  - [x] 2.4 实现 protocol/mod.rs 模块导出
    - 统一导出所有协议类型
    - _Requirements: 1.1, 1.2, 1.3_

  - [x] 2.5 编写协议类型 JSON round-trip 属性测试
    - **Property 1: Protocol types JSON round-trip**
    - 为 Submission, Op, Event, EventMsg, SandboxPolicy, AskForApproval, ContentItem, UserInput, ResponseInputItem, ReviewDecision, TurnStatus, Effort, ServiceTier, CollaborationMode, Personality, FunctionCallOutputPayload, ContentOrItems, DynamicToolSpec, DynamicToolCallRequest, RealtimeConfig 实现 Arbitrary
    - 使用 proptest 验证所有类型 JSON 序列化后反序列化产生等价对象
    - 每个属性测试 100 次迭代
    - **Validates: Requirements 1.4**

  - [x] 2.6 编写协议类型 camelCase 序列化属性测试
    - **Property 2: Protocol types use camelCase serialization**
    - 验证所有协议类型 JSON 输出仅包含 camelCase 字段名
    - 使用正则表达式检查 JSON key 不包含 snake_case
    - **Validates: Requirements 1.5**

- [x] 3. 检查点 — 协议层完成
  - 确保所有测试通过，如有问题请向用户确认。

- [x] 4. Shell 命令解析模块（mosaic_shell_command）
  - [x] 4.1 实现 parse_command 函数（shell_command/mod.rs）
    - 实现 shell 命令字符串解析为 token 列表
    - 支持引号、转义字符、空格分隔
    - 有效输入返回 Ok(Vec<String>)，无效输入返回 CodexError
    - _Requirements: 22.1, 22.2, 22.3_

  - [x]* 4.2 编写 shell 命令解析 round-trip 属性测试
    - **Property 43: Shell command parsing round-trip**
    - 验证非空 token 组成的命令字符串解析后用空格连接可重构等价命令
    - **Validates: Requirements 22.4**

- [x] 5. 执行策略引擎（mosaic_execpolicy）
  - [x] 5.1 实现 PrefixPattern 和 PrefixRule（execpolicy/prefix_rule.rs）
    - 定义 PolicyDecision 枚举（Allow, Prompt, Forbidden { reason }）
    - 定义 PrefixPattern 结构体（segments, is_wildcard）
    - 定义 PrefixRule 结构体（pattern, decision）
    - 实现前缀匹配逻辑
    - _Requirements: 10.1, 10.2_

  - [x] 5.2 实现 NetworkRule（execpolicy/network_rule.rs）
    - 定义 NetworkRule 结构体（domain_pattern, decision）
    - 实现域名模式匹配逻辑
    - _Requirements: 10.3_

  - [x] 5.3 实现 PolicyParser（execpolicy/parser.rs）
    - 实现 .codexpolicy 格式解析器
    - parse 方法解析命令规则，parse_network_rules 方法解析网络规则
    - 实现 Pretty_Printer 将规则格式化回 .codexpolicy 文件内容
    - _Requirements: 10.4, 10.5_

  - [x] 5.4 实现 ExecPolicyEngine（execpolicy/mod.rs）
    - 实现 evaluate 方法评估命令决策
    - 实现 evaluate_network 方法评估域名访问决策
    - 实现 load_from_file 方法从文件加载策略
    - Forbidden 规则阻止执行并返回原因，Prompt 规则发出审批信号，无匹配时应用默认策略
    - _Requirements: 10.7, 10.8, 10.9, 10.10, 10.11_

  - [x]* 5.5 编写执行策略决策映射属性测试
    - **Property 17: Execution policy decision mapping**
    - 验证 PrefixRule 匹配逻辑：Allow/Forbidden/Prompt 规则正确映射，无匹配时返回默认策略
    - **Validates: Requirements 10.1, 10.3, 10.4, 10.5**

  - [ ]* 5.6 编写 .codexpolicy 格式 round-trip 属性测试
    - **Property 6 (部分): PrefixRule/NetworkRule round-trip**
    - 验证解析后打印再解析产生等价规则列表
    - **Validates: Requirements 10.6**

- [x] 6. 分层配置系统（mosaic_config）
  - [x] 6.1 实现 ConfigToml 和 MCP 配置类型（config/toml_types.rs）
    - 定义 ConfigToml 结构体，使用 kebab-case 命名
    - 定义 McpServerTransportConfig 枚举（Stdio, Http, OAuth）
    - 定义 McpServerConfig 结构体（transport, disabled, disabled_reason, tool_filter）
    - 定义 McpToolFilter 结构体（enabled, disabled）
    - _Requirements: 11.1, 12.1, 12.2, 12.3_

  - [x] 6.2 实现 ConfigLayerStack（config/layer_stack.rs）
    - 定义 ConfigLayer 枚举（Mdm, System, User, Project, Session）
    - 实现 ConfigLayerStack 的 merge 方法，按优先级合并配置
    - MDM > System > User > Project > Session 优先级
    - 支持 profile 覆盖基础配置
    - _Requirements: 11.2, 11.3, 11.4, 11.5_

  - [x] 6.3 实现 ConfigEdit 构建器（config/edit.rs）
    - 实现构建器模式用于原子配置修改
    - _Requirements: 11.6_

  - [x] 6.4 实现 config/mod.rs 模块导出和 TOML 序列化支持
    - 支持 ConfigToml 序列化回 TOML 格式
    - 无效 TOML 返回描述性解析错误
    - _Requirements: 11.7, 11.8_

  - [x]* 6.5 编写 ConfigToml TOML round-trip 属性测试
    - **Property 3: ConfigToml TOML round-trip**
    - 为 ConfigToml, McpServerConfig, McpServerTransportConfig, McpToolFilter 实现 Arbitrary
    - 验证序列化为 TOML 后反序列化产生等价对象
    - **Validates: Requirements 11.9**

  - [x]* 6.6 编写配置层级优先级合并属性测试
    - **Property 18: Config layer priority merge**
    - 验证多层级定义相同字段时使用最高优先级层级的值
    - **Validates: Requirements 11.2, 11.3, 11.4**

  - [x]* 6.7 编写配置 profile 覆盖属性测试
    - **Property 19: Config profile override**
    - 验证命名 profile 激活后覆盖基础配置值
    - **Validates: Requirements 11.5**

  - [x]* 6.8 编写无效 TOML 解析错误属性测试
    - **Property 20: Invalid TOML returns parse error**
    - 验证无效 TOML 字符串返回描述性错误，不 panic
    - **Validates: Requirements 11.7**

- [x] 7. 状态存储（mosaic_state）
  - [x] 7.1 实现 StateDb 和 SQLite 基础（state/db.rs）
    - 定义 StateDb, StateRuntime, StateConfig, StateMetrics, LogDb 结构体
    - 实现 StateDb::open 方法初始化 SQLite 数据库
    - 实现 run_migrations 方法执行 18 个迁移脚本
    - 实现 init_tables 创建所需表
    - 定义 SessionMeta, ThreadMetadata, AgentJob, AgentJobItem, AgentJobStatus, BackfillState 类型
    - _Requirements: 13.1, 13.2, 13.4, 13.5, 13.6, 13.7, 13.8, 13.10_

  - [x] 7.2 实现 Rollout 存储（state/rollout.rs）
    - 定义 Rollout 结构体（session_id, events, created_at）
    - 实现 save_rollout 和 load_rollout 方法
    - 使用事务确保数据完整性
    - _Requirements: 13.3_

  - [x] 7.3 实现 Memory 系统（state/memory.rs）
    - 定义 Memory 结构体（phase, content, timestamp, relevance_score）
    - 定义 MemoryPhase 枚举（Phase1, Phase2）
    - 实现 Memory 的 CRUD 操作
    - _Requirements: 13.9_

  - [x] 7.4 实现 state/mod.rs 模块导出
    - 统一导出所有状态存储类型和方法
    - 数据库操作失败返回描述性错误，使用事务回滚保护已有数据
    - _Requirements: 13.11_

  - [ ]* 7.5 编写 Rollout 存储/检索 round-trip 属性测试
    - **Property 21: Rollout store/retrieve round-trip**
    - 使用 tempfile 创建隔离的 SQLite 数据库
    - 验证存储后检索产生等价 Rollout，事件顺序一致
    - **Validates: Requirements 13.12**

  - [ ]* 7.6 编写 SessionMeta 存储/检索 round-trip 属性测试
    - **Property 22: SessionMeta store/retrieve round-trip**
    - 验证存储后检索产生等价 SessionMeta
    - **Validates: Requirements 13.13**

  - [ ]* 7.7 编写 Memory 存储属性测试
    - **Property 23: Memory phase storage**
    - 验证存储后检索保留 phase, content, timestamp, relevance_score
    - **Validates: Requirements 13.14**

  - [ ]* 7.8 编写数据库错误保护属性测试
    - **Property 24: Database error preserves existing data**
    - 验证失败操作不破坏已有数据
    - **Validates: Requirements 13.11**

- [x] 8. 检查点 — 基础设施层完成
  - 确保所有测试通过，如有问题请向用户确认。

- [x] 9. 敏感信息管理（mosaic_secrets）
  - [x] 9.1 实现 SecretName 和 SecretScope（secrets/mod.rs）
    - 定义 SecretName newtype，带格式验证
    - 定义 SecretScope 枚举（Global, Environment(String)）
    - 实现 scan_for_secrets 函数，检测 API 密钥、bearer token、私钥等模式
    - 返回 Vec<SecretMatch>，每个匹配包含 kind, range, redacted
    - _Requirements: 23.1, 23.8_

  - [x] 9.2 实现 SecretsBackend trait 和 SecretsManager（secrets/manager.rs, secrets/backend.rs）
    - 定义 SecretsBackend async trait（get, set, delete, list）
    - 实现 SecretsManager 结构体
    - 实现 new_with_keyring 方法创建 keyring 集成实例
    - 提供 get_secret, set_secret, delete_secret, list_secrets 方法
    - 实现内存后端用于测试
    - _Requirements: 23.2, 23.3, 23.4_

  - [x] 9.3 实现 redact_secrets 输出脱敏（secrets/sanitizer.rs）
    - 实现 redact_secrets 函数，将已知密钥值替换为脱敏占位符
    - 确保输出不包含任何原始密钥值
    - _Requirements: 23.10_

  - [ ]* 9.4 编写 SecretsManager CRUD round-trip 属性测试
    - **Property 48: SecretsManager CRUD round-trip**
    - 为 SecretName, SecretScope 实现 Arbitrary
    - 验证 set_secret → get_secret 返回存储值
    - 验证 delete_secret → get_secret 返回 None
    - 验证 list_secrets 包含所有已设置密钥
    - **Validates: Requirements 23.5, 23.6, 23.7**

  - [ ]* 9.5 编写敏感信息检测和脱敏属性测试
    - **Property 44: Secrets detection and redaction completeness**
    - 验证包含已知敏感模式的字符串被正确检测
    - 验证 redacted 字段不包含原始密钥值
    - 验证 redact_secrets 输出不包含任何原始密钥值
    - **Validates: Requirements 23.9, 23.11**

- [x] 10. 命令执行沙箱（mosaic_exec）
  - [x] 10.1 实现 CommandExecutor（exec/sandbox.rs）
    - 定义 CommandExecutor 结构体（sandbox_policy, approval_policy, allow_list, tx_event）
    - 定义 ExecResult 结构体（exit_code, stdout, stderr）
    - 实现 execute 方法，在执行前发送 ExecCommandBegin 事件，完成后发送 ExecCommandEnd 事件
    - 根据 SandboxPolicy 限制文件操作：ReadOnly 禁止写、WorkspaceWriteOnly 限制写目录、DangerFullAccess 允许全部
    - 违反策略时终止进程并发送 Error 事件
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5, 9.6, 9.11_

  - [x] 10.2 实现审批策略逻辑（exec/sandbox.rs）
    - Always：每个命令前暂停请求审批
    - Never：直接执行
    - OnFailure：非零退出码时请求审批
    - UnlessAllowListed：允许列表内直接执行，其他请求审批
    - _Requirements: 9.7, 9.8, 9.9, 9.10_

  - [x] 10.3 实现 exec/mod.rs 模块导出
    - 统一导出 CommandExecutor, ExecResult
    - _Requirements: 9.1_

  - [ ]* 10.4 编写沙箱策略执行属性测试
    - **Property 15: Sandbox policy enforcement**
    - 验证 ReadOnly 限制写操作、WorkspaceWriteOnly 限制写目录、DangerFullAccess 允许全部
    - **Validates: Requirements 9.3, 9.4, 9.5, 9.6**

  - [ ]* 10.5 编写审批策略行为属性测试
    - **Property 16: Approval policy behavior**
    - 验证四种审批策略的正确行为
    - **Validates: Requirements 9.7, 9.8, 9.9, 9.10**

  - [ ]* 10.6 编写命令执行 bracket 事件属性测试
    - **Property 14: Command execution bracket events**
    - 验证执行前发送 ExecCommandBegin，完成后发送 ExecCommandEnd
    - **Validates: Requirements 9.1, 9.2**

- [x] 11. 网络代理（mosaic_netproxy）
  - [x] 11.1 实现 NetworkPolicyDecider 和 NetworkProxy（netproxy/proxy.rs）
    - 定义 NetworkPolicyDecider 结构体（allow_rules, deny_rules）
    - 实现 evaluate 方法评估域名和端口访问决策
    - deny 规则优先于 allow 规则
    - 定义 NetworkProxy 结构体（http_proxy, socks5_proxy, cert_manager, policy_decider, config_reloader）
    - 定义 NetworkProxyConfig 结构体（listen_addr, socks5_addr, allowed_domains, blocked_domains, mitm_enabled）
    - 定义 HttpProxyServer, Socks5ProxyServer, CertificateManager 结构体
    - 实现 start, stop, is_domain_allowed, reload_config 方法
    - 使用 tokio::sync::watch 通道支持运行时配置重载
    - _Requirements: 21.1, 21.2, 21.3, 21.4, 21.5, 21.6, 21.7, 21.8_

  - [x] 11.2 实现 netproxy/mod.rs 模块导出
    - 统一导出网络代理类型
    - _Requirements: 21.1_

  - [ ]* 11.3 编写网络代理域名过滤属性测试
    - **Property 42: NetworkProxy domain filtering**
    - 为 NetworkPolicyDecider 实现 Arbitrary
    - 验证 allow/deny 规则评估逻辑，deny 优先于 allow
    - **Validates: Requirements 21.3, 21.4, 21.5**

- [x] 12. 检查点 — 执行与安全层完成
  - 确保所有测试通过，如有问题请向用户确认。

- [x] 13. 核心引擎 — 会话与 TurnContext（mosaic_core）
  - [x] 13.1 实现 Session 和 SessionState（core/session.rs）
    - 定义 Session 结构体（state: Mutex<SessionState>, config, tool_registry, mcp_manager, hooks, tx_event）
    - 定义 SessionState 结构体（history, turn_active, pending_approval, turn_context）
    - 定义 TurnContext 结构体（model_info, sandbox_policy, approval_policy, cwd）
    - 定义 ModelInfo, PendingApproval 辅助类型
    - 实现 TurnContext 创建，从活跃配置 profile 继承默认值
    - 实现 OverrideTurnContext 逻辑，仅修改指定字段
    - 实现 history 有序列表维护
    - 实现轮次活跃状态检查，拒绝并发 UserTurn
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6_

  - [x] 13.2 实现 Session 的 rollback 方法（core/session.rs）
    - 接受 steps 参数（usize），移除最后 n 条历史记录
    - 保持剩余条目顺序不变
    - _Requirements: 25.1, 25.2, 25.3_

  - [ ]* 13.3 编写 TurnContext 继承属性测试
    - **Property 9: TurnContext inherits from config**
    - 验证新 TurnContext 从活跃配置继承默认值
    - **Validates: Requirements 6.2, 6.3**

  - [ ]* 13.4 编写 OverrideTurnContext 属性测试
    - **Property 10: OverrideTurnContext applies overrides**
    - 验证仅更新指定字段，其他字段不变
    - **Validates: Requirements 6.4**

  - [ ]* 13.5 编写会话历史顺序保持属性测试
    - **Property 11: Session history preserves order**
    - 验证 ResponseInputItem 条目插入顺序保持
    - **Validates: Requirements 6.5**

  - [ ]* 13.6 编写并发 UserTurn 拒绝属性测试
    - **Property 8: Reject concurrent UserTurn**
    - 验证活跃轮次期间拒绝额外 UserTurn 并发送 Error 事件
    - **Validates: Requirements 6.6**

  - [ ]* 13.7 编写历史回滚属性测试
    - **Property 45: History rollback restores previous state**
    - 验证 rollback(n) 后历史长度为 L-n，剩余条目为前 L-n 条
    - **Validates: Requirements 25.2, 25.3**

- [x] 14. 核心引擎 — 工具处理器系统
  - [x] 14.1 实现 ToolHandler trait 和 ToolRegistry（core/tools/mod.rs）
    - 定义 ToolHandler async trait（matches_kind, kind, handle）
    - 定义 ToolKind 类型
    - 实现 ToolRegistry（register, dispatch 方法）
    - _Requirements: 7.1, 7.2, 7.6_

  - [x] 14.2 实现 ToolRouter（core/tools/router.rs）
    - 定义 ToolRouter 结构体（registry, mcp_manager, dynamic_tools）
    - 实现 route_tool_call 方法，按优先级路由：内置 → MCP → 动态
    - 未找到处理器返回 ToolExecutionFailed 错误
    - 实现 register_dynamic_tool 方法
    - 实现 list_all_tools 方法
    - _Requirements: 8.1, 8.2, 8.3, 8.4_

  - [x] 14.3 实现动态工具处理器（core/tools/handlers/dynamic.rs）
    - 实现 DynamicToolSpec 注册和调用逻辑
    - 调用时发送 DynamicToolCallRequest 事件，等待 DynamicToolResponse
    - _Requirements: 27.1, 27.2, 27.3, 27.4_

  - [ ]* 14.4 编写工具处理器注册 round-trip 属性测试
    - **Property 12: Tool handler registry round-trip**
    - 验证注册后通过 ToolKind 查询返回 matches_kind 为 true 的处理器
    - **Validates: Requirements 7.2, 7.6**

  - [ ]* 14.5 编写 ToolRouter 路由正确性属性测试
    - **Property 50: ToolRouter routing correctness**
    - 验证路由优先级：内置 → MCP → 动态，未知工具返回错误
    - **Validates: Requirements 8.1, 8.2**

- [x] 15. 核心引擎 — Codex 结构体与 submission_loop
  - [x] 15.1 实现 Codex 结构体和 spawn 方法（core/codex.rs）
    - 定义 Codex 结构体（tx_sub: async_channel::Sender<Submission>, rx_event: async_channel::Receiver<Event>）
    - 实现 Codex::spawn 方法，接受 Config 参数，初始化 Session，启动 submission_loop
    - 使用 tokio::sync::Mutex 保护 Session 状态
    - _Requirements: 5.1, 5.2, 5.8_

  - [x] 15.2 实现 submission_loop 主循环（core/codex.rs）
    - 从 SQ 接收 Submission，根据 Op 类型分发处理
    - Op::UserTurn → 调用 run_turn，发送 TurnStarted/TurnComplete bracket 事件
    - Op::Interrupt → 取消当前轮次，发送 TurnComplete
    - Op::Shutdown → 优雅关闭 Session 和所有通道
    - Op::ExecApproval → 根据 ReviewDecision.approved 继续或取消待执行命令
    - Op::PatchApproval → 根据 ReviewDecision.approved 继续或取消待应用补丁
    - Op::OverrideTurnContext → 更新当前 TurnContext
    - Op::AddToHistory → 添加条目到会话历史
    - Op::ListMcpTools / RefreshMcpServers → 委托 MCP 管理器
    - Op::DynamicToolResponse → 匹配 call_id 返回工具结果
    - Op::ReloadUserConfig / ListSkills / ListCustomPrompts → 委托配置/技能模块
    - Op::RealtimeConversation* → 委托实时对话管理器
    - Op::CleanBackgroundTerminals → 清理后台终端
    - _Requirements: 5.3, 5.4, 5.5, 5.6, 5.7, 5.9, 5.10, 5.11_

  - [x] 15.3 实现 run_turn 函数（core/codex.rs）
    - 创建 TurnContext，发送 TurnStarted 事件
    - 处理模型交互和工具调用
    - 工具调用时发送 McpToolCallBegin/McpToolCallEnd 或 Error 事件
    - 完成后发送 TurnComplete 事件
    - _Requirements: 5.9, 5.10, 5.11, 7.3, 7.4, 7.5_

  - [x] 15.4 实现 core/mod.rs 模块导出
    - 统一导出 Codex, Session, TurnContext, ToolHandler, ToolRegistry, ToolRouter 等
    - _Requirements: 5.1_

  - [ ]* 15.5 编写 Turn 生命周期 bracket 事件属性测试
    - **Property 4: Turn lifecycle bracket events**
    - 使用 TestCodexBuilder 模式构建测试实例
    - 验证 UserTurn 事件序列以 TurnStarted 开始、TurnComplete 结束
    - **Validates: Requirements 5.3, 5.9, 5.10**

  - [ ]* 15.6 编写 Interrupt 取消活跃轮次属性测试
    - **Property 5: Interrupt cancels active turn**
    - 验证 Interrupt 取消当前轮次并发送 TurnComplete
    - **Validates: Requirements 5.4**

  - [ ]* 15.7 编写 Shutdown 关闭通道属性测试
    - **Property 6: Shutdown closes all channels**
    - 验证 Shutdown 后 Session 优雅关闭，通道关闭
    - **Validates: Requirements 5.5**

  - [ ]* 15.8 编写审批提交控制待执行操作属性测试
    - **Property 7: Approval submission controls pending operation**
    - 验证 approved=true 继续操作，approved=false 取消操作
    - **Validates: Requirements 5.6, 5.7**

  - [ ]* 15.9 编写工具调用 bracket 事件属性测试
    - **Property 13: Tool call bracket events**
    - 验证工具调用前发送 McpToolCallBegin，完成后发送 McpToolCallEnd 或 Error
    - **Validates: Requirements 7.3, 7.4, 7.5**

- [x] 16. 检查点 — 核心引擎完成
  - 确保所有测试通过，如有问题请向用户确认。

- [ ] 17. 钩子系统（Hooks System）
  - [ ] 17.1 实现 HookEvent、HookResult 和 HookRegistry（core/hooks.rs）
    - 定义 HookEvent 枚举（AfterAgent, AfterToolUse），使用 camelCase 序列化
    - 定义 HookResult 枚举（Success, FailedContinue, FailedAbort）
    - 定义 HookHandler async trait（execute 方法）
    - 定义 HookDefinition 结构体（name, event, handler）
    - 实现 HookRegistry（register, fire 方法）
    - fire 方法执行所有匹配事件类型的钩子，收集 HookResult 列表
    - FailedAbort 停止后续处理并发送 Error 事件
    - FailedContinue 记录错误但继续处理
    - 不得静默忽略 FailedAbort
    - _Requirements: 19.1, 19.2, 19.3, 19.4, 19.5, 19.6, 19.7_

  - [ ]* 17.2 编写钩子事件执行属性测试
    - **Property 36: Hook execution after events**
    - 验证钩子仅在事件后触发，所有匹配钩子执行，FailedAbort 停止处理
    - **Validates: Requirements 19.1, 19.2, 19.3**

  - [ ]* 17.3 编写 HookResult 三状态处理属性测试
    - **Property 37: HookResult three-state handling**
    - 验证 Success 继续、FailedContinue 记录错误继续、FailedAbort 停止并发送 Error
    - **Validates: Requirements 19.4**

- [ ] 18. 补丁应用系统（Patch Application）
  - [ ] 18.1 实现补丁应用逻辑（core/patch.rs）
    - 实现补丁应用前发送 PatchApplyBegin 事件（含目标文件路径）
    - 成功时发送 PatchApplyEnd（success=true）
    - 失败时发送 PatchApplyEnd（success=false）和 Error 事件
    - Always/UnlessAllowListed 审批策略下请求用户审批
    - _Requirements: 20.1, 20.2, 20.3, 20.4_

  - [ ]* 18.2 编写补丁应用 bracket 事件属性测试
    - **Property 38: Patch application bracket events**
    - 验证 PatchApplyBegin/PatchApplyEnd 事件序列
    - **Validates: Requirements 20.1, 20.2, 20.3**

  - [ ]* 18.3 编写补丁审批策略属性测试
    - **Property 39: Patch approval policy**
    - 验证 Always/UnlessAllowListed 下请求审批
    - **Validates: Requirements 20.4**

- [ ] 19. 审批决策语义（ReviewDecision）
  - [ ] 19.1 实现 ReviewDecision 语义逻辑（core/codex.rs 集成）
    - approved=false 取消操作，不论其他字段
    - approved=true + always_approve=true 将命令模式添加到允许列表
    - custom_instructions 转发给 Agent 用于下一轮次
    - _Requirements: 28.1, 28.2, 28.3_

  - [ ]* 19.2 编写 ReviewDecision 语义属性测试
    - **Property 49: ReviewDecision semantics**
    - 验证三种语义场景的正确行为
    - **Validates: Requirements 28.1, 28.2, 28.3**

- [ ] 20. 对话历史截断与压缩
  - [ ] 20.1 实现 TruncationPolicy（core/truncation.rs）
    - 定义 TruncationPolicy 枚举（KeepRecent, KeepRecentTokens, AutoCompact）
    - KeepRecent：保留最近 N 条消息
    - KeepRecentTokens：从末尾保留累计 token 数不超过 N 的消息
    - _Requirements: 24.1, 24.2, 24.3_

  - [ ] 20.2 实现 compact 和 compact_remote 函数（core/compact.rs）
    - compact：使用 SUMMARIZATION_PROMPT 模板调用模型生成摘要
    - compact_remote：调用远程 API 进行压缩
    - 历史实际缩短时发送 Compacted 事件（含 new_length）
    - 已压缩历史再次调用 compact 返回不变（幂等性）
    - _Requirements: 24.4, 24.5, 24.6, 24.7_

  - [ ] 20.3 集成 Session 的 compact_history 方法（core/session.rs）
    - 在 Session 中集成截断和压缩逻辑
    - _Requirements: 24.1_

  - [ ]* 20.4 编写截断策略执行属性测试
    - **Property 52: Truncation policy enforcement**
    - 验证 KeepRecent 保留最后 N 条、KeepRecentTokens 按 token 数保留、AutoCompact 调用模型
    - **Validates: Requirements 24.2, 24.3, 24.4**

  - [ ]* 20.5 编写压缩幂等性属性测试
    - **Property 53: Compact idempotence**
    - 验证已压缩历史再次 compact 不变，不发送 Compacted 事件
    - **Validates: Requirements 24.7**

- [ ] 21. 检查点 — 核心功能模块完成
  - 确保所有测试通过，如有问题请向用户确认。

- [ ] 22. MCP 客户端集成
  - [ ] 22.1 实现 McpConnectionManager（core/mcp_client.rs）
    - 定义 McpConnectionManager 结构体（connections: HashMap<String, McpConnection>）
    - 实现 connect 方法，支持 Stdio、HTTP、OAuth 三种传输协议
    - OAuth 传输：先从 token_url 获取 bearer token，再建立 MCP 会话
    - OAuth token 获取失败返回 McpServerUnavailable 错误
    - 维护活跃连接池，提供连接生命周期管理
    - 连接失败发送 Error 事件，标记连接为不可用
    - _Requirements: 14.1, 14.2, 14.7, 14.9, 14.10_

  - [ ] 22.2 实现工具发现和调用（core/mcp_client.rs）
    - 新连接建立时自动调用 tools/list 执行工具发现
    - 使用 `mcp__{server}__{tool}` 格式限定工具名称，64 字符限制
    - 超过 64 字符时应用 SHA1 哈希去重
    - 使用 JSON-RPC 协议路由工具调用到正确服务器
    - 支持 McpToolFilter 的 enabled/disabled 列表进行工具过滤
    - disabled 服务器不尝试连接，保留 disabled_reason
    - disabled 变为 false 时允许重新连接
    - _Requirements: 14.3, 14.4, 14.5, 14.6, 14.8_

  - [ ]* 22.3 编写 MCP 限定工具命名属性测试
    - **Property 25: MCP qualified tool naming**
    - 验证 `mcp__{server}__{tool}` 格式和 64 字符限制
    - 验证超长名称的 SHA1 哈希去重
    - **Validates: Requirements 14.4, 14.5**

  - [ ]* 22.4 编写 MCP 工具调用路由属性测试
    - **Property 26: MCP tool call routing**
    - 使用 wiremock 模拟 MCP 服务器
    - 验证调用路由到正确服务器
    - **Validates: Requirements 14.6**

  - [ ]* 22.5 编写 MCP 连接失败处理属性测试
    - **Property 27: MCP connection failure handling**
    - 验证连接失败发送 Error 事件并标记不可用
    - **Validates: Requirements 14.7**

  - [ ]* 22.6 编写 MCP 工具过滤属性测试
    - **Property 28: MCP tool filtering**
    - 验证 disabled 列表排除工具，enabled 列表仅包含指定工具
    - **Validates: Requirements 14.8**

  - [ ]* 22.7 编写 MCP 连接时工具发现属性测试
    - **Property 41: MCP tool discovery on connection**
    - 验证新连接自动调用 tools/list
    - **Validates: Requirements 14.3**

  - [ ]* 22.8 编写 OAuth MCP 传输认证属性测试
    - **Property 47: OAuth MCP transport authentication**
    - 使用 wiremock 模拟 token_url
    - 验证 OAuth 流程和失败处理
    - **Validates: Requirements 14.9, 14.10**

  - [ ]* 22.9 编写 McpServerConfig disabled 追踪属性测试
    - **Property 55: McpServerConfig disabled tracking**
    - 验证 disabled=true 不连接，disabled_reason 保留，重新启用允许连接
    - **Validates: Requirements 12.4, 12.5**

- [ ] 23. MCP 服务器暴露
  - [ ] 23.1 实现 MCP 服务器接口（core/mcp_server.rs）
    - 暴露 JSON-RPC MCP 服务器接口
    - 实现 initialize 方法
    - 实现 tools/list 方法，返回所有已注册 ToolHandler 的名称和输入 schema
    - 实现 tools/call 方法，分发到匹配的 ToolHandler 并返回结果
    - 未知工具返回 JSON-RPC 错误（code: -32602）
    - _Requirements: 15.1, 15.2, 15.3, 15.4_

  - [ ]* 23.2 编写 MCP 服务器 tools/list 属性测试
    - **Property 29: MCP server tools/list returns all handlers**
    - 验证返回所有已注册处理器的名称和 schema
    - **Validates: Requirements 15.2**

  - [ ]* 23.3 编写 MCP 服务器 tools/call 分发属性测试
    - **Property 30: MCP server tools/call dispatches correctly**
    - 验证已知工具正确分发，未知工具返回 -32602 错误
    - **Validates: Requirements 15.3, 15.4**

- [ ] 24. 技能系统（Skills System）
  - [ ] 24.1 实现技能发现和加载（core/skills.rs）
    - 定义 SkillMetadata 结构体（完整字段：name, short_description, description, version, triggers, interface, dependencies, policy, permission_profile, path_to_skills_md, scope）
    - 定义 SkillInterface, SkillDependencies, SkillPolicy 辅助结构体
    - 定义 SkillScope 枚举（Repo, User, System, Admin）
    - 定义 SkillLoadOutcome 结构体（skills, errors, disabled_paths, implicit_skills）
    - 实现 load_skills_from_roots 函数
    - 多根目录搜索，优先级：Repo > User > System > Admin
    - 解析 SKILL.md 文件的 YAML frontmatter
    - 同名技能使用最高优先级根目录定义
    - 广度优先遍历，最大深度 6 层
    - 提供技能列表接口返回 SkillMetadata
    - _Requirements: 16.1, 16.2, 16.3, 16.4, 16.5, 16.6, 16.7_

  - [ ]* 24.2 编写技能发现优先级解析属性测试
    - **Property 31: Skill discovery with priority resolution**
    - 使用 tempfile 创建多根目录结构
    - 验证 BFS 遍历、深度限制、优先级解析
    - **Validates: Requirements 16.1, 16.3, 16.4**

  - [ ]* 24.3 编写 SKILL.md 解析属性测试
    - **Property 32: SKILL.md parsing**
    - 验证 YAML frontmatter 正确提取 name, description, version, triggers
    - **Validates: Requirements 16.2**

  - [ ]* 24.4 编写技能列表完整性属性测试
    - **Property 33: Skill listing completeness**
    - 验证列表接口返回所有已发现技能的元数据
    - **Validates: Requirements 16.5**

- [ ] 25. 多 Agent 系统
  - [ ] 25.1 实现 AgentControl 和 ThreadManagerState（core/agents.rs）
    - 定义 AgentControl 结构体（state: Mutex<ThreadManagerState>, max_recursion_depth）
    - 定义 ThreadManagerState（agents: HashMap<String, Weak<AgentInstance>>, next_nickname）
    - 定义 SpawnAgentOptions（model, sandbox_policy, cwd, fork, max_depth）
    - 定义 Guards 结构体（spawn_slot, nickname）
    - 实现 spawn_agent：创建 AgentInstance，支持 fork 模式
    - 实现 send_input, resume_agent, wait, close_agent 方法
    - close_agent 优雅终止 Agent 并释放资源
    - 通过 max_recursion_depth 强制执行最大递归深度
    - 超过深度限制拒绝生成并返回错误
    - 使用弱引用避免循环引用
    - _Requirements: 17.1, 17.2, 17.3, 17.4, 17.5, 17.6, 17.7, 17.8_

  - [ ]* 25.2 编写 Agent 递归深度强制执行属性测试
    - **Property 34: Agent recursion depth enforcement**
    - 验证深度限制内成功，超过限制拒绝
    - **Validates: Requirements 17.4, 17.5**

  - [ ]* 25.3 编写 Agent 生命周期管理属性测试
    - **Property 35: Agent lifecycle management**
    - 验证 close_agent 终止 Agent 并释放资源
    - **Validates: Requirements 17.2, 17.3**

- [ ] 26. 批量任务系统
  - [ ] 26.1 实现 BatchJobConfig 和 run_batch_jobs（core/agents.rs）
    - 定义 BatchJobConfig 结构体（csv_path, concurrency）
    - 定义 BatchResult 结构体（row_index, success, output）
    - 实现 run_batch_jobs 函数
    - 同时执行任务数不超过 concurrency 限制
    - 每个输入行返回一个 BatchResult
    - _Requirements: 18.1, 18.2, 18.3, 18.4_

  - [ ]* 26.2 编写批量任务并发限制属性测试
    - **Property 46: Batch job concurrency limit enforcement**
    - 验证不超过 N 个任务同时执行
    - 验证返回结果数量与输入行数一致
    - **Validates: Requirements 18.2, 18.4**

- [ ] 27. 动态工具生命周期
  - [ ] 27.1 集成动态工具完整生命周期（core/tools/router.rs + core/codex.rs）
    - register_dynamic_tool 注册后立即可用于路由
    - 调用动态工具时发送 DynamicToolCallRequest 事件
    - 等待匹配 call_id 的 DynamicToolResponse
    - 将 response 的 result 作为工具调用结果返回
    - _Requirements: 27.1, 27.2, 27.3, 27.4_

  - [ ]* 27.2 编写动态工具生命周期属性测试
    - **Property 51: Dynamic tool lifecycle**
    - 验证注册、调用请求、响应匹配的完整流程
    - **Validates: Requirements 27.1, 27.2, 27.3**

- [ ] 28. 实时对话管理器
  - [ ] 28.1 实现 RealtimeConversationManager（core/realtime.rs）
    - 定义 RealtimeConversationManager 结构体（active_session, tx_event）
    - 定义 RealtimeSession 结构体（session_id, model, voice, started_at）
    - 实现 start 方法：创建 RealtimeSession，is_active() 返回 true
    - 实现 stop 方法：终止会话，is_active() 返回 false
    - 实现 send_audio 方法：发送音频到活跃会话
    - 无活跃会话时发送音频返回错误
    - _Requirements: 26.1, 26.2, 26.3, 26.4, 26.5_

  - [ ]* 28.2 编写实时对话生命周期属性测试
    - **Property 54: Realtime conversation lifecycle**
    - 验证 start/stop 生命周期和 is_active 状态
    - 验证无活跃会话时发送音频返回错误
    - **Validates: Requirements 26.2, 26.3, 26.5**

- [ ] 29. 检查点 — 扩展模块完成
  - 确保所有测试通过，如有问题请向用户确认。

- [ ] 30. Tauri 命令接口
  - [ ] 30.1 实现 Tauri 命令绑定（commands.rs）
    - 实现 user_turn 命令：向 SQ 提交 UserTurn 操作
    - 实现 interrupt 命令：向 SQ 提交 Interrupt 操作
    - 实现 exec_approval 命令：向 SQ 提交 ExecApproval 操作
    - 实现 patch_approval 命令：向 SQ 提交 PatchApproval 操作
    - 实现 shutdown 命令：向 SQ 提交 Shutdown 操作
    - 实现 poll_event 命令：从 EQ 接收序列化为 JSON 的事件
    - 实现 get_config 命令：读取当前解析后的配置
    - 实现 list_skills 命令：列出已发现的技能
    - 无效输入返回 CodexError（ErrorCode::InvalidInput）
    - _Requirements: 30.1, 30.2, 30.3, 30.4, 30.5_

  - [ ] 30.2 集成 Tauri Builder 配置（lib.rs）
    - 在 Tauri Builder 中注册所有命令
    - 初始化 AppState（包含 Codex 实例）
    - _Requirements: 30.1_

  - [ ]* 30.3 编写 Tauri 命令无效输入处理属性测试
    - **Property 40: Tauri command invalid input handling**
    - 验证无效参数返回 CodexError（ErrorCode::InvalidInput）
    - **Validates: Requirements 30.5**

- [ ] 31. 集成测试 — TestCodexBuilder 模式
  - [ ] 31.1 实现 TestCodexBuilder 测试辅助（tests/helpers/mod.rs）
    - 实现 Builder 模式构建测试用 Codex 实例
    - 支持 with_model, with_sandbox_policy, with_approval_policy, with_temp_dir, with_mock_mcp_server 链式配置
    - 集成 wiremock MockServer 用于 MCP HTTP/OAuth 传输测试
    - 集成 tempfile 用于 SQLite 和文件系统隔离
    - 实现事件驱动断言模式（wait_for_event）
    - _Requirements: 全局_

  - [ ]* 31.2 编写端到端集成测试
    - 使用 TestCodexBuilder 验证完整的 UserTurn → TurnStarted → 工具调用 → TurnComplete 流程
    - 验证 Interrupt 和 Shutdown 流程
    - 验证 MCP 工具发现和调用流程
    - _Requirements: 5.3, 5.4, 5.5, 7.3, 14.3_

- [ ] 32. 最终检查点 — 全系统集成验证
  - 确保所有测试通过，如有问题请向用户确认。

## Notes

- 标记 `*` 的任务为可选测试任务，可跳过以加速 MVP 开发
- 每个任务引用具体的 Requirements 编号以确保可追溯性
- 检查点确保增量验证，避免问题累积
- 属性测试验证设计文档中的 55 个 Correctness Properties
- 单元测试验证具体的边界条件和 edge case
- 所有属性测试使用 proptest，每个测试至少 100 次迭代
- 集成测试使用 TestCodexBuilder 模式 + wiremock + tempfile
- 模块实现顺序遵循依赖关系：protocol → config/state/execpolicy/shell_command/secrets → exec/netproxy → core → MCP/skills/agents → Tauri commands
