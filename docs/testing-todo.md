# Tauri 端测试任务清单

> 创建时间：2026-03-25
> lib 测试基线：930 | 独立集成测试：26
> 原则：测试代码独立于业务代码，放在 `tests/` 目录

---

## 一、Mock API 集成测试（tests/e2e_mock_api.rs）

| ID | 测试用例 | 状态 |
|----|---------|------|
| V-01 | 纯文本响应 → 完整 v2 事件序列 | ✅ |
| V-02 | 工具调用 → McpToolCallBegin/End + 第二轮文本 | ✅ |
| V-03 | 推理 + 文本 → Reasoning + AgentMessage items | ❌ |
| V-04 | 空响应 → 无 AgentMessage | ✅ |
| V-05 | API 错误 → Error + TurnComplete | ✅ |
| V-06 | Token 用量 → TokenCount 正确数值 | ✅ |
| V-07 | 无 legacy 事件 | ✅ |

## 二、边界测试（tests/boundary_tests.rs）

| ID | 测试用例 | 状态 |
|----|---------|------|
| B-01 | UserTurn 空 items | ✅ |
| B-02 | 多轮 item_id 唯一性 | ✅ |
| B-03 | ItemStarted/Completed id 一致性 | ✅ |
| B-04 | shell handler 空命令 | ✅ |
| B-05 | shell handler 超时 | ✅ |
| B-06 | list_dir 不存在路径 | ✅ |
| B-07 | read_file 不存在文件 | ✅ |
| B-08 | event_mapping 空 role | ✅ |
| B-09 | event_mapping FunctionCall → None | ✅ |

## 三、持久化策略测试（tests/rollout_policy_tests.rs）

| ID | 测试用例 | 状态 |
|----|---------|------|
| P-01 | streaming delta 不持久化 | ✅ |
| P-02 | ItemStarted/Completed 不持久化 | ✅ |
| P-03 | ResponseItem 始终持久化 | ✅ |
| P-04 | 核心事件 Limited 模式持久化 | ✅ |
| P-05 | Extended 事件仅 Extended 模式 | ✅ |

## 四、工具 Handler 测试（tests/tool_handler_tests.rs）

| ID | 测试用例 | 状态 |
|----|---------|------|
| T-01 | Session 注册 5 个内置工具 | ✅ |
| T-02 | shell echo 正常执行 | ✅ |
| T-03 | list_dir 列出当前目录 | ✅ |
| T-04 | tool_specs 包含必需工具 | ✅ |
| T-05 | apply_patch 无效 patch | ✅ |
| T-06 | grep_files 缺失 pattern | ✅ |

## 五、Fuzz 测试（fuzz/）

> 运行前需临时修改 `Cargo.toml` 中 `crate-type = ["rlib"]`，运行后恢复。
> 详见 `fuzz/Cargo.toml` 中的 NOTE。

| ID | 目标 | 状态 |
|----|------|------|
| F-01 | EventMsg roundtrip | ✅ |
| F-02 | Op roundtrip | ✅ |
| F-03 | ResponseItem roundtrip | ✅ |
| F-04 | TurnItem roundtrip | ✅ |

---

## 进度

| 阶段 | 任务数 | 完成 | 剩余 |
|------|--------|------|------|
| Mock API 测试 | 7 | 7 | 0 |
| 边界测试 | 9 | 9 | 0 |
| 持久化策略测试 | 5 | 5 | 0 |
| 工具 Handler 测试 | 6 | 6 | 0 |
| Fuzz 测试 | 4 | 4 | 0 |
| **总计** | **31** | **31** | **0** |
