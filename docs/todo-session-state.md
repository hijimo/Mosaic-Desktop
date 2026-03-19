# 会话状态管理重构 — TODO

## 目标
将 `core/session.rs` 单文件拆分为 `core/state/` 模块（mod.rs + session.rs + service.rs + turn.rs），对齐 Codex 架构。

## 任务清单

- [x] 1. 添加 `indexmap` 依赖到 Cargo.toml
- [x] 2. 创建 `core/state/turn.rs` — ActiveTurn、RunningTask、TurnState
- [x] 3. 创建 `core/state/service.rs` — SessionServices（适配 Mosaic 已有组件）
- [x] 4. 创建 `core/state/session.rs` — SessionState（增强版会话状态）
- [x] 5. 创建 `core/state/mod.rs` — 模块声明和 re-export
- [x] 6. 重构 `core/session.rs` — SessionState → SessionInternalState，引用更新
- [x] 7. 更新 `core/mod.rs` — 添加 state 模块声明，更新 re-export
- [x] 8. 编译验证 `cargo check` — ✅ 通过
- [x] 9. 运行测试 `cargo test` — ✅ 826 passed, 0 failed
