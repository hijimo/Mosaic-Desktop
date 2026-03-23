# 前端架构设计

> 版本：1.0 | 更新时间：2026-03-23

## 技术栈

| 层级 | 技术选型 | 说明 |
|-----|---------|------|
| 框架 | React 19 | 函数式组件 + Hooks |
| 路由 | react-router-dom v7 | 声明式路由 |
| 状态管理 | Zustand | 轻量级，支持 immer |
| 数据请求 | SWR | 数据获取、缓存、重验证 |
| UI 组件 | MUI v7 | Material Design |
| 图标 | Lucide React | 统一图标库 |
| Markdown | Streamdown | AI 流式渲染优化 |
| 样式 | Emotion (CSS-in-JS) | MUI 内置 |
| 构建 | Vite 7 | 快速 HMR |
| 类型 | TypeScript 5.8 | 严格模式 |

## 目录结构

```
src/
├── main.tsx                 # 应用入口
├── App.tsx                  # 根组件，路由配置
├── vite-env.d.ts
│
├── assets/                  # 静态资源
│
├── components/              # 通用组件
│   ├── common/              # 基础组件 (Button, Input, Modal...)
│   └── chat/                # 聊天相关组件
│       ├── MessageList.tsx
│       ├── MessageItem.tsx
│       ├── InputArea.tsx
│       └── ToolCallDisplay.tsx
│
├── layouts/                 # 布局组件
│   ├── MainLayout.tsx       # 主布局 (侧边栏 + 内容区)
│   └── ChatLayout.tsx       # 聊天页布局
│
├── pages/                   # 页面组件
│   └── index/               # 首页 (聊天主界面)
│       ├── IndexPage.tsx
│       └── index.ts
│
├── hooks/                   # 自定义 Hooks
│   ├── useThread.ts         # 线程管理
│   ├── useCodexEvent.ts     # 后端事件监听
│   ├── useSubmitOp.ts       # 提交操作
│   └── useSWR*.ts           # SWR 数据请求 hooks
│
├── stores/                  # Zustand 状态
│   ├── threadStore.ts       # 线程状态
│   ├── messageStore.ts      # 消息状态
│   └── uiStore.ts           # UI 状态
│
├── services/                # 服务层
│   ├── tauri/               # Tauri IPC 封装
│   │   ├── commands.ts      # invoke 封装
│   │   └── events.ts        # 事件监听封装
│   └── api.ts               # 统一导出
│
├── types/                   # 类型定义 (已有)
│   ├── events.ts
│   ├── commands.ts
│   ├── file-search.ts
│   └── index.ts
│
├── utils/                   # 工具函数
│   ├── format.ts            # 格式化
│   └── cn.ts                # classnames 封装
│
└── styles/                  # 全局样式
    ├── theme.ts             # MUI 主题配置
    └── global.css           # 全局 CSS
```

## 路由设计

```typescript
// App.tsx
const router = createBrowserRouter([
  {
    path: '/',
    element: <MainLayout />,
    children: [
      { index: true, element: <IndexPage /> },
      { path: 'thread/:threadId', element: <IndexPage /> },
      { path: 'settings', element: <SettingsPage /> },
    ],
  },
]);
```

| 路由 | 页面 | 说明 |
|-----|------|------|
| `/` | IndexPage | 首页，新建对话 |
| `/thread/:threadId` | IndexPage | 指定线程的对话 |
| `/settings` | SettingsPage | 设置页 (后续) |

## 状态管理

### 线程状态 (threadStore)

```typescript
interface ThreadState {
  threads: Map<string, ThreadMeta>;
  activeThreadId: string | null;
  
  // Actions
  setActiveThread: (id: string) => void;
  addThread: (meta: ThreadMeta) => void;
  removeThread: (id: string) => void;
}
```

### 消息状态 (messageStore)

