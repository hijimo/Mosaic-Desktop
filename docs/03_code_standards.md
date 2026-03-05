# Codex 项目代码规范

## 1. Rust 编码规范

### 1.1 Crate 命名规范
- 所有 crate 名称必须使用 `codex-` 前缀
- 示例：`core` 文件夹的 crate 命名为 `codex-core`

### 1.2 格式化规范
- 使用 `format!` 时，如果可以内联变量到 `{}`，必须这样做
```rust
// ✅ 正确
format!("Hello {name}")

// ❌ 错误
format!("Hello {}", name)
```

### 1.3 控制流规范
- 必须折叠 if 语句，遵循 `clippy::collapsible_if`
```rust
// ✅ 正确
if condition1 && condition2 {
    // code
}

// ❌ 错误
if condition1 {
    if condition2 {
        // code
    }
}
```

### 1.4 函数调用规范
- 优先使用方法引用而非闭包，遵循 `clippy::redundant_closure_for_method_calls`
```rust
// ✅ 正确
items.map(String::from)

// ❌ 错误
items.map(|x| String::from(x))
```

### 1.5 模式匹配规范
- `match` 语句应尽可能穷举，避免使用通配符 `_`
```rust
// ✅ 正确
match status {
    Status::Active => handle_active(),
    Status::Inactive => handle_inactive(),
    Status::Pending => handle_pending(),
}

// ❌ 避免
match status {
    Status::Active => handle_active(),
    _ => handle_other(),
}
```

### 1.6 测试规范
- 测试中优先比较完整对象，而非逐字段比较
```rust
// ✅ 正确
assert_eq!(actual_config, expected_config);

// ❌ 避免
assert_eq!(actual_config.field1, expected_config.field1);
assert_eq!(actual_config.field2, expected_config.field2);
```

## 2. TUI 样式规范

### 2.1 Stylize Helpers 使用
- 优先使用 ratatui 的 Stylize trait 简洁样式助手
```rust
// ✅ 基础 spans
"text".into()

// ✅ 样式化 spans
"text".red()
"text".green()
"text".magenta()
"text".dim()
```

### 2.2 样式链式调用
- 支持链式调用以提高可读性
```rust
// ✅ 正确
url.cyan().underlined()
"warning".yellow().bold()
```

### 2.3 颜色使用规范
- 避免硬编码 `.white()`，使用默认前景色
- 不使用颜色时保持默认外观

### 2.4 转换规范
- 简单转换使用 `.into()`
- 构建行时使用 `vec![…].into()`
```rust
// ✅ 单项
"text".into()

// ✅ 构建行
vec!["  └ ".into(), "M".red(), " ".dim(), "tui/src/app.rs".dim()]
```

### 2.5 文本换行
- 纯字符串使用 `textwrap::wrap`
- ratatui Line 使用 `tui/src/wrapping.rs` 中的助手
- 需要缩进时使用 `RtOptions` 的 `initial_indent`/`subsequent_indent`

## 3. 配置变更规范

### 3.1 配置架构更新
- 修改 `ConfigToml` 或嵌套配置类型后，必须运行：
```bash
just write-config-schema
```

### 3.2 依赖管理
- 修改 Rust 依赖（`Cargo.toml` 或 `Cargo.lock`）后，必须运行：
```bash
just bazel-lock-update  # 从仓库根目录运行
```

- 依赖变更后，运行检查：
```bash
just bazel-lock-check  # 从仓库根目录运行
```

## 4. 测试规范

### 4.1 测试执行顺序
1. 先运行特定项目测试：
```bash
cargo test -p codex-tui  # 如果修改了 tui
```

2. 共享 crate 变更后运行完整测试：
```bash
cargo test  # 或 just test（如果安装了 cargo-nextest）
```

### 4.2 测试特性
- 避免日常本地运行使用 `--all-features`
- 仅在需要完整特性覆盖时使用

### 4.3 快照测试
- UI 变更必须包含对应的 `insta` 快照覆盖
- 更新快照流程：
```bash
cargo test -p codex-tui
cargo insta pending-snapshots -p codex-tui
cargo insta accept -p codex-tui  # 仅在确认所有变更后
```

### 4.4 测试断言
- 使用 `pretty_assertions::assert_eq` 获得更清晰的差异
- 优先深度相等比较整个对象

## 5. 代码质量规范

### 5.1 格式化
- 完成 Rust 代码变更后，自动运行：
```bash
just fmt  # 在 codex-rs 目录中
```

### 5.2 代码检查
- 大型变更最终确定前运行：
```bash
just fix -p <project>  # 修复特定项目的 linter 问题
```

- 仅在修改共享 crate 时运行：
```bash
just fix  # 无 -p 参数
```

### 5.3 代码结构
- 不创建只引用一次的小型 helper 方法
- 保持代码简洁和直接

## 6. 安全规范

### 6.1 沙箱环境
- 绝不添加或修改与 `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` 或 `CODEX_SANDBOX_ENV_VAR` 相关的代码
- 这些变量用于沙箱环境控制，已有代码考虑了沙箱限制

### 6.2 进程环境
- 测试中避免修改进程环境
- 优先从上层传递环境派生的标志或依赖

## 7. 文档维护

### 7.1 API 文档
- 添加或变更 API 时，确保 `docs/` 文件夹中的文档保持最新

### 7.2 二进制路径
- 测试中需要生成第一方二进制时，优先使用 `codex_utils_cargo_bin::cargo_bin("...")`
- 定位测试资源时，避免使用 `env!("CARGO_MANIFEST_DIR")`，优先使用 `codex_utils_cargo_bin::find_resource!`

## 8. 工作流程

### 8.1 标准开发流程
1. 进行代码变更
2. 运行 `just fmt`
3. 运行特定项目测试
4. 如需要，运行完整测试套件
5. 大型变更前运行 `just fix -p <project>`
6. 更新相关文档

### 8.2 配置变更流程
1. 修改配置文件
2. 运行 `just write-config-schema`（如果修改了 ConfigToml）
3. 运行 `just bazel-lock-update`（如果修改了依赖）
4. 运行 `just bazel-lock-check`
5. 提交变更

遵循这些规范确保 Codex 项目代码质量和一致性。