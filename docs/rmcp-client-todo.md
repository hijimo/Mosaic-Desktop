# RMCP 客户端实现任务清单

## 目标
参照 Codex `codex-rs/rmcp-client/` 在 Mosaic 中实现 RMCP 客户端模块，支持 streamable HTTP MCP 服务器连接（含 OAuth 认证）。

## 任务清单

- [x] 1. 添加依赖：在 Cargo.toml 中添加 `rmcp` 的 `auth` feature、`oauth2`、`urlencoding`、`webbrowser` 等依赖
- [x] 2. 创建 `src-tauri/src/rmcp_client/` 模块目录结构
- [x] 3. 实现 `rmcp_client/utils.rs` — 工具函数（超时、环境变量、HTTP headers）
- [x] 4. 实现 `rmcp_client/program_resolver.rs` — 平台相关的程序路径解析
- [x] 5. 实现 `rmcp_client/oauth.rs` — OAuth 凭证存储（文件回退方式）
- [x] 6. 实现 `rmcp_client/logging_client_handler.rs` — MCP 客户端事件处理器
- [x] 7. 实现 `rmcp_client/rmcp_client.rs` — 核心 RmcpClient（stdio + streamable HTTP + OAuth token）
- [x] 8. 实现 `rmcp_client/auth_status.rs` — 认证状态检测（OAuth discovery）
- [x] 9. 实现 `rmcp_client/perform_oauth_login.rs` — OAuth 登录流程（本地回调服务器）
- [x] 10. 实现 `rmcp_client/mod.rs` — 模块导出
- [x] 11. 在 `lib.rs` 中注册 `rmcp_client` 模块
- [x] 12. 编译验证通过 ✅
- [x] 13. 运行测试确保无回归 ✅ (859 passed, 0 failed, 11 rmcp_client tests)
