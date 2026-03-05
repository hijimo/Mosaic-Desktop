# Codex 自动化测试体系完整文档

> 生成时间: 2026-03-04 | 项目: OpenAI Codex CLI

---

## 1. 测试体系总览

### 1.1 规模统计

| 指标 | 数量 |
|------|------|
| 测试函数总数 | **2,762** |
| 含测试的 Rust 文件 | 349 |
| `#[cfg(test)]` 模块 | 453 |
| 异步测试 (`#[tokio::test]`) | 1,365 |
| 快照文件 (`.snap`) | 297 |
| 集成测试目录 | 20+ |
| TypeScript 测试文件 | 6 |
| CI 测试矩阵平台 | 5 (macOS aarch64/x86_64, Linux gnu/musl/aarch64) |

### 1.2 测试分布 (按 crate)

```
core            117 文件  ████████████████████████████████████████  (33.5%)
tui              67 文件  ████████████████████████                  (19.2%)
utils            24 文件  ████████                                  (6.9%)
windows-sandbox  11 文件  ████                                      (3.2%)
execpolicy-leg.  10 文件  ███                                       (2.9%)
app-server        9 文件  ███                                       (2.6%)
otel              9 文件  ███                                       (2.6%)
network-proxy     8 文件  ███                                       (2.3%)
exec              8 文件  ███                                       (2.3%)
app-server-proto  8 文件  ███                                       (2.3%)
其他 (~20 crates) 78 文件  ██████████████████████████                (22.3%)
```

### 1.3 断言使用统计

| 断言类型 | 使用次数 |
|----------|----------|
| `assert_eq!` | 6,876 |
| `assert!` | 3,577 |
| `insta::assert_snapshot!` | 231 |
| `assert_ne!` | 65 |
| `assert_matches!` | 64 |
| `#[should_panic]` | 6 |

---

## 2. 测试框架与工具链

### 2.1 核心框架

| 工具 | 用途 | 配置 |
|------|------|------|
| **cargo-nextest** | Rust 测试运行器 (替代 `cargo test`) | `codex-rs/.config/nextest.toml` |
| **tokio::test** | 异步测试运行时 | 1,365 处使用 |
| **insta** | 快照测试 | 297 个 `.snap` 文件 |
| **wiremock** | HTTP Mock 服务器 | 380 处使用 |
| **tempfile** | 临时目录/文件 | 1,362 处使用 |
| **assert_cmd** | CLI 命令测试 | 集成测试中使用 |
| **Jest** | TypeScript 测试 | 2 个配置文件 |
| **cargo-deny** | 依赖安全审计 | `codex-rs/deny.toml` |

### 2.2 自定义测试宏 (`codex-test-macros`)

```rust
// #[large_stack_test] - 创建 16MB 栈线程运行测试
// 用于栈密集型测试，自动处理 sync/async
#[large_stack_test]
async fn my_stack_heavy_test() {
    // 在 16MB 栈线程中运行
}
```

功能：
- 创建 16MB 栈的独立线程
- 自动处理 `#[tokio::test]` → 自定义 Tokio 运行时
- 保留 `#[test_case]` 等其他属性

### 2.3 条件跳过宏

```rust
skip_if_sandbox!();                    // CODEX_SANDBOX=seatbelt 时跳过
skip_if_no_network!();                 // CODEX_SANDBOX_NETWORK_DISABLED=1 时跳过
skip_if_windows!();                    // Windows 平台跳过
```

### 2.4 Nextest 配置

```toml
# codex-rs/.config/nextest.toml
[profile.default]
slow-timeout = { period = "15s", terminate-after = 2 }

# 特殊超时覆盖
[[profile.default.overrides]]
filter = 'test(rmcp_client) | test(humanlike_typing_1000_chars...)'
slow-timeout = { period = "1m", terminate-after = 4 }

[[profile.default.overrides]]
filter = 'test(approval_matrix_covers_all_modes)'
slow-timeout = { period = "30s", terminate-after = 2 }
```

### 2.5 Cargo Profile

```toml
[profile.ci-test]
debug = 1         # 减少调试符号大小
inherits = "test"
opt-level = 0     # 最快编译
```

---

## 3. 测试分层架构

### 3.1 三层测试金字塔

```
                    ┌─────────────┐
                    │  E2E / CI   │  CI 多平台矩阵
                    │  集成测试    │  app-server-test-client
                   ─┼─────────────┼─
                  │  集成测试       │  core/tests/suite/ (80+ 场景)
                  │  (crate 级)    │  app-server/tests/suite/ (40+ 场景)
                 ─┼────────────────┼─
               │    单元测试         │  #[cfg(test)] 内嵌模块 (453 个)
               │    快照测试         │  insta 快照 (297 个)
               └────────────────────┘
```

