# 测试策略

> 版本：1.0 | 更新时间：2026-03-23

## 测试金字塔

```
        ┌───────────┐
        │   E2E     │  ← Playwright (关键用户流程)
        │  Tests    │
       ─┴───────────┴─
      ┌───────────────┐
      │  Integration  │  ← Vitest + Testing Library (组件交互)
      │    Tests      │
     ─┴───────────────┴─
    ┌───────────────────┐
    │    Unit Tests     │  ← Vitest (工具函数、Hooks、Store)
    │                   │
    └───────────────────┘
```

## 工具选型

| 类型 | 工具 | 用途 |
|-----|------|------|
| 单元测试 | Vitest | 工具函数、Hooks、Store |
| 组件测试 | Vitest + React Testing Library | 组件渲染、交互 |
| E2E 测试 | Playwright | 完整用户流程 |
| Mock | MSW | API Mock |
| 覆盖率 | c8 (Vitest 内置) | 代码覆盖率报告 |

## 目录结构

```
src/
├── __tests__/               # 测试文件
│   ├── unit/                # 单元测试
│   │   ├── utils/
│   │   ├── hooks/
│   │   └── stores/
│   ├── integration/         # 集成测试
│   │   └── components/
│   └── setup.ts             # 测试配置
│
e2e/                         # E2E 测试 (项目根目录)
├── tests/
│   ├── chat.spec.ts
│   └── thread.spec.ts
├── fixtures/
└── playwright.config.ts
```

## 依赖安装

```bash
# 单元测试 + 组件测试
pnpm add -D vitest @testing-library/react @testing-library/jest-dom @testing-library/user-event jsdom

# E2E 测试
pnpm add -D @playwright/test
```

## 配置文件

### vitest.config.ts

```typescript
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/__tests__/setup.ts'],
    include: ['src/__tests__/**/*.{test,spec}.{ts,tsx}'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html'],
      exclude: ['node_modules/', 'src/__tests__/'],
    },
  },
});
```

### src/__tests__/setup.ts

```typescript
import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));
```

### playwright.config.ts

```typescript
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e/tests',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: 'http://localhost:1420', // Tauri dev server
    trace: 'on-first-retry',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
  ],
  webServer: {
    command: 'pnpm tauri dev',
    url: 'http://localhost:1420',
    reuseExistingServer: !process.env.CI,
  },
});
```

## NPM Scripts

```json
{
  "scripts": {
    "test": "vitest",
    "test:ui": "vitest --ui",
    "test:coverage": "vitest run --coverage",
    "test:e2e": "playwright test",
    "test:e2e:ui": "playwright test --ui"
  }
}
```

## 测试示例

### 单元测试 (Hook)

```typescript
// src/__tests__/unit/hooks/useThread.test.ts
import { renderHook, act } from '@testing-library/react';
import { useThread } from '@/hooks/useThread';
import { invoke } from '@tauri-apps/api/core';
import { vi, describe, it, expect, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core');

describe('useThread', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should start a new thread', async () => {
    const mockThreadId = 'test-thread-id';
    vi.mocked(invoke).mockResolvedValue(mockThreadId);

    const { result } = renderHook(() => useThread());

    await act(async () => {
      await result.current.startThread('/path/to/cwd');
    });

    expect(invoke).toHaveBeenCalledWith('thread_start', { cwd: '/path/to/cwd' });
    expect(result.current.threadId).toBe(mockThreadId);
  });
});
```

### 组件测试

```typescript
// src/__tests__/integration/components/MessageItem.test.tsx
import { render, screen } from '@testing-library/react';
import { MessageItem } from '@/components/chat/MessageItem';
import { describe, it, expect } from 'vitest';

describe('MessageItem', () => {
  it('renders user message', () => {
    render(
      <MessageItem
        role="user"
        content="Hello, AI!"
      />
    );

    expect(screen.getByText('Hello, AI!')).toBeInTheDocument();
  });

  it('renders assistant message with markdown', () => {
    render(
      <MessageItem
        role="assistant"
        content="**Bold** text"
      />
    );

    expect(screen.getByText('Bold')).toBeInTheDocument();
  });
});
```

### E2E 测试

```typescript
// e2e/tests/chat.spec.ts
import { test, expect } from '@playwright/test';

test.describe('Chat Flow', () => {
  test('should send message and receive response', async ({ page }) => {
    await page.goto('/');

    // 输入消息
    const input = page.getByPlaceholder('输入消息...');
    await input.fill('Hello');
    await input.press('Enter');

    // 验证消息显示
    await expect(page.getByText('Hello')).toBeVisible();

    // 等待 AI 响应 (mock 或真实)
    await expect(page.locator('[data-testid="assistant-message"]')).toBeVisible({
      timeout: 30000,
    });
  });
});
```

## 覆盖率目标

| 类型 | 目标 |
|-----|------|
| 工具函数 | 90%+ |
| Hooks | 80%+ |
| Store | 80%+ |
| 组件 | 70%+ |
| 整体 | 75%+ |

## CI 集成

```yaml
# .github/workflows/test.yml
name: Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v2
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'pnpm'
      
      - run: pnpm install
      - run: pnpm test:coverage
      
      - name: Upload coverage
        uses: codecov/codecov-action@v3
```
