# Message Share Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an assistant-message action bar with copy, copy-as-Markdown, thumbs up, thumbs down, and OSS-backed share-to-link behavior that opens the system share sheet.

**Architecture:** The front end will build copy/share payloads from `TurnGroup`, render a dedicated `MessageActionBar`, and keep reaction/share UI state in a focused zustand store. Tauri Rust will accept a normalized share payload, upload attachments and a generated standalone HTML page to Alibaba Cloud OSS, then return the public URL for the front end to pass into the system share plugin.

**Tech Stack:** React 19, MUI 7, Zustand, Vitest, Tauri 2, Rust, `aliyun-oss-rust-sdk`, `@vnidrop/tauri-plugin-share`

---

## File Structure

### Front-end files

- Create: `src/components/chat/agent/MessageActionBar.tsx`
  - Renders copy, copy menu, reaction toggles, share trigger, and in-place feedback.
- Create: `src/components/chat/agent/messageShareContent.ts`
  - Builds plain text, Markdown, and normalized share payloads from `TurnGroup`.
- Create: `src/stores/messageActionStore.ts`
  - Stores reaction state and per-message share task state.
- Create: `src/__tests__/unit/components/MessageActionBar.test.tsx`
  - Covers interaction behavior for the action bar.
- Create: `src/__tests__/unit/components/messageShareContent.test.ts`
  - Covers plain-text, Markdown, and share payload generation.
- Modify: `src/components/chat/Message.tsx`
  - Renders the new action bar below assistant content.
- Modify: `src/services/tauri/commands.ts`
  - Adds `shareMessage`.
- Modify: `src/services/api.ts`
  - Re-exports `shareMessage`.

### Tauri files

- Create: `src-tauri/src/share/mod.rs`
  - Re-exports share modules and the main `share_message` orchestration function.
- Create: `src-tauri/src/share/config.rs`
  - Loads OSS config from Rust-side env vars with controlled fallback.
- Create: `src-tauri/src/share/types.rs`
  - Defines request, response, share page, and attachment DTOs.
- Create: `src-tauri/src/share/render.rs`
  - Renders standalone HTML with OSS asset URLs.
- Create: `src-tauri/src/share/oss.rs`
  - Wraps Alibaba Cloud OSS upload calls.
- Modify: `src-tauri/src/commands.rs`
  - Adds a Tauri command that delegates to the share service.
- Modify: `src-tauri/src/lib.rs`
  - Registers the new command and share plugin.
- Modify: `src-tauri/Cargo.toml`
  - Adds `aliyun-oss-rust-sdk` and `tauri-plugin-vnidrop-share`.
- Modify: `src-tauri/capabilities/default.json`
  - Adds the share plugin permission.

### Documentation

- Modify: `README.md`
  - Documents the Rust-only OSS env vars and the security boundary.

## Task 1: Build share-content helpers first

**Files:**
- Create: `src/components/chat/agent/messageShareContent.ts`
- Create: `src/__tests__/unit/components/messageShareContent.test.ts`
- Test: `src/__tests__/unit/components/messageShareContent.test.ts`

- [ ] **Step 1: Write the failing helper tests**

