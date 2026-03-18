# Agentic Loop 实施任务清单

## 核心目标
实现 run_turn 中的 agentic tool call loop，使 AI 能够调用 tools 并循环对话。

## 任务清单

### Phase 1: Tool Spec 收集
- [x] 1.1 给 ToolHandler trait 添加 `tool_spec()` 默认方法
- [x] 1.2 给 ToolRegistry 添加 `collect_tool_specs()` 方法
- [x] 1.3 给核心 handlers 实现 `tool_spec()`: shell, read_file, list_dir, grep_files, apply_patch
- [x] 1.4 在 ToolRouter 中添加 `collect_tool_specs()` 聚合 built-in + dynamic tools

### Phase 2: stream_response 传入 tools
- [x] 2.1 修改 `stream_response` 接受 `tools: Option<Vec<Value>>` 参数
- [x] 2.2 当有 tools 时设置 `tool_choice: "auto"`

### Phase 3: Agentic Loop
- [x] 3.1 在 ResponseEvent 中添加 `FunctionCall` 变体
- [x] 3.2 在 SSE 和 WebSocket 解析中识别 function_call 类型的 OutputItemDone
- [x] 3.3 重构 `run_turn` 为循环结构：stream → 检测 function_call → dispatch → 继续
- [x] 3.4 将 function_call 和 function_call_output 加入 history
- [x] 3.5 添加 MAX_TOOL_ROUNDS=32 安全限制

### Phase 4: 测试验证
- [x] 4.1 所有现有测试通过 — 745 passed, 0 failed
- [x] 4.2 codex 模块测试通过 — 22 passed
- [x] 4.3 client 模块测试通过 — 22 passed

## 完成状态: ✅ 全部完成