### 3.2 单元测试 (内嵌 `#[cfg(test)]`)

**位置**: 与实现代码同文件

**特点**:
- 测试私有函数和内部逻辑
- 每个重要模块都有 `#[cfg(test)] mod tests`
- 核心文件如 `codex.rs` 有 9 个 `#[cfg(test)]` 模块

**示例** (来自 `mcp_connection_manager.rs`):
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_qualify_tools_short_non_duplicated_names() {
        // 测试工具名称限定逻辑
    }

    #[test]
    fn tool_filter_applies_enabled_list() {
        // 测试工具过滤器
    }
}
```

### 3.3 集成测试

#### 3.3.1 Core 集成测试 (`codex-rs/core/tests/`)

**结构**:
```
core/tests/
├── all.rs                    # 测试入口，引入所有模块
├── common/                   # 共享测试基础设施
│   ├── lib.rs               # 公共导出
│   ├── test_codex.rs        # TestCodexBuilder
│   ├── test_codex_exec.rs   # CLI 执行测试
│   ├── responses.rs         # wiremock Mock 响应
│   ├── streaming_sse.rs     # SSE 流构建器
│   ├── context_snapshot.rs  # 上下文快照
│   ├── apps_test_server.rs  # Apps 测试服务器
│   ├── process.rs           # 进程管理
│   └── zsh_fork.rs          # Zsh fork 测试
├── suite/                    # 测试场景 (80+ 文件)
│   ├── abort_tasks.rs
│   ├── agent_jobs.rs
│   ├── approvals.rs
│   ├── compact.rs
│   ├── exec.rs
│   ├── hierarchical_agents.rs
│   ├── js_repl.rs
│   ├── memories.rs
│   ├── realtime_conversation.rs
│   ├── skills.rs
│   ├── tools.rs
│   ├── web_search.rs
│   └── ... (80+ 场景)
└── fixtures/                 # 测试数据
    ├── incomplete_sse.json
    └── scenarios/
```

**覆盖领域**:

| 类别 | 测试文件 | 说明 |
|------|----------|------|
| 任务管理 | abort_tasks, pending_input, turn_state | 任务中断、待处理输入、轮次状态 |
| 命令执行 | exec, unified_exec, shell_command | 沙箱执行、统一执行、Shell 命令 |
| 工具系统 | tools, tool_harness, tool_parallelism | 工具调用、并行执行 |
| Agent | hierarchical_agents, agent_jobs, subagent_notifications | 多层Agent、批量作业 |
| MCP | rmcp_client | MCP 客户端集成 |
| 模型 | model_overrides, model_switching, model_info_overrides | 模型切换、配置覆盖 |
| 会话 | compact, compact_resume_fork, fork_thread | 压缩、恢复、分叉 |
| 流式 | cli_stream, stream_error_allows_next_turn | 流式输出、错误恢复 |
| 安全 | approvals, exec_policy, seatbelt, safety_check_downgrade | 审批、执行策略、沙箱 |
| Skills | skills, skill_approval | 技能加载、技能审批 |
| 存储 | sqlite_state, rollout_list_find, memories | SQLite、Rollout、记忆 |
| 实时 | realtime_conversation | 实时对话 |
| 认证 | auth_refresh | 认证刷新 |

#### 3.3.2 App Server 集成测试 (`codex-rs/app-server/tests/`)

**结构**:
```
app-server/tests/
├── all.rs
├── common/
│   ├── lib.rs
│   ├── mock_model_server.rs    # 模型服务器 Mock
│   ├── responses.rs            # 响应构建器
│   ├── config.rs               # 测试配置
│   ├── auth_fixtures.rs        # 认证 fixture
│   ├── mcp_process.rs          # MCP 进程管理
│   ├── models_cache.rs         # 模型缓存
│   └── rollout.rs              # Rollout 工具
└── suite/
    ├── v2/                     # V2 API 测试 (20+ 文件)
    │   ├── thread_start.rs
    │   ├── turn_start.rs
    │   ├── thread_resume.rs
    │   ├── thread_fork.rs
    │   ├── config_rpc.rs
    │   ├── skills_list.rs
    │   └── ...
    ├── send_message.rs
    ├── create_thread.rs
    ├── auth.rs
    └── ...
