import { render, screen, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MessageList } from '@/components/chat/MessageList';
import { useMessageStore } from '@/stores/messageStore';
import type { TurnGroup } from '@/types';

// Mock Streamdown
vi.mock('streamdown', () => ({
  Streamdown: ({ children }: { children: string }) => <div>{children}</div>,
}));
vi.mock('@streamdown/code', () => ({ code: {} }));
vi.mock('@streamdown/cjk', () => ({ cjk: {} }));
vi.mock('streamdown/styles.css', () => ({}));

// Mock Message component
vi.mock('@/components/chat/Message', () => ({
  Message: ({ group }: { group: TurnGroup }) => (
    <div data-testid={`turn-${group.turn_id}`}>
      {group.items.map((item) => (
        <div key={item.id} data-testid={`message-${item.id}`}>{item.type}</div>
      ))}
    </div>
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
    const groups: TurnGroup[] = [
      {
        turn_id: 'turn-1',
        items: [
          { type: 'UserMessage', id: 'u1', content: [{ type: 'text', text: 'Hi', text_elements: [] }] },
          { type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: 'Hello' }] },
        ],
      },
    ];
    act(() => {
      useMessageStore.setState({
        messagesByThread: new Map([['t1', groups]]),
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
