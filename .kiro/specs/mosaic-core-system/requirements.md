# Requirements Document

## Introduction

Mosaic Core System 是基于 Tauri v2 的桌面应用 Rust 后端核心系统，复刻 OpenAI Codex CLI 的核心后端逻辑。系统采用 SQ/EQ（Submission Queue / Event Queue）异步通信模式，实现 AI 编程助手的会话管理、工具执行、MCP 协议双向集成、分层配置、沙箱安全执行、网络代理、敏感信息管理、对话历史压缩与回滚、实时对话、批量任务等核心能力。本系统仅实现 Rust 后端核心逻辑，不包含 TUI/UI 层。所有模块在同一 crate 内作为子模块组织，通过 Tauri Command 暴露给 React 前端。

## Glossary

- **Mosaic_Core**: 核心业务逻辑引擎，管理会话、工具处理、MCP 连接和配置加载，包含 submission_loop 主循环
- **Mosaic_Protocol**: 协议定义层，定义 Submission、Event、Op、EventMsg、SandboxPolicy 等核心类型，使用 serde camelCase 序列化
- **Mosaic_Config**: 配置管理模块，支持 TOML 格式的分层配置系统，使用 kebab-case 命名
- **Mosaic_State**: 状态存储模块，基于 SQLite 的持久化存储，含 18 个迁移脚本
- **Mosaic_Exec**: 命令执行沙箱模块，提供进程隔离和安全执行
- **Mosaic_ExecPolicy**: 执行策略引擎，基于 .codexpolicy 格式的前缀匹配规则（PrefixRule / PrefixPattern）
- **Mosaic_NetProxy**: 网络代理模块，提供 MITM 和 SOCKS5 代理、域名过滤和证书管理
- **Mosaic_ShellCommand**: Shell 命令解析模块，将命令字符串解析为 token 列表
- **Mosaic_Secrets**: 敏感信息管理模块，含 keyring 集成、敏感信息检测和输出脱敏
- **Session**: 会话管理器，使用 tokio::sync::Mutex 保护的共享状态，管理对话历史和上下文
- **TurnContext**: 每轮对话的执行上下文，包含 model_info、sandbox_policy、approval_policy 和 cwd
- **Submission**: 入站消息结构，包含 id（String）和 op（Op 枚举）
- **Event**: 出站消息结构，包含 id（String）和 msg（EventMsg 枚举）
- **SQ**: Submission Queue，入站队列，基于 async_channel 实现多生产者多消费者
- **EQ**: Event Queue，出站队列，向外发送事件流
- **Op**: 操作枚举类型，包含 20+ 变体（UserTurn、Interrupt、ExecApproval、PatchApproval、Shutdown 等）
- **EventMsg**: 事件消息枚举，包含 25+ 变体（TurnStarted、AgentMessage、ExecCommandBegin/End 等）
- **SandboxPolicy**: 沙箱策略枚举，包含 ReadOnly、WorkspaceWriteOnly（带 writable_roots 列表）、DangerFullAccess 三级权限
- **AskForApproval**: 审批策略枚举，包含 Never、OnFailure、UnlessAllowListed、Always 四种模式
- **ReviewDecision**: 审批决策结构体，包含 approved、always_approve 和 custom_instructions 字段
- **ToolHandler**: 工具处理器 async trait，定义 matches_kind、kind、handle 方法
- **ToolRouter**: 工具路由器，按优先级路由到内置工具、MCP 工具或动态工具
- **MCP**: Model Context Protocol，模型上下文协议，支持 Stdio/HTTP/OAuth 三种传输
- **MCP_Connection_Manager**: MCP 连接管理器，管理多个 MCP 服务器连接的生命周期
- **PolicyDecision**: 策略决策枚举，包含 Allow、Prompt、Forbidden（含 reason）三种状态
- **PrefixRule**: 命令前缀规则，包含 PrefixPattern 和 PolicyDecision
- **PrefixPattern**: 前缀匹配模式，包含 segments 列表和 is_wildcard 标志
- **NetworkRule**: 网络访问规则，包含 domain_pattern 和 PolicyDecision
- **Skill**: 技能模块，包含 SKILL.md 文件和 YAML frontmatter 元数据
- **SkillLoadOutcome**: 技能加载结果，包含 skills、errors、disabled_paths、implicit_skills
- **HookEvent**: 钩子事件枚举，仅包含 AfterAgent 和 AfterToolUse 两种后置事件
- **HookResult**: 钩子执行结果，包含 Success、FailedContinue、FailedAbort 三种状态
- **TruncationPolicy**: 历史截断策略枚举，包含 KeepRecent、KeepRecentTokens、AutoCompact
- **Rollout**: 事件序列持久化记录，支持会话历史存储和回放
- **ConfigToml**: TOML 格式的配置结构体，支持 serde 反序列化
- **Config_Layer_Stack**: 分层配置栈，按 MDM > System > User > Project > Session 优先级合并
- **ContentItem**: 内容项枚举，支持 Text、Image、InputAudio 多模态内容
- **DynamicToolSpec**: 动态工具规格，包含 name、description、input_schema
- **DynamicToolCallRequest**: 动态工具调用请求，包含 call_id、tool_name、arguments
- **SecretName**: 密钥名称，带格式验证的 newtype
- **SecretScope**: 密钥作用域枚举，包含 Global 和 Environment(String)
- **SecretsBackend**: 密钥后端 async trait，定义 get、set、delete、list 方法
- **Tauri_Command**: Tauri v2 命令接口，通过 `@tauri-apps/api/core` 从前端调用
- **CodexError**: 统一错误结构体，包含 code（ErrorCode）、message（String）和 details（Option）
- **ErrorCode**: 错误码枚举，包含 InvalidInput、ToolExecutionFailed、McpServerUnavailable、ConfigurationError、SandboxViolation、ApprovalDenied、SessionError、InternalError
- **ThreadMetadata**: 线程元数据，包含 thread_id、created_at、title、model
- **AgentJob**: Agent 作业，包含 job_id、thread_id、status、items
- **AgentJobStatus**: 作业状态枚举，包含 Pending、Running、Completed、Failed
- **RealtimeConversationManager**: 实时对话管理器，管理 RealtimeSession 生命周期
- **NetworkProxy**: 完整代理服务器，包含 HTTP/SOCKS5 代理、证书管理和策略决策器
- **NetworkPolicyDecider**: 网络策略决策器，基于 allow/deny 规则评估域名访问

