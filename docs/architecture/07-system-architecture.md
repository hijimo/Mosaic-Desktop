# 系统架构文档

## 整体架构

```mermaid
graph TB
    subgraph Frontend["前端 (React + TypeScript)"]
        UI[UI 组件]
        Store[Zustand Store]
    end

    subgraph Tauri["Tauri IPC 层"]
        CMD[Tauri Commands<br/>submit_op / thread_start / thread_get_info / get_config]
    end

    subgraph Backend["后端 (Rust)"]
        subgraph Protocol["协议层"]
            SQ[Submission Queue<br/>Op 枚举]
            EQ[Event Queue<br/>EventMsg 枚举]
        end

        subgraph Core["核心引擎"]
            Codex[Codex<br/>事件循环 + Agentic Loop]
            Session[Session<br/>会话状态]
            Client[Client<br/>OpenAI API]
        end

        subgraph Tools["工具系统"]
            Router[ToolRouter]
            Registry[ToolRegistry]
            Handlers[Built-in Handlers]
            MCPClient[MCP Client]
            DynTools[Dynamic Tools]
        end

        subgraph Agent["Agent 系统"]
            Control[AgentControl]
            Instances[AgentInstance × N]
        end

        subgraph Infra["基础设施"]
            Config[Config LayerStack]
            Exec[Unified Exec / PTY]
            Policy[Exec Policy]
            Skills[Skills Manager]
            Memories[Memories]
            State[State DB]
            Secrets[Secrets Manager]
        end
    end

    subgraph External["外部服务"]
        API[OpenAI API]
        MCPServers[MCP Servers]
    end

    UI --> Store
    Store --> CMD
    CMD --> SQ
    EQ --> CMD
    CMD --> UI

    SQ --> Codex
    Codex --> EQ
    Codex --> Session
    Codex --> Client
    Codex --> Router

    Client --> API
    Router --> Registry
    Router --> MCPClient
    Router --> DynTools
    Registry --> Handlers
    MCPClient --> MCPServers
    Handlers --> Exec
    Exec --> Policy

    Codex --> Control
    Control --> Instances

    Session --> Skills
    Session --> Memories
    Codex --> Config
    Codex --> State
```

## 数据流

### 用户输入 → AI 响应

```mermaid
sequenceDiagram
    participant UI as React UI
    participant Tauri as Tauri IPC
    participant SQ as Submission Queue
    participant Codex as Codex Engine
    participant API as OpenAI API
    participant Tools as ToolRouter
    participant EQ as Event Queue

    UI->>Tauri: submit_op({ type: "user_turn", items, model, ... })
    Tauri->>SQ: Submission { id, op: UserTurn }
    SQ->>Codex: 消费 Submission
    Codex->>Codex: run_turn()

    loop Agentic Loop (最多 32 轮)
        Codex->>API: stream_response(history, tools)
        API-->>Codex: OutputTextDelta
        Codex->>EQ: AgentMessageDelta
        API-->>Codex: FunctionCall { name, args }
        Codex->>Tools: dispatch_tool_call(name, args)
        Tools-->>Codex: tool result
        Codex->>Codex: 更新 history
    end

    API-->>Codex: Completed
    Codex->>EQ: TurnComplete
    EQ-->>Tauri: poll_events()
    Tauri-->>UI: Vec<Event>
```

### 命令审批流程

```mermaid
sequenceDiagram
    participant Codex as Codex
    participant Policy as ExecPolicy
    participant EQ as Event Queue
    participant UI as UI
    participant SQ as Submission Queue
    participant Shell as Shell Handler

    Codex->>Policy: 检查命令权限
    Policy-->>Codex: 需要审批
    Codex->>EQ: ExecApprovalRequest
    EQ-->>UI: 显示审批对话框
    UI->>SQ: ExecApproval { decision: Approved }
    SQ->>Codex: 处理审批
    Codex->>Shell: 执行命令
    Shell-->>Codex: ExecResult
```

## 并发模型

- **主事件循环** — Codex 在单个 tokio task 中运行，顺序处理 Submission
- **流式响应** — API 流通过 `futures::Stream` 异步消费
- **工具执行** — 工具调用在独立 tokio task 中执行
- **多 Agent** — 每个 AgentInstance 可在独立 task 中运行
- **PTY 监听** — 后台 task 持续读取 PTY 输出

## 安全架构

```mermaid
graph TD
    CMD[命令请求] --> Sandbox{SandboxPolicy}
    Sandbox -->|DangerFullAccess| Direct[直接执行]
    Sandbox -->|ReadOnly| RO[只读沙箱]
    Sandbox -->|WorkspaceWrite| WS[工作区写入]
    Sandbox -->|ExternalSandbox| Ext[外部沙箱]

    WS --> Seatbelt[macOS Seatbelt]
    RO --> Seatbelt
    WS --> ExecPolicy[ExecPolicy 检查]
    ExecPolicy -->|匹配规则| Auto[自动执行]
    ExecPolicy -->|未匹配| Approval[用户审批]
```