```

### 3.4 快照测试 (Insta)

**分布**: 主要在 TUI crate (270/297 个快照文件)

**快照目录**:
```
tui/src/snapshots/                              # 主快照目录
tui/src/bottom_pane/snapshots/                  # 底部面板
tui/src/bottom_pane/request_user_input/snapshots/ # 用户输入请求
tui/src/status/snapshots/                       # 状态栏
tui/src/chatwidget/snapshots/                   # 聊天组件
tui/src/onboarding/snapshots/                   # 引导页
core/tests/suite/snapshots/                     # Core 集成测试快照
```

**快照文件格式**:
```
---
source: tui/src/diff_render.rs
expression: terminal.backend()
---
"• Added new_file.txt (+2 -0)                                    "
"    1 +alpha                                                     "
"    2 +beta                                                      "
"                                                                 "
```

**命名规范**: `{crate}__{module}__{tests}__{test_name}.snap`

**快照测试模式**:
```rust
// 1. 构建组件
let overlay = RequestUserInputOverlay::new(
    request_event("turn-1", vec![question_with_options("q1", "Area")]),
    tx, true, false, false,
);

// 2. 定义渲染区域
let area = Rect::new(0, 0, 120, 16);

// 3. 渲染并断言快照
insta::assert_snapshot!(
    "request_user_input_options",
    render_snapshot(&overlay, area)
);
```

**辅助函数**:
```rust
fn snapshot_buffer(buf: &Buffer) -> String { /* Buffer -> 文本 */ }
fn render_snapshot(pane: &BottomPane, area: Rect) -> String { /* 渲染 -> 文本 */ }
fn snapshot_lines(name: &str, lines: Vec<RtLine>, width: u16, height: u16) { /* 行渲染快照 */ }
fn render_lines(lines: &[Line]) -> Vec<String> { /* 行 -> 字符串 */ }
```

**更新快照**: `cargo insta review` 或 `cargo insta accept`

---

## 4. Mock 与测试基础设施

### 4.1 TestCodexBuilder (核心测试构建器)

```rust
// 流式 API 构建测试 Codex 实例
let (codex, mock_server) = TestCodexBuilder::new()
    .with_model("gpt-4")
    .with_sandbox_policy(SandboxPolicy::ReadOnly)
    .with_approval_policy(AskForApproval::Never)
    .with_temp_dir(temp_dir)
    .build()
    .await;
```

**功能**:
- 创建隔离的临时目录
- 配置 Mock 模型服务器
- 设置沙箱和审批策略
- 提供事件等待和断言工具

### 4.2 Wiremock Mock 服务器

```rust
// 设置 Mock 响应
let mock_server = MockServer::start().await;
Mock::given(method("POST"))
    .and(path("/v1/responses"))
    .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
    .mount(&mock_server)
    .await;
```

**Mock 响应构建**:
- SSE 流事件构建器
- 多请求序列支持
- 有状态响应器 (跨请求维护状态)
- 请求体自动验证

### 4.3 App Server Test Client

```rust
// 完整的 E2E 测试客户端
let client = CodexClient::connect(endpoint).await?;
client.initialize().await?;
client.thread_start(params).await?;
client.send_user_message("Hello").await?;
let events = client.stream_turn().await?;
```

**功能**:
- stdio/WebSocket 连接
- JSON-RPC 请求/响应
- 事件流监听
- 审批请求处理

### 4.4 测试 Fixture

```
core/tests/fixtures/          # SSE 响应数据
tui/tests/fixtures/           # TUI 渲染数据
apply-patch/tests/fixtures/   # 补丁测试数据
exec/tests/fixtures/          # 执行测试数据
backend-client/tests/fixtures/ # 后端客户端数据
```

---

## 5. 测试模式与最佳实践

### 5.1 Mock 驱动集成测试模式

```
1. 启动 Mock 服务器 (wiremock)
2. 构建 TestCodex (TestCodexBuilder)
3. 提交操作 (Op::UserTurn)
4. 等待特定事件 (wait_for_event)
5. 断言结果
6. 验证请求体 (自动)
```

### 5.2 事件驱动断言模式

```rust
// 等待特定事件类型
let event = codex.wait_for_event(|e| matches!(e.msg, EventMsg::TurnComplete(_))).await;

