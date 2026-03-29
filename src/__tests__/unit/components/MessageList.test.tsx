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

function runNextFrame(timestamp = 16): void {
  const iterator = frameQueue.entries().next();
  if (iterator.done) {
    throw new Error('No animation frame scheduled');
  }

  const [id, callback] = iterator.value;
  frameQueue.delete(id);
  callback(timestamp);
}

function runAllFrames(): void {
  while (frameQueue.size > 0) {
    runNextFrame();
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
        streamingTurn: null,
        streamingBuffer: null,
        streamingView: null,
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
        streamingView: { turnId: 'turn1', isStreaming: true, items: new Map(), revision: 0 },
      });
    });

    render(<MessageList threadId="t1" />);
    act(() => {
      runAllFrames();
    });
    expect(screen.getByTestId('turn-turn1')).toBeInTheDocument();
  });

  it('shows streaming text when available', () => {
    act(() => {
      useMessageStore.setState({
        streamingTurn: { turnId: 'turn1', agentText: 'Partial response...', isStreaming: true, items: new Map() },
        streamingView: {
          turnId: 'turn1',
          isStreaming: true,
          revision: 1,
          items: new Map([
            ['a1', {
              threadId: 't1',
              turnId: 'turn1',
              itemId: 'a1',
              itemType: 'AgentMessage',
              agentText: 'Partial response...',
              reasoningSummary: [],
              reasoningRaw: [],
              planText: '',
            }],
          ]),
        },
      });
    });

    render(<MessageList threadId="t1" />);
    act(() => {
      runAllFrames();
    });
    expect(screen.getByTestId('message-a1')).toBeInTheDocument();
  });

  it('does not force scroll when streaming text changes without revision change', () => {
    const scrollTopSet = vi.fn();
    let scrollTopValue = 0;

    Object.defineProperty(HTMLElement.prototype, 'scrollTop', {
      configurable: true,
      get() {
        return scrollTopValue;
      },
      set(value: number) {
        scrollTopValue = value;
        scrollTopSet(value);
      },
    });
    Object.defineProperty(HTMLElement.prototype, 'scrollHeight', {
      configurable: true,
      get() {
        return 800;
      },
    });
    Object.defineProperty(HTMLElement.prototype, 'clientHeight', {
      configurable: true,
      get() {
        return 400;
      },
    });

    act(() => {
      useMessageStore.setState({
        streamingTurn: { turnId: 'turn1', agentText: 'A', isStreaming: true, items: new Map() },
        streamingView: { turnId: 'turn1', isStreaming: true, items: new Map(), revision: 0 },
      });
    });

    render(<MessageList threadId="t1" />);
    act(() => {
      runAllFrames();
    });
    scrollTopSet.mockClear();

    act(() => {
      useMessageStore.setState({
        streamingTurn: { turnId: 'turn1', agentText: 'AB', isStreaming: true, items: new Map() },
        streamingView: { turnId: 'turn1', isStreaming: true, items: new Map(), revision: 0 },
      });
    });

    expect(scrollTopSet).not.toHaveBeenCalled();
  });

  it('reconciles scroll when streaming revision changes', () => {
    const scrollTopSet = vi.fn();
    let scrollTopValue = 0;
    let scrollHeightValue = 900;

    Object.defineProperty(HTMLElement.prototype, 'scrollTop', {
      configurable: true,
      get() {
        return scrollTopValue;
      },
      set(value: number) {
        scrollTopValue = value;
        scrollTopSet(value);
      },
    });
    Object.defineProperty(HTMLElement.prototype, 'scrollHeight', {
      configurable: true,
      get() {
        return scrollHeightValue;
      },
    });
    Object.defineProperty(HTMLElement.prototype, 'clientHeight', {
      configurable: true,
      get() {
        return 400;
      },
    });

    act(() => {
      useMessageStore.setState({
        streamingTurn: { turnId: 'turn1', agentText: 'A', isStreaming: true, items: new Map() },
        streamingView: { turnId: 'turn1', isStreaming: true, items: new Map(), revision: 0 },
      });
    });

    render(<MessageList threadId="t1" />);
    act(() => {
      runAllFrames();
    });
    scrollTopSet.mockClear();

    act(() => {
      scrollHeightValue = 960;
      useMessageStore.setState({
        streamingTurn: { turnId: 'turn1', agentText: 'AB', isStreaming: true, items: new Map() },
        streamingView: { turnId: 'turn1', isStreaming: true, items: new Map(), revision: 1 },
      });
    });

    act(() => {
      runAllFrames();
    });

    expect(scrollTopSet.mock.lastCall?.[0]).toBe(960);
  });

  it('flushes buffered streaming content on animation frame', () => {
    act(() => {
      useMessageStore.setState({
        streamingTurn: { turnId: 'turn1', agentText: '', isStreaming: true, items: new Map() },
        streamingBuffer: {
          turnId: 'turn1',
          isStreaming: true,
          dirtyItemCount: 1,
          items: new Map([
            ['a1', {
              threadId: 't1',
              turnId: 'turn1',
              itemId: 'a1',
              itemType: 'AgentMessage',
              pendingAgentText: 'Buffered text',
              pendingReasoningSummary: [],
              pendingReasoningRaw: [],
              pendingPlanText: '',
              dirty: true,
            }],
          ]),
        },
        streamingView: {
          turnId: 'turn1',
          isStreaming: true,
          revision: 0,
          items: new Map([
            ['a1', {
              threadId: 't1',
              turnId: 'turn1',
              itemId: 'a1',
              itemType: 'AgentMessage',
              agentText: '',
              reasoningSummary: [],
              reasoningRaw: [],
              planText: '',
            }],
          ]),
        },
      });
    });

    render(<MessageList threadId="t1" />);
    act(() => {
      runAllFrames();
    });
    expect(screen.getByTestId('message-a1')).toBeInTheDocument();
    expect(useMessageStore.getState().streamingView?.revision).toBe(1);
  });
});
