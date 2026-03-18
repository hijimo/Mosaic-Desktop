# 模块定义总览

Mosaic Desktop 是一个基于 Tauri v2 + React 的 AI 编程助手桌面应用。后端为 Rust，前端为 TypeScript/React。

## 顶层模块树

```
src-tauri/src/
├── main.rs                    # 桌面入口
├── lib.rs                     # Tauri 插件注册 & Codex 引擎启动
├── commands.rs                # Tauri IPC 命令 (submit_op, poll_events, get_config 等)
│
├── protocol/                  # 通信协议层 — 前后端共享的类型契约
│   ├── types.rs               # 核心领域类型 (SandboxPolicy, UserInput, ResponseInputItem 等)
│   ├── event.rs               # 后端→前端事件 (EventMsg 枚举, 60+ 事件类型)
│   ├── submission.rs          # 前端→后端操作 (Op 枚举, 30+ 操作类型)
│   ├── error.rs               # 统一错误类型 (CodexError, ErrorCode)
│   └── thread_id.rs           # 线程 ID 类型
│
├── core/                      # 核心引擎 — 所有业务逻辑
│   ├── codex.rs               # Codex 主引擎 — 事件循环、turn 调度、agentic loop
│   ├── client.rs              # OpenAI Responses API 客户端 (SSE/WebSocket 流式)
│   ├── session.rs             # 会话状态管理 (history, model info, turn context)
│   ├── agent/                 # 多 Agent 系统
│   │   ├── control.rs         # AgentControl — 生命周期管理、spawn/close/wait
│   │   ├── guards.rs          # RAII 资源守卫 (SpawnSlotGuard, SpawnReservation)
│   │   ├── role.rs            # Agent 角色配置 (AgentRoleConfig)
│   │   └── status.rs          # Agent 状态机辅助函数
│   ├── tools/                 # 工具系统
│   │   ├── mod.rs             # ToolHandler trait, ToolRegistry, ToolKind 枚举
│   │   ├── router.rs          # ToolRouter — 三级路由 (builtin → MCP → dynamic)
│   │   ├── handlers/          # 具体工具实现
│   │   │   ├── shell.rs       # Shell 命令执行
│   │   │   ├── read_file.rs   # 文件读取
│   │   │   ├── list_dir.rs    # 目录列表
│   │   │   ├── grep_files.rs  # 文件搜索
│   │   │   ├── apply_patch.rs # Unified diff 补丁应用
│   │   │   ├── unified_exec.rs# 统一执行引擎 handler
│   │   │   ├── mcp.rs         # MCP 工具代理
│   │   │   ├── dynamic.rs     # 动态工具处理
│   │   │   ├── multi_agents.rs# 多 Agent 协作工具
│   │   │   ├── agent_jobs.rs  # Agent 批量任务
│   │   │   ├── js_repl.rs     # JavaScript REPL
│   │   │   ├── plan.rs        # 计划工具
│   │   │   ├── view_image.rs  # 图片查看
│   │   │   └── runtimes/      # 底层运行时 (shell, apply_patch, unified_exec)
│   │   ├── orchestrator.rs    # 工具编排器
│   │   ├── parallel.rs        # 并行工具执行
│   │   ├── sandboxing.rs      # 沙箱策略执行
│   │   ├── network_approval.rs# 网络访问审批
│   │   ├── context.rs         # 工具执行上下文
│   │   ├── events.rs          # 工具事件发射
│   │   └── spec.rs            # 工具规格定义
│   ├── mcp_client/            # MCP 客户端
│   │   ├── connection_manager.rs # MCP 服务器连接池
│   │   ├── tool_call.rs       # MCP 工具调用
│   │   ├── auth.rs            # MCP OAuth 认证
│   │   └── skill_dependencies.rs # Skill 依赖解析
│   ├── mcp_server.rs          # MCP 服务端 (暴露 Mosaic 能力给外部)
│   ├── unified_exec/          # 统一执行引擎 (PTY/Pipe)
│   │   ├── process.rs         # UnifiedExecProcess — PTY 进程抽象
│   │   ├── process_manager.rs # 进程生命周期管理
│   │   ├── async_watcher.rs   # 异步输出流监听
│   │   ├── head_tail_buffer.rs# 输出截断缓冲
│   │   └── errors.rs          # 执行错误类型
│   ├── exec_policy/           # 命令执行策略
│   │   ├── manager.rs         # 策略管理器
│   │   ├── bash.rs            # Bash 命令解析
│   │   └── heuristics.rs      # 安全启发式规则
│   ├── context_manager/       # 上下文窗口管理
│   │   ├── history.rs         # 历史记录管理
│   │   ├── normalize.rs       # 输入规范化
│   │   └── updates.rs         # 上下文更新
│   ├── compact.rs             # 上下文压缩 (本地/远程)
│   ├── truncation.rs          # Token 截断策略
│   ├── skills/                # Skills 系统
│   │   ├── loader.rs          # Skill 加载器
│   │   ├── manager.rs         # SkillsManager
│   │   ├── injection.rs       # Skill 注入到 prompt
│   │   ├── permissions.rs     # Skill 权限控制
│   │   ├── remote.rs          # 远程 Skill 下载
│   │   └── model.rs           # Skill 数据模型
│   ├── memories/              # 记忆系统 (跨会话持久化)
│   │   ├── phase1.rs          # 阶段 1: 原始记忆收集
│   │   ├── phase2.rs          # 阶段 2: 记忆摘要
│   │   ├── storage.rs         # 记忆存储
│   │   └── prompts.rs         # 记忆相关 prompt
│   ├── models_manager/        # 模型管理
│   │   ├── manager.rs         # 模型列表刷新
│   │   ├── model_info.rs      # 模型描述符
│   │   └── cache.rs           # 模型缓存
│   ├── rollout/               # 会话录制 & 回放
│   │   ├── recorder.rs        # 录制器
│   │   ├── policy.rs          # 录制策略
│   │   ├── session_index.rs   # 会话索引
│   │   └── truncation.rs      # 录制截断
│   ├── tasks/                 # 后台任务
│   │   ├── regular.rs         # 常规任务
│   │   ├── compact.rs         # 压缩任务
│   │   ├── review.rs          # 代码审查任务
│   │   └── undo.rs            # 撤销任务
│   ├── hooks.rs               # Hook 系统 (事件钩子)
│   ├── patch.rs               # Patch 应用器
│   ├── realtime.rs            # 实时对话 (语音/文本)
│   ├── git_info.rs            # Git 仓库信息收集
│   ├── project_doc.rs         # 项目文档发现
│   ├── file_watcher.rs        # 文件变更监听
│   ├── message_history.rs     # 消息历史持久化
│   ├── thread_manager.rs      # 线程/会话管理
│   ├── analytics_client.rs    # 分析事件上报
│   ├── shell.rs               # Shell 类型检测
│   ├── shell_snapshot.rs      # Shell 环境快照
│   ├── state_db.rs            # 状态数据库
│   ├── text_encoding.rs       # 文本编码处理
│   ├── turn_diff_tracker.rs   # Turn 级别 diff 追踪
│   ├── network_policy_decision.rs # 网络策略决策
│   ├── features/              # Feature flags
│   ├── external_agent_config.rs   # 外部 Agent 配置迁移
│   └── seatbelt.rs            # macOS 沙箱 (Seatbelt)
│
├── config/                    # 配置系统
│   ├── schema.rs              # 配置 schema 定义
│   ├── layer_stack.rs         # 分层配置 (Default → User → Project → Session)
│   ├── merge.rs               # 配置合并
│   ├── edit.rs                # 配置编辑
│   ├── permissions.rs         # 权限配置
│   ├── toml_types.rs          # TOML 类型映射
│   └── diagnostics.rs         # 配置诊断
│
├── auth/                      # 认证
│   ├── mod.rs                 # 认证流程
│   └── storage.rs             # Token 存储
│
├── secrets/                   # 密钥管理
│   ├── manager.rs             # 密钥管理器
│   ├── backend.rs             # 存储后端
│   └── sanitizer.rs           # 输出脱敏
│
├── state/                     # 持久化状态
│   ├── db.rs                  # SQLite 数据库
│   ├── memories_db.rs         # 记忆数据库
│   ├── memory.rs              # 内存状态
│   └── rollout.rs             # Rollout 状态
│
├── pty/                       # PTY 抽象层
│   ├── pty.rs                 # PTY 实现
│   ├── pipe.rs                # Pipe 实现
│   ├── process.rs             # 进程管理
│   └── process_group.rs       # 进程组
│
├── exec/                      # 执行抽象
│   ├── mod.rs                 # 执行接口
│   └── sandbox.rs             # 沙箱执行
│
├── execpolicy/                # 执行策略引擎
│   ├── parser.rs              # 策略文件解析
│   ├── prefix_rule.rs         # 前缀匹配规则
│   ├── network_rule.rs        # 网络规则
│   ├── amend.rs               # 策略修正
│   └── error.rs               # 策略错误
│
├── netproxy/                  # 网络代理
│   └── proxy.rs               # SOCKS5/HTTP 代理
│
├── file_search/               # 文件搜索 (BM25)
├── shell_command/             # Shell 命令解析
├── shell_escalation/          # Shell 权限提升
├── provider/                  # AI Provider 抽象
└── responses_api_proxy/       # Responses API 代理
```

## 前端 (src/)

```
src/
├── main.tsx          # React 入口
├── App.tsx           # 主应用组件
├── App.css           # 样式
├── assets/           # 静态资源
└── vite-env.d.ts     # Vite 类型声明
```

> 前端目前为 Tauri 模板状态，核心逻辑集中在 Rust 后端。
