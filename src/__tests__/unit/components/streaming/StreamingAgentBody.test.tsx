import { render, screen, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { StreamingAgentBody } from '@/components/chat/streaming/StreamingAgentBody';
import { useMessageStore } from '@/stores/messageStore';

const streamdownSpy = vi.fn(({ children }: { children: string }) => (
  <div>{children}</div>
));

vi.mock('streamdown', () => ({
  Streamdown: (props: { children: string }) => {
    streamdownSpy(props);
    return <div>{props.children}</div>;
  },
}));
vi.mock('@streamdown/code', () => ({ code: {} }));
vi.mock('@streamdown/cjk', () => ({ cjk: {} }));
vi.mock('streamdown/styles.css', () => ({}));

describe('StreamingAgentBody', () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
    useMessageStore.setState({
      messagesByThread: new Map(),
      streamingTurn: null,
      streamingBuffer: null,
      streamingView: null,
    });
  });

  it('renders visible body text from streamingView', () => {
    useMessageStore.setState({
      streamingView: {
        turnId: 'turn-1',
        isStreaming: true,
        revision: 1,
        items: new Map([
          ['a1', {
            threadId: 't1',
            turnId: 'turn-1',
            itemId: 'a1',
            itemType: 'AgentMessage',
            agentText: 'Hello',
            reasoningSummary: [],
            reasoningRaw: [],
            planText: '',
          }],
        ]),
      },
    });

    render(<StreamingAgentBody />);
    expect(screen.getByText('Hello')).toBeInTheDocument();
  });

  it('shows thinking placeholder when streaming with empty visible body', () => {
    useMessageStore.setState({
      streamingView: {
        turnId: 'turn-1',
        isStreaming: true,
        revision: 0,
        items: new Map(),
      },
    });

    render(<StreamingAgentBody />);
    expect(screen.getByText('思考中...')).toBeInTheDocument();
  });

  it('passes stability-first props to Streamdown while streaming', () => {
    useMessageStore.setState({
      streamingView: {
        turnId: 'turn-1',
        isStreaming: true,
        revision: 1,
        items: new Map([
          ['a1', {
            threadId: 't1',
            turnId: 'turn-1',
            itemId: 'a1',
            itemType: 'AgentMessage',
            agentText: 'Hello',
            reasoningSummary: [],
            reasoningRaw: [],
            planText: '',
          }],
        ]),
      },
    });

    render(<StreamingAgentBody />);
    expect(streamdownSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        children: 'Hello',
        isAnimating: false,
        parseIncompleteMarkdown: true,
        className: 'streaming-stable-markdown',
      }),
    );
  });
});
