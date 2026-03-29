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
      streamingItemOrder: new Map(),
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
            order: 1,
            itemType: 'AgentMessage',
            agentText: '我正在查找技能。',
            reasoningSummary: [],
            reasoningRaw: [],
            planText: '',
          }],
        ]),
      },
      streamingItemOrder: new Map([['a1', 1]]),
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
    const container = screen.getByTestId('agent-turn-content');
    expect(container).toBeInTheDocument();
    expect(container).toContainElement(screen.getByText('我正在查找技能。'));
    expect(container).toContainElement(screen.getByText(/read_file/));
  });

  it('does not duplicate content when completed turn items and streaming items share the same text', () => {
    useMessageStore.setState({
      messagesByThread: new Map([
        ['t1', [{
          turn_id: 'turn-1',
          items: [
            {
              type: 'AgentMessage',
              id: 'a1',
              content: [{ type: 'Text', text: '使用技能： find-skills，用来帮你查找“桌面自动化”相关可安装技能。' }],
            },
          ],
        }]],
      ]),
      streamingView: {
        turnId: 'turn-1',
        isStreaming: true,
        revision: 2,
        items: new Map([
          ['a1-streaming', {
            threadId: 't1',
            turnId: 'turn-1',
            itemId: 'a1-streaming',
            order: 2,
            itemType: 'AgentMessage',
            agentText: '使用技能： find-skills，用来帮你查找“桌面自动化”相关可安装技能。',
            reasoningSummary: [],
            reasoningRaw: [],
            planText: '',
          }],
        ]),
      },
      streamingItemOrder: new Map([['a1-streaming', 2]]),
    });

    render(<StreamingTurnRoot threadId='t1' />);

    expect(
      screen.getAllByText('使用技能： find-skills，用来帮你查找“桌面自动化”相关可安装技能。'),
    ).toHaveLength(1);
  });

  it('renders active tool calls in event order instead of pinning them to the top', () => {
    useMessageStore.setState({
      messagesByThread: new Map([
        ['t1', [{
          turn_id: 'turn-1',
          items: [
            {
              type: 'AgentMessage',
              id: 'a1',
              content: [{ type: 'Text', text: '第一段回答。' }],
            },
          ],
        }]],
      ]),
      streamingView: {
        turnId: 'turn-1',
        isStreaming: true,
        revision: 2,
        items: new Map([
          ['a2', {
            threadId: 't1',
            turnId: 'turn-1',
            itemId: 'a2',
            order: 3,
            itemType: 'AgentMessage',
            agentText: '第二段回答。',
            reasoningSummary: [],
            reasoningRaw: [],
            planText: '',
          }],
        ]),
      },
      streamingItemOrder: new Map([
        ['a1', 1],
        ['a2', 3],
      ]),
    });
    useToolCallStore.setState({
      toolCalls: new Map([
        ['tool-1', {
          callId: 'tool-1',
          type: 'mcp',
          status: 'completed',
          order: 2,
          name: 'read_file',
          serverName: 'filesystem',
          toolName: 'read_file',
          arguments: { path: '/tmp/demo.txt' },
        }],
      ]),
    });

    render(<StreamingTurnRoot threadId='t1' />);

    const content = screen.getByTestId('agent-turn-content').textContent ?? '';
    expect(content.indexOf('第一段回答。')).toBeLessThan(content.indexOf('read_file'));
    expect(content.indexOf('read_file')).toBeLessThan(content.indexOf('第二段回答。'));
  });
});
