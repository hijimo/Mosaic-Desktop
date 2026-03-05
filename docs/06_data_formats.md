# Codex 数据格式文档

## 1. 协议数据格式

### 1.1 核心消息结构

#### Submission (提交)

```rust
struct Submission {
    id: String,
    op: Op,
}
```

#### Event (事件)

```rust
struct Event {
    id: String,
    msg: EventMsg,
}
```

### 1.2 Op 枚举类型

```rust
enum Op {
    UserTurn {
        input: UserInput,
        cwd: Option<PathBuf>,
        policies: Option<Policies>,
        model: Option<String>,
        effort: Option<Effort>,
        // ...其他字段
    },
    Interrupt,
    ExecApproval { decision: ReviewDecision },
    PatchApproval { decision: ReviewDecision },
    ResolveElicitation { response: serde_json::Value },
    OverrideTurnContext { overrides: TurnContextOverrides },
    AddToHistory { items: Vec<ResponseInputItem> },
    ListMcpTools,
    RefreshMcpServers,
    DynamicToolResponse { call_id: String, result: serde_json::Value },
    Shutdown,
    // ...其他操作类型
}

// 审批决策（替代简单的 bool approved）
struct ReviewDecision {
    approved: bool,
    always_approve: bool,
    custom_instructions: Option<String>,
}
```

### 1.3 EventMsg 枚举类型

```rust
enum EventMsg {
    TurnStarted,
    AgentMessage { content: String },
    AgentMessageDelta { delta: String },
    ExecCommandBegin { command: Vec<String> },
    ExecCommandEnd { exit_code: i32, output: String },
    PatchApplyBegin { path: PathBuf },
    PatchApplyEnd { success: bool },
    McpToolCallBegin { server: String, tool: String },
    McpToolCallEnd { result: serde_json::Value },
    TurnComplete,
    Error { message: String },
}
```

## 2. 用户输入格式

### 2.1 UserInput 结构

```rust
struct UserInput {
    content_items: Vec<ContentItem>,
}
```

### 2.2 ContentItem 类型

```rust
enum ContentItem {
    Text { text: String },
    Image { 
        source: ImageSource,
        detail: Option<String>,
    },
    InputAudio { 
        format: String,
        data: Vec<u8>,
    },
}
```

### 2.3 ResponseInputItem 类型

```rust
enum ResponseInputItem {
    Message { content: String },
    FunctionCall { 
        name: String,
        arguments: serde_json::Value,
    },
    FunctionOutput { 
        call_id: String,
        output: String,
    },
}
```

## 3. 工具调用格式

### 3.1 函数调用输出

```rust
struct FunctionCallOutputPayload {
    content: ContentOrItems,
}

enum ContentOrItems {
    String(String),
    Items(Vec<ContentItem>),
}
```

### 3.2 Shell 工具参数

```rust
struct ShellToolCallParams {
    command: Vec<String>,
    workdir: Option<String>,
}
```

### 3.3 MCP 工具调用

- 命名规范: `mcp__{server}__{tool}`
- 参数格式: JSON 对象
- 示例: `mcp__filesystem__read_file`

## 4. 配置文件格式

### 4.1 config.toml 结构

```toml
model = "gpt-4"
approval-policy = "unless-allow-listed"
sandbox-policy = "workspace-write-only"

[mcp-servers.filesystem]
type = "stdio"
command = "node"
args = ["server.js"]

[mcp-servers.web]
type = "http"
url = "http://localhost:3000"

[profile.dev]
model = "gpt-3.5-turbo"
approval-policy = "always"
```

### 4.2 MCP 服务器配置

```rust
// 传输配置（支持三种传输方式）
enum McpServerTransportConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    Http {
        url: String,
        headers: HashMap<String, String>,
    },
    OAuth {
        url: String,
        client_id: String,
        client_secret: String,
        token_url: String,
    },
}

// 完整服务器配置（含 disabled 原因追踪和工具过滤）
struct McpServerConfig {
    transport: McpServerTransportConfig,
    disabled: bool,
    disabled_reason: Option<String>,
    tool_filter: Option<McpToolFilter>,
}

struct McpToolFilter {
    enabled: Option<Vec<String>>,
    disabled: Option<Vec<String>>,
}
```

## 5. Skill 文件格式

### 5.1 SKILL.md 结构

```markdown
---
name: "技能名称"
description: "技能描述"
version: "1.0.0"
triggers:
  - "关键词1"
  - "关键词2"
---

# 技能内容

Markdown 格式的技能文档...
```

### 5.2 agents/openai.yaml

```yaml
interface: openai
dependencies:
  - mcp_server: filesystem
policy:
  approval_required: false
permissions:
  - read_files
  - execute_commands
```

## 6. 状态存储格式

### 6.1 Rollout (事件序列)

```rust
struct Rollout {
    events: Vec<Event>,
    metadata: RolloutMetadata,
}

struct RolloutMetadata {
    created_at: DateTime<Utc>,
    session_id: String,
    version: String,
}
```

### 6.2 SessionMeta (会话元数据)

```rust
struct SessionMeta {
    id: String,
    created_at: DateTime<Utc>,
    last_activity: DateTime<Utc>,
    user_id: Option<String>,
    config_profile: Option<String>,
}
```

### 6.3 Memories (记忆处理)

```rust
struct Memory {
    phase: MemoryPhase,
    content: String,
    timestamp: DateTime<Utc>,
    relevance_score: f64,
}

enum MemoryPhase {
    Phase1, // 短期记忆
    Phase2, // 长期记忆
}
```

## 7. API 格式 (V2 JSON-RPC)

### 7.1 线程操作

```json
// ThreadStartParams
{
  "method": "thread/start",
  "params": {
    "configProfile": "dev",
    "initialMessage": "Hello"
  }
}

// ThreadStartResponse
{
  "result": {
    "threadId": "thread-123",
    "status": "active"
  }
}
```

### 7.2 回合操作

```json
// TurnStartParams
{
  "method": "turn/start", 
  "params": {
    "threadId": "thread-123",
    "userInput": {
      "contentItems": [
        {
          "type": "text",
          "text": "用户消息"
        }
      ]
    }
  }
}
```

### 7.3 ThreadItem 枚举

```rust
enum ThreadItem {
    UserMessage { 
        content: Vec<ContentItem>,
        timestamp: DateTime<Utc>,
    },
    AgentMessage {
        content: String,
        timestamp: DateTime<Utc>,
    },
    ToolCall {
        name: String,
        arguments: serde_json::Value,
        result: Option<serde_json::Value>,
    },
}
```

### 7.4 序列化规则

- 所有字段使用 camelCase 命名
- 时间戳使用 ISO 8601 格式
- 二进制数据使用 Base64 编码
- 枚举类型使用字符串标识符

## 8. 错误格式

### 8.1 标准错误结构

```rust
struct CodexError {
    code: ErrorCode,
    message: String,
    details: Option<serde_json::Value>,
}

enum ErrorCode {
    InvalidInput,
    ToolExecutionFailed,
    McpServerUnavailable,
    ConfigurationError,
    // ...其他错误类型
}
```

### 8.2 JSON-RPC 错误响应

```json
{
  "error": {
    "code": -32602,
    "message": "Invalid params",
    "data": {
      "field": "userInput",
      "reason": "Missing required field"
    }
  }
}
```
