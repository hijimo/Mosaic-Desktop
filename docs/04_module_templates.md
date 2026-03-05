# Codex 模块模板文档

本文档提供 Codex 项目中常见模块模式的模板，基于现有代码库的最佳实践。

## 1. 工具处理器模板 (Tool Handler)

基于 `codex-rs/core/src/tools/handlers/` 模式，实现标准的工具处理接口。

### 基础结构

```rust
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::tools::{ToolHandler, ToolKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyToolArgs {
    pub input: String,
    #[serde(default)]
    pub options: Vec<String>,
}

pub struct MyToolHandler;

impl ToolHandler for MyToolHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::MyTool)
    }

    fn kind(&self) -> ToolKind {
        ToolKind::MyTool
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value> {
        let args: MyToolArgs = serde_json::from_value(args)?;
        
        // 处理逻辑
        let result = process_tool(&args).await?;
        
        Ok(serde_json::to_value(result)?)
    }
}

async fn process_tool(args: &MyToolArgs) -> Result<String> {
    // 实现具体逻辑
    Ok(format!("处理结果: {}", args.input))
}
```

## 2. MCP服务器模板

基于 `codex-rs/mcp-server/src/` 模式，实现 JSON-RPC over stdio 通信。

### 消息处理器

```rust
use serde_json::{json, Value};
use anyhow::Result;
use crate::mcp::{MessageProcessor, JsonRpcRequest, JsonRpcResponse};

pub struct MyMcpServer {
    tools: Vec<String>,
}

impl MyMcpServer {
    pub fn new() -> Self {
        Self {
            tools: vec!["my_tool".to_string()],
        }
    }
}

impl MessageProcessor for MyMcpServer {
    async fn process_request(&mut self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        match request.method.as_str() {
            "tools/list" => self.list_tools(request).await,
            "tools/call" => self.call_tool(request).await,
            _ => Ok(JsonRpcResponse::error(
                request.id,
                -32601,
                "Method not found",
            )),
        }
    }

    async fn list_tools(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let tools = json!({
            "tools": self.tools.iter().map(|name| {
                json!({
                    "name": name,
                    "description": format!("{} 工具", name),
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "input": {"type": "string"}
                        }
                    }
                })
            }).collect::<Vec<_>>()
        });

        Ok(JsonRpcResponse::success(request.id, tools))
    }

    async fn call_tool(&mut self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let params = request.params.unwrap_or_default();
        let tool_name = params["name"].as_str().unwrap_or("");
        let arguments = &params["arguments"];

        let result = match tool_name {
            "my_tool" => self.execute_my_tool(arguments).await?,
            _ => return Ok(JsonRpcResponse::error(request.id, -32602, "Unknown tool")),
        };

        Ok(JsonRpcResponse::success(request.id, result))
    }

    async fn execute_my_tool(&self, args: &Value) -> Result<Value> {
        let input = args["input"].as_str().unwrap_or("");
        Ok(json!({"result": format!("处理: {}", input)}))
    }
}
```

## 3. 配置模块模板

基于 `codex-rs/core/src/config/` 模式，支持 TOML 反序列化和分层合并。

### 配置结构

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MyModuleConfig {
    pub enabled: bool,
    pub timeout_ms: u64,
    pub endpoints: Vec<String>,
    #[serde(default)]
    pub advanced: AdvancedConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AdvancedConfig {
    pub retry_count: u32,
    pub cache_dir: Option<PathBuf>,
}

impl Default for MyModuleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_ms: 5000,
            endpoints: vec!["http://localhost:8080".to_string()],
            advanced: AdvancedConfig::default(),
        }
    }
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            retry_count: 3,
            cache_dir: None,
        }
    }
}

// 配置构建器
pub struct MyModuleConfigEdit {
    config: MyModuleConfig,
}

impl MyModuleConfigEdit {
    pub fn new(base: MyModuleConfig) -> Self {
        Self { config: base }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }

    pub fn timeout_ms(mut self, timeout: u64) -> Self {
        self.config.timeout_ms = timeout;
        self
    }

    pub fn add_endpoint(mut self, endpoint: String) -> Self {
        self.config.endpoints.push(endpoint);
        self
    }

    pub fn build(self) -> MyModuleConfig {
        self.config
    }
}
```

## 4. TUI组件模板

基于 `codex-rs/tui/src/` 模式，实现标准的 TUI 组件接口。

### 组件结构

```rust
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use crossterm::event::{KeyCode, KeyEvent};

pub struct MyComponent {
    title: String,
    content: Vec<String>,
    selected: usize,
}

