# 残留差距修复 TODO

## P1 — 影响正确性
- [x] 1. `saw_legacy_compaction_without_replacement_history` — 正向重放追踪此标志，legacy compaction 后清除 reference_context_item
- [x] 2. Fork 后追加 initial context — 架构差异：Mosaic 通过 API `instructions` 参数动态注入，不需要 history items

## P2 — 影响健壮性
- [x] 3. Fork 源 rollout 持久化 — thread_fork 创建 recorder 后将 forked rollout items 写入新 recorder
- [x] 4. Fork 后 reference_context_item 覆写 — 架构差异：Mosaic 不依赖 reference diff
- [x] 5. `clear_mcp_tool_selection` — run_with_history 注入历史前先清除 MCP 选择
- [x] 6. CustomToolCallOutput truncation — `truncate_output_with_limit` 增加 CustomToolCallOutput 分支

## 验证
- [x] 7. cargo check 零错误
- [x] 8. cargo test --lib 全部通过 (912 passed)
- [x] 9. 更新 diff-analysis.md 文档标记已修复
