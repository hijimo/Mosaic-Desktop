# Mosaic-Desktop vs codex-main：Thread Resume/Fork 方案对比

> 最后更新：2026-03-24（Infrastructure Gap Fix 完成后）

## 1. 架构层面

| 维度 | codex-main | Mosaic-Desktop | 状态 |
|------|-----------|----------------|------|
| **历史类型枚举** | `InitialHistory::New / Resumed / Forked` 三态枚举，定义在 `codex_protocol::protocol` | `InitialHistory::New / Resumed / Forked` 三态枚举，定义在 `core/initial_history.rs` | ✅ 已对齐 |
| **重建函数归属** | `Session::reconstruct_history_from_rollout()` — Session 方法，接收 `&TurnContext` | 独立 `pub fn reconstruct_history_from_rollout()` — 纯函数，接收 `ItemTruncationPolicy` | ⚠️ 设计差异（见下文） |
| **历史注入方式** | `record_into_history()` → `state.record_items()` → `ContextManager::record_items(policy)` | `session.add_to_history()` → `state.history.record_items()` → `ContextManager::record_items(default_policy)` | ✅ 已对齐（均经过 truncation） |
| **Session 状态存储** | `SessionState` 内含 `ContextManager` + `previous_turn_settings` + `mcp_tool_selection` | `SessionInternalState` 内含 `ContextManager` + `previous_turn_settings` + `active_mcp_tool_selection` | ✅ 已对齐 |
| **线程管理** | `ThreadManager` 统一管理所有线程的生命周期 | `commands.rs` 中各命令直接操作 `AppState.threads` HashMap | ℹ️ 架构差异，不影响 resume/fork 正确性 |

## 2. Resume 流程对比

| 步骤 | codex-main | Mosaic-Desktop | 状态 |
|------|-----------|----------------|------|
| 加载 rollout | `RolloutRecorder::get_rollout_history()` → `InitialHistory::Resumed` | 同样调用 `get_rollout_history()` → `InitialHistory::Resumed` | ✅ 一致 |
| 重建历史 | `reconstruct_history_from_rollout(&turn_context, &rollout_items)` 接收 `TurnContext`（含 `truncation_policy`） | `reconstruct_history_from_rollout(&items, ItemTruncationPolicy::default())` 接收独立 policy | ✅ 已对齐（均做 truncation） |
| 注入 reference_context_item | `state.set_reference_context_item(reconstructed.reference_context_item)` | `session.set_reference_context_item(reconstruction.reference_context_item)` | ✅ 已对齐 |
| 注入 previous_turn_settings | `self.set_previous_turn_settings(settings)` | `session.set_previous_turn_settings(reconstruction.previous_turn_settings)` | ✅ 已对齐 |
| 恢复 token 信息 | `state.set_token_info(Self::last_token_info_from_rollout(&items))` — 完整 `TokenUsageInfo` | `session.set_token_info(reconstruction.last_token_info)` — 完整 `TokenUsageInfo` | ✅ 已对齐 |
| 恢复 MCP 工具选择 | `Self::extract_mcp_tool_selection_from_rollout()` → `set_mcp_tool_selection()` | `extract_mcp_tool_selection_from_rollout()` → `session.set_mcp_tool_selection()` | ✅ 已对齐 |
| 模型不一致警告 | ✅ 有 | ✅ 有 | ✅ 一致 |
| flush rollout | `self.flush_rollout()` 确保恢复后立即持久化 | 无对应操作（Mosaic 的 recorder 在 event bridge 中处理） | ℹ️ 低影响差异 |
| clear_mcp_tool_selection | `self.clear_mcp_tool_selection()` 在 `record_initial_history` 开头调用 | 无对应操作 | ⚠️ 残留差距 |

## 3. Fork 流程对比

| 步骤 | codex-main | Mosaic-Desktop | 状态 |
|------|-----------|----------------|------|
| 截断历史 | `truncate_before_nth_user_message(history, nth)` | `truncate_before_nth_user_message(&resumed.history, nth)` | ✅ 已对齐 |
| 参数传递 | `nth_user_message: usize` | `nth_user_message: Option<usize>`（默认 `usize::MAX` = 不截断） | ✅ 已对齐 |
| 重建 + 状态注入 | 与 Resume 共用 `reconstruct_history_from_rollout` + 全部 setter | 与 Resume 共用同一路径（`run_with_history` 中统一处理 Resumed/Forked） | ✅ 已对齐 |
| 追加 initial context | `build_initial_context()` → `record_conversation_items()` — Fork 后追加新 session 的初始上下文 | **无对应操作** | ⚠️ 残留差距 |
| 源 rollout 持久化 | `persist_rollout_items(&rollout_items)` — 将源线程 rollout 写入新线程 | 无对应操作 | ⚠️ 残留差距 |
| ensure_rollout_materialized | `self.ensure_rollout_materialized()` — 确保 Fork 后立即文件化 | 无对应操作 | ⚠️ 残留差距 |
| set_reference_context_item 覆写 | Fork 后用 `turn_context.to_turn_context_item()` 覆写 reference | 无对应操作（保留重建时提取的 reference） | ⚠️ 残留差距 |

