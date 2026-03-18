# Unified Exec 补全任务清单

## 全部完成 ✅ — 622 tests passed, 0 failed

### Phase 1: 基础设施补全
- [x] 1.1 truncation.rs 添加 `approx_token_count` + `formatted_truncate_text`
- [x] 1.2 mod.rs 添加 `UnifiedExecProcessManager` (带 process_store + max_write_stdin_yield_time_ms)
- [x] 1.3 mod.rs 添加 `apply_exec_env` 辅助函数
- [x] 1.4 ProcessEntry 增加 `process: Arc<UnifiedExecProcess>` + `call_id` 字段

### Phase 2: process_manager.rs 重写核心方法
- [x] 2.1 `open_session_with_exec_env` — PTY/pipe spawn → UnifiedExecProcess
- [x] 2.2 `collect_output_until_deadline` — deadline 轮询 + post-exit grace
- [x] 2.3 `exec_command` — 完整流程: spawn → stream → collect → store/release
- [x] 2.4 `write_stdin` — 获取进程 handles → 发送 input → 收集 output → 刷新状态
- [x] 2.5 `allocate_process_id` / `release_process_id`
- [x] 2.6 `store_process` + `prune_processes_if_needed`
- [x] 2.7 `terminate_all_processes`

### Phase 3: async_watcher.rs 流式事件
- [x] 3.1 `start_streaming_output` — 后台 task 读 PTY → HeadTailBuffer + UTF-8 split
- [x] 3.2 `spawn_exit_watcher` — 监听退出 + output drain

### Phase 4: handler 重写
- [x] 4.1 handlers/unified_exec.rs 接入 UnifiedExecProcessManager (exec_command + write_stdin)
- [x] 4.2 runtimes/unified_exec.rs 接入真实 PTY spawn
- [x] 4.3 format_response 函数 (匹配 codex-main 格式)

### Phase 5: 集成 + 测试
- [x] 5.1 编译通过 (0 errors, warnings only)
- [x] 5.2 全量测试通过 (622 passed)
- [x] 5.3 新增 PTY 集成测试 (5 个: exec_output, nonzero_exit, write_stdin, unknown_process, terminate_all)