```ts
import { describe, expect, it } from 'vitest';
import type { TurnGroup } from '@/types';
import {
  buildCopyMarkdown,
  buildCopyText,
  buildSharePayload,
} from '@/components/chat/agent/messageShareContent';

const group: TurnGroup = {
  turn_id: 'turn-share-1',
  items: [
    {
      type: 'UserMessage',
      id: 'user-1',
      content: [
        { type: 'text', text: '请总结下面这段代码', text_elements: [] },
        { type: 'local_image', path: '/tmp/request.png' },
      ],
    },
    {
      type: 'AgentMessage',
      id: 'agent-1',
      content: [{ type: 'Text', text: '这里是回答。\n\n```ts\nconsole.log(1)\n```' }],
    },
    {
      type: 'ImageView',
      id: 'img-1',
      path: '/tmp/result.png',
    },
    {
      type: 'Reasoning',
      id: 'reason-1',
      summary_text: ['不会出现在分享页'],
      raw_content: [],
    },
  ],
};

describe('messageShareContent', () => {
  it('builds plain text for direct copy', () => {
    expect(buildCopyText(group)).toContain('用户');
    expect(buildCopyText(group)).toContain('助手');
    expect(buildCopyText(group)).toContain('这里是回答。');
    expect(buildCopyText(group)).not.toContain('不会出现在分享页');
  });

  it('builds markdown for markdown copy', () => {
    const markdown = buildCopyMarkdown(group);
    expect(markdown).toContain('## 用户问题');
    expect(markdown).toContain('## 助手回答');
    expect(markdown).toContain('```ts');
    expect(markdown).toContain('![request.png]');
  });

  it('builds normalized share payload with attachments', () => {
    const payload = buildSharePayload(group);
    expect(payload.turnId).toBe('turn-share-1');
    expect(payload.userText).toContain('请总结下面这段代码');
    expect(payload.answerMarkdown).toContain('console.log(1)');
    expect(payload.attachments).toEqual([
      expect.objectContaining({
        kind: 'image',
        sourcePath: '/tmp/request.png',
        displayName: 'request.png',
      }),
      expect.objectContaining({
        kind: 'image',
        sourcePath: '/tmp/result.png',
        displayName: 'result.png',
      }),
    ]);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm vitest run src/__tests__/unit/components/messageShareContent.test.ts`  
Expected: FAIL with module-not-found errors for `messageShareContent.ts`.

- [ ] **Step 3: Write the minimal helper implementation**

```ts
import type { TurnGroup, TurnItem, UserInput } from '@/types';

export interface ShareAttachmentInput {
  kind: 'image' | 'file';
  sourcePath: string;
  displayName: string;
}

export interface ShareMessagePayload {
  turnId: string;
  title: string;
  generatedAt: string;
  userText: string;
  answerMarkdown: string;
  attachments: ShareAttachmentInput[];
}

export function buildCopyText(group: TurnGroup): string {
  const userText = group.items
    .filter((item): item is Extract<TurnItem, { type: 'UserMessage' }> => item.type === 'UserMessage')
    .flatMap((item) => item.content)
    .filter((input): input is Extract<UserInput, { type: 'text' }> => input.type === 'text')
    .map((input) => input.text.trim())
    .filter(Boolean)
    .join('\n\n');

  const answerText = group.items
    .filter((item): item is Extract<TurnItem, { type: 'AgentMessage' }> => item.type === 'AgentMessage')
    .flatMap((item) => item.content)
    .map((content) => content.text)
    .join('');

  return [`用户\n${userText}`, `助手\n${answerText}`].filter(Boolean).join('\n\n');
}

export function buildCopyMarkdown(group: TurnGroup): string {
  const payload = buildSharePayload(group);
  const imageLines = payload.attachments
    .filter((attachment) => attachment.kind === 'image')
    .map((attachment) => `![${attachment.displayName}](${attachment.sourcePath})`);

  return [
    '# 对话分享',
    '',
    '## 用户问题',
    payload.userText,
    '',
    '## 助手回答',
    payload.answerMarkdown,
    ...(imageLines.length > 0 ? ['', '## 附件', ...imageLines] : []),
  ].join('\n');
}

export function buildSharePayload(group: TurnGroup): ShareMessagePayload {
  const attachments: ShareAttachmentInput[] = [];

  for (const item of group.items) {
    if (item.type === 'UserMessage') {
      for (const input of item.content) {
        if (input.type === 'local_image') {
          attachments.push({
            kind: 'image',
            sourcePath: input.path,
            displayName: fileNameOf(input.path),
          });
        }
      }
    }

    if (item.type === 'ImageView') {
      attachments.push({
        kind: 'image',
        sourcePath: item.path,
        displayName: fileNameOf(item.path),
      });
    }
  }

  return {
    turnId: group.turn_id,
    title: `对话分享 ${group.turn_id}`,
    generatedAt: new Date().toISOString(),
    userText: group.items
      .filter((item): item is Extract<TurnItem, { type: 'UserMessage' }> => item.type === 'UserMessage')
      .flatMap((item) => item.content)
      .filter((input): input is Extract<UserInput, { type: 'text' }> => input.type === 'text')
      .map((input) => input.text.trim())
      .filter(Boolean)
      .join('\n\n'),
    answerMarkdown: group.items
      .filter((item): item is Extract<TurnItem, { type: 'AgentMessage' }> => item.type === 'AgentMessage')
      .flatMap((item) => item.content)
      .map((content) => content.text)
      .join(''),
    attachments,
  };
}

function fileNameOf(path: string): string {
  const segments = path.split('/');
  return segments[segments.length - 1] || 'attachment';
}
```

- [ ] **Step 4: Run the helper test to verify it passes**

Run: `pnpm vitest run src/__tests__/unit/components/messageShareContent.test.ts`  
Expected: PASS with 3 passing tests.

- [ ] **Step 5: Commit the helper layer**

```bash
git add src/components/chat/agent/messageShareContent.ts src/__tests__/unit/components/messageShareContent.test.ts
git commit -m "feat: add message share content helpers"
```

## Task 2: Add the action bar and UI state

**Files:**
- Create: `src/components/chat/agent/MessageActionBar.tsx`
- Create: `src/stores/messageActionStore.ts`
- Modify: `src/components/chat/Message.tsx`
- Create: `src/__tests__/unit/components/MessageActionBar.test.tsx`
- Modify: `src/__tests__/unit/components/Message.test.tsx`
- Test: `src/__tests__/unit/components/MessageActionBar.test.tsx`
- Test: `src/__tests__/unit/components/Message.test.tsx`

- [ ] **Step 1: Write the failing UI tests**

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TurnGroup } from '@/types';
import { MessageActionBar } from '@/components/chat/agent/MessageActionBar';

const shareMessage = vi.fn().mockResolvedValue({ url: 'https://example.com/share/1' });
const share = vi.fn().mockResolvedValue(undefined);

vi.mock('@/services/api', () => ({ shareMessage: (...args: unknown[]) => shareMessage(...args) }));
vi.mock('@vnidrop/tauri-plugin-share', () => ({ share: (...args: unknown[]) => share(...args) }));

Object.assign(navigator, {
  clipboard: {
    writeText: vi.fn().mockResolvedValue(undefined),
  },
});

const group: TurnGroup = {
  turn_id: 'turn-action-1',
  items: [
    { type: 'UserMessage', id: 'u1', content: [{ type: 'text', text: '你好', text_elements: [] }] },
    { type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: '你好，我可以帮你整理内容。' }] },
  ],
};

describe('MessageActionBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('copies plain text on primary copy click', async () => {
    const user = userEvent.setup();
    render(<MessageActionBar group={group} messageId='a1' />);

    await user.click(screen.getByRole('button', { name: '复制消息' }));

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(expect.stringContaining('你好，我可以帮你整理内容。'));
  });

  it('opens the copy menu and copies markdown', async () => {
    const user = userEvent.setup();
    render(<MessageActionBar group={group} messageId='a1' />);

    await user.click(screen.getByRole('button', { name: '更多复制方式' }));
    await user.click(screen.getByRole('menuitem', { name: '复制为 Markdown' }));

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(expect.stringContaining('## 助手回答'));
  });

  it('toggles thumbs up and thumbs down mutually', async () => {
    const user = userEvent.setup();
    render(<MessageActionBar group={group} messageId='a1' />);

    await user.click(screen.getByRole('button', { name: '点赞' }));
    expect(screen.getByRole('button', { name: '点赞' })).toHaveAttribute('data-selected', 'true');

    await user.click(screen.getByRole('button', { name: '点踩' }));
    expect(screen.getByRole('button', { name: '点赞' })).toHaveAttribute('data-selected', 'false');
    expect(screen.getByRole('button', { name: '点踩' })).toHaveAttribute('data-selected', 'true');
  });

  it('shares the generated URL through the system share sheet', async () => {
    const user = userEvent.setup();
    render(<MessageActionBar group={group} messageId='a1' />);

    await user.click(screen.getByRole('button', { name: '分享消息' }));

    expect(shareMessage).toHaveBeenCalledWith(expect.objectContaining({ turnId: 'turn-action-1' }));
    expect(share).toHaveBeenCalledWith(expect.objectContaining({ url: 'https://example.com/share/1' }));
  });
});
```

- [ ] **Step 2: Run the UI tests to verify they fail**

Run: `pnpm vitest run src/__tests__/unit/components/MessageActionBar.test.tsx src/__tests__/unit/components/Message.test.tsx`  
Expected: FAIL with missing component/store errors.

- [ ] **Step 3: Implement the zustand store**

```ts
import { create } from 'zustand';

export type MessageReaction = 'up' | 'down' | 'none';
export type ShareTaskStatus = 'idle' | 'preparing' | 'uploading' | 'sharing' | 'success' | 'failed';

interface MessageActionState {
  reactions: Record<string, MessageReaction>;
  shareTasks: Record<string, ShareTaskStatus>;
  setReaction: (messageId: string, reaction: Exclude<MessageReaction, 'none'>) => void;
  clearReaction: (messageId: string) => void;
  setShareTask: (messageId: string, status: ShareTaskStatus) => void;
}

export const useMessageActionStore = create<MessageActionState>((set) => ({
  reactions: {},
  shareTasks: {},
  setReaction: (messageId, reaction) =>
    set((state) => ({
      reactions: {
        ...state.reactions,
        [messageId]: state.reactions[messageId] === reaction ? 'none' : reaction,
      },
    })),
  clearReaction: (messageId) =>
    set((state) => ({
      reactions: {
        ...state.reactions,
        [messageId]: 'none',
      },
    })),
  setShareTask: (messageId, status) =>
    set((state) => ({
      shareTasks: {
        ...state.shareTasks,
        [messageId]: status,
      },
    })),
}));
```

- [ ] **Step 4: Implement the action bar and mount it in `Message.tsx`**

```tsx
import { useState } from 'react';
import { Box, IconButton, Menu, MenuItem, Tooltip, Typography } from '@mui/material';
import { Check, Copy, Ellipsis, Share2, ThumbsDown, ThumbsUp } from 'lucide-react';
import { share } from '@vnidrop/tauri-plugin-share';
import type { TurnGroup } from '@/types';
import { shareMessage } from '@/services/api';
import {
  buildCopyMarkdown,
  buildCopyText,
  buildSharePayload,
} from './messageShareContent';
import { useMessageActionStore } from '@/stores/messageActionStore';

interface MessageActionBarProps {
  group: TurnGroup;
  messageId: string;
}

export function MessageActionBar({
  group,
  messageId,
}: MessageActionBarProps): React.ReactElement {
  const [anchorEl, setAnchorEl] = useState<HTMLElement | null>(null);
  const [feedback, setFeedback] = useState<string>('');
  const reaction = useMessageActionStore((state) => state.reactions[messageId] ?? 'none');
  const shareStatus = useMessageActionStore((state) => state.shareTasks[messageId] ?? 'idle');
  const setReaction = useMessageActionStore((state) => state.setReaction);
  const setShareTask = useMessageActionStore((state) => state.setShareTask);

  const copy = async (value: string, label: string): Promise<void> => {
    await navigator.clipboard.writeText(value);
    setFeedback(label);
    window.setTimeout(() => setFeedback(''), 1500);
  };

  const handleShare = async (): Promise<void> => {
    setShareTask(messageId, 'preparing');
    try {
      const result = await shareMessage(buildSharePayload(group));
      setShareTask(messageId, 'sharing');
      await share({ title: 'Mosaic 对话分享', text: result.url, url: result.url });
      setShareTask(messageId, 'success');
    } catch {
      setShareTask(messageId, 'failed');
    }
  };

  return (
    <Box sx={{ mt: 1.5, display: 'flex', alignItems: 'center', justifyContent: 'flex-end', gap: 0.5 }}>
      <Tooltip title={feedback || '复制'}>
        <IconButton aria-label='复制消息' onClick={() => void copy(buildCopyText(group), '已复制')}>
          {feedback === '已复制' ? <Check size={16} /> : <Copy size={16} />}
        </IconButton>
      </Tooltip>
      <IconButton aria-label='更多复制方式' onClick={(event) => setAnchorEl(event.currentTarget)}>
        <Ellipsis size={16} />
      </IconButton>
      <Menu anchorEl={anchorEl} open={Boolean(anchorEl)} onClose={() => setAnchorEl(null)}>
        <MenuItem
          onClick={() => {
            setAnchorEl(null);
            void copy(buildCopyText(group), '已复制');
          }}
        >
          复制
        </MenuItem>
        <MenuItem
          onClick={() => {
            setAnchorEl(null);
            void copy(buildCopyMarkdown(group), '已复制为 Markdown');
          }}
        >
          复制为 Markdown
        </MenuItem>
      </Menu>
      <IconButton
        aria-label='点赞'
        data-selected={reaction === 'up'}
        onClick={() => setReaction(messageId, 'up')}
      >
        <ThumbsUp size={16} />
      </IconButton>
      <IconButton
        aria-label='点踩'
        data-selected={reaction === 'down'}
        onClick={() => setReaction(messageId, 'down')}
      >
        <ThumbsDown size={16} />
      </IconButton>
      <IconButton
        aria-label='分享消息'
        disabled={shareStatus === 'preparing' || shareStatus === 'uploading' || shareStatus === 'sharing'}
        onClick={() => void handleShare()}
      >
        <Share2 size={16} />
      </IconButton>
      {shareStatus === 'failed' && (
        <Typography sx={{ fontSize: 12, color: 'error.main' }}>分享失败，请稍后再试</Typography>
      )}
    </Box>
  );
}
```

`src/components/chat/Message.tsx` patch target:

```tsx
const firstAgentMessage = agentItems.find(
  (item): item is Extract<TurnItem, { type: 'AgentMessage' }> => item.type === 'AgentMessage',
);

// inside the agent card, after {agentItems.map(...)}
{firstAgentMessage ? (
  <MessageActionBar
    group={group}
    messageId={firstAgentMessage.id}
  />
) : null}
```

- [ ] **Step 5: Run the UI tests to verify they pass**

Run: `pnpm vitest run src/__tests__/unit/components/MessageActionBar.test.tsx src/__tests__/unit/components/Message.test.tsx`  
Expected: PASS with the action bar rendered under assistant content and all interaction tests green.

- [ ] **Step 6: Commit the UI layer**

```bash
git add src/components/chat/agent/MessageActionBar.tsx src/stores/messageActionStore.ts src/components/chat/Message.tsx src/__tests__/unit/components/MessageActionBar.test.tsx src/__tests__/unit/components/Message.test.tsx
git commit -m "feat: add message action bar interactions"
```

## Task 3: Add the Tauri command contract and system share plumbing

**Files:**
- Modify: `src/services/tauri/commands.ts`
- Modify: `src/services/api.ts`
- Modify: `src-tauri/Cargo.toml`
- Test: app compiles with new plugin and command exports

- [ ] **Step 1: Install the front-end and Rust dependencies**

Run:

```bash
pnpm add @vnidrop/tauri-plugin-share
cd src-tauri && cargo add aliyun-oss-rust-sdk tauri-plugin-vnidrop-share
```

Expected: `package.json`, `pnpm-lock.yaml`, `Cargo.toml`, and `Cargo.lock` are updated with the new dependencies.

- [ ] **Step 2: Add the failing command wrapper test by extending `MessageActionBar.test.tsx`**

```tsx
it('copies the share url when the system share sheet throws', async () => {
  const user = userEvent.setup();
  shareMessage.mockResolvedValue({ url: 'https://example.com/share/2' });
  share.mockRejectedValue(new Error('share unavailable'));

  render(<MessageActionBar group={group} messageId='a1' />);

  await user.click(screen.getByRole('button', { name: '分享消息' }));

  expect(navigator.clipboard.writeText).toHaveBeenCalledWith('https://example.com/share/2');
});
```

- [ ] **Step 3: Add the `shareMessage` wrapper**

```ts
import { invoke } from '@tauri-apps/api/core';
import type { ShareMessagePayload } from '@/components/chat/agent/messageShareContent';

export interface ShareMessageResult {
  url: string;
}

export async function shareMessage(payload: ShareMessagePayload): Promise<ShareMessageResult> {
  return invoke<ShareMessageResult>('share_message', { payload });
}
```

`src/services/api.ts` patch target:

```ts
export {
  threadStart,
  threadList,
  threadGetInfo,
  threadArchive,
  threadResume,
  threadGetMessages,
  submitOp,
  getCwd,
  getConfig,
  shareMessage,
} from './tauri/commands';
```

- [ ] **Step 4: Run the front-end tests again**

Run:

```bash
pnpm vitest run src/__tests__/unit/components/MessageActionBar.test.tsx
```

Expected: Vitest is green, including the clipboard fallback case for failed system share.

- [ ] **Step 5: Commit the command wrapper and dependency wiring**

```bash
git add package.json pnpm-lock.yaml src/services/tauri/commands.ts src/services/api.ts src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: add message share command wrapper"
```

## Task 4: Implement the Rust share pipeline

**Files:**
- Create: `src-tauri/src/share/mod.rs`
- Create: `src-tauri/src/share/config.rs`
- Create: `src-tauri/src/share/types.rs`
- Create: `src-tauri/src/share/render.rs`
- Create: `src-tauri/src/share/oss.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/default.json`
- Test: `src-tauri/src/share/render.rs`
- Test: `src-tauri/src/share/config.rs`

- [ ] **Step 1: Write the failing Rust tests**

Add to `src-tauri/src/share/render.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::types::{ShareAttachment, SharePage};

    #[test]
    fn renders_oss_asset_urls_into_html() {
        let html = render_share_html(&SharePage {
            title: "对话分享".into(),
            generated_at: "2026-03-28T12:00:00Z".into(),
            user_text: "你好".into(),
            answer_html: "<pre><code>console.log(1)</code></pre>".into(),
            attachments: vec![ShareAttachment {
                kind: "image".into(),
                display_name: "result.png".into(),
                url: "https://example.com/assets/result.png".into(),
            }],
        });

        assert!(html.contains("https://example.com/assets/result.png"));
        assert!(html.contains("console.log(1)"));
        assert!(html.contains("对话分享"));
    }
}
```

Add to `src-tauri/src/share/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_mosaic_env_but_accepts_vite_fallback() {
        std::env::set_var("VITE_OSS_BUCKET", "fallback-bucket");
        std::env::set_var("VITE_OSS_REGION", "oss-cn-hangzhou");
        std::env::set_var("VITE_OSS_ACCESSKEY_ID", "ak");
        std::env::set_var("VITE_OSS_ACCESSKEY_SECRET", "sk");
        std::env::set_var("VITE_OSS_HOST", "https://example.com");
        std::env::set_var("VITE_OSS_DIST", "ai-share/");

        let config = OssConfig::from_env().expect("config loads");

        assert_eq!(config.bucket, "fallback-bucket");
        assert_eq!(config.dist_prefix, "ai-share/");
    }
}
```

- [ ] **Step 2: Run the Rust tests to verify they fail**

Run: `cd src-tauri && cargo test share:: --lib -- --nocapture`  
Expected: FAIL with missing `share` module and missing functions.

- [ ] **Step 3: Implement the share DTOs, config loader, renderer, and OSS service**

`src-tauri/src/share/types.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareMessageRequest {
    pub turn_id: String,
    pub title: String,
    pub generated_at: String,
    pub user_text: String,
    pub answer_markdown: String,
    pub attachments: Vec<ShareAttachmentInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareAttachmentInput {
    pub kind: String,
    pub source_path: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareMessageResponse {
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct ShareAttachment {
    pub kind: String,
    pub display_name: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct SharePage {
    pub title: String,
    pub generated_at: String,
    pub user_text: String,
    pub answer_html: String,
    pub attachments: Vec<ShareAttachment>,
}
```

`src-tauri/src/share/config.rs`

```rust
#[derive(Debug, Clone)]
pub struct OssConfig {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub bucket: String,
    pub region: String,
    pub dist_prefix: String,
    pub host: String,
}

impl OssConfig {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            access_key_id: read_env("MOSAIC_OSS_ACCESS_KEY_ID", "VITE_OSS_ACCESSKEY_ID")?,
            access_key_secret: read_env("MOSAIC_OSS_ACCESS_KEY_SECRET", "VITE_OSS_ACCESSKEY_SECRET")?,
            bucket: read_env("MOSAIC_OSS_BUCKET", "VITE_OSS_BUCKET")?,
            region: read_env("MOSAIC_OSS_REGION", "VITE_OSS_REGION")?,
            dist_prefix: read_env("MOSAIC_OSS_DIST", "VITE_OSS_DIST")?,
            host: read_env("MOSAIC_OSS_HOST", "VITE_OSS_HOST")?,
        })
    }
}

fn read_env(primary: &str, fallback: &str) -> Result<String, String> {
    std::env::var(primary)
        .or_else(|_| std::env::var(fallback))
        .map_err(|_| format!("missing env: {primary} or {fallback}"))
}
```

`src-tauri/src/share/render.rs`

```rust
use crate::share::types::SharePage;

pub fn render_share_html(page: &SharePage) -> String {
    let attachments = page
        .attachments
        .iter()
        .map(|attachment| {
            if attachment.kind == "image" {
                format!(
                    "<figure><img src=\"{}\" alt=\"{}\" /><figcaption>{}</figcaption></figure>",
                    attachment.url, attachment.display_name, attachment.display_name
                )
            } else {
                format!(
                    "<p><a href=\"{}\" target=\"_blank\" rel=\"noreferrer\">下载 {}</a></p>",
                    attachment.url, attachment.display_name
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{}</title>
    <style>
      body {{ margin: 0; padding: 32px; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #f5f7fb; color: #1f2937; }}
      main {{ max-width: 880px; margin: 0 auto; background: #fff; border-radius: 24px; padding: 32px; box-shadow: 0 20px 45px rgba(15, 23, 42, 0.08); }}
      pre {{ overflow-x: auto; background: #0f172a; color: #e2e8f0; padding: 16px; border-radius: 16px; }}
      img {{ max-width: 100%; border-radius: 16px; display: block; }}
      h2 {{ margin-top: 32px; }}
    </style>
  </head>
  <body>
    <main>
      <h1>{}</h1>
      <p>生成时间：{}</p>
      <h2>用户问题</h2>
      <p>{}</p>
      <h2>助手回答</h2>
      {}
      {}
    </main>
  </body>
</html>"#,
        page.title, page.title, page.generated_at, page.user_text, page.answer_html, attachments
    )
}
```

`src-tauri/src/share/oss.rs`

```rust
use aliyun_oss_rust_sdk::request::Client;
use crate::share::config::OssConfig;

pub async fn upload_bytes(
    config: &OssConfig,
    object_key: &str,
    body: Vec<u8>,
    content_type: &str,
) -> Result<String, String> {
    let client = Client::new(
        &config.access_key_id,
        &config.access_key_secret,
        &config.endpoint(),
        &config.bucket,
    );

    client
        .put_content_base64(object_key, body, content_type)
        .await
        .map_err(|error| format!("oss upload failed: {error}"))?;

    Ok(format!(
        "{}/{}",
        config.host.trim_end_matches('/'),
        object_key.trim_start_matches('/')
    ))
}
```

`src-tauri/src/share/mod.rs`

```rust
pub mod config;
pub mod oss;
pub mod render;
pub mod types;

use uuid::Uuid;

use self::config::OssConfig;
use self::oss::upload_bytes;
use self::render::render_share_html;
use self::types::{ShareAttachment, ShareMessageRequest, ShareMessageResponse, SharePage};

pub async fn share_message(payload: ShareMessageRequest) -> Result<ShareMessageResponse, String> {
    let config = OssConfig::from_env()?;
    let share_id = Uuid::new_v4().to_string();
    let root = format!("{}/{share_id}", config.normalized_prefix());
    let mut uploaded_attachments = Vec::new();

    for attachment in payload.attachments {
      let bytes = std::fs::read(&attachment.source_path)
          .map_err(|error| format!("read attachment failed {}: {error}", attachment.source_path))?;
      let object_key = format!("{root}/assets/{}", attachment.display_name);
      let url = upload_bytes(&config, &object_key, bytes, "application/octet-stream").await?;
      uploaded_attachments.push(ShareAttachment {
          kind: attachment.kind,
          display_name: attachment.display_name,
          url,
      });
    }

    let answer_html = format!("<article>{}</article>", payload.answer_markdown.replace('\n', "<br />"));
    let html = render_share_html(&SharePage {
      title: payload.title,
      generated_at: payload.generated_at,
      user_text: payload.user_text,
      answer_html,
      attachments: uploaded_attachments,
    });

    let html_key = format!("{root}/index.html");
    let url = upload_bytes(&config, &html_key, html.into_bytes(), "text/html; charset=utf-8").await?;

    Ok(ShareMessageResponse { url })
}

impl OssConfig {
    pub fn normalized_prefix(&self) -> String {
        self.dist_prefix.trim_end_matches('/').to_string()
    }

    pub fn endpoint(&self) -> String {
        format!("https://{}.aliyuncs.com", self.region)
    }
}
```

`src-tauri/src/commands.rs` patch target:

```rust
#[tauri::command]
pub async fn share_message(payload: crate::share::types::ShareMessageRequest) -> Result<crate::share::types::ShareMessageResponse, String> {
    crate::share::share_message(payload).await
}
```

`src-tauri/src/lib.rs` patch target:

```rust
pub mod share;

tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_vnidrop_share::init())
    .manage(app_state)
    .invoke_handler(tauri::generate_handler![
        commands::submit_op,
        commands::thread_start,
        commands::thread_list,
        commands::thread_get_info,
        commands::thread_archive,
        commands::thread_get_messages,
        commands::thread_resume,
        commands::thread_fork,
        commands::fuzzy_file_search,
        commands::get_config,
        commands::update_config,
        commands::get_cwd,
        commands::share_message,
    ])
```

`src-tauri/capabilities/default.json` patch target:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "opener:default",
    "share:default",
    "core:event:default",
    "core:event:allow-listen",
    "core:event:allow-unlisten"
  ]
}
```

- [ ] **Step 4: Run the Rust tests to verify they pass**

Run: `cd src-tauri && cargo test share:: --lib -- --nocapture`  
Expected: PASS with the new render/config tests and the share module compiling.

- [ ] **Step 5: Run a focused end-to-end Rust compile check**

Run: `cd src-tauri && cargo test --lib commands::tests -- --nocapture`  
Expected: PASS or no matching command tests; if there are no command tests, the build phase must still complete cleanly with the new `share_message` command available.

- [ ] **Step 6: Commit the Rust share pipeline**

```bash
git add src-tauri/src/share src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/capabilities/default.json
git commit -m "feat: add oss-backed share pipeline"
```

## Task 5: Add fallback behavior, docs, and final verification

**Files:**
- Modify: `src/components/chat/agent/MessageActionBar.tsx`
- Modify: `README.md`
- Test: `src/__tests__/unit/components/MessageActionBar.test.tsx`

- [ ] **Step 1: Implement clipboard fallback in the action bar**

Update the `handleShare` branch in `MessageActionBar.tsx`:

```tsx
const handleShare = async (): Promise<void> => {
  setShareTask(messageId, 'preparing');
  try {
    const result = await shareMessage(buildSharePayload(group));
    setShareTask(messageId, 'sharing');

    try {
      await share({ title: 'Mosaic 对话分享', text: result.url, url: result.url });
      setShareTask(messageId, 'success');
      setFeedback('已调起系统分享');
    } catch {
      await navigator.clipboard.writeText(result.url);
      setShareTask(messageId, 'success');
      setFeedback('已复制分享链接');
    }
  } catch {
    setShareTask(messageId, 'failed');
    setFeedback('分享失败');
  }
};
```

- [ ] **Step 2: Document the Rust-only OSS env vars**

Append to `README.md`:

~~~md
## Share Feature Environment Variables

The message share feature reads OSS credentials only from the Tauri Rust process.

Preferred variables:

```bash
MOSAIC_OSS_ACCESS_KEY_ID=...
MOSAIC_OSS_ACCESS_KEY_SECRET=...
MOSAIC_OSS_BUCKET=juxieyun
MOSAIC_OSS_REGION=oss-cn-hangzhou
MOSAIC_OSS_DIST=ai-share/
MOSAIC_OSS_HOST=https://juxieyun.oss-cn-hangzhou.aliyuncs.com
```

Compatibility fallback:

- `VITE_OSS_ACCESSKEY_ID`
- `VITE_OSS_ACCESSKEY_SECRET`
- `VITE_OSS_BUCKET`
- `VITE_OSS_REGION`
- `VITE_OSS_DIST`
- `VITE_OSS_HOST`

Do not read these variables in React code. Keep OSS credentials inside the Rust process boundary only.
~~~

- [ ] **Step 3: Run the final front-end and Rust verification**

Run:

```bash
pnpm vitest run src/__tests__/unit/components/messageShareContent.test.ts src/__tests__/unit/components/MessageActionBar.test.tsx src/__tests__/unit/components/Message.test.tsx
pnpm typecheck
cd src-tauri && cargo test share:: --lib -- --nocapture
```

Expected:

- Vitest passes for helper, action bar, and message integration.
- `pnpm typecheck` passes.
- Rust share tests pass.

- [ ] **Step 4: Run a manual smoke check**

Run: `pnpm test:smoke`  
Expected manual checklist:

- assistant messages show the new action bar,
- primary copy copies plain text,
- copy menu copies Markdown,
- thumbs up / thumbs down toggle visually and stay mutually exclusive,
- share uploads a standalone page and opens the system share sheet,
- if the share sheet is unavailable, the final OSS URL lands in the clipboard.

- [ ] **Step 5: Commit the fallback and docs**

```bash
git add src/components/chat/agent/MessageActionBar.tsx README.md
git commit -m "docs: document message share env vars"
```

## Self-Review

### Spec coverage

- Message action bar: covered by Task 2.
- Copy and copy-as-Markdown: covered by Tasks 1 and 2.
- Local thumbs up / thumbs down state: covered by Task 2.
- OSS-backed HTML generation and upload flow: covered by Task 4.
- System share sheet plus clipboard fallback: covered by Tasks 3 and 5.
- Rust-only credential boundary and README update: covered by Task 5.

### Placeholder scan

- No `TODO`, `TBD`, or “implement later” placeholders remain.
- Each task names exact files, commands, and verification expectations.

### Type consistency

- Front-end payload type is `ShareMessagePayload`.
- Rust request DTO is `ShareMessageRequest`.
- Command wrapper is `shareMessage`.
- Tauri response DTO is `ShareMessageResponse`.
