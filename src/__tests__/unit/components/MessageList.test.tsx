import { render, screen, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MessageList } from '@/components/chat/MessageList';
import { useMessageStore } from '@/stores/messageStore';
import type { TurnItem } from '@/types';

// Mock Streamdown
vi.mock('streamdown', () => ({
  Streamdown: ({ children }: { children: string }) => <div>{children}</div>,
}));
vi.mock('@streamdown/code', () => ({ code: {} }));
vi.mock('@streamdown/cjk', () => ({ cjk: {} }));
vi.mock('streamdown/styles.css', () => ({}));

// Mock Message component
vi.mock('@/components/chat/Message', () => ({
  Message: ({ item }: { item: TurnItem }) => (
    <div data-testid={`message-${item.id}`}>{item.type}</div>
  ),
}));

describe('MessageList', () => {
  beforeEach(() => {
    act(() => {
      useMessageStore.setState({
        messagesByThread: new Map(),
        streamingTurn: null,
      });
    });
  });

  it('renders empty list when no messages', () => {
    const { container } = render(<MessageList threadId="t1" />);
    expect(container.querySelector('[data-testid^="message-"]')).toBeNull();
  });

  it('renders messages for a thread', () => {
    const msgs: TurnItem[] = [
      { type: 'UserMessage', id: 'u1', content: [{ type: 'text', text: 'Hi', text_elements: [] }] },
      { type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: 'Hello' }] },
    ];
    act(() => {
      useMessageStore.setState({
        messagesByThread: new Map([['t1', msgs]]),
      });
    });

    render(<MessageList threadId="t1" />);
    expect(screen.getByTestId('message-u1')).toBeInTheDocument();
    expect(screen.getByTestId('message-a1')).toBeInTheDocument();
  });

  it('shows streaming indicator when streaming', () => {
    act(() => {
      useMessageStore.setState({
        streamingTurn: { turnId: 'turn1', agentText: '', isStreaming: true, items: new Map() },
      });
    });

    render(<MessageList threadId="t1" />);
    // Now renders Chinese text
    expect(screen.getByText('思考中...')).toBeInTheDocument();
  });

  it('shows streaming text when available', () => {
    act(() => {
      useMessageStore.setState({
        streamingTurn: { turnId: 'turn1', agentText: 'Partial response...', isStreaming: true, items: new Map() },
      });
    });

    render(<MessageList threadId="t1" />);
    expect(screen.getByText('Partial response...')).toBeInTheDocument();
  });
});