impl MyComponent {
    pub fn new(title: String) -> Self {
        Self {
            title,
            content: Vec::new(),
            selected: 0,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(self.title.clone())
            .borders(Borders::ALL);

        let lines: Vec<Line> = self.content
            .iter()
            .enumerate()
            .map(|(i, item)| {
                if i == self.selected {
                    Line::from(item.clone()).style(Style::default().bg(Color::Blue))
                } else {
                    Line::from(item.clone())
                }
            })
            .collect();

        let paragraph = Paragraph::new(lines).block(block);
        paragraph.render(area, buf);
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                true
            }
            KeyCode::Down => {
                if self.selected < self.content.len().saturating_sub(1) {
                    self.selected += 1;
                }
                true
            }
            _ => false,
        }
    }

    pub fn desired_height(&self) -> u16 {
        (self.content.len() as u16).max(3) + 2 // +2 for borders
    }

    pub fn add_item(&mut self, item: String) {
        self.content.push(item);
    }
}

// 快照测试
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_my_component_render() {
        let mut component = MyComponent::new("测试组件".to_string());
        component.add_item("项目 1".to_string());
        component.add_item("项目 2".to_string());

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                component.render(f.size(), f.buffer_mut());
            })
            .unwrap();

        // 使用 insta 进行快照测试
        insta::assert_debug_snapshot!(terminal.backend().buffer());
    }
}
```

## 5. 协议类型模板

基于 `codex-rs/app-server-protocol/src/protocol/v2.rs` 模式，支持 camelCase 序列化和 TypeScript 导出。

### 协议定义

```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MyOperationParams {
    pub operation_id: String,
    pub input_data: serde_json::Value,
    #[serde(default)]
    pub options: MyOperationOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MyOperationOptions {
    pub timeout_ms: Option<u64>,
    pub retry_count: Option<u32>,
    #[cfg(feature = "experimental")]
    pub experimental_feature: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MyOperationResponse {
    pub operation_id: String,
    pub status: OperationStatus,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OperationStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MyOperationNotification {
    pub operation_id: String,
    pub progress: f64,
    pub message: Option<String>,
}

impl Default for MyOperationOptions {
    fn default() -> Self {
        Self {
            timeout_ms: Some(30000),
            retry_count: Some(3),
            #[cfg(feature = "experimental")]
            experimental_feature: None,
        }
    }
}
```

## 6. Skill定义模板

基于 SKILL.md 格式和 YAML frontmatter 的技能定义模板。

### SKILL.md 结构

```markdown
---
name: my-skill
description: 我的技能描述
version: 1.0.0
author: 作者名称
tags:
  - utility
  - automation
triggers:
  - "执行我的技能"
  - "运行自定义操作"
dependencies:
  - some-other-skill
experimental: false
---

# 我的技能

这是一个示例技能，展示如何创建自定义技能模块。

## 使用场景

- 当用户需要执行特定操作时
- 自动化常见任务
- 提供专业领域的支持

## 功能特性

- 支持多种输入格式
- 提供详细的错误处理
- 集成现有工具链

## 使用方法

```bash
# 基本用法
my-skill --input "示例输入"

# 高级选项
my-skill --input "输入" --format json --output result.json
```

## 配置选项

| 选项 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| input | string | - | 输入数据 |
| format | string | "text" | 输出格式 |
| timeout | number | 30 | 超时时间(秒) |

## 示例

### 基础示例

```javascript
const result = await mySkill.execute({
  input: "处理这个数据",
  format: "json"
});
```

### 高级示例

```javascript
const result = await mySkill.execute({
  input: "复杂数据处理",
  format: "json",
  options: {
    timeout: 60,
    retries: 3
  }
});
```

## 错误处理

技能会返回标准化的错误信息：

```json
{
  "success": false,
  "error": {
    "code": "INVALID_INPUT",
    "message": "输入数据格式不正确",
    "details": {}
  }
}
```

## 更新日志

### v1.0.0
- 初始版本发布
- 基础功能实现
```

### agents/openai.yaml 元数据

```yaml
name: my-skill
version: 1.0.0
description: 我的技能描述
author: 作者名称
category: utility
tags:
  - automation
  - custom

# 触发条件
triggers:
  patterns:
    - "执行我的技能"
    - "运行自定义操作"
    - "my-skill"
  contexts:
    - development
    - automation

# 依赖关系
dependencies:
  skills:
    - some-other-skill
  tools:
    - shell
    - file-operations

# 配置选项
config:
  timeout: 30
  max_retries: 3
  output_format: "json"

# 权限要求
permissions:
  - file-read
  - file-write
  - network-access

# 实验性功能
experimental:
  enabled: false
  features:
    - advanced-processing
    - beta-integration
```

## 使用指南

1. **工具处理器**: 用于扩展 Codex 的工具能力
2. **MCP服务器**: 实现外部服务集成
3. **配置模块**: 管理模块配置和设置
4. **TUI组件**: 构建终端用户界面
5. **协议类型**: 定义 API 通信格式
6. **Skill定义**: 创建可复用的技能模块

每个模板都遵循 Codex 项目的编码规范和最佳实践，确保代码质量和一致性。