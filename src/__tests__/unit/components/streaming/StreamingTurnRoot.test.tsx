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

describe('StreamingTurnRoot', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useMessageStore.setState({
      messagesByThread: new Map(),
      streamingTurn: null,
      streamingBuffer: null,
      streamingView: null,
    });
    useToolCallStore.setState({ toolCalls: new Map() });
    useApprovalStore.setState({ approvals: new Map() });
    useClarificationStore.setState({ requests: new Map() });
  });

  it('renders tool calls and streaming body inside one assistant turn container', () => {
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
            agentText: '我正在查找技能。',
            reasoningSummary: [],
            reasoningRaw: [],
            planText: '',
          }],
        ]),
      },
    });
    useToolCallStore.setState({
      toolCalls: new Map([
        ['tool-1', {
          callId: 'tool-1',
          type: 'mcp',
          status: 'completed',
          name: 'read_file',
          serverName: 'filesystem',
          toolName: 'read_file',
          arguments: { path: '/tmp/demo.txt' },
        }],
      ]),
    });

    render(<StreamingTurnRoot threadId='t1' />);

    expect(screen.getAllByTestId('agent-avatar')).toHaveLength(1);
    const container = screen.getByTestId('streaming-agent-turn-content');
    expect(container).toBeInTheDocument();
    expect(container).toContainElement(screen.getByText('我正在查找技能。'));
    expect(container).toContainElement(screen.getByText(/read_file/));
  });
});