## Requirements

### Requirement 1: 协议层 — Submission 与 Event 结构定义

**User Story:** 作为开发者，我希望有一个类型安全的协议层定义所有消息类型和枚举，以便所有系统组件通过一致的接口通信。

#### Acceptance Criteria

1. THE Mosaic_Protocol SHALL 定义 Submission 结构体，包含 id 字段（String 类型）和 op 字段（Op 枚举类型）。
2. THE Mosaic_Protocol SHALL 定义 Event 结构体，包含 id 字段（String 类型）和 msg 字段（EventMsg 枚举类型）。
3. THE Mosaic_Protocol SHALL 为所有公共类型派生 Serialize 和 Deserialize，使用 serde 的 camelCase 字段命名。
4. FOR ALL 有效的 Mosaic_Protocol 类型实例（Submission、Event、Op、EventMsg、SandboxPolicy、AskForApproval、ContentItem、UserInput、ResponseInputItem、CodexError），序列化为 JSON 后再反序列化回来 SHALL 产生等价对象（round-trip 属性）。
5. FOR ALL 有效的 Mosaic_Protocol 类型实例，JSON 序列化输出 SHALL 仅包含 camelCase 字段名，不包含 snake_case 或其他格式。

### Requirement 2: 协议层 — Op 操作枚举

**User Story:** 作为开发者，我希望有一个完整的操作枚举定义所有入站操作类型，以便前端可以通过统一接口提交各种操作。

#### Acceptance Criteria

1. THE Mosaic_Protocol SHALL 定义 Op 枚举，使用 serde 的 camelCase 命名和 tag = "type" 内部标签。
2. THE Mosaic_Protocol SHALL 在 Op 枚举中包含核心对话操作变体：UserTurn（含 input、cwd、policies、model、effort、summary、service_tier、collaboration_mode、personality、final_output_json_schema 字段）、UserInput、UserInputAnswer、Interrupt 和 Shutdown。
3. THE Mosaic_Protocol SHALL 在 Op 枚举中包含审批操作变体：ExecApproval（含 ReviewDecision）、PatchApproval（含 ReviewDecision）和 ResolveElicitation。
4. THE Mosaic_Protocol SHALL 在 Op 枚举中包含上下文覆盖变体 OverrideTurnContext 和历史管理变体 AddToHistory。
5. THE Mosaic_Protocol SHALL 在 Op 枚举中包含 MCP 管理变体：ListMcpTools 和 RefreshMcpServers。
6. THE Mosaic_Protocol SHALL 在 Op 枚举中包含动态工具变体 DynamicToolResponse（含 call_id 和 result）。
7. THE Mosaic_Protocol SHALL 在 Op 枚举中包含配置与技能变体：ReloadUserConfig、ListSkills 和 ListCustomPrompts。
8. THE Mosaic_Protocol SHALL 在 Op 枚举中包含实时对话变体：RealtimeConversationStart（含 RealtimeConfig）、RealtimeConversationStop 和 RealtimeConversationSendAudio。
9. THE Mosaic_Protocol SHALL 在 Op 枚举中包含后台管理变体 CleanBackgroundTerminals。

### Requirement 3: 协议层 — EventMsg 事件枚举

**User Story:** 作为开发者，我希望有一个完整的事件枚举定义所有出站事件类型，以便前端可以接收和处理各种系统事件。

#### Acceptance Criteria

1. THE Mosaic_Protocol SHALL 定义 EventMsg 枚举，使用 serde 的 camelCase 命名和 tag = "type" 内部标签。
2. THE Mosaic_Protocol SHALL 在 EventMsg 中包含会话生命周期事件：SessionConfigured、TurnStarted、TurnComplete（含 TurnStatus）和 TurnAborted（含 reason）。
3. THE Mosaic_Protocol SHALL 在 EventMsg 中包含 Agent 消息流事件：AgentMessage、AgentMessageDelta、ReasoningDelta 和 PlanDelta。
4. THE Mosaic_Protocol SHALL 在 EventMsg 中包含结构化项目事件：ItemStarted、ItemCompleted 和 RawResponseItem。
5. THE Mosaic_Protocol SHALL 在 EventMsg 中包含命令执行事件：ExecCommandBegin（含 command）、ExecCommandEnd（含 exit_code 和 output）和 ExecCommandOutputDelta。
6. THE Mosaic_Protocol SHALL 在 EventMsg 中包含补丁应用事件：PatchApplyBegin（含 path）和 PatchApplyEnd（含 success）。
7. THE Mosaic_Protocol SHALL 在 EventMsg 中包含 MCP 工具调用事件：McpToolCallBegin（含 server 和 tool）、McpToolCallEnd（含 result）、McpStartupUpdate 和 McpStartupComplete。
8. THE Mosaic_Protocol SHALL 在 EventMsg 中包含 Token 使用统计事件：TokenUsageUpdate 和 TokenUsageSummary。
9. THE Mosaic_Protocol SHALL 在 EventMsg 中包含历史压缩事件 Compacted（含 new_length）。
10. THE Mosaic_Protocol SHALL 在 EventMsg 中包含警告与错误事件：Warning（含 message）和 Error（含 message）。

### Requirement 4: 协议层 — 附加类型定义

**User Story:** 作为开发者，我希望协议层定义所有辅助类型，以便系统各组件使用一致的数据结构。

#### Acceptance Criteria

