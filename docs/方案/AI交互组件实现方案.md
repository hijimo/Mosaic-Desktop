# AI 交互组件实现方案

> 版本：1.0 | 创建时间：2026-03-26

## 一、需求概述

根据 Figma 设计稿（node-id: 108-1631），实现完整的 AI 对话交互组件体系。设计稿中从上到下包含以下 9 个交互组件：

1. **任务开始指示器** (Task Started) — 分隔线 + 标签
2. **思考中** (Thinking) — 可折叠的推理过程面板
3. **网络搜索** (Web Search) — 工具调用卡片，显示搜索状态
4. **MCP 工具调用** (MCP Tool Call) — 工具调用卡片，显示 MCP 调用状态
5. **代码执行** (Code / Terminal Block) — 终端风格代码块，深色背景
6. **执行审批** (Execution Approval Required) — 警告风格审批卡片
7. **代码差异** (Code Diff) — 文件变更差异展示
8. **需要澄清** (Clarification Needed) — 用户输入请求卡片
9. **任务完成指示器** (Task Completed) — 分隔线 + 标签

## 二、技术方案

### 2.1 Streamdown 集成

项目已安装 `streamdown@^2.5.0`，但 Message 组件尚未使用。需要：

- 在 `AgentMessage` 渲染中使用 `<Streamdown>` 替代纯文本 `<Typography>`
- 安装 `@streamdown/code` 插件用于代码高亮
- 安装 `@streamdown/cjk` 插件用于中文支持
- 由于项目使用 MUI (Emotion CSS-in-JS) 而非 Tailwind，需要通过 `components` prop 自定义渲染器来适配 MUI 样式体系

### 2.2 组件架构

```
src/components/chat/
├── Message.tsx              # 主消息组件（重构）
├── MessageList.tsx          # 消息列表（重构）
├── InputArea.tsx            # 输入区域（已有）
├── index.ts                 # 导出（更新）
│
├── indicators/              # 指示器组件
│   ├── TaskStartedIndicator.tsx
│   └── TaskCompletedIndicator.tsx
│
├── agent/                   # Agent 响应子组件
│   ├── ThinkingPanel.tsx        # 思考中面板
│   ├── WebSearchCard.tsx        # 网络搜索卡片
│   ├── McpToolCallCard.tsx      # MCP 工具调用卡片
│   ├── CodeExecutionBlock.tsx   # 代码执行终端块
│   ├── CodeDiffBlock.tsx        # 代码差异块
│   ├── ApprovalRequestCard.tsx  # 执行审批卡片
│   └── ClarificationCard.tsx   # 需要澄清卡片
│
└── shared/                  # 共享子组件
    ├── AgentAvatar.tsx
    ├── UserAvatar.tsx
    ├── StatusBadge.tsx
    └── StreamdownRenderer.tsx   # Streamdown 封装
```

### 2.3 数据流映射

| 设计组件 | 后端事件 (EventMsg.type) | Store |
|---------|------------------------|-------|
| 任务开始 | `task_started` | messageStore.streamingTurn |
| 思考中 | `reasoning_content_delta` | messageStore.streamingTurn.items |
| 网络搜索 | `web_search_begin/end` | toolCallStore |
| MCP 工具调用 | `mcp_tool_call_begin/end` | toolCallStore |
| 代码执行 | `exec_command_begin/output_delta/end` | toolCallStore |
| 执行审批 | `exec_approval_request` / `apply_patch_approval_request` | 新增 approvalStore |
| 代码差异 | `patch_apply_begin/end` + `apply_patch_approval_request` | toolCallStore + approvalStore |
| 需要澄清 | `request_user_input` | 新增 clarificationStore |
| 任务完成 | `task_complete` | messageStore.streamingTurn |

### 2.4 设计稿色彩映射

