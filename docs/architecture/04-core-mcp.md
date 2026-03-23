# MCP 客户端/服务端 (`core/mcp_client/`, `core/mcp_server.rs`)

## 概述

Mosaic 同时作为 MCP 客户端（连接外部 MCP 服务器获取工具）和 MCP 服务端（暴露自身能力给外部）。

## MCP 客户端 (`core/mcp_client/`)

### 模块结构

| 文件 | 职责 |
|------|------|
| `connection_manager.rs` | `McpConnectionManager` — 管理多个 MCP 服务器连接 |
| `tool_call.rs` | MCP 工具调用执行 |
| `auth.rs` | OAuth 认证流程 |
| `skill_dependencies.rs` | Skill 对 MCP 工具的依赖解析 (`SkillDependencies`) |

### McpConnectionManager

```mermaid
graph LR
    Manager[McpConnectionManager] --> S1[Server A<br/>Ready]
    Manager --> S2[Server B<br/>Starting]
    Manager --> S3[Server C<br/>Failed]
```

关键能力：
- 并发管理多个 MCP 服务器连接
- 连接状态追踪 (`McpConnectionState`)
- 工具发现 (`McpToolInfo`)
- 自动重连

### 工具命名约定

MCP 工具使用双下划线分隔的命名格式：`mcp__{server}__{tool}`

例如：`mcp__filesystem__read_file`

## MCP 服务端 (`core/mcp_server.rs`)

`McpServer` 将 Mosaic 的能力暴露为 MCP 协议，允许外部客户端调用 Mosaic 的工具。

## 相关事件

| 事件 | 说明 |
|------|------|
| `McpStartupUpdate` | 服务器启动状态更新 |
| `McpStartupComplete` | 所有服务器启动完成 |
| `McpToolCallBegin` | MCP 工具调用开始 |
| `McpToolCallEnd` | MCP 工具调用结束 (含结果和耗时) |
| `McpListToolsResponse` | 工具列表响应 |
| `ElicitationRequest` | MCP 服务器请求用户输入 |

## RMCP 客户端 (`rmcp_client/`)

基于 RMCP SDK 的 MCP 客户端实现，支持 Streamable HTTP 传输和 OAuth 2.1 认证。与 `core/mcp_client/` 的区别在于：`core/mcp_client/` 是高层连接管理器，`rmcp_client/` 是底层 RMCP 协议客户端。

### 模块结构

| 文件 | 职责 |
|------|------|
| `rmcp_client.rs` | `RmcpClient` — RMCP 协议客户端，管理工具调用和 Elicitation |
| `oauth.rs` | OAuth 2.1 token 管理（存储、刷新、删除） |
| `perform_oauth_login.rs` | OAuth 登录流程（启动本地回调服务器，打开浏览器授权） |
| `auth_status.rs` | 认证状态检测 (`McpAuthStatus`) — 判断服务器是否需要 OAuth |
| `logging_client_handler.rs` | 日志记录客户端处理器 |
| `program_resolver.rs` | MCP 服务器程序路径解析 |
| `utils.rs` | 工具函数 |

### 核心类型

```rust
struct RmcpClient { /* RMCP 协议客户端 */ }

enum McpAuthStatus {
    NoAuthRequired,
    OAuthRequired { authorization_url: String },
    TokenValid,
    TokenExpired,
}

struct OauthLoginHandle {
    // 管理 OAuth 登录生命周期
    // 启动本地 HTTP 服务器接收回调
}

struct Elicitation { /* MCP 服务器请求用户输入 */ }
enum ElicitationResponse { Accept, Decline, Cancel }
```

### OAuth 2.1 登录流程

```mermaid
sequenceDiagram
    participant App as Mosaic
    participant Browser as 浏览器
    participant Server as MCP Server
    participant Callback as 本地回调服务器

    App->>App: determine_auth_status()
    App->>Callback: 启动本地 HTTP 服务器 (随机端口)
    App->>Browser: 打开授权 URL
    Browser->>Server: 用户授权
    Server->>Callback: 重定向回调 (authorization_code)
    Callback->>App: 收到 code
    App->>Server: 交换 token (code → access_token)
    App->>App: save_oauth_tokens()
```