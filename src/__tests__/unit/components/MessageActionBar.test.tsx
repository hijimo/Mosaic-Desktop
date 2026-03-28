import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MessageActionBar } from '@/components/chat/agent/MessageActionBar';
import { useMessageActionStore } from '@/stores/messageActionStore';
import type { TurnGroup } from '@/types';

const shareMessageMock = vi.fn();
const shareMock = vi.fn();
const clipboardWriteTextMock = vi.fn();
const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
const consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

vi.mock('@/services/api', () => ({
  shareMessage: (...args: unknown[]) => shareMessageMock(...args),
}));

vi.mock('@vnidrop/tauri-plugin-share', () => ({
  share: (...args: unknown[]) => shareMock(...args),
}));

vi.mock('@/services/clipboard', () => ({
  writeClipboardText: (...args: unknown[]) => clipboardWriteTextMock(...args),
}));

const group: TurnGroup = {
  turn_id: 'turn-share-1',
  items: [
    {
      type: 'UserMessage',
      id: 'user-1',
      content: [{ type: 'text', text: '请总结下面这段代码', text_elements: [] }],
    },
    {
      type: 'AgentMessage',
      id: 'agent-1',
      content: [{ type: 'Text', text: '这里是回答。' }],
    },
  ],
};

describe('MessageActionBar', () => {
  beforeEach(() => {
    useMessageActionStore.getState().reset();
    shareMessageMock.mockReset();
    shareMock.mockReset();
    clipboardWriteTextMock.mockReset().mockResolvedValue(undefined);
    consoleErrorSpy.mockClear();
    consoleWarnSpy.mockClear();
  });

  it('copies plain text by default', async () => {
    const user = userEvent.setup();
    render(
      <MessageActionBar
        group={group}
        messageId='agent-1'
      />,
    );

    await user.click(screen.getByRole('button', { name: '复制' }));

    await waitFor(() => {
      expect(clipboardWriteTextMock).toHaveBeenCalledTimes(1);
    });
    expect(clipboardWriteTextMock.mock.calls[0]?.[0]).toContain('这里是回答。');
    expect(await screen.findByText('已复制')).toBeInTheDocument();
  });

  it('copies markdown through the menu option', async () => {
    const user = userEvent.setup();
    render(
      <MessageActionBar
        group={group}
        messageId='agent-1'
      />,
    );

    await user.click(screen.getByRole('button', { name: '更多复制选项' }));
    await user.click(screen.getByRole('menuitem', { name: '复制为 Markdown' }));

    await waitFor(() => {
      expect(clipboardWriteTextMock).toHaveBeenCalledTimes(1);
    });
    expect(clipboardWriteTextMock.mock.calls[0]?.[0]).toContain('## 助手回答');
    expect(await screen.findByText('已复制 Markdown')).toBeInTheDocument();
  });

  it('toggles thumbs up and thumbs down reactions', async () => {
    const user = userEvent.setup();
    render(
      <MessageActionBar
        group={group}
        messageId='agent-1'
      />,
    );

    const upButton = screen.getByRole('button', { name: '点赞' });
    const downButton = screen.getByRole('button', { name: '点踩' });

    await user.click(upButton);
    expect(useMessageActionStore.getState().reactions['agent-1']).toBe('up');

    await user.click(downButton);
    expect(useMessageActionStore.getState().reactions['agent-1']).toBe('down');

    await user.click(downButton);
    expect(useMessageActionStore.getState().reactions['agent-1']).toBe('none');
  });

  it('shares through system share and falls back to copying link', async () => {
    const user = userEvent.setup();
    shareMessageMock.mockResolvedValue({ url: 'https://example.com/share/turn-share-1' });
    shareMock.mockRejectedValue(new Error('share unavailable'));

    render(
      <MessageActionBar
        group={group}
        messageId='agent-1'
      />,
    );

    await user.click(screen.getByRole('button', { name: '分享' }));

    await waitFor(() => {
      expect(shareMessageMock).toHaveBeenCalledTimes(1);
    });
    expect(shareMock).toHaveBeenCalledWith({ url: 'https://example.com/share/turn-share-1' });
    await waitFor(() => {
      expect(clipboardWriteTextMock).toHaveBeenCalledWith('https://example.com/share/turn-share-1');
    });
    expect(consoleWarnSpy).toHaveBeenCalledTimes(1);
    expect(await screen.findByText('已复制分享链接')).toBeInTheDocument();
  });

  it('logs the share failure and shows snackbar alert when shareMessage rejects', async () => {
    const user = userEvent.setup();
    shareMessageMock.mockRejectedValue(new Error('share message failed: missing oss config'));

    render(
      <MessageActionBar
        group={group}
        messageId='agent-1'
      />,
    );

    await user.click(screen.getByRole('button', { name: '分享' }));

    await waitFor(() => {
      expect(consoleErrorSpy).toHaveBeenCalledTimes(1);
    });
    expect(consoleErrorSpy.mock.calls[0]?.[0]).toContain('消息分享失败');
    expect(await screen.findByText('分享失败：missing oss config')).toBeInTheDocument();
  });
});
