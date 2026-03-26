# Tauri 端自动化测试方案

> 基于 steering/testing.md 规范，目标覆盖率 80%+

## 1. 现状分析

### 已有测试（930 个）

| 模块 | 测试数 | 覆盖情况 |
|------|--------|---------|
| core/skills | 60+ | 充分 |
| protocol/roundtrip | 24 | 充分 |
| core/codex | 22 | 仅覆盖 Op 分发，**未覆盖 agentic loop** |
| core/client | 22 | SSE 解析 |
| core/session | 21+12 | 状态管理 |
| core/agent | 31 | 多 Agent 控制 |
| provider | 18 | Provider 注册 |
| exec/sandbox | 13 | 沙箱策略 |
| core/tools/router | 9 | 路由分发 |
| core/event_mapping | 9 | ResponseItem → TurnItem 转换 |
| commands | 9 | Rollout 解析 |

### 测试缺口

| 缺口 | 影响 | 优先级 |
|------|------|--------|
| **v2 结构化事件发射** — ItemStarted/ContentDelta/ItemCompleted 序列 | 前端无法渲染流式内容 | P0 |
| **Agentic loop** — FunctionCall → dispatch → FunctionOutput → 继续 | 工具调用链路无验证 | P0 |
| **工具注册验证** — Session 初始化后 tool specs 是否正确 | 工具调用完全不工作 | P0 |
| **dispatch_tool_call 事件** — McpToolCallBegin/End 发射 | 前端工具调用 UI 无数据 | P1 |
| **Event bridge 持久化** — RawResponseItem → RolloutItem::ResponseItem | Resume 丢失工具调用历史 | P1 |
| **Rollout 持久化策略** — 哪些事件被持久化、哪些不被 | 数据完整性 | P1 |
| **Fuzz testing** — 协议序列化/反序列化 | 边界情况崩溃 | P2 |

## 2. 测试架构原则

### 核心原则：测试代码与业务代码完全分离

- 测试文件放在独立的 `tests/` 目录或独立的 `_test.rs` 文件中
- **禁止**在业务模块（`codex.rs`、`session.rs`、`router.rs` 等）的 `#[cfg(test)] mod tests` 中添加新测试
- 已有的模块内测试保持不动，新增测试全部放在独立文件

### 目录结构

```
src-tauri/
├── src/
│   ├── core/codex.rs          # 业务代码（不添加测试）
│   ├── core/session.rs        # 业务代码（不添加测试）
│   └── ...
├── tests/                     # 独立集成测试目录
│   ├── e2e_mock_api.rs        # Mock SSE server + 真实 Codex 引擎
│   ├── boundary_tests.rs      # 边界条件测试
│   ├── tool_handler_tests.rs  # 工具 handler 集成测试
│   └── rollout_policy_tests.rs # 持久化策略测试
└── fuzz/                      # Fuzz 测试（cargo-fuzz）
    └── fuzz_targets/
        └── protocol_roundtrip.rs
```

### Mock API 方案

通过 `ConfigLayerStack.add_layer` 注入自定义 provider（`base_url` 指向本地 mock TCP server），
让 `run_turn` → `stream_response` 的 HTTP 请求打到 mock server。
整条链路都是生产代码，零修改核心代码。

```rust
// 测试中创建 mock provider
let mut config = ConfigLayerStack::new();
config.add_layer(ConfigLayer::Session, ConfigToml {
    model: Some("mock-model".into()),
    model_provider: Some("mock".into()),
    model_providers: HashMap::from([("mock".into(), ModelProviderInfo {
        base_url: Some(mock_server.base_url()),
        env_key: Some("TEST_API_KEY".into()),
        ...
    })]),
    ..Default::default()
});
let codex = Codex::new(sq_rx, eq_tx, config, cwd);
```

## 3. 测试用例

### P0：v2 结构化事件

| ID | 测试用例 | 验证 |
|----|---------|------|
| V-01 | UserTurn → 纯文本响应 | ItemStarted(UserMessage) + ItemCompleted(UserMessage) + ItemStarted(AgentMessage) + AgentMessageContentDelta×N + ItemCompleted(AgentMessage) |
| V-02 | UserTurn → 工具调用 + 文本 | McpToolCallBegin + McpToolCallEnd + 第二轮 AgentMessageContentDelta |
| V-03 | UserTurn → 推理 + 文本 | ItemStarted(Reasoning) + ReasoningContentDelta + ItemCompleted(Reasoning) + AgentMessage |
| V-04 | 空响应 | TurnComplete(last_agent_message=None)，无 AgentMessage |
| V-05 | API 错误 | Error 事件 + TurnComplete |
| V-06 | Token 用量 | TokenCount 事件包含正确数值 |
| V-07 | 无 legacy 事件 | 不发射 AgentMessageDelta/AgentMessage/AgentReasoningDelta |

### P0：边界条件

| ID | 测试用例 | 验证 |
|----|---------|------|
| B-01 | UserTurn 空 items | 正常发射 bracket 事件 |
| B-02 | 多轮 UserTurn item_id 唯一性 | 所有 item_id 全局唯一 |
| B-03 | ItemStarted/Completed id 一致性 | 每个 Started 有对应的 Completed |
| B-04 | shell handler 空命令 | 返回 InvalidInput |
| B-05 | shell handler 命令超时 | 返回 timed_out |
| B-06 | list_dir 不存在路径 | 返回错误 |
| B-07 | read_file 不存在文件 | 返回错误 |
| B-08 | event_mapping 空 role | 返回 None |
| B-09 | event_mapping FunctionCall | 返回 None（不展示） |

### P1：持久化策略

| ID | 测试用例 | 验证 |
|----|---------|------|
| P-01 | AgentMessageContentDelta 不持久化 | Limited 和 Extended 模式都不写入 |
| P-02 | ItemStarted/Completed 不持久化 | 仅 UI 事件 |
| P-03 | RawResponseItem 作为 ResponseItem 持久化 | event bridge 中转换 |
| P-04 | TurnStarted/Complete 在 Limited 模式持久化 | 核心对话事件 |
| P-05 | ExecCommandEnd 仅在 Extended 模式持久化 | 工具调用详情 |

### P2：Fuzz 测试

| ID | 目标 |
|----|------|
| F-01 | EventMsg 序列化/反序列化 |
| F-02 | Op 序列化/反序列化 |
| F-03 | ResponseItem 序列化/反序列化 |
| F-04 | TurnItem 序列化/反序列化 |

## 4. 实施顺序

1. 创建 `tests/` 目录结构
2. P0 Mock API 测试（`tests/e2e_mock_api.rs`）
3. P0 边界测试（`tests/boundary_tests.rs`）
4. P1 持久化策略测试（`tests/rollout_policy_tests.rs`）
5. P2 Fuzz 测试（`fuzz/`）
