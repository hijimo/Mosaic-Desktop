import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { StreamingTurnRoot } from '@/components/chat/streaming/StreamingTurnRoot';
import { useMessageStore } from '@/stores/messageStore';
import { useToolCallStore } from '@/stores/toolCallStore';
import { useApprovalStore } from '@/stores/approvalStore';
import { useClarificationStore } from '@/stores/clarificationStore';

vi.mock('streamdown', () => ({
  Streamdown: ({ children }: { children: string }) => <div>{children}</div>,
}));
vi.mock('@streamdown/code', () => ({ code: {} }));
vi.mock('@streamdown/cjk', () => ({ cjk: {} }));
vi.mock('streamdown/styles.css', () => ({}));
vi.mock('@/components/chat/shared/AgentAvatar', () => ({
  AgentAvatar: () => <div data-testid='agent-avatar'>M</div>,
}));

function setThreadStreaming(threadId: string, view: Record<string, unknown>, itemOrder?: Map<string, number>): void {
  const next = new Map(useMessageStore.getState().streamingByThread);
  next.set(threadId, {
    streamingView: view as never,
    streamingItemOrder: itemOrder ?? new Map(),
  });
  useMessageStore.setState({ streamingByThread: next });
}

describe('StreamingTurnRoot', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useMessageStore.setState({ messagesByThread: new Map(), streamingByThread: new Map() });
    useToolCallStore.setState({ byThread: new Map() });
    useApprovalStore.setState({ byThread: new Map() });
    useClarificationStore.setState({ byThread: new Map() });
  });

  it('renders streaming body with tool calls', () => {
    setThreadStreaming('t1', {
      turnId: 'turn-1', isStreaming: true, revision: 1,
      items: new Map([['a1', {
        threadId: 't1', turnId: 'turn-1', itemId: 'a1', order: 1, itemType: 'AgentMessage',
        agentText: '我正在查找技能。', reasoningSummary: [], reasoningRaw: [], planText: '',
      }]]),
    }, new Map([['a1', 1]]));

    useToolCallStore.getState().beginToolCall('t1', {
      callId: 'tool-1', type: 'mcp', status: 'completed', name: 'read_file',
      serverName: 'filesystem', toolName: 'read_file', arguments: { path: '/tmp/demo.txt' },
    });

    render(<StreamingTurnRoot threadId='t1' />);
    const container = screen.getByTestId('agent-turn-content');
    expect(container).toContainElement(screen.getByText('我正在查找技能。'));
    expect(container).toContainElement(screen.getByText(/read_file/));
  });

  it('does not show streaming from another thread', () => {
    setThreadStreaming('t2', {
      turnId: 'turn-other', isStreaming: true, revision: 1,
      items: new Map([['a1', {
        threadId: 't2', turnId: 'turn-other', itemId: 'a1', order: 1, itemType: 'AgentMessage',
        agentText: 'wrong thread', reasoningSummary: [], reasoningRaw: [], planText: '',
      }]]),
    });

    const { container } = render(<StreamingTurnRoot threadId='t1' />);
    expect(container.textContent).not.toContain('wrong thread');
  });
});
