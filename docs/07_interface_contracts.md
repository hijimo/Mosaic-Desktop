# Codex 接口契约文档

## 1. Core <-> Consumer 接口 (SQ/EQ)

### 1.1 通信机制

- **入站通道**: `async_channel::Sender<Submission>`
- **出站通道**: `async_channel::Receiver<Event>`
- **异步模式**: 非阻塞消息传递

### 1.2 入站消息格式

```rust
struct Submission {
    id: SubmissionId,
    op: Op,
}
```

### 1.3 关键操作类型 (Op)

- `UserTurn`: 用户回合开始
- `Interrupt`: 中断当前操作
- `ExecApproval`: 执行命令审批
- `PatchApproval`: 文件补丁审批
- `OverrideTurnContext`: 覆盖回合上下文
- `Shutdown`: 系统关闭

### 1.4 出站消息格式

```rust
struct Event {
    id: EventId,
    msg: EventMsg,
}
```

### 1.5 关键事件类型 (EventMsg)

- `TurnStarted`: 回合开始
- `AgentMessage/Delta`: 代理消息/增量更新
- `ExecCommandBegin/End`: 命令执行开始/结束
- `PatchApplyBegin/End`: 补丁应用开始/结束
- `TurnComplete`: 回合完成
- `Error`: 错误事件

## 2. App Server JSON-RPC 接口 (V2)

### 2.1 客户端请求方法

- `thread/start`: 启动新线程
- `thread/resume`: 恢复线程
- `turn/start`: 开始新回合
- `turn/interrupt`: 中断回合
- `turn/steer`: 引导回合

### 2.2 服务器通知

- `turnStarted`: 回合已开始
- `agentMessageDelta`: 代理消息增量
- `itemStarted`: 项目开始
- `itemCompleted`: 项目完成
- `turnCompleted`: 回合完成

### 2.3 服务器请求

- `commandExecutionRequestApproval`: 命令执行审批请求
- `fileChangeRequestApproval`: 文件变更审批请求
- `toolRequestUserInput`: 工具用户输入请求

### 2.4 配置管理接口

- `config/read`: 读取配置
- `config/write`: 写入配置
- `model/list`: 列出模型
- `skills/list`: 列出技能

## 3. MCP 接口

### 3.1 客户端到服务器

- `initialize`: 初始化连接
- `tools/list`: 列出可用工具
- `tools/call`: 调用工具
- `resources/list`: 列出资源
- `resources/read`: 读取资源

### 3.2 服务器到客户端

- `notifications`: 沙盒状态变更通知

### 3.3 工具命名规范

```
mcp__{server}__{tool}
```

### 3.4 参数验证

- 使用 JSON Schema 验证工具参数

## 4. 工具处理器接口

### 4.1 Trait 定义

```rust
#[async_trait]
trait ToolHandler: Send + Sync {
    fn matches_kind(&self, kind: &ToolKind) -> bool;
    fn kind(&self) -> ToolKind;
    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError>;
}

struct ToolRegistry {
    handlers: Vec<Box<dyn ToolHandler>>,
}

impl ToolRegistry {
    fn register(&mut self, handler: Box<dyn ToolHandler>);
    fn dispatch(&self, kind: &ToolKind, args: serde_json::Value)
        -> Result<serde_json::Value, CodexError>;
}
```

### 4.2 调用结构

```rust
struct ToolInvocation {
    call_id: String,
    name: String,
    arguments: serde_json::Value,
}
```

### 4.3 输出结构

```rust
struct ToolOutput {
    content: String,
    is_error: bool,
}
```

## 5. 配置层接口

### 5.1 分层配置栈

```rust
struct ConfigLayerStack {
    // 分层合并配置
}
```

### 5.2 配置编辑

```rust
struct ConfigEdit {
    // 原子编辑操作
}
```

### 5.3 编辑构建器

```rust
struct ConfigEditsBuilder {
    // 构建器模式
}
```

## 6. Skill 加载接口

### 6.1 加载函数

```rust
fn load_skills_from_roots(roots: Vec<SkillRoot>) -> SkillLoadOutcome
```

`SkillLoadOutcome` 包含成功加载的技能列表、加载错误、被禁用的路径和隐式技能映射。

### 6.2 根目录解析

```rust
fn skill_roots_from_layer_stack(stack: &ConfigLayerStack) -> Vec<SkillRoot>
```

## 接口契约要点

1. **异步性**: 所有核心通信都是异步的
2. **类型安全**: 使用强类型定义所有接口
3. **错误处理**: 统一的错误传播机制
4. **可扩展性**: 支持插件式工具和技能扩展
5. **配置驱动**: 通过分层配置控制行为