```typescript
interface MessageState {
  // threadId -> messages
  messagesByThread: Map<string, TurnItem[]>;
  
  // 当前 turn 的流式状态
  streamingTurn: StreamingTurn | null;
  
  // Actions
  appendMessage: (threadId: string, turn: TurnItem) => void;
  updateStreamingTurn: (update: Partial<StreamingTurn>) => void;
}
```

## SWR 数据请求

SWR 用于处理与 Tauri 后端的数据获取、缓存和重验证。

### 使用场景

| 场景 | 方案 |
|-----|------|
| 一次性数据获取 | SWR + Tauri invoke |
| 实时流式数据 | Tauri events (不用 SWR) |
| 列表数据 | SWR + 分页/无限加载 |
| 表单提交 | useSWRMutation |

### 基础用法

```typescript
// hooks/useThreadList.ts
import useSWR from 'swr';
import { invoke } from '@tauri-apps/api/core';

const fetcher = () => invoke<ThreadMeta[]>('list_threads');

export function useThreadList() {
  return useSWR('threads', fetcher);
}
```

### Mutation 用法

```typescript
// hooks/useCreateThread.ts
import useSWRMutation from 'swr/mutation';
import { invoke } from '@tauri-apps/api/core';

export function useCreateThread() {
  return useSWRMutation('threads', async () => {
    return invoke<ThreadMeta>('create_thread');
  });
}
```

### 全局配置

```typescript
// main.tsx
import { SWRConfig } from 'swr';

<SWRConfig value={{
  revalidateOnFocus: false,  // Tauri 桌面应用不需要
  dedupingInterval: 2000,
}}>
  <App />
</SWRConfig>
```

### SWR vs Zustand 选择

| 数据类型 | 推荐方案 |
|---------|---------|
| 服务端数据 (线程列表、历史记录) | SWR |
| 客户端 UI 状态 (侧边栏展开、主题) | Zustand |
| 实时流式数据 (AI 响应) | Zustand + Tauri events |

## 数据流

```
┌─────────────────────────────────────────────────────────────┐
│                        React 组件                           │
│  ┌─────────┐    ┌─────────┐    ┌─────────────────────────┐ │
│  │ 页面组件 │ ←→ │  Hooks  │ ←→ │     Zustand Store       │ │
│  └─────────┘    └─────────┘    └─────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                          ↑ ↓
┌─────────────────────────────────────────────────────────────┐
│                     Services 层                             │
│  ┌─────────────────────┐    ┌─────────────────────────────┐│
│  │ commands.ts         │    │ events.ts                   ││
│  │ invoke() 封装       │    │ listen() 封装               ││
│  └─────────────────────┘    └─────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
                          ↑ ↓
┌─────────────────────────────────────────────────────────────┐
│                   Tauri IPC                                 │
│              @tauri-apps/api/core                           │
└─────────────────────────────────────────────────────────────┘
                          ↑ ↓
┌─────────────────────────────────────────────────────────────┐
│                   Rust Backend                              │
│                   Codex Engine                              │
└─────────────────────────────────────────────────────────────┘
```

## 事件处理流程

1. 组件调用 `useSubmitOp` hook 提交用户输入
2. Hook 调用 `services/tauri/commands.ts` 的 `submitOp()`
3. `submitOp()` 调用 `invoke('submit_op', params)`
4. 后端处理后通过 `emit('codex-event')` 推送事件
5. `useCodexEvent` hook 监听事件，更新 Zustand store
6. 组件通过 store 订阅获取更新，重新渲染

## 组件设计原则

1. **单一职责**：每个组件只做一件事
2. **Props 向下，Events 向上**：数据单向流动
3. **Hooks 抽象逻辑**：UI 与业务逻辑分离
4. **TypeScript 严格模式**：所有 props 必须有类型定义
5. **Lucide 图标**：禁止使用 emoji 作为图标
6. **cursor-pointer**：所有可点击元素必须有手型光标