1. THE Mosaic_Protocol SHALL 定义 SandboxPolicy 枚举，包含 ReadOnly、WorkspaceWriteOnly（含 writable_roots: Vec&lt;PathBuf&gt;）和 DangerFullAccess 三个变体。
2. THE Mosaic_Protocol SHALL 定义 AskForApproval 枚举，包含 Never、OnFailure、UnlessAllowListed 和 Always 四个变体。
3. THE Mosaic_Protocol SHALL 定义 ReviewDecision 结构体，包含 approved（bool）、always_approve（bool）和 custom_instructions（Option&lt;String&gt;）字段。
4. THE Mosaic_Protocol SHALL 定义 TurnStatus 枚举，包含 Completed、Aborted 和 Error 三个变体。
5. THE Mosaic_Protocol SHALL 定义 Effort 枚举（Low、Medium、High）、ServiceTier 枚举（Default、Flex）和 CollaborationMode 枚举（Solo、Pair）。
6. THE Mosaic_Protocol SHALL 定义 Personality 结构体，包含 name（Option&lt;String&gt;）和 style（Option&lt;String&gt;）字段。
7. THE Mosaic_Protocol SHALL 定义 ContentItem 枚举，包含 Text、Image 和 InputAudio 变体。
8. THE Mosaic_Protocol SHALL 定义 UserInput 结构体，包含 content_items 字段（Vec&lt;ContentItem&gt; 类型）。
9. THE Mosaic_Protocol SHALL 定义 ResponseInputItem 枚举，包含 Message、FunctionCall 和 FunctionOutput 变体。
10. THE Mosaic_Protocol SHALL 定义 FunctionCallOutputPayload 结构体（含 ContentOrItems），其中 ContentOrItems 为 untagged 枚举，包含 String 和 Items 变体。
11. THE Mosaic_Protocol SHALL 定义 DynamicToolSpec 结构体（含 name、description、input_schema）和 DynamicToolCallRequest 结构体（含 call_id、tool_name、arguments）。
12. THE Mosaic_Protocol SHALL 定义 RealtimeConfig 结构体，包含 model（String）和 voice（Option&lt;String&gt;）字段。

### Requirement 5: 核心引擎 — SQ/EQ 通信与 submission_loop

**User Story:** 作为开发者，我希望核心引擎管理 SQ/EQ 异步通信循环，以便系统可以异步处理用户输入并发送事件。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 暴露 Codex 结构体，包含 tx_sub 字段（async_channel::Sender&lt;Submission&gt; 类型）和 rx_event 字段（async_channel::Receiver&lt;Event&gt; 类型）。
2. THE Mosaic_Core SHALL 提供 Codex::spawn 方法，接受 Config 参数，初始化 Session 并在 Tokio 异步运行时启动 submission_loop。
3. WHEN SQ 上接收到 Op::UserTurn 的 Submission 时，THE Mosaic_Core SHALL 调用 run_turn 函数处理模型交互。
4. WHEN SQ 上接收到 Op::Interrupt 的 Submission 时，THE Mosaic_Core SHALL 取消当前轮次并在 EQ 上发送 EventMsg::TurnComplete 事件。
5. WHEN SQ 上接收到 Op::Shutdown 的 Submission 时，THE Mosaic_Core SHALL 优雅关闭 Session 并关闭所有通道。
6. WHEN SQ 上接收到 Op::ExecApproval 的 Submission 时，THE Mosaic_Core SHALL 根据 ReviewDecision 的 approved 字段决定继续或取消待执行的命令。
7. WHEN SQ 上接收到 Op::PatchApproval 的 Submission 时，THE Mosaic_Core SHALL 根据 ReviewDecision 的 approved 字段决定继续或取消待应用的补丁。
8. THE Mosaic_Core SHALL 使用 tokio::sync::Mutex 保护 Session 状态，确保异步安全访问。
9. WHEN run_turn 开始执行时，THE Mosaic_Core SHALL 在 EQ 上发送 EventMsg::TurnStarted 事件。
10. WHEN run_turn 完成执行时，THE Mosaic_Core SHALL 在 EQ 上发送 EventMsg::TurnComplete 事件。
11. FOR ALL UserTurn 提交，EQ 上的事件序列 SHALL 以 TurnStarted 事件开始，以 TurnComplete 事件结束，该轮次的所有其他事件 SHALL 出现在两者之间。

### Requirement 6: 会话与 TurnContext 管理

**User Story:** 作为开发者，我希望有会话和轮次上下文管理，以便每个对话维护其状态，每个轮次使用正确的运行时配置执行。

#### Acceptance Criteria

1. THE Session SHALL 维护 Mutex 保护的 SessionState，包含 history（对话历史）、turn_active（轮次活跃标志）、pending_approval（待审批项）和 turn_context（当前轮次上下文）。
2. THE Session SHALL 支持为每个新轮次创建 TurnContext，包含 model_info、sandbox_policy、approval_policy 和 cwd 字段。
3. WHEN 创建新的 TurnContext 时，THE Session SHALL 从活跃配置 profile 继承默认值。
4. WHEN 接收到 Op::OverrideTurnContext 时，THE Session SHALL 使用提供的覆盖值更新当前 TurnContext，仅修改指定字段，保持其他字段不变。
5. THE Session SHALL 维护有序的 ResponseInputItem 列表作为对话历史，保持条目的插入顺序。
6. WHILE 一个轮次正在进行中，THE Session SHALL 拒绝额外的 UserTurn 提交，并发送 EventMsg::Error 事件指示轮次已在活跃状态。

### Requirement 7: 工具处理器系统

