# 辅助模块说明

## 配置系统 (`config/`)

分层配置架构，优先级从低到高：Default → User → Project → Session。

| 模块 | 说明 |
|------|------|
| `ConfigLayerStack` | 分层配置栈，支持动态添加/合并 |
| `schema.rs` | 配置 schema 定义 |
| `merge.rs` | 多层配置合并逻辑 |
| `toml_types.rs` | TOML 到 Rust 类型映射 |
| `permissions.rs` | 权限相关配置 |
| `diagnostics.rs` | 配置诊断和校验 |

## 认证 (`auth/`)

管理 API 认证 token 的获取和存储。支持 API Key 和 ChatGPT 登录两种方式。

## 密钥管理 (`secrets/`)

| 模块 | 说明 |
|------|------|
| `manager.rs` | 密钥管理器 — 统一的密钥获取接口 |
| `backend.rs` | 存储后端 (keychain / 文件) |
| `sanitizer.rs` | 输出脱敏 — 防止密钥泄露到日志和输出 |

## 持久化状态 (`state/`)

| 模块 | 说明 |
|------|------|
| `db.rs` | SQLite 数据库封装 |
| `memories_db.rs` | 记忆专用数据库 |
| `memory.rs` | 内存状态管理 |
| `rollout.rs` | Rollout 状态持久化 |

## Skills 系统 (`core/skills/`)

可扩展的技能系统，支持本地和远程 Skill。

| 模块 | 说明 |
|------|------|
| `loader.rs` | 从文件系统加载 Skill |
| `manager.rs` | `SkillsManager` — Skill 生命周期管理 |
| `injection.rs` | 将 Skill 指令注入到 system prompt |
| `permissions.rs` | Skill 权限控制 (文件访问、网络等) |
| `remote.rs` | 远程 Skill 下载和安装 |
| `model.rs` | Skill 数据模型 (`SkillMetadata`, `SkillInterface`) |

## 记忆系统 (`core/memories/`)

跨会话的持久化记忆，分两阶段处理：

1. **Phase 1** — 收集原始记忆 (用户偏好、项目上下文)
2. **Phase 2** — 摘要和压缩记忆

| 模块 | 说明 |
|------|------|
| `storage.rs` | 记忆文件存储 |
| `prompts.rs` | 记忆相关的 prompt 模板 |
| `start.rs` | 启动时加载记忆 |

## 上下文管理 (`core/context_manager/`)

管理模型上下文窗口，防止超出 token 限制。

| 模块 | 说明 |
|------|------|
| `history.rs` | 历史记录管理和截断 |
| `normalize.rs` | 输入规范化 |
| `updates.rs` | 上下文增量更新 |

配合 `compact.rs`（上下文压缩）和 `truncation.rs`（截断策略）使用。

## 模型管理 (`core/models_manager/`)

| 模块 | 说明 |
|------|------|
| `manager.rs` | `ModelsManager` — 模型列表刷新和选择 |
| `model_info.rs` | `ModelDescriptor` — 模型元信息 |
| `cache.rs` | 模型列表缓存 |

## Rollout 系统 (`core/rollout/`)

会话录制和回放，用于调试和审计。

| 模块 | 说明 |
|------|------|
| `recorder.rs` | `RolloutRecorder` — 录制器 |
| `policy.rs` | 录制策略 (何时录制) |
| `session_index.rs` | 会话索引 |
| `truncation.rs` | 录制数据截断 |

## 其他辅助模块

| 模块 | 说明 |
|------|------|
| `hooks.rs` | Hook 系统 — 事件钩子注册和触发 |
| `git_info.rs` | Git 仓库信息收集 (分支、commit、diff) |
| `project_doc.rs` | 项目文档发现 (AGENTS.md 等) |
| `file_watcher.rs` | 文件变更监听 |
| `realtime.rs` | 实时对话 (语音/文本) |
| `patch.rs` | Unified diff 补丁应用器 |
| `seatbelt.rs` | macOS Seatbelt 沙箱 |
| `analytics_client.rs` | 分析事件上报 |
| `netproxy/` | SOCKS5/HTTP 网络代理 |
| `file_search/` | BM25 文件搜索 |
| `shell_escalation/` | Shell 权限提升 |