## 4. 重建算法对比

| 方面 | codex-main | Mosaic-Desktop | 状态 |
|------|-----------|----------------|------|
| 反向扫描 — 结构 | `ActiveReplaySegment` + `finalize_active_segment` | 相同结构 | ✅ 一致 |
| 反向扫描 — user turn 检测 | 仅 `EventMsg::UserMessage` 标记 `counts_as_user_turn` | `EventMsg::UserMessage` + `ResponseItem::Message{role:"user"}` 双重检测 | ✅ 已增强（兼容新旧格式） |
| 反向扫描 — token info | 不在反向扫描中提取（单独 `last_token_info_from_rollout` 函数） | 在反向扫描中提取 `last_token_info`（集成到 `RolloutReconstruction`） | ✅ 已对齐（方式不同但结果等价） |
| 正向重放 — ResponseItem | `history.record_items(iter::once(item), turn_context.truncation_policy)` | `history.record_items_with_policy(iter::once(item.into()), item_truncation)` | ✅ 已对齐 |
| 正向重放 — Compacted (有 replacement) | `history.replace(replacement_history.clone())` | `history.replace(replacement.clone())` | ✅ 一致 |
| 正向重放 — Compacted (legacy) | `compact::build_compacted_history(Vec::new(), &user_messages, &message)` — 使用 `collect_user_messages` + token 预算 + `approx_token_count` | 自行实现：收集 user messages → token 预算（`len/4`）→ 保留近期消息 + summary | ✅ 已对齐（实现略简化但语义等价） |
| 正向重放 — UserMessage/AgentMessage | 不处理（这些已通过 ResponseItem 路径处理） | 不处理（已移除双重记录） | ✅ 已对齐 |
| `saw_legacy_compaction_without_replacement_history` | 追踪并在最后清除 `reference_context_item` | **无此逻辑** | ⚠️ 残留差距 |

## 5. 数据类型对比

| 类型 | codex-main | Mosaic-Desktop | 状态 |
|------|-----------|----------------|------|
| 历史 item 类型 | `ResponseItem`（模型 API 原始类型，含 Reasoning/LocalShellCall/WebSearchCall/Compaction/GhostSnapshot/Other） | `ResponseInputItem`（输入侧类型，含 Message/FunctionCall/FunctionCallOutput/McpToolCallOutput/CustomToolCallOutput） | ℹ️ 设计差异 |
| ContextManager.record_items | 接受 `TruncationPolicy`（`Bytes/Tokens`），对 FunctionCallOutput + CustomToolCallOutput 做截断 | 接受 `ItemTruncationPolicy`（`Bytes/Tokens`），仅对 FunctionCallOutput 做截断 | ⚠️ 微小差距（CustomToolCallOutput 未截断） |
| ContextManager 额外功能 | `for_prompt()` 含 image stripping、`estimate_token_count()` 含 base_instructions、`replace_last_turn_images()`、`get_total_token_usage()` 含 reasoning 估算 | `for_prompt()` 仅做 normalize、`estimate_total_tokens()` 简化版、无 image stripping | ℹ️ 功能差距（非 resume/fork 关键路径） |
| InitialHistory 辅助方法 | `forked_from_id()`、`session_cwd()`、`get_rollout_items()` | 无辅助方法 | ℹ️ 低影响 |

## 6. 已修复的差距（Infrastructure Gap Fix 成果）

以下 6 个关键差距已在第一轮修复中解决：

1. ✅ **Session 状态恢复不完整** — `reference_context_item`、`previous_turn_settings`、`token_info` 现在全部在 `run_with_history` 中注入 Session
2. ✅ **Fork 缺少截断** — `InitialHistory::Forked` 枚举 + `truncate_before_nth_user_message` + `thread_fork` 接受 `nth_user_message` 参数
3. ✅ **MCP 工具选择恢复缺失** — `extract_mcp_tool_selection_from_rollout` 从 rollout 中提取并通过 `session.set_mcp_tool_selection` 注入
4. ✅ **正向重放缺少 truncation policy** — `ItemTruncationPolicy` 类型 + `record_items_with_policy` + 重建算法接受 policy 参数
5. ✅ **Legacy compaction 处理简化** — 保留近期用户消息（token 预算内）+ 摘要，替代原来的纯文本替换
6. ✅ **UserMessage/AgentMessage 双重记录** — 正向重放移除 EventMsg 分支 + `AddToHistory` 发射 `RawResponseItem` + 反向扫描增加 `ResponseItem` 用户消息检测

以下 4 个残留差距已在第二轮修复中解决：

