import { render, screen, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { StreamingAgentBody } from '@/components/chat/streaming/StreamingAgentBody';
import { useMessageStore } from '@/stores/messageStore';

vi.mock('streamdown', () => ({
  Streamdown: (props: { children: string }) => <div>{props.children}</div>,
}));
vi.mock('@streamdown/code', () => ({ code: {} }));
vi.mock('@streamdown/cjk', () => ({ cjk: {} }));
vi.mock('streamdown/styles.css', () => ({}));

function setThreadStreaming(threadId: string, items: Map<string, unknown>, isStreaming = true, revision = 1): void {
  const next = new Map(useMessageStore.getState().streamingByThread);
  next.set(threadId, {
    streamingView: { turnId: 'turn-1', isStreaming, items: items as never, revision },
    streamingItemOrder: new Map(),
  });
  useMessageStore.setState({ streamingByThread: next });
}

describe('StreamingAgentBody', () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
    useMessageStore.setState({ messagesByThread: new Map(), streamingByThread: new Map() });
  });

  it('renders visible body text', () => {
    setThreadStreaming('t1', new Map([
      ['a1', { threadId: 't1', turnId: 'turn-1', itemId: 'a1', order: 1, itemType: 'AgentMessage', agentText: 'Hello', reasoningSummary: [], reasoningRaw: [], planText: '' }],
    ]));
    render(<StreamingAgentBody threadId="t1" />);
    expect(screen.getByText('Hello')).toBeInTheDocument();
  });

  it('shows thinking placeholder when streaming with empty body', () => {
    setThreadStreaming('t1', new Map());
    render(<StreamingAgentBody threadId="t1" />);
    expect(screen.getByText('思考中...')).toBeInTheDocument();
  });
});
