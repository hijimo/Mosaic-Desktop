# AI 对话能力差异补全任务清单

> 源项目: codex-main/codex-rs/core/src/
> Mosaic: src-tauri/src/
> 目标: 补全 WebSocket 传输、多 Provider、Auth 管理、模型 Fallback
> 测试: 622 → 722 个 (全部通过)

## Task 1: WebSocket 传输 ✅

### 1.1 provider/mod.rs — WebSocket URL 支持
- [x] `websocket_url_for_path()` 已存在
- [x] `supports_websockets` 字段已存在

### 1.2 client.rs — WebSocket 连接管理
- [x] 添加 `ResponsesWebsocketVersion` 枚举 (V1/V2)
- [x] 添加 `ws_version` / `ws_disabled` 到 `ModelClientState`
- [x] 实现 `stream_via_ws()` — 通过 WebSocket 发送请求并接收流式响应
- [x] 实现 `parse_ws_event()` — WebSocket 事件解析 (与 SSE 共享格式)
- [x] `ModelClientSession::stream()` 中根据 provider 配置选择 SSE 或 WebSocket
- [x] WebSocket 连接失败后自动 fallback 到 SSE (`disable_ws()`)
- [x] `ws_available()` / `disable_ws()` 方法

### 1.3 Cargo.toml — 添加 WebSocket 依赖
- [x] 添加 `tokio-tungstenite` 0.26 (native-tls)

### 1.4 测试
- [x] WebSocket URL scheme 转换测试 (已有)
- [x] `ws_version_from_provider` 返回 V1/None 测试
- [x] `ws_available` 尊重 provider 和 disable 状态测试
- [x] `parse_ws_event` 各事件类型测试 (delta/completed/error/unknown)

## Task 2: 多 Provider 支持 ✅

### 2.1 provider/mod.rs — 扩展 Provider 注册
- [x] `built_in_providers()` 已有 openai/ollama/lmstudio
- [x] 新增 ChatGPT provider (`create_chatgpt()`)
- [x] 添加 `ProviderRegistry` 结构体 — 管理多 provider 生命周期
- [x] 实现 `select()` — 根据 ID 选择 provider
- [x] 实现 `with_user_providers()` — 用户覆盖内置 provider
- [x] 实现 `register()` — 运行时注册自定义 provider
- [x] 实现 `set_default()` — 设置默认 provider

### 2.2 测试
- [x] Provider 注册和查找测试
- [x] 用户自定义 provider 覆盖测试
- [x] ChatGPT provider 构造测试
- [x] ProviderRegistry select/default/register 测试
- [x] 未知 provider 返回错误测试

## Task 3: Auth 管理 ✅

### 3.1 新建 auth 模块
- [x] 创建 `src-tauri/src/auth/mod.rs` — AuthMode 枚举 (ApiKey/Chatgpt)
- [x] 创建 `src-tauri/src/auth/storage.rs` — AuthDotJson + FileAuthStorage + MemoryAuthStorage
- [x] 实现 `AuthManager` — 管理认证状态、token 缓存
- [x] 实现 `initialize()` — 从 auth.json 或环境变量加载凭证
- [x] 实现 `refresh_token()` — ChatGPT token 刷新逻辑
- [x] 实现 `UnauthorizedRecovery` — 401 响应后的恢复策略
- [x] `CodexAuth` 枚举 (ApiKey/Chatgpt) + `bearer_token()` 方法

### 3.2 lib.rs 集成
- [x] 注册 `auth` 模块到 lib.rs

### 3.3 测试
- [x] AuthDotJson 序列化/反序列化测试 (ApiKey + Chatgpt)
- [x] AuthDotJson 跳过 None 字段测试
- [x] MemoryAuthStorage CRUD 测试
- [x] FileAuthStorage 文件读写测试
- [x] ApiKey 从环境变量加载测试
- [x] Auth 从 storage 加载测试
- [x] ChatGPT auth 从 storage 加载测试
- [x] UnauthorizedRecovery 策略测试 (ChatGPT vs ApiKey)
- [x] 无 auth 返回错误测试
- [x] AuthMode 序列化测试

## Task 4: 模型 Fallback ✅

### 4.1 client.rs — Fallback 逻辑
- [x] 添加 `ModelFallbackConfig` 结构体 (primary_model, fallback_models)
- [x] 实现 `is_fallback_eligible()` — 判断错误是否触发 fallback
- [x] `ModelClientSession::stream()` 中集成 fallback 逻辑
- [x] `ModelClient::with_fallback()` 构造方法

### 4.2 测试
- [x] `is_fallback_eligible` 检测 model_not_found 测试
- [x] `is_fallback_eligible` 检测 capacity/overloaded 测试
- [x] `is_fallback_eligible` 检测 rate limit 测试
- [x] `is_fallback_eligible` 拒绝 auth 错误测试
- [x] `ModelFallbackConfig` 构造测试

## 架构说明

### WebSocket 传输
源项目使用 `codex-api` crate 的 `ResponsesWebsocketClient` 和 `ResponsesWebsocketConnection`。
Mosaic 直接使用 `tokio-tungstenite` 实现 WebSocket 连接，不依赖外部 API crate。
事件解析逻辑 (`parse_ws_event`) 与 SSE 共享相同的 JSON 格式。

### Auth 管理
源项目的 `auth.rs` (64KB) 包含完整的 OAuth 流程、keyring 集成、ChatGPT 登录。
Mosaic 实现了核心的 AuthManager + FileAuthStorage，支持 API Key 和 ChatGPT token 两种模式。
Token 刷新使用标准 OAuth2 refresh_token grant。

### 多 Provider
源项目通过 `model_provider_info.rs` + `config/types.rs` 管理 provider。
Mosaic 新增 `ProviderRegistry` 统一管理，支持内置 + 用户自定义 + 运行时注册。

### 模型 Fallback
源项目在 `codex.rs` 的 turn 循环中实现 fallback。
Mosaic 在 `ModelClientSession::stream()` 中集成，按 primary → fallback 顺序尝试。
