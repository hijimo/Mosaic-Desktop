# API / 接口契约

## 1. Tauri IPC Commands (前端 → 后端)

前端通过 `@tauri-apps/api/core` 的 `invoke()` 调用以下命令：

### submit_op

提交操作到核心引擎。

```typescript
invoke('submit_op', { id: string, op: Op })
// 返回: Result<void, string>
```

### poll_events

轮询引擎事件。

```typescript
invoke('poll_events', { maxCount?: number })
// 返回: Result<Event[], string>
// 默认最多返回 100 个事件
```

### get_config

获取当前合并后的配置。

```typescript
invoke('get_config')
// 返回: Result<ConfigValue, string>
```

### update_config

更新会话级配置。

```typescript
invoke('update_config', { tomlContent: string })
// 返回: Result<void, string>
```

### get_cwd

获取当前工作目录。

```typescript
invoke('get_cwd')
// 返回: Result<string, string>
```

## 2. Submission Queue — Op 枚举 (前端 → 后端)

所有操作通过 `submit_op` 提交，`Op` 是一个 tagged union：

### 核心对话

| Op | 关键字段 | 说明 |
|----|---------|------|
| `user_turn` | items, cwd, model, sandbox_policy, approval_policy | 发起一轮对话 |
| `user_input` | items | 旧版用户输入 |
| `user_input_answer` | id, response | 回答引擎的输入请求 |
| `interrupt` | — | 中断当前 turn |
| `shutdown` | — | 关闭引擎 |

### 审批

| Op | 关键字段 | 说明 |
|----|---------|------|
| `exec_approval` | id, decision, custom_instructions | 命令执行审批 |
| `patch_approval` | id, decision | 补丁应用审批 |
| `resolve_elicitation` | server_name, request_id, decision | MCP 请求审批 |

### 上下文管理

| Op | 说明 |
|----|------|
| `override_turn_context` | 覆盖当前 turn 的 model/cwd/policy 等 |
| `compact` | 压缩上下文 |
| `undo` | 撤销上一步操作 |
| `thread_rollback` | 回滚 N 个 turn |

### MCP / Skills / 配置

| Op | 说明 |
|----|------|
| `list_mcp_tools` | 列出 MCP 工具 |
| `refresh_mcp_servers` | 刷新 MCP 服务器连接 |
| `list_skills` | 列出可用 Skills |
| `reload_user_config` | 重新加载用户配置 |

### 其他

| Op | 说明 |
|----|------|
| `dynamic_tool_response` | 动态工具调用结果 |
| `review` | 进入代码审查模式 |
| `set_thread_name` | 设置线程名称 |
| `drop_memories` / `update_memories` | 记忆管理 |
| `run_user_shell_command` | 用户直接执行 Shell 命令 |

## 3. Event Queue — EventMsg 枚举 (后端 → 前端)

引擎通过 `poll_events` 返回事件，`EventMsg` 是一个 tagged union，包含 60+ 事件类型：

### Turn 生命周期

| 事件 | 关键字段 | 说明 |
|------|---------|------|
| `task_started` | turn_id, model_context_window | Turn 开始 |
| `task_complete` | turn_id, last_agent_message | Turn 完成 |
| `turn_aborted` | turn_id, reason | Turn 中止 |

### Agent 消息

| 事件 | 说明 |
|------|------|
| `agent_message` | 完整的 Agent 消息 |
| `agent_message_delta` | 流式文本增量 |
| `agent_reasoning_delta` | 推理过程增量 |
| `user_message` | 用户消息回显 |

### 命令执行

| 事件 | 说明 |
|------|------|
| `exec_command_begin` | 命令开始执行 |
| `exec_command_output_delta` | 命令输出增量 |
| `exec_command_end` | 命令执行完成 (含 stdout/stderr/exit_code) |

### 审批请求

| 事件 | 说明 |
|------|------|
| `exec_approval_request` | 请求用户审批命令执行 |
| `apply_patch_approval_request` | 请求用户审批补丁应用 |
| `request_user_input` | 请求用户输入 |
| `elicitation_request` | MCP 服务器请求用户输入 |

### 补丁

| 事件 | 说明 |
|------|------|
| `patch_apply_begin` | 补丁开始应用 |
| `patch_apply_end` | 补丁应用完成 |

### MCP

| 事件 | 说明 |
|------|------|
| `mcp_startup_update` | MCP 服务器启动状态 |
| `mcp_startup_complete` | 所有 MCP 服务器启动完成 |
| `mcp_tool_call_begin/end` | MCP 工具调用 |

### 会话管理

| 事件 | 说明 |
|------|------|
| `session_configured` | 会话配置完成 |
| `token_count` | Token 使用统计 |
| `context_compacted` | 上下文已压缩 |

## 4. 内部 Trait 接口

### ToolHandler

```rust
#[async_trait]
trait ToolHandler: Send + Sync {
    fn matches_kind(&self, kind: &ToolKind) -> bool;
    fn kind(&self) -> ToolKind;
    async fn handle(&self, args: Value) -> Result<Value, CodexError>;
    fn tool_spec(&self) -> Option<Value> { None }
}
```

### ToolRouter 路由优先级

1. `ToolRegistry` (Built-in) — 精确匹配 `ToolKind::Builtin(name)`
2. `McpConnectionManager` — 匹配 `mcp__{server}__{tool}` 格式
3. `dynamic_tools` HashMap — 返回 `RouteResult::DynamicTool` 由调用方处理

## 5. OpenAI Responses API 接口

### 请求格式

```rust
struct ResponsesApiRequest {
    model: String,
    input: Vec<Value>,       // 对话历史
    stream: bool,            // 始终为 true
    instructions: Option<String>,
    previous_response_id: Option<String>,
    tool_choice: Option<String>,  // "auto" 当有 tools 时
    tools: Option<Vec<Value>>,    // 工具定义列表
}
```

### 工具定义格式 (tool_spec)

```json
{
    "type": "function",
    "name": "shell",
    "description": "Runs a shell command...",
    "parameters": {
        "type": "object",
        "properties": {
            "command": { "type": "array", "items": { "type": "string" } },
            "workdir": { "type": "string" },
            "timeout_ms": { "type": "integer" }
        },
        "required": ["command"],
        "additionalProperties": false
    }
}
```

### 响应事件流

| SSE 事件类型 | 映射到 | 说明 |
|-------------|--------|------|
| `response.output_text.delta` | `OutputTextDelta` | 文本增量 |
| `response.output_item.done` (type=message) | `OutputItemDone` | 完整消息 |
| `response.output_item.done` (type=function_call) | `FunctionCall` | 工具调用 |
| `response.reasoning_summary_text.delta` | `ReasoningDelta` | 推理增量 |
| `response.completed` | `Completed` | 响应完成 |
| `response.failed` | `Failed` | 响应失败 |