// 断言事件内容
match event.msg {
    EventMsg::TurnComplete(tc) => {
        assert_eq!(tc.status, TurnStatus::Completed);
    }
    _ => panic!("unexpected event"),
}
```

### 5.3 快照测试模式

```rust
#[test]
fn ui_snapshot_xxx() {
    // 1. 构建组件状态
    let widget = build_test_widget(params);
    // 2. 渲染到 Buffer
    let buf = render_to_buffer(&widget, Rect::new(0, 0, 80, 24));
    // 3. 快照断言
    insta::assert_snapshot!("test_name", snapshot_buffer(&buf));
}
```

### 5.4 沙箱测试模式

```rust
#[test]
fn sandbox_restricts_file_access() {
    skip_if_sandbox!();  // 已在沙箱中则跳过

    // 自重执行模式: 以沙箱模式重新运行自身
    if std::env::var("IN_SANDBOX").is_ok() {
        // 在沙箱内验证限制
        assert!(fs::write("/tmp/forbidden", "data").is_err());
        return;
    }

    // 启动沙箱子进程
    let status = Command::new(std::env::current_exe().unwrap())
        .env("IN_SANDBOX", "1")
        .status();
    assert!(status.unwrap().success());
}
```

### 5.5 平台条件测试

```rust
#[cfg(target_os = "macos")]
#[test]
fn seatbelt_sandbox_test() { /* macOS 专用 */ }

#[cfg(target_os = "linux")]
#[test]
fn landlock_sandbox_test() { /* Linux 专用 */ }
```

---

## 6. CI/CD 测试流水线

### 6.1 流水线架构

```
┌─────────────────────────────────────────────────────┐
│                    rust-ci.yml                       │
├─────────────────────────────────────────────────────┤
│                                                     │
│  ┌──────────────┐  ┌──────────────┐                │
│  │ Detect Changes│→│ Format Check │                │
│  │              │  │ cargo fmt    │                │
│  └──────────────┘  └──────────────┘                │
│                                                     │
│  ┌──────────────────────────────────────────┐      │
│  │ Lint/Build Matrix (5 平台)                │      │
│  │ ┌────────────┐ ┌────────────┐            │      │
│  │ │macOS arm64 │ │macOS x86_64│            │      │
│  │ │  clippy     │ │  clippy    │            │      │
│  │ │  --tests    │ │  --tests   │            │      │
│  │ └────────────┘ └────────────┘            │      │
│  │ ┌────────────┐ ┌────────────┐ ┌────────┐│      │
│  │ │Linux gnu   │ │Linux musl  │ │Linux   ││      │
│  │ │  clippy     │ │  clippy    │ │arm64   ││      │
│  │ └────────────┘ └────────────┘ └────────┘│      │
│  └──────────────────────────────────────────┘      │
│                                                     │
│  ┌──────────────────────────────────────────┐      │
│  │ Test Matrix (3 平台)                      │      │
│  │ ┌────────────┐ ┌────────────┐ ┌────────┐│      │
│  │ │macOS arm64 │ │Linux gnu   │ │Linux   ││      │
│  │ │  nextest    │ │  nextest   │ │arm64   ││      │
│  │ │  --all-feat │ │  --all-feat│ │nextest ││      │
│  │ └────────────┘ └────────────┘ └────────┘│      │
│  └──────────────────────────────────────────┘      │
│                                                     │
│  ┌──────────────┐                                  │
│  │ cargo shear  │  (未使用依赖检测)                  │
│  └──────────────┘                                  │
└─────────────────────────────────────────────────────┘

┌──────────────┐  ┌──────────────────┐  ┌────────────┐
│ cargo-deny   │  │ shell-tool-mcp-ci│  │ sdk.yml    │
│ 依赖安全审计  │  │ Jest + pnpm      │  │ Jest + pnpm│
└──────────────┘  └──────────────────┘  └────────────┘
```

### 6.2 CI 测试执行命令

```bash
# Lint (所有平台)
cargo clippy --target $TARGET --all-features --tests --profile $PROFILE -- -D warnings

# 测试 (3 平台)
cargo nextest run --all-features --no-fail-fast --target $TARGET --cargo-profile ci-test

# TypeScript 测试
pnpm install --frozen-lockfile
pnpm -r --filter ./sdk/typescript run test
pnpm -r --filter ./shell-tool-mcp run test
```

### 6.3 CI 优化策略

| 策略 | 实现 |
|------|------|
| **变更检测** | 只在相关文件变更时触发 |
| **sccache** | 编译缓存加速 |
| **cargo-chef** | 依赖预热缓存 |
| **ci-test profile** | `opt-level=0` 最快编译 |
| **并行矩阵** | 5 平台并行 Lint, 3 平台并行测试 |
| **超时控制** | 默认 15s, 特殊测试 30s-1m |
| **Cargo timings** | 上传编译时间报告 |

### 6.4 依赖安全审计 (cargo-deny)

```toml
# codex-rs/deny.toml
[advisories]
vulnerability = "deny"      # 拒绝已知漏洞
unmaintained = "warn"        # 警告未维护依赖

