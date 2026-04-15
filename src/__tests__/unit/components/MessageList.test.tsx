import { render, screen, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MessageList } from '@/components/chat/MessageList';
import { useMessageStore } from '@/stores/messageStore';
import type { TurnGroup } from '@/types';

let nextFrameId = 1;
const frameQueue = new Map<number, FrameRequestCallback>();

vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
  const id = nextFrameId++;
  frameQueue.set(id, callback);
  return id;
});
vi.stubGlobal('cancelAnimationFrame', vi.fn((id: number) => {
  frameQueue.delete(id);
}));
vi.stubGlobal(
  'ResizeObserver',
  class {
    observe = vi.fn();
    disconnect = vi.fn();
  },
);

vi.mock('streamdown', () => ({
  Streamdown: ({ children }: { children: string }) => <div>{children}</div>,
}));
vi.mock('@streamdown/code', () => ({ code: {} }));
vi.mock('@streamdown/cjk', () => ({ cjk: {} }));
vi.mock('streamdown/styles.css', () => ({}));

vi.mock('@/components/chat/Message', () => ({
  Message: ({ group }: { group: TurnGroup }) => (
    <div data-testid={`turn-${group.turn_id}`}>
      {group.items.map((item) => (
        <div key={item.id} data-testid={`message-${item.id}`}>{item.type}</div>
      ))}
    </div>
  ),
}));

function runAllFrames(): void {
  while (frameQueue.size > 0) {
    const iterator = frameQueue.entries().next();
    if (iterator.done) break;
    const [id, callback] = iterator.value;
    frameQueue.delete(id);
    callback(16);
  }
}

describe('MessageList', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    frameQueue.clear();
    nextFrameId = 1;
    act(() => {
      useMessageStore.setState({
        messagesByThread: new Map(),
        streamingByThread: new Map(),
      });
    });
  });

  it('renders empty list when no messages', () => {
    const { container } = render(<MessageList threadId="t1" />);
    expect(container.querySelector('[data-testid^="message-"]')).toBeNull();
  });

  it('renders messages for a thread', () => {
    const groups: TurnGroup[] = [{
      turn_id: 'turn-1',
      items: [
        { type: 'UserMessage', id: 'u1', content: [{ type: 'text', text: 'Hi', text_elements: [] }] },
        { type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: 'Hello' }] },
      ],
    }];
    act(() => {
      useMessageStore.setState({ messagesByThread: new Map([['t1', groups]]) });
    });
    render(<MessageList threadId="t1" />);
    expect(screen.getByTestId('message-u1')).toBeInTheDocument();
    expect(screen.getByTestId('message-a1')).toBeInTheDocument();
  });

  it('shows streaming indicator when streaming for the correct thread', () => {
    act(() => {
      useMessageStore.getState().startStreaming('t1', 'turn1');
    });
    render(<MessageList threadId="t1" />);
    act(() => { runAllFrames(); });
    expect(screen.getByTestId('turn-turn1')).toBeInTheDocument();
  });

  it('does not show streaming from another thread', () => {
    act(() => {
      useMessageStore.getState().startStreaming('t2', 'turn-other');
    });
    const { container } = render(<MessageList threadId="t1" />);
    expect(container.querySelector('[data-testid="turn-turn-other"]')).toBeNull();
  });
});
