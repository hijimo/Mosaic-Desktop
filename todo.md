# LLM API 客户端实现任务清单

## 架构说明
Codex 源码中 `codex-client/` 和 `codex-api/` 是两个独立 crate。
Mosaic 是单 crate (Tauri app)，因此将它们作为模块内嵌到 `src-tauri/src/` 下：
- `src/mosaic_client/` — 对应 codex-client (HTTP 传输层)
- `src/mosaic_api/` — 对应 codex-api (API 端点层)

## 任务列表

### Phase 1: mosaic_client 模块 (HTTP 传输层) ✅
- [x] 1.1 创建 `mosaic_client/error.rs` — TransportError, StreamError
- [x] 1.2 创建 `mosaic_client/request.rs` — Request, Response, RequestCompression
- [x] 1.3 创建 `mosaic_client/retry.rs` — RetryPolicy, RetryOn, backoff, run_with_retry
- [x] 1.4 创建 `mosaic_client/transport.rs` — HttpTransport trait, ReqwestTransport
- [x] 1.5 创建 `mosaic_client/sse.rs` — sse_stream helper
- [x] 1.6 创建 `mosaic_client/telemetry.rs` — RequestTelemetry trait
- [x] 1.7 创建 `mosaic_client/mod.rs` — 模块声明和 re-exports

### Phase 2: mosaic_api 模块 (API 端点层) ✅
- [x] 2.1 创建 `mosaic_api/error.rs` — ApiError
- [x] 2.2 创建 `mosaic_api/auth.rs` — AuthProvider trait
- [x] 2.3 创建 `mosaic_api/provider.rs` — Provider, RetryConfig
- [x] 2.4 创建 `mosaic_api/common.rs` — ResponseEvent, ResponsesApiRequest, ResponseStream 等
- [x] 2.5 创建 `mosaic_api/telemetry.rs` — SseTelemetry, run_with_request_telemetry
- [x] 2.6 创建 `mosaic_api/rate_limits.rs` — 速率限制解析
- [x] 2.7 创建 `mosaic_api/requests/` — headers, responses (Compression, attach_item_ids)
- [x] 2.8 创建 `mosaic_api/endpoint/session.rs` — EndpointSession
- [x] 2.9 创建 `mosaic_api/endpoint/responses.rs` — ResponsesClient (SSE 流式)
- [x] 2.10 创建 `mosaic_api/endpoint/models.rs` — ModelsClient
- [x] 2.11 创建 `mosaic_api/endpoint/compact.rs` — CompactClient
- [x] 2.12 创建 `mosaic_api/endpoint/memories.rs` — MemoriesClient
- [x] 2.13 创建 `mosaic_api/sse/responses.rs` — SSE 事件解析 (process_sse, spawn_response_stream)
- [x] 2.14 创建 `mosaic_api/mod.rs` — 模块声明和 re-exports

### Phase 3: 集成与验证 ✅
- [x] 3.1 更新 `Cargo.toml` — 添加缺失依赖 (bytes, http, url)
- [x] 3.2 更新 `src/lib.rs` — 注册 mosaic_client 和 mosaic_api 模块
- [x] 3.3 编译验证 — `cargo check` 通过 ✅
- [x] 3.4 单元测试 — `cargo test` 通过 (748 passed, 0 failed) ✅