[licenses]
unlicensed = "deny"          # 拒绝无许可证
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", ...]

[sources]
unknown-registry = "deny"    # 拒绝未知注册表
unknown-git = "deny"         # 拒绝未知 Git 源
```

---

## 7. TypeScript 测试

### 7.1 shell-tool-mcp

```javascript
// jest.config.cjs
module.exports = { preset: "ts-jest" };

// tests/bashSelection.test.ts - Shell 选择逻辑
// tests/osRelease.test.ts     - OS 发行版检测
```

### 7.2 SDK TypeScript

```javascript
// jest.config.cjs - ESM 配置
module.exports = {
  extensionsToTreatAsEsm: [".ts"],
  transform: { "^.+\\.tsx?$": ["ts-jest", { useESM: true }] },
};

// tests/
// ├── run.test.ts          - 基本运行
// ├── runStreamed.test.ts   - 流式运行
// ├── exec.test.ts          - 命令执行
// └── abort.test.ts         - 中断处理
```

---

## 8. 本地开发测试命令

### 8.1 Justfile 命令

```bash
# 运行所有测试 (推荐)
just test                    # cargo nextest run --no-fail-fast

# 格式化
just fmt                     # cargo fmt

# Lint 修复
just fix -p codex-tui        # cargo clippy --fix --tests -p codex-tui

# Clippy 检查
just clippy                  # cargo clippy --tests

# App Server 测试客户端
just app-server-test-client  # 构建并运行 E2E 测试客户端

# Bazel 测试
just bazel-test              # bazel test //...

# 更新 API Schema
just write-app-server-schema
just write-config-schema
```

### 8.2 常用 Cargo 命令

```bash
# 特定 crate 测试
cargo test -p codex-core
cargo test -p codex-tui

# 特定测试函数
cargo test -p codex-core -- test_name

# 快照更新
cargo insta review
cargo insta accept

# 全量测试 (共享 crate 变更后)
cargo nextest run --no-fail-fast

# CI 模式测试
cargo nextest run --all-features --cargo-profile ci-test
```

---

## 9. 测试覆盖领域矩阵

| 领域 | 单元测试 | 集成测试 | 快照测试 | E2E | CI |
|------|:--------:|:--------:|:--------:|:---:|:--:|
| AI 对话循环 | ✅ | ✅ | | | ✅ |
| 工具执行 | ✅ | ✅ | | | ✅ |
| MCP 协议 | ✅ | ✅ | | | ✅ |
| Skills 系统 | ✅ | ✅ | | | ✅ |
| 多 Agent | ✅ | ✅ | | | ✅ |
| TUI 渲染 | ✅ | | ✅ (270) | | ✅ |
| Diff 渲染 | ✅ | | ✅ | | ✅ |
| 配置系统 | ✅ | ✅ | | | ✅ |
| 沙箱安全 | ✅ | ✅ | | | ✅ |
| 执行策略 | ✅ | ✅ | | | ✅ |
| 网络代理 | ✅ | ✅ | | | ✅ |
| 认证 | ✅ | ✅ | | | ✅ |
| 补丁应用 | ✅ | ✅ | | | ✅ |
| 状态存储 | ✅ | ✅ | | | ✅ |
| App Server API | ✅ | ✅ | | ✅ | ✅ |
| TypeScript SDK | | | | ✅ | ✅ |
| Shell MCP | | ✅ | | | ✅ |
| 依赖安全 | | | | | ✅ |

---

## 10. 测试架构设计原则

### 10.1 隔离性
- 每个测试使用 `tempfile::TempDir` 创建独立工作目录
- Mock 服务器独立端口，避免测试间干扰
- 环境变量通过 `EnvVarGuard` 模式安全设置/恢复

### 10.2 确定性
- 快照测试固定渲染尺寸 (如 80x24, 120x40)
- Mock 响应预定义，不依赖外部服务
- 超时控制防止测试挂起 (默认 15s)

### 10.3 可维护性
- `TestCodexBuilder` 流式 API 减少样板代码
- 共享 `common/` 模块避免重复
- 快照文件自动管理 (`cargo insta review`)

### 10.4 跨平台
- CI 矩阵覆盖 macOS/Linux 多架构
- 条件编译 (`#[cfg(target_os)]`) 处理平台差异
- 沙箱测试自动检测平台能力

### 10.5 性能
- `cargo-nextest` 并行执行
- `sccache` 编译缓存
- `ci-test` profile 最小优化级别
- 严格超时防止慢测试
