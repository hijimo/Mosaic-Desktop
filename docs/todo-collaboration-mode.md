# 协作模式实现 — TODO

## 目标
实现 collaboration_mode_presets.rs 和 model_presets.rs，并集成到 ModelsManager。

## 任务清单

- [x] 1. 添加 `CollaborationModeMask` 类型（protocol/types.rs）
- [x] 2. 给 `ModeKind` 添加 `display_name`/`allows_request_user_input`/`is_tui_visible` 方法
- [x] 3. 添加 `TUI_VISIBLE_COLLABORATION_MODES` 常量
- [x] 4. 创建 `models_manager/collaboration_mode_presets.rs`
- [x] 5. 创建 `models_manager/model_presets.rs`
- [x] 6. 更新 `models_manager/mod.rs` 注册新模块
- [x] 7. 更新 `models_manager/manager.rs` 集成协作模式
- [x] 8. 更新 `core/mod.rs` re-export
- [x] 9. 消除 `request_user_input.rs` 中重复的 `ModeKind` 定义
- [x] 10. cargo check — ✅ 通过
- [x] 11. cargo test — ✅ 835 passed, 0 failed