**User Story:** 作为开发者，我希望有一个可扩展的工具处理器系统，以便核心引擎可以将工具调用分发到适当的处理器并返回结果。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 定义 ToolHandler async trait，包含三个方法：matches_kind（接受 ToolKind 引用返回 bool）、kind（返回 ToolKind）和 handle（接受 serde_json::Value 返回 Result&lt;serde_json::Value, CodexError&gt;）。
2. THE Mosaic_Core SHALL 维护 ToolRegistry，支持通过 register 方法注册 ToolHandler 实现，通过 dispatch 方法按 ToolKind 查询和调用处理器。
3. WHEN run_turn 期间接收到工具调用时，THE Mosaic_Core SHALL 在 EQ 上发送 EventMsg::McpToolCallBegin 事件，包含工具名称。
4. WHEN 工具调用完成时，THE Mosaic_Core SHALL 在 EQ 上发送 EventMsg::McpToolCallEnd 事件，包含调用结果。
5. IF 工具调用失败，THEN THE Mosaic_Core SHALL 在 EQ 上发送 EventMsg::Error 事件，包含描述性错误消息。
6. THE Mosaic_Core SHALL 支持在运行时注册自定义 ToolHandler 实现。
7. FOR ALL 在运行时注册的 ToolHandler 实现，通过其 ToolKind 查询注册表 SHALL 返回 matches_kind 为 true 的处理器。

### Requirement 8: 工具路由器（ToolRouter）

**User Story:** 作为开发者，我希望有一个工具路由器按优先级将工具调用路由到正确的处理器，以便内置工具、MCP 工具和动态工具可以统一调度。

#### Acceptance Criteria

1. THE ToolRouter SHALL 按以下优先级路由工具调用：首先检查内置 ToolRegistry 处理器，然后检查 MCP_Connection_Manager 的 MCP 工具，最后检查动态工具。
2. IF ToolRouter 未找到匹配的处理器，THEN THE ToolRouter SHALL 返回 ErrorCode::ToolExecutionFailed 错误。
3. THE ToolRouter SHALL 支持通过 register_dynamic_tool 方法注册 DynamicToolSpec，注册后的动态工具 SHALL 立即可用于路由。
4. THE ToolRouter SHALL 提供 list_all_tools 方法，返回所有可用工具（内置 + MCP + 动态）的 ToolInfo 列表。

### Requirement 9: 命令执行沙箱（mosaic-exec）

**User Story:** 作为开发者，我希望有一个沙箱化的命令执行环境，以便 shell 命令在配置的安全策略下运行。

#### Acceptance Criteria

1. WHEN 请求执行 shell 命令时，THE Mosaic_Exec SHALL 在 EQ 上发送 EventMsg::ExecCommandBegin 事件，包含命令参数。
2. WHEN shell 命令完成时，THE Mosaic_Exec SHALL 在 EQ 上发送 EventMsg::ExecCommandEnd 事件，包含 exit_code 和 output。
3. WHILE SandboxPolicy 为 ReadOnly 时，THE Mosaic_Exec SHALL 限制执行环境中的所有文件写操作。
4. WHILE SandboxPolicy 为 WorkspaceWriteOnly 时，THE Mosaic_Exec SHALL 仅允许在配置的 writable_roots 目录列表内进行文件写操作。
5. WHILE SandboxPolicy 为 DangerFullAccess 时，THE Mosaic_Exec SHALL 允许所有文件系统操作。
6. IF 命令执行违反活跃的 SandboxPolicy，THEN THE Mosaic_Exec SHALL 终止进程并在 EQ 上发送 EventMsg::Error 事件。
7. WHILE AskForApproval 为 Always 时，THE Mosaic_Exec SHALL 在执行每个命令前暂停并发送审批请求事件。
8. WHILE AskForApproval 为 Never 时，THE Mosaic_Exec SHALL 直接执行命令，不请求审批。
9. WHILE AskForApproval 为 OnFailure 时，THE Mosaic_Exec SHALL 仅在命令返回非零退出码时请求审批。
10. WHILE AskForApproval 为 UnlessAllowListed 时，THE Mosaic_Exec SHALL 直接执行允许列表中的命令，对其他命令请求审批。
11. THE Mosaic_Exec SHALL 返回 ExecResult 结构体，包含 exit_code（i32）、stdout（String）和 stderr（String）字段。

### Requirement 10: 执行策略引擎（mosaic-execpolicy）

**User Story:** 作为开发者，我希望有一个基于规则的执行策略引擎，以便对命令执行和网络访问实施细粒度权限控制。

#### Acceptance Criteria

1. THE Mosaic_ExecPolicy SHALL 定义 PolicyDecision 枚举，包含 Allow、Prompt 和 Forbidden（含 reason: String）三个变体。
2. THE Mosaic_ExecPolicy SHALL 定义 PrefixPattern 结构体（含 segments: Vec&lt;String&gt; 和 is_wildcard: bool）和 PrefixRule 结构体（含 pattern: PrefixPattern 和 decision: PolicyDecision）。
3. THE Mosaic_ExecPolicy SHALL 定义 NetworkRule 结构体，包含 domain_pattern（String）和 decision（PolicyDecision）字段。
4. THE Mosaic_ExecPolicy SHALL 提供 PolicyParser，支持从 .codexpolicy 格式文件解析命令规则（parse 方法）和网络规则（parse_network_rules 方法）。
5. THE Mosaic_ExecPolicy SHALL 提供 .codexpolicy 格式的 Pretty_Printer，将 PrefixRule 和 NetworkRule 列表格式化回有效的 .codexpolicy 文件内容。
6. FOR ALL 有效的 PrefixRule 和 NetworkRule 列表，解析后打印再解析 SHALL 产生等价的规则列表（round-trip 属性）。
7. WHEN 命令匹配 Forbidden 规则时，THE Mosaic_ExecPolicy SHALL 阻止执行并返回拒绝原因。
8. WHEN 命令匹配 Prompt 规则时，THE Mosaic_ExecPolicy SHALL 发出信号表示需要用户审批。
9. WHEN 没有规则匹配命令时，THE Mosaic_ExecPolicy SHALL 应用活跃配置中的默认策略。
10. THE Mosaic_ExecPolicy SHALL 提供 evaluate_network 方法，根据 NetworkRule 列表评估域名访问决策。
11. THE Mosaic_ExecPolicy SHALL 支持通过 load_from_file 方法从文件路径加载策略引擎配置。

### Requirement 11: 分层配置系统（mosaic-config）

**User Story:** 作为开发者，我希望有一个支持 TOML 格式的分层配置系统，以便配置可以在多个层级间继承和覆盖。

#### Acceptance Criteria

1. THE Mosaic_Config SHALL 定义 ConfigToml 结构体，使用 serde Deserialize 和 kebab-case 字段命名，包含 model、approval_policy、sandbox_policy、mcp_servers 和 profiles 字段。
2. THE Mosaic_Config SHALL 支持五个配置层级，优先级从高到低为：MDM、System、User、Project、Session。
3. WHEN 多个配置层级定义相同字段时，THE Mosaic_Config SHALL 使用最高优先级层级的值。
4. THE Mosaic_Config SHALL 提供 Config_Layer_Stack，通过 merge 方法将所有活跃层级合并为单一解析后的配置。
5. THE Mosaic_Config SHALL 支持基于 profile 的配置，命名 profile 可以覆盖基础配置值。
6. THE Mosaic_Config SHALL 提供 ConfigEdit 构建器模式，用于构造原子配置修改。
7. WHEN 配置文件包含无效 TOML 语法时，THE Mosaic_Config SHALL 返回描述性解析错误，不得静默成功或 panic。
8. THE Mosaic_Config SHALL 支持将 ConfigToml 对象序列化回有效的 TOML 格式。
9. FOR ALL 有效的 ConfigToml 对象，序列化为 TOML 后再反序列化回来 SHALL 产生等价对象（round-trip 属性）。

### Requirement 12: MCP 服务器传输配置

**User Story:** 作为开发者，我希望 MCP 服务器配置支持多种传输协议和工具过滤，以便灵活管理外部 MCP 服务器连接。

#### Acceptance Criteria

1. THE Mosaic_Config SHALL 定义 McpServerTransportConfig 枚举，包含 Stdio（含 command、args、env）、Http（含 url、headers）和 OAuth（含 url、client_id、client_secret、token_url）三个变体。
2. THE Mosaic_Config SHALL 定义 McpServerConfig 结构体，包含 transport（McpServerTransportConfig）、disabled（bool）、disabled_reason（Option&lt;String&gt;）和 tool_filter（Option&lt;McpToolFilter&gt;）字段。
3. THE Mosaic_Config SHALL 定义 McpToolFilter 结构体，包含 enabled（Option&lt;Vec&lt;String&gt;&gt;）和 disabled（Option&lt;Vec&lt;String&gt;&gt;）字段。
4. WHEN McpServerConfig 的 disabled 字段为 true 时，THE MCP_Connection_Manager SHALL 不尝试连接该服务器，并保留 disabled_reason 用于状态查询。
5. WHEN McpServerConfig 的 disabled 字段从 true 变为 false 时，THE MCP_Connection_Manager SHALL 允许重新连接。

### Requirement 13: 状态存储（mosaic-state）

**User Story:** 作为开发者，我希望有持久化状态存储，以便会话数据、事件历史和记忆条目在应用重启后保留。

#### Acceptance Criteria

1. THE Mosaic_State SHALL 提供 StateDb 结构体，基于 SQLite 实现持久化存储，包含 StateRuntime（含 StateConfig 和 StateMetrics）和 LogDb 低级存储引擎。
2. THE Mosaic_State SHALL 支持 18 个数据库迁移脚本，通过 run_migrations 方法执行。
3. THE Mosaic_State SHALL 支持存储和检索 Rollout 记录，包含有序的 Event 序列。
4. THE Mosaic_State SHALL 支持存储和检索 SessionMeta 记录，包含 id、created_at、last_activity 和 config_profile 字段。
5. THE Mosaic_State SHALL 支持存储和检索 ThreadMetadata 记录，包含 thread_id、created_at、title 和 model 字段。
6. THE Mosaic_State SHALL 支持存储和检索 AgentJob 记录，包含 job_id、thread_id、status 和 items（Vec&lt;AgentJobItem&gt;）字段。
7. THE Mosaic_State SHALL 定义 AgentJobStatus 枚举，包含 Pending、Running、Completed 和 Failed 四个变体。
8. THE Mosaic_State SHALL 支持 BackfillState 记录，包含 last_processed_id 和 total_processed 字段。
9. THE Mosaic_State SHALL 支持 Memory 系统，包含 Phase1（短期记忆）和 Phase2（长期记忆）两个阶段。
10. WHEN 应用启动时，THE Mosaic_State SHALL 初始化 SQLite 数据库并创建所需表（如果不存在）。
11. IF 数据库操作失败，THEN THE Mosaic_State SHALL 返回描述性错误，不得破坏已有数据（使用事务回滚）。
12. FOR ALL 有效的 Rollout 记录，存储到 StateDb 后通过 session_id 检索 SHALL 产生等价的 Rollout，事件顺序保持一致。
13. FOR ALL 有效的 SessionMeta 记录，存储到 StateDb 后通过 id 检索 SHALL 产生等价的 SessionMeta。
14. FOR ALL 有效的 Memory 条目，存储到 StateDb 后检索 SHALL 保留 phase、content、timestamp 和 relevance_score。

### Requirement 14: MCP 客户端集成

**User Story:** 作为开发者，我希望系统可以连接外部 MCP 服务器、发现工具并调用它们，以便扩展系统的工具能力。

#### Acceptance Criteria

1. THE MCP_Connection_Manager SHALL 支持通过 Stdio、HTTP 和 OAuth 三种传输协议连接 MCP 服务器。
2. THE MCP_Connection_Manager SHALL 维护活跃 MCP 服务器连接池，提供连接生命周期管理。
3. WHEN 新的 MCP 服务器连接建立时，THE MCP_Connection_Manager SHALL 自动调用 tools/list 方法执行工具发现。
4. THE MCP_Connection_Manager SHALL 使用 `mcp__{server}__{tool}` 格式限定发现的工具名称，长度限制为 64 字符。
5. WHEN 限定工具名称超过 64 字符时，THE MCP_Connection_Manager SHALL 应用 SHA1 哈希进行去重。
6. WHEN 请求 MCP 工具调用时，THE MCP_Connection_Manager SHALL 使用 JSON-RPC 协议将调用路由到正确的服务器。
7. IF MCP 服务器连接失败，THEN THE MCP_Connection_Manager SHALL 在 EQ 上发送 EventMsg::Error 事件，并将连接标记为不可用。
8. THE MCP_Connection_Manager SHALL 支持通过 McpToolFilter 的 enabled 和 disabled 列表进行工具过滤。
9. WHEN 配置了 OAuth 传输的 MCP 服务器时，THE MCP_Connection_Manager SHALL 先从配置的 token_url 获取 bearer token，再建立 MCP 会话。
10. IF OAuth token 获取失败，THEN THE MCP_Connection_Manager SHALL 以 ErrorCode::McpServerUnavailable 错误使连接失败。

