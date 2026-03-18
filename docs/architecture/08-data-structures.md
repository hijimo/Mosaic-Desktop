# 数据结构文档

本文档列出 `protocol/` 层定义的所有核心数据类型，这些类型构成前后端通信的契约。

## 错误类型 (`protocol/error.rs`)

### ErrorCode

```rust
enum ErrorCode {
    InvalidInput,           // 无效输入
    ToolExecutionFailed,    // 工具执行失败
    McpServerUnavailable,   // MCP 服务器不可用
    ConfigurationError,     // 配置错误
    SandboxViolation,       // 沙箱违规
    ApprovalDenied,         // 审批被拒
    SessionError,           // 会话错误
    InternalError,          // 内部错误
}
```

### CodexError

```rust
struct CodexError {
    code: ErrorCode,
    message: String,
    details: Option<Value>,  // 可选的结构化详情
}
```

## 沙箱策略 (`SandboxPolicy`)

```rust
enum SandboxPolicy {
    DangerFullAccess,                    // 无限制
    ReadOnly { access: ReadOnlyAccess }, // 只读
    ExternalSandbox { network_access },  // 外部沙箱
    WorkspaceWrite {                     // 工作区写入
        writable_roots: Vec<PathBuf>,
        read_only_access: ReadOnlyAccess,
        network_access: bool,
    },
}
```

## 用户输入 (`UserInput`)

```rust
enum UserInput {
    Text { text: String, text_elements: Vec<TextElement> },
    Image { image_url: String },
    LocalImage { path: PathBuf },
    Skill { name: String, path: PathBuf },
    Mention { name: String, path: String },
}
```

## 对话历史项 (`ResponseInputItem`)

```rust
enum ResponseInputItem {
    Message { role: String, content: String },
    FunctionCall { call_id: String, name: String, arguments: String },
    FunctionOutput { call_id: String, output: FunctionCallOutputPayload },
}
```

## Agent 状态 (`AgentStatus`)

```rust
enum AgentStatus {
    PendingInit,              // 等待初始化
    Running,                  // 运行中
    Completed(Option<String>),// 已完成
    Errored(String),          // 出错
    Shutdown,                 // 已关闭
    NotFound,                 // 未找到
}
```

## 审批策略 (`AskForApproval`)

```rust
enum AskForApproval {
    UnlessTrusted,           // 仅信任命令自动放行
    OnFailure,               // (已废弃) 失败时审批
    OnRequest,               // 模型决定何时请求审批 (默认)
    Reject(RejectConfig),    // 细粒度拒绝控制
    Never,                   // 从不审批
}
```

## 审批决策 (`ReviewDecision`)

```rust
enum ReviewDecision {
    Approved,
    ApprovedExecpolicyAmendment { proposed_execpolicy_amendment },
    ApprovedForSession,
    NetworkPolicyAmendment { network_policy_amendment },
    Denied,
    Abort,
}
```

## 协作模式 (`CollaborationMode`)

```rust
struct CollaborationMode {
    mode: ModeKind,                    // Plan | Default
    settings: CollaborationModeSettings {
        model: String,
        reasoning_effort: Option<Effort>,  // Low | Medium | High
        developer_instructions: Option<String>,
    },
}
```

## Token 使用统计 (`TokenUsage`)

```rust
struct TokenUsage {
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
}

struct TokenUsageInfo {
    total_token_usage: TokenUsage,
    last_token_usage: TokenUsage,
    model_context_window: Option<i64>,
}
```

## 动态工具 (`DynamicToolSpec`)

```rust
struct DynamicToolSpec {
    name: String,
    description: String,
    input_schema: Value,
}

struct DynamicToolCallRequest {
    call_id: String,
    turn_id: String,
    tool: String,
    arguments: Value,
}
```

## 文件变更 (`FileChange`)

```rust
enum FileChange {
    Add { content: String },
    Delete { content: String },
    Update { unified_diff: String, move_path: Option<PathBuf> },
}
```

## MCP 类型

```rust
struct McpInvocation { server: String, tool: String, arguments: Option<Value> }
struct CallToolResult { content: Option<Value>, is_error: Option<bool> }
enum McpStartupStatus { Starting, Ready, Failed { error }, Cancelled }
```

## 其他枚举类型

| 类型 | 变体 | 说明 |
|------|------|------|
| `Effort` | Low, Medium, High | 推理努力程度 |
| `ReasoningSummary` | Auto, Concise, Detailed, None | 推理摘要模式 |
| `Verbosity` | Low, Medium, High | 输出详细程度 |
| `WebSearchMode` | Disabled, Cached, Live | 网络搜索模式 |
| `SandboxMode` | ReadOnly, WorkspaceWrite, DangerFullAccess | 沙箱模式 (TOML 配置用) |
| `ModeKind` | Plan, Default | 协作模式类型 |
| `Personality` | None, Friendly, Pragmatic | Agent 人格 |
| `TrustLevel` | Trusted, Untrusted | 项目信任级别 |
| `TurnAbortReason` | Interrupted, Replaced, ReviewEnded | Turn 中止原因 |
| `ExecCommandSource` | Agent, UserShell | 命令来源 |
| `ExecCommandStatus` | Completed, Failed, Declined | 命令执行状态 |
| `ElicitationAction` | Accept, Decline, Cancel | MCP 请求用户输入的决策 |
| `NetworkAccess` | Restricted, Enabled | 网络访问权限 |
| `MessagePhase` | Planning, Executing, Summarizing | 消息阶段 |
