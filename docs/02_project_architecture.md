# Codex 项目架构说明文档

## 1. 项目结构概览

Codex 采用 monorepo 结构，包含约 2800 个文件、463K 行代码。主要由 Rust 工作空间和 TypeScript 组件组成。

```
codex-main/
├── codex-rs/          # Rust 工作空间 (~60 crates)
├── codex-cli/         # Node.js CLI 包装层
├── shell-tool-mcp/    # TypeScript MCP 服务器
├── sdk/typescript/    # TypeScript SDK
├── docs/              # 项目文档
├── scripts/           # 构建和工具脚本
├── patches/           # 第三方补丁
├── .codex/skills/     # 内置技能定义
├── .github/           # CI/CD 工作流
└── third_party/       # 第三方依赖
```

## 2. Rust 工作空间 Crate 依赖关系

### 2.1 核心层

```
codex-protocol          # 协议定义层 (Event, Op, SandboxPolicy, 模型类型)
    ↑
codex-core              # 核心业务逻辑 (Session, 工具处理, MCP管理, 配置)
    ↑
┌───┴───┐
codex-tui  codex-app-server   # 消费层
```

### 2.2 Crate 详细说明

| Crate | 职责 | 关键类型 |
|-------|------|----------|
| `codex-core` | 核心逻辑：Session管理、工具处理、MCP连接、配置加载 | `Session`, `Codex`, `TurnContext`, `Config`, `McpConnectionManager` |
| `codex-protocol` | 协议定义：事件、操作、安全策略、模型类型 | `Event`, `EventMsg`, `Op`, `Submission`, `SandboxPolicy` |
| `codex-tui` | 终端UI：聊天界面、差异渲染、Markdown渲染 | `ChatWidget`, `App`, `BottomPane`, `DiffRenderStyleContext` |
| `codex-app-server` | JSON-RPC服务器：处理客户端请求 | `CodexMessageProcessor` |
| `codex-app-server-protocol` | API类型定义：V2协议、线程历史 | `ThreadStartParams`, `TurnStartParams`, `ThreadItem`, `ThreadHistoryBuilder` |
| `codex-network-proxy` | HTTP代理：域名过滤、MITM | `NetworkProxyState`, `HttpProxy` |
| `codex-state` | SQLite状态存储：记忆系统 | `StateDb`, memories模块 |
| `codex-exec` | 命令执行沙箱：进程隔离 | 沙箱执行器 |
| `codex-execpolicy` | 执行策略引擎（.codexpolicy 格式，PrefixRule 前缀匹配） | `ExecPolicyEngine`, `PrefixRule`, `PrefixPattern` |
| `codex-skills` | 技能系统：系统技能管理 | 技能安装和缓存 |
| `codex-rmcp-client` | MCP客户端：连接外部MCP服务器 | `RmcpClient`, `PendingTransport` |
| `codex-mcp-server` | MCP服务器：暴露Codex工具 | `MessageProcessor` |
| `codex-cli` | CLI入口点 | 命令行参数解析 |
| `codex-hooks` | 钩子系统：事件前后回调 | `Hooks`, `HookEvent` |
| `codex-apply-patch` | 补丁应用：文件变更 | 补丁解析和应用 |
| `codex-artifact-spreadsheet` | 电子表格制品 | `SpreadsheetArtifactManager` |
| `codex-artifact-presentation` | 演示文稿制品 | `PresentationArtifactManager` |
| `codex-cloud-requirements` | 云端需求获取 | `CloudRequirementsService` |
| `codex-cloud-tasks` | 云端任务管理 | 任务列表和状态 |
| `codex-auth` / `codex-login` | 认证和登录 | `AuthManager`, `CodexAuth` |
| `codex-config` | 配置类型定义 | 配置TOML类型 |
| `codex-backend-client` | 后端API客户端 | HTTP客户端 |
| `codex-file-search` | 文件搜索 | 模糊搜索 |
| `codex-shell-command` | Shell命令解析 | `parse_command` |
| `codex-secrets` | 密钥检测 | 敏感信息扫描 |
| `codex-otel` | OpenTelemetry集成 | 追踪和指标 |
| `codex-feedback` | 反馈上传 | 用户反馈 |

### 2.3 依赖关系图

```
codex-cli
  └── codex-tui
        ├── codex-core
        │     ├── codex-protocol
        │     ├── codex-rmcp-client
        │     ├── codex-network-proxy
        │     ├── codex-exec
        │     ├── codex-execpolicy
        │     ├── codex-state
        │     ├── codex-hooks
        │     ├── codex-apply-patch
        │     ├── codex-shell-command
        │     ├── codex-secrets
        │     ├── codex-artifact-spreadsheet
        │     ├── codex-artifact-presentation
        │     └── codex-cloud-requirements
        └── codex-app-server-protocol

codex-app-server
  ├── codex-core
  └── codex-app-server-protocol
        └── codex-protocol

codex-mcp-server
  └── codex-core
```

## 3. TypeScript 组件

### 3.1 shell-tool-mcp

- 位置: `shell-tool-mcp/`
- 职责: 提供平台特定的Shell二进制工具作为MCP服务器
- 技术栈: TypeScript + tsup 构建
- 入口: `src/index.ts`

### 3.2 sdk/typescript

- 位置: `sdk/typescript/`
- 职责: Codex TypeScript SDK，供外部集成使用
- 包含自动生成的类型定义

### 3.3 codex-cli (Node.js)

- 位置: `codex-cli/`
- 职责: npm 包装层，提供 `codex` 命令
- 入口: `bin/codex.js`
- 负责下载和管理原生Rust二进制

## 4. 构建系统

### 4.1 Cargo (Rust)

- 工作空间定义: `codex-rs/Cargo.toml`
- 本地开发主要使用 Cargo
- 命令: `cargo build`, `cargo test -p codex-xxx`

### 4.2 Bazel (CI)

- 配置: `MODULE.bazel`, `BUILD.bazel`, `defs.bzl`
- CI/CD 使用 Bazel 确保可重现构建
- 锁文件: `MODULE.bazel.lock`

### 4.3 pnpm (JavaScript)

- 工作空间: `pnpm-workspace.yaml`
- 管理 TypeScript 包依赖
- 锁文件: `pnpm-lock.yaml`

### 4.4 Just (任务运行器)

- 配置: `justfile`
- 常用命令:
  - `just fmt` - 格式化代码
  - `just fix -p <project>` - Clippy修复
  - `just test` - 运行测试
  - `just write-config-schema` - 更新配置Schema
  - `just bazel-lock-update` - 更新Bazel锁文件

## 5. 关键目录说明

| 目录 | 说明 |
|------|------|
| `codex-rs/core/src/codex.rs` | 核心Session管理 (~9900 LOC) |
| `codex-rs/core/src/config/` | 分层配置系统 |
| `codex-rs/core/src/tools/handlers/` | 工具处理器 (multi_agents, agent_jobs等) |
| `codex-rs/core/src/skills/` | 技能加载器 |
| `codex-rs/core/src/mcp_connection_manager.rs` | MCP连接管理 |
| `codex-rs/protocol/src/` | 协议类型定义 |
| `codex-rs/tui/src/` | TUI组件 (chatwidget, app, diff_render等) |
| `codex-rs/app-server/src/` | App Server实现 |
| `codex-rs/app-server-protocol/src/protocol/` | V2 API类型 |
| `codex-rs/network-proxy/src/` | 网络代理 |
| `codex-rs/state/src/` | 状态存储 |
| `.codex/skills/` | 内置技能 |
| `docs/` | 项目文档 |