### Requirement 15: MCP 服务器暴露

**User Story:** 作为开发者，我希望系统将自身工具作为 MCP 服务器暴露，以便外部 MCP 客户端可以发现和调用 Mosaic 工具。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 暴露 JSON-RPC MCP 服务器接口，支持 initialize、tools/list 和 tools/call 方法。
2. WHEN 接收到 tools/list 请求时，THE Mosaic_Core SHALL 返回所有已注册 ToolHandler 的名称和输入 schema。
3. WHEN 接收到 tools/call 请求时，THE Mosaic_Core SHALL 将调用分发到匹配的 ToolHandler 并返回结果。
4. IF tools/call 请求引用未知工具，THEN THE Mosaic_Core SHALL 返回 JSON-RPC 错误，code 为 -32602，包含描述性消息。

### Requirement 16: 技能系统（Skills System）

**User Story:** 作为开发者，我希望有一个技能系统从多个目录发现和加载技能定义，以便系统可以通过可复用的技能模块扩展。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 通过搜索多个根目录发现 Skill 定义，优先级顺序为：Repo > User > System > Admin。
2. THE Mosaic_Core SHALL 解析 SKILL.md 文件，提取 YAML frontmatter 中的 name、short_description、description、version、triggers、interface、dependencies、policy、permission_profile 字段。
3. WHEN 多个 Skill 定义共享相同名称时，THE Mosaic_Core SHALL 使用最高优先级根目录的定义。
4. THE Mosaic_Core SHALL 使用广度优先遍历搜索 Skill 定义，最大深度为 6 层。
5. THE Mosaic_Core SHALL 提供技能列表接口，返回所有已发现的 SkillMetadata 条目。
6. THE Mosaic_Core SHALL 返回 SkillLoadOutcome，包含成功加载的 skills、加载错误的 errors、被禁用的 disabled_paths 和隐式技能的 implicit_skills。
7. THE Mosaic_Core SHALL 定义 SkillScope 枚举，包含 Repo、User、System 和 Admin 四个变体。

### Requirement 17: 多 Agent 系统

**User Story:** 作为开发者，我希望有一个多 Agent 协作系统，以便可以生成、协调和管理多个 AI Agent 执行复杂任务。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 通过 AgentControl 支持五种 Agent 操作：spawn_agent、send_input、resume_agent、wait 和 close_agent。
2. WHEN 调用 spawn_agent 时，THE Mosaic_Core SHALL 创建新的 AgentInstance，具有独立的执行上下文，支持 SpawnAgentOptions 中的 model、sandbox_policy、cwd、fork 和 max_depth 配置。
3. WHEN 调用 close_agent 时，THE Mosaic_Core SHALL 优雅终止 Agent 并释放其资源。
4. THE Mosaic_Core SHALL 通过 max_recursion_depth 配置强制执行 Agent 生成的最大递归深度。
5. IF Agent 生成超过最大深度，THEN THE Mosaic_Core SHALL 拒绝生成请求并返回深度限制错误。
6. THE Mosaic_Core SHALL 使用 ThreadManagerState 管理 Agent 线程，使用弱引用（Weak&lt;AgentInstance&gt;）避免循环引用。
7. THE Mosaic_Core SHALL 通过 Guards 结构体管理 spawn slot 预留和 nickname 分配。
8. THE Mosaic_Core SHALL 支持 fork 模式，允许 Agent 在独立的执行分支中运行。

### Requirement 18: 批量任务系统

**User Story:** 作为开发者，我希望有一个批量任务系统，以便可以通过 CSV 输入驱动并发执行多个任务。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 支持通过 BatchJobConfig 配置批量任务，包含 csv_path（PathBuf）和 concurrency（usize）字段。
2. WHEN 执行批量任务时，THE Mosaic_Core SHALL 同时执行的任务数不超过配置的 concurrency 限制。
3. THE Mosaic_Core SHALL 为每个输入行返回一个 BatchResult，包含 row_index、success 和 output 字段。
4. FOR ALL BatchJobConfig 配置，返回的 Vec&lt;BatchResult&gt; SHALL 包含与输入行数完全相同数量的结果。

### Requirement 19: 钩子系统（Hooks System）

**User Story:** 作为开发者，我希望有一个事件驱动的钩子系统，以便可以注册自定义回调在特定系统事件后执行。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 定义 HookEvent 枚举，仅包含 AfterAgent（含 agent_id 和 result）和 AfterToolUse（含 tool_name 和 result）两种后置事件类型。
2. THE Mosaic_Core SHALL 定义 HookResult 枚举，包含 Success、FailedContinue（含 error）和 FailedAbort（含 error）三种状态。
3. THE Mosaic_Core SHALL 通过 HookRegistry 支持注册 HookDefinition，每个定义包含 name、event 和 handler。
4. WHEN 可钩子事件发生后，THE Mosaic_Core SHALL 执行所有匹配事件类型的已注册钩子，并收集 HookResult 列表。
5. IF 任何钩子返回 FailedAbort，THEN THE Mosaic_Core SHALL 停止后续处理并在 EQ 上发送 EventMsg::Error 事件。
6. WHEN 钩子返回 FailedContinue 时，THE Mosaic_Core SHALL 记录错误但继续正常处理。
7. THE Mosaic_Core SHALL 不得静默忽略 FailedAbort 结果。

### Requirement 20: 补丁应用（Patch Application）