| 设计元素 | 颜色值 |
|---------|--------|
| 背景 | `#f7f9fb` |
| 任务开始分隔线 | `rgba(192,199,207,0.2)` |
| 任务开始文字 | `#94a3b8` |
| 用户消息背景 | `#f0f7ff` |
| 用户消息边框 | `#d4e6ff` |
| Agent 渐变头像 | `linear-gradient(135deg, #7cb9e8, #005bc1)` |
| 思考面板背景 | `#f8fafc` |
| 思考面板边框 | `#e2e8f0` |
| 网络搜索图标背景 | `#eff6ff` |
| MCP 工具图标背景 | `#fff7ed` |
| 终端背景 | `#0f172a` |
| 终端边框 | `#1e293b` |
| 终端提示符 | `#34d399` |
| 终端文字 | `#bfdbfe` |
| 审批背景 | `#fffbeb` |
| 审批边框 | `rgba(253,230,138,0.5)` |
| 审批标题 | `#78350f` |
| 审批按钮 | `#d97706` |
| Diff 背景 | `#f2f4f6` |
| Diff 删除行 | `#fef2f2` / `#b91c1c` |
| Diff 新增行 | `#ecfdf5` / `#047857` |
| 澄清背景 | `#f0f7ff` |
| 澄清边框 | `rgba(124,185,232,0.3)` |
| 澄清标题 | `#005bc1` |
| 任务完成分隔线 | `#d1fae5` |
| 任务完成文字 | `#10b981` |
| 完成状态 | `rgba(119,220,122,0.2)` / `#006e20` |
| 运行状态 | `#3b82f6` |

### 2.5 中文化

设计稿为英文，实现时翻译为中文：

| 英文 | 中文 |
|------|------|
| Task Started | 任务开始 |
| Thinking... | 思考中... |
| Web Search | 网络搜索 |
| MCP Tool Call | MCP 工具调用 |
| COMPLETE | 已完成 |
| RUNNING | 运行中 |
| Execution Approval Required | 需要执行审批 |
| Approve Action | 批准执行 |
| Reject | 拒绝 |
| Clarification Needed | 需要澄清 |
| Task Completed | 任务完成 |
| Code Diff | 代码差异 |
| Update | 更新 |

## 三、Streamdown 集成方案

### 3.1 安装依赖

```bash
pnpm add @streamdown/code @streamdown/cjk
```

### 3.2 封装 StreamdownRenderer

由于项目使用 MUI 而非 Tailwind，需要创建一个封装组件：

```typescript
// src/components/chat/shared/StreamdownRenderer.tsx
import { Streamdown } from 'streamdown';
import { code } from '@streamdown/code';
import { cjk } from '@streamdown/cjk';

interface StreamdownRendererProps {
  children: string;
  isStreaming?: boolean;
}

export function StreamdownRenderer({ children, isStreaming }: StreamdownRendererProps) {
  return (
    <Streamdown
      plugins={{ code, cjk }}
      isAnimating={isStreaming}
    >
      {children}
    </Streamdown>
  );
}
```

### 3.3 样式适配

需要在 `global.css` 中引入 streamdown 样式并覆盖以适配 MUI 主题。

## 四、Store 扩展

### 4.1 新增 approvalStore

```typescript
interface ApprovalStoreState {
  approvals: Map<string, ApprovalRequestState>;
  addApproval: (approval: ApprovalRequestState) => void;
  removeApproval: (callId: string) => void;
  clearAll: () => void;
}
```

### 4.2 新增 clarificationStore

```typescript
interface ClarificationState {
  id: string;
  message: string;
  schema?: unknown;
}

interface ClarificationStoreState {
  requests: Map<string, ClarificationState>;
  addRequest: (req: ClarificationState) => void;
  removeRequest: (id: string) => void;
}
```

### 4.3 扩展 useCodexEvent

在事件处理中增加对以下事件的处理：
- `exec_approval_request` → approvalStore
- `apply_patch_approval_request` → approvalStore
- `request_user_input` → clarificationStore

## 五、组件组装

最终在 `Message.tsx` 中，根据 `TurnItem.type` 和关联的 toolCall/approval 状态，按顺序渲染各子组件。在 `MessageList.tsx` 中，在消息流的开头和结尾插入任务指示器。