7. ✅ **`saw_legacy_compaction_without_replacement_history`** — 正向重放追踪此标志，legacy compaction 后清除 `reference_context_item`（强制下一轮 full context reinjection）
8. ✅ **Fork 源 rollout 持久化** — `thread_fork` 创建 recorder 后将 forked rollout items 写入新线程的 rollout 文件
9. ✅ **`clear_mcp_tool_selection`** — `run_with_history` 注入历史前先调用 `session.clear_mcp_tool_selection()`
10. ✅ **CustomToolCallOutput truncation** — `truncate_output_with_limit` 增加 `CustomToolCallOutput` 分支

## 7. 残留差距（按优先级排序）

### P1 — 影响正确性

| # | 差距 | 影响 | 状态 |
|---|------|------|------|
| 1 | **Fork 后不追加 initial context** | codex-main 在 Fork 后调用 `build_initial_context()` + `record_conversation_items()` 追加新 session 的系统指令。Mosaic 通过 API `instructions` 参数动态注入，不需要作为 history items | ℹ️ 架构差异，非缺失 |
| 2 | ~~`saw_legacy_compaction_without_replacement_history` 未追踪~~ | ~~codex-main 在遇到 legacy compaction 后清除 `reference_context_item`~~ | ✅ 已修复 — 正向重放追踪标志，legacy compaction 后清除 reference |

### P2 — 影响健壮性

| # | 差距 | 影响 | 状态 |
|---|------|------|------|
| 3 | ~~Fork 源 rollout 未持久化到新线程~~ | ~~codex-main 调用 `persist_rollout_items` 将源线程 rollout 写入新线程~~ | ✅ 已修复 — `thread_fork` 创建 recorder 后写入 forked items |
| 4 | **Fork 后 reference_context_item 未覆写** | codex-main Fork 后用当前 `turn_context.to_turn_context_item()` 覆写 reference。Mosaic 通过 API `instructions` 参数动态注入 context，不依赖 reference diff | ℹ️ 架构差异，非缺失 |
| 5 | ~~`clear_mcp_tool_selection` 未在 resume/fork 开头调用~~ | ~~codex-main 在 `record_initial_history` 开头清除 MCP 选择再恢复~~ | ✅ 已修复 — `run_with_history` 注入历史前先 `clear_mcp_tool_selection` |
| 6 | ~~CustomToolCallOutput 未做 truncation~~ | ~~codex-main 的 `process_item` 对 `CustomToolCallOutput` 也做截断~~ | ✅ 已修复 — `truncate_output_with_limit` 增加 `CustomToolCallOutput` 分支 |

### P3 — 功能差异（非关键路径）

| # | 差距 | 说明 |
|---|------|------|
| 7 | **`ensure_rollout_materialized` 缺失** | codex-main Fork 后调用此方法确保 rollout 立即文件化。Mosaic 的 recorder 在 event bridge 中异步处理 |
| 8 | **`flush_rollout` 缺失** | codex-main Resume/Fork 后调用 flush 确保持久化。Mosaic 依赖 event bridge 的自然写入 |
| 9 | **ContextManager 功能差距** | codex-main 有 image stripping、reasoning token 估算、`replace_last_turn_images` 等。Mosaic 的 ContextManager 更简化 |
| 10 | **InitialHistory 辅助方法缺失** | codex-main 有 `forked_from_id()`、`session_cwd()` 等。Mosaic 在 `commands.rs` 中直接处理 |
| 11 | **`user_message_positions` 实现差异** | codex-main 使用 `event_mapping::parse_turn_item` 检测用户消息（可过滤 contextual messages）。Mosaic 直接检查 `role == "user"` |

## 8. 文件变更清单

第一轮修复涉及的文件：

```
src-tauri/src/core/context_manager/history.rs  — ContextManager 扩展（token_info, reference_context_item, ItemTruncationPolicy）
src-tauri/src/core/session.rs                  — SessionInternalState 迁移到 ContextManager + 新 setter/getter
src-tauri/src/core/codex.rs                    — run_with_history 完整状态注入 + extract_mcp_tool_selection
src-tauri/src/core/rollout/reconstruction.rs   — 重建算法 + ItemTruncationPolicy + legacy compaction 精确重建
src-tauri/src/core/initial_history.rs          — InitialHistory 三态枚举（新文件）
src-tauri/src/core/rollout/truncation.rs       — user_message_positions 双格式检测
src-tauri/src/commands.rs                      — thread_fork 接受 nth_user_message + InitialHistory::Forked
src-tauri/src/core/mod.rs                      — 注册 initial_history 模块
```

第二轮修复涉及的文件：

```
src-tauri/src/core/rollout/reconstruction.rs   — saw_legacy_compaction_without_replacement_history 追踪 + 清除 reference
src-tauri/src/core/context_manager/history.rs  — CustomToolCallOutput truncation
src-tauri/src/core/session.rs                  — 新增 clear_mcp_tool_selection()
src-tauri/src/core/codex.rs                    — run_with_history 注入前 clear_mcp_tool_selection
src-tauri/src/commands.rs                      — thread_fork 持久化 forked rollout items 到新 recorder
```