**User Story:** 作为开发者，我希望有一个补丁应用系统，以便 AI 模型建议的文件变更可以应用到工作区。

#### Acceptance Criteria

1. WHEN 请求补丁应用时，THE Mosaic_Core SHALL 在 EQ 上发送 EventMsg::PatchApplyBegin 事件，包含目标文件路径。
2. WHEN 补丁成功应用时，THE Mosaic_Core SHALL 在 EQ 上发送 EventMsg::PatchApplyEnd 事件，success 设为 true。
3. IF 补丁应用失败，THEN THE Mosaic_Core SHALL 在 EQ 上发送 EventMsg::PatchApplyEnd 事件（success 为 false）和 EventMsg::Error 事件（包含失败原因）。
4. WHILE AskForApproval 为 Always 或 UnlessAllowListed 时，THE Mosaic_Core SHALL 在应用每个补丁前请求用户审批。

### Requirement 21: 网络代理（mosaic-netproxy）

**User Story:** 作为开发者，我希望有一个网络代理模块提供域名过滤和安全隔离，以便控制沙箱环境中的网络访问。

#### Acceptance Criteria

1. THE Mosaic_NetProxy SHALL 提供 NetworkProxy 结构体，包含 HTTP 代理（HttpProxyServer）、SOCKS5 代理（Socks5ProxyServer）、证书管理器（CertificateManager）和策略决策器（NetworkPolicyDecider）。
2. THE Mosaic_NetProxy SHALL 提供 NetworkProxyConfig 结构体，包含 listen_addr、socks5_addr、allowed_domains、blocked_domains 和 mitm_enabled 字段。
3. THE Mosaic_NetProxy SHALL 通过 NetworkPolicyDecider 评估域名和端口的访问决策，返回 PolicyDecision。
4. WHEN 域名匹配 allow 规则且不匹配 deny 规则时，THE NetworkPolicyDecider SHALL 返回 Allow。
5. WHEN 域名匹配 deny 规则时，THE NetworkPolicyDecider SHALL 返回 Forbidden，deny 规则优先于 allow 规则。
6. THE Mosaic_NetProxy SHALL 支持通过 start 方法启动代理服务器，通过 stop 方法停止。
7. THE Mosaic_NetProxy SHALL 支持通过 reload_config 方法在运行时重新加载配置，使用 tokio::sync::watch 通道。
8. THE Mosaic_NetProxy SHALL 通过 CertificateManager 管理 CA 证书和密钥，支持 MITM 代理功能。

### Requirement 22: Shell 命令解析（mosaic-shell-command）

**User Story:** 作为开发者，我希望有一个 Shell 命令解析模块，以便将命令字符串安全地解析为 token 列表。

#### Acceptance Criteria

1. THE Mosaic_ShellCommand SHALL 提供 parse_command 函数，接受命令字符串输入，返回 Result&lt;Vec&lt;String&gt;, CodexError&gt;。
2. WHEN 输入有效的 shell 命令字符串时，THE Mosaic_ShellCommand SHALL 返回非空的 Vec&lt;String&gt;，其元素为解析后的命令 token。
3. IF 输入无效的命令字符串，THEN THE Mosaic_ShellCommand SHALL 返回描述性的 CodexError。
4. FOR ALL 由非空 token 组成的有效 shell 命令字符串，parse_command 产生的 Vec&lt;String&gt; 元素用空格连接后 SHALL 重构出等价的命令字符串（round-trip 属性）。

### Requirement 23: 敏感信息管理（mosaic-secrets）

**User Story:** 作为开发者，我希望有一个敏感信息管理模块，以便安全存储密钥、检测敏感信息并对输出内容脱敏。

#### Acceptance Criteria

1. THE Mosaic_Secrets SHALL 定义 SecretName newtype（带格式验证）和 SecretScope 枚举（Global、Environment(String)）。
2. THE Mosaic_Secrets SHALL 定义 SecretsBackend async trait，包含 get、set、delete 和 list 四个方法。
3. THE Mosaic_Secrets SHALL 提供 SecretsManager 结构体，支持通过 new_with_keyring 方法创建 keyring 集成的实例。
4. THE Mosaic_Secrets SHALL 通过 SecretsManager 提供 get_secret、set_secret、delete_secret 和 list_secrets 方法。
5. FOR ALL 有效的 SecretName 和 SecretScope，调用 set_secret 后再调用 get_secret（相同 name 和 scope）SHALL 返回存储的值。
6. FOR ALL 有效的 SecretName 和 SecretScope，调用 delete_secret 后再调用 get_secret SHALL 返回 None。
7. FOR ALL 给定 scope 下已设置的密钥，list_secrets SHALL 包含所有已设置的密钥名称。
8. THE Mosaic_Secrets SHALL 提供 scan_for_secrets 函数，接受内容字符串，返回 Vec&lt;SecretMatch&gt;，每个匹配包含 kind、range 和 redacted 字段。
9. FOR ALL 包含已知敏感模式（API 密钥、bearer token、私钥）的字符串，scan_for_secrets SHALL 为每个嵌入的模式返回至少一个 SecretMatch，且 redacted 字段不包含原始密钥值。
10. THE Mosaic_Secrets SHALL 提供 redact_secrets 函数，接受内容字符串和已知密钥列表，将所有已知密钥值替换为脱敏占位符。
11. FOR ALL 调用 redact_secrets 的输出，SHALL 不包含任何原始密钥值。

### Requirement 24: 对话历史截断与压缩

