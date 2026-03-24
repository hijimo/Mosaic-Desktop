# 基础设施差距清单与工作量评估

根据 Mosaic-Desktop 与 codex-main 的对比分析，需要补齐 8 项基础设施。按依赖关系排序（先补底层，再补上层）。

## 第一层：ContextManager 扩展（被后续所有项依赖）

### 1. ContextManager 增加 `reference_context_item` 和 `token_info` 字段

- 现状：Mosaic 的 `ContextManager` 只有 `items` + `last_api_total_tokens`
- 目标：增加 `reference_context_item: Option<TurnContextItem>` 和 `token_info: Option<TokenUsageInfo>`（或复用现有的 `last_api_total_tokens` 扩展为完整 `TokenUsageInfo`）
- 改动文件：`context_manager/history.rs`
- 工作量：**小** — 加 2 个字段 + 4 个 getter/setter

### 2. ContextManager.record_items 支持 TruncationPolicy 参数

- 现状：`record_items` 只做 `is_api_item` 过滤 + `truncate_output_if_needed`，不接受 policy
- 目标：接受可选的 truncation policy，对 function_call_output 内容做字节/token 级截断（和 codex-main 一致）
- 改动文件：`context_manager/history.rs`
- 注意：Mosaic 已有 `TruncationPolicy` 枚举（`KeepRecent / KeepRecentTokens / AutoCompact`），但这是**历史级别**的截断策略，不是 codex-main 的**单条 item 级别**的 `Bytes(usize) / Tokens(usize)` 截断。需要新增一个 item 级别的截断策略，或者在 `truncate_output_if_needed` 中参数化限制值
- 工作量：**中** — 需要理清两套 TruncationPolicy 的关系，可能需要新增 `ItemTruncationPolicy` 类型

## 第二层：Session 状态统一

### 3. 将 `SessionInternalState` 与 `state::session::SessionState` 合并或桥接

- 现状：两个独立的状态结构
  - `Session.state: Mutex<SessionInternalState>` — 实际使用中，`history` 是 `Vec<ResponseInputItem>`
  - `state::session::SessionState` — 有 `ContextManager`、`active_mcp_tool_selection` 等，但**未被 Session 引用**
- 目标：Session 的历史管理走 `ContextManager`（经过 truncation + normalization），而不是裸 `Vec::extend`
- 方案选择：
  - **方案 A**：将 `SessionInternalState.history` 从 `Vec<ResponseInputItem>` 改为 `ContextManager`，并把 `state::SessionState` 的字段合并进来 — 改动大但彻底
  - **方案 B**：在 `Session` 上增加 `reference_context_item`、`previous_turn_settings`、`token_info` 的 setter/getter，委托给 `ContextManager` — 改动小但不够干净
- 改动文件：`session.rs`、`state/session.rs`、`codex.rs`（调用方）
- 工作量：**大** — 这是最核心的改造，涉及 Session 的所有历史读写路径
- 风险：`add_to_history`、`rollback`、`compact_history` 等现有方法都直接操作 `Vec`，需要逐一迁移到 `ContextManager`

## 第三层：Resume/Fork 逻辑完善（依赖第一、二层）

### 4. 重建算法注入 TurnContext（truncation policy）

- 现状：`reconstruct_history_from_rollout()` 是纯函数，不接收 TurnContext
- 目标：正向重放时 `ContextManager::record_items` 使用 truncation policy
- 改动文件：`reconstruction.rs`、`codex.rs`
- 工作量：**小** — 函数签名加参数，内部传递给 `record_items`

### 5. Session 状态完整注入

- 现状：`run_with_history` 只调用 `session.add_to_history(reconstruction.history)`
- 目标：
  - `session.set_reference_context_item(reconstruction.reference_context_item)`
  - `session.set_previous_turn_settings(reconstruction.previous_turn_settings)`
  - `session.set_token_info(reconstruction.last_token_count)` → 扩展为完整 `TokenUsageInfo`
  - `session.set_mcp_tool_selection(extract_mcp_tool_selection_from_rollout(...))`
- 前提：第 3 项完成后 Session 才有这些 setter
- 改动文件：`codex.rs`、`reconstruction.rs`（提取 MCP 工具选择）
- 工作量：**中**

### 6. Fork 独立后处理

- 现状：Fork 和 Resume 共用 `Option<ResumedHistory>` 路径
- 目标：
  - 引入 `InitialHistory` 三态枚举（`New / Resumed / Forked`）
  - Fork 分支：截断历史（`truncate_before_nth_user_message`）、持久化源 rollout、追加 initial context
  - `thread_fork` 命令接受 `nth_user_message` 参数
- 改动文件：`codex.rs`、`commands.rs`、`recorder.rs`（或新建 `initial_history.rs`）
- 工作量：**中**

## 第四层：边缘修复

### 7. 正向重放去除 UserMessage/AgentMessage 双重记录

- 现状：同时处理 `ResponseItem` 和 `EventMsg::UserMessage/AgentMessage`，可能重复
- 目标：只通过 `ResponseItem` 路径记录，移除 `EventMsg::UserMessage/AgentMessage` 分支
- 工作量：**小**

### 8. Legacy compaction 精确重建

- 现状：用文本摘要替代
- 目标：实现 `build_compacted_history`（从 codex-main 移植 `collect_user_messages` + `build_compacted_history`）
- 工作量：**中** — 需要移植 `compact.rs` 中的相关函数，且依赖 `event_mapping::parse_turn_item` 等辅助函数
- 优先级：**低** — legacy compaction 只影响旧格式 rollout，新 rollout 都有 `replacement_history`

## 工作量总结

| 项 | 工作量 | 依赖 | 优先级 |
|----|--------|------|--------|
| 1. ContextManager 加字段 | 小 (30min) | 无 | P0 |
| 2. record_items 支持 truncation | 中 (1h) | 无 | P0 |
| 3. Session 状态统一 | 大 (2-3h) | #1 | P0 |
| 4. 重建算法加 TurnContext | 小 (30min) | #2 | P1 |
| 5. Session 状态完整注入 | 中 (1h) | #3 | P1 |
| 6. Fork 独立后处理 + InitialHistory 枚举 | 中 (1.5h) | #5 | P1 |
| 7. 去除双重记录 | 小 (15min) | 无 | P1 |
| 8. Legacy compaction 精确重建 | 中 (1.5h) | 无 | P2 |

**总计约 7-9 小时工作量。** 核心瓶颈是第 3 项（Session 状态统一），它影响所有历史读写路径，需要谨慎迁移。

## 建议执行顺序

```
1 → 2 → 3 → 7 → 4 → 5 → 6 → 8
```
