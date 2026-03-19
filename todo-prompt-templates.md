# Prompt 模板体系实施清单 — 已完成 ✅

## 任务概述
参照 codex-main 的 `codex-rs/core/templates/` 和 `custom_prompts.rs`，为 Mosaic 补齐完整的 Prompt 模板体系。

## 任务清单

### 1. 创建模板文件 (templates/) — ✅ 全部完成
- [x] `templates/compact/prompt.md`
- [x] `templates/compact/summary_prefix.md`
- [x] `templates/collaboration_mode/default.md`
- [x] `templates/collaboration_mode/pair_programming.md`
- [x] `templates/collaboration_mode/execute.md`
- [x] `templates/collaboration_mode/plan.md`
- [x] `templates/agents/orchestrator.md`
- [x] `templates/search_tool/tool_description.md`
- [x] `templates/collab/experimental_prompt.md`
- [x] `templates/personalities/pragmatic.md`
- [x] `templates/personalities/friendly.md`
- [x] `templates/model_instructions/instructions_template.md`
- [x] `templates/review/exit_success.xml`
- [x] `templates/review/exit_interrupted.xml`
- [x] `templates/review/history_message_completed.md`
- [x] `templates/review/history_message_interrupted.md`
- [x] `templates/memories/stage_one_system.md` (已有)
- [x] `templates/memories/stage_one_input.md`
- [x] `templates/memories/read_path.md`
- [x] `templates/memories/consolidation.md`
- [x] `templates/tools/presentation_artifact.md`

### 2. 创建 custom_prompts.rs 模块 — ✅ 全部完成
- [x] 实现 `CustomPrompt` 结构体
- [x] 实现 `default_prompts_dir()` 函数
- [x] 实现 `discover_prompts_in()` 函数
- [x] 实现 `discover_prompts_in_excluding()` 函数
- [x] 实现 `parse_frontmatter()` 函数
- [x] 7 个单元测试全部通过

### 3. 集成到核心引擎 — ✅ 全部完成
- [x] 在 `core/mod.rs` 中注册 `custom_prompts` 模块
- [x] 在 `codex.rs` 的 `ListCustomPrompts` 处理中接入真实实现

### 4. 编译验证 — ✅ 全部通过
- [x] `cargo check` 通过 (无新错误)
- [x] 7 个单元测试全部通过