**User Story:** 作为开发者，我希望有对话历史截断和压缩机制，以便管理长对话的内存和 token 使用。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 定义 TruncationPolicy 枚举，包含 KeepRecent（含 max_items: usize）、KeepRecentTokens（含 max_tokens: usize）和 AutoCompact 三个变体。
2. WHEN TruncationPolicy 为 KeepRecent 时，THE Mosaic_Core SHALL 仅保留最近 N 条消息。
3. WHEN TruncationPolicy 为 KeepRecentTokens 时，THE Mosaic_Core SHALL 从末尾保留累计 token 数不超过 N 的消息。
4. WHEN TruncationPolicy 为 AutoCompact 时，THE Mosaic_Core SHALL 使用 SUMMARIZATION_PROMPT 模板调用模型生成摘要，替换较旧的消息。
5. THE Mosaic_Core SHALL 提供 compact 函数（本地压缩）和 compact_remote 函数（远程 API 压缩）。
6. WHEN 历史实际被缩短时，THE Mosaic_Core SHALL 在 EQ 上发送 EventMsg::Compacted 事件，包含 new_length。
7. FOR ALL 已压缩的历史（长度 ≤ 策略阈值），再次调用 compact SHALL 返回历史不变（幂等性），且不发送 Compacted 事件。

### Requirement 25: 对话历史回滚

**User Story:** 作为开发者，我希望有对话历史回滚机制，以便可以撤销最近的对话轮次。

#### Acceptance Criteria

1. THE Session SHALL 提供 rollback 方法，接受 steps 参数（usize 类型）。
2. FOR ALL 历史长度为 L 且回滚步数为 n（0 < n ≤ L）的会话，调用 rollback(n) 后历史长度 SHALL 为 L − n。
3. FOR ALL 回滚操作，剩余条目 SHALL 恰好是原始历史的前 L − n 条，顺序保持一致。

### Requirement 26: 实时对话管理器

**User Story:** 作为开发者，我希望有一个实时对话管理器，以便支持语音交互的实时对话会话。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 提供 RealtimeConversationManager，管理 RealtimeSession 的生命周期。
2. WHEN 接收到 Op::RealtimeConversationStart 时，THE RealtimeConversationManager SHALL 创建 RealtimeSession（含 session_id、model、voice、started_at），且 is_active() 返回 true。
3. WHEN 接收到 Op::RealtimeConversationStop 时，THE RealtimeConversationManager SHALL 终止会话，且 is_active() 返回 false。
4. WHEN 接收到 Op::RealtimeConversationSendAudio 时，THE RealtimeConversationManager SHALL 将音频数据发送到活跃会话。
5. IF 没有活跃会话时发送音频，THEN THE RealtimeConversationManager SHALL 返回错误。

### Requirement 27: 动态工具生命周期

**User Story:** 作为开发者，我希望有动态工具注册和调用机制，以便运行时可以扩展系统的工具能力。

#### Acceptance Criteria

1. THE ToolRouter SHALL 支持通过 register_dynamic_tool 方法注册 DynamicToolSpec（含 name、description、input_schema）。
2. WHEN 调用动态工具时，THE Mosaic_Core SHALL 发送 DynamicToolCallRequest 事件（含 call_id、tool_name、arguments），并等待对应的 Op::DynamicToolResponse（匹配 call_id）。
3. WHEN 接收到匹配 call_id 的 DynamicToolResponse 时，THE Mosaic_Core SHALL 将 response 的 result 作为工具调用结果返回。
4. THE ToolRouter SHALL 在注册动态工具后立即使其可用于路由。

### Requirement 28: 审批决策语义（ReviewDecision）

**User Story:** 作为开发者，我希望审批决策支持丰富的语义，以便用户可以批准、拒绝、永久批准命令模式或附加自定义指令。

#### Acceptance Criteria

1. WHEN ReviewDecision 的 approved 为 false 时，THE Mosaic_Core SHALL 取消待执行操作，不论其他字段的值。
2. WHEN ReviewDecision 的 approved 为 true 且 always_approve 为 true 时，THE Mosaic_Core SHALL 将命令模式添加到允许列表，后续执行无需审批。
3. WHEN ReviewDecision 包含 custom_instructions 时，THE Mosaic_Core SHALL 将自定义指令转发给 Agent 用于下一轮次。

### Requirement 29: 错误处理系统

**User Story:** 作为开发者，我希望有一个标准化的错误处理系统，以便所有错误结构一致并提供可操作的信息。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 定义 CodexError 结构体，包含 code 字段（ErrorCode 枚举）、message 字段（String）和可选的 details 字段（serde_json::Value）。
2. THE Mosaic_Core SHALL 定义 ErrorCode 枚举，包含 InvalidInput、ToolExecutionFailed、McpServerUnavailable、ConfigurationError、SandboxViolation、ApprovalDenied、SessionError 和 InternalError 变体。
3. WHEN 任何 Mosaic 模块发生错误时，THE 模块 SHALL 构造 CodexError，使用适当的 ErrorCode 和描述性消息。
4. THE Mosaic_Core SHALL 使用 camelCase 字段命名将 CodexError 序列化为 JSON。
5. FOR ALL 有效的 CodexError 对象，序列化为 JSON 后再反序列化回来 SHALL 产生等价对象（round-trip 属性）。
6. THE Mosaic_Core SHALL 确保所有公共函数返回 Result&lt;T, CodexError&gt;。
7. THE Mosaic_Core SHALL 禁止使用 unwrap() 或 expect() 处理可恢复错误。
8. THE Mosaic_Core SHALL 确保错误消息不泄露敏感信息（文件路径、内部状态细节）。

### Requirement 30: Tauri 命令接口

**User Story:** 作为开发者，我希望有 Tauri 命令绑定，以便 React 前端可以通过类型安全的 IPC 与 Mosaic 后端交互。

#### Acceptance Criteria

1. THE Mosaic_Core SHALL 暴露 Tauri 命令用于向 SQ 提交操作，包括 user_turn、interrupt、exec_approval、patch_approval 和 shutdown。
2. THE Mosaic_Core SHALL 暴露 poll_event Tauri 命令，用于从 EQ 接收序列化为 JSON 的事件。
3. THE Mosaic_Core SHALL 暴露 get_config Tauri 命令，用于读取当前解析后的配置。
4. THE Mosaic_Core SHALL 暴露 list_skills Tauri 命令，用于列出已发现的技能。
5. WHEN Tauri 命令接收到无效输入参数时，THE Mosaic_Core SHALL 返回序列化的 CodexError，ErrorCode 为 InvalidInput。
