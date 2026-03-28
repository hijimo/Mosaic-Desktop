import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { Message } from '@/components/chat/Message';
import type { TurnGroup } from '@/types';

// Mock Streamdown to avoid Tailwind dependency in tests
vi.mock('streamdown', () => ({
  Streamdown: ({ children }: { children: string }) => <div>{children}</div>,
}));
vi.mock('@streamdown/code', () => ({ code: {} }));
vi.mock('@streamdown/cjk', () => ({ cjk: {} }));
vi.mock('streamdown/styles.css', () => ({}));
vi.mock('@/components/chat/agent/MessageActionBar', () => ({
  MessageActionBar: ({ messageId }: { messageId: string }) => (
    <div data-testid='message-action-bar'>{messageId}</div>
  ),
}));

describe('Message', () => {
  it('renders user message text', () => {
    const group: TurnGroup = {
      turn_id: 'turn-1',
      items: [{ type: 'UserMessage', id: 'u1', content: [{ type: 'text', text: 'Hello, AI!', text_elements: [] }] }],
    };
    render(<Message group={group} />);
    expect(screen.getByText('Hello, AI!')).toBeInTheDocument();
  });

  it('renders agent message text via Streamdown', () => {
    const group: TurnGroup = {
      turn_id: 'turn-1',
      items: [{ type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: 'Here is my response.' }] }],
    };
    render(<Message group={group} />);
    expect(screen.getByText('Here is my response.')).toBeInTheDocument();
  });

  it('renders tool calls within agent message', () => {
    const group: TurnGroup = {
      turn_id: 'turn-1',
      items: [{ type: 'AgentMessage', id: 'a3', content: [{ type: 'Text', text: 'Running command...' }] }],
    };
    const toolCalls = [{ callId: 'tc1', type: 'exec' as const, status: 'running' as const, name: 'ls', command: ['ls'] }];
    render(<Message group={group} toolCalls={toolCalls} />);
    expect(screen.getByText(/bash/)).toBeInTheDocument();
    expect(screen.getByText('Running command...')).toBeInTheDocument();
  });

  it('renders approval requests within agent message', () => {
    const group: TurnGroup = {
      turn_id: 'turn-1',
      items: [{ type: 'AgentMessage', id: 'a4', content: [{ type: 'Text', text: 'Need approval' }] }],
    };
    const approvalRequests = [{ callId: 'ar1', turnId: 't1', type: 'exec' as const, command: ['rm', '-rf'] }];
    render(<Message group={group} approvalRequests={approvalRequests} />);
    expect(screen.getByText('需要执行审批')).toBeInTheDocument();
    expect(screen.getByText('批准执行')).toBeInTheDocument();
  });

  it('renders reasoning item as ThinkingPanel', () => {
    const group: TurnGroup = {
      turn_id: 'turn-1',
      items: [{ type: 'Reasoning', id: 'r1', summary_text: ['Thinking about the problem...'], raw_content: [] }],
    };
    render(<Message group={group} />);
    expect(screen.getByText('思考过程')).toBeInTheDocument();
  });

  it('renders plan item via Streamdown', () => {
    const group: TurnGroup = {
      turn_id: 'turn-1',
      items: [{ type: 'Plan', id: 'p1', text: 'Step 1: Analyze\nStep 2: Implement' }],
    };
    render(<Message group={group} />);
    expect(screen.getByText(/Step 1: Analyze/)).toBeInTheDocument();
  });

  it('returns null for empty items', () => {
    const group: TurnGroup = { turn_id: 'turn-1', items: [] };
    const { container } = render(<Message group={group} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders a full turn with user + reasoning + agent message', () => {
    const group: TurnGroup = {
      turn_id: 'turn-1',
      items: [
        { type: 'UserMessage', id: 'u1', content: [{ type: 'text', text: 'What is 2+2?', text_elements: [] }] },
        { type: 'Reasoning', id: 'r1', summary_text: ['Calculating...'], raw_content: [] },
        { type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: 'The answer is 4.' }] },
      ],
    };
    render(<Message group={group} />);
    expect(screen.getByText('What is 2+2?')).toBeInTheDocument();
    expect(screen.getByText('思考过程')).toBeInTheDocument();
    expect(screen.getByText('The answer is 4.')).toBeInTheDocument();
  });

  it('renders multiple agent segments in one assistant message flow', () => {
    const group: TurnGroup = {
      turn_id: 'turn-1',
      items: [
        { type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: '我会先查找相关技能。' }] },
        {
          type: 'McpToolCall',
          id: 'mcp-1',
          server: 'filesystem',
          tool: 'read_file',
          status: 'Completed',
          arguments: { path: '/tmp/example.txt' },
        },
        { type: 'AgentMessage', id: 'a2', content: [{ type: 'Text', text: '我现在在搜索相关技能。' }] },
        {
          type: 'CommandExecution',
          id: 'cmd-1',
          command: 'rg desktop automation',
          cwd: '/tmp',
          status: 'Completed',
          command_actions: [],
          aggregated_output: 'desktop automation',
          exit_code: 0,
        },
        { type: 'AgentMessage', id: 'a3', content: [{ type: 'Text', text: '我查了几组关键词。' }] },
      ],
    };

    render(<Message group={group} />);

    expect(screen.getByTestId('agent-turn-content')).toBeInTheDocument();
    expect(screen.getAllByTestId('agent-message-segment')).toHaveLength(3);
    expect(screen.getByText('我会先查找相关技能。')).toBeInTheDocument();
    expect(screen.getByText('我现在在搜索相关技能。')).toBeInTheDocument();
    expect(screen.getByText('我查了几组关键词。')).toBeInTheDocument();
    expect(screen.getByText(/read_file/)).toBeInTheDocument();
    expect(screen.getByText(/bash/)).toBeInTheDocument();
  });

  it('renders action bar for the first agent message', () => {
    const group: TurnGroup = {
      turn_id: 'turn-1',
      items: [
        { type: 'Reasoning', id: 'r1', summary_text: ['思考中'], raw_content: [] },
        { type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: '最终回答' }] },
        { type: 'AgentMessage', id: 'a2', content: [{ type: 'Text', text: '补充说明' }] },
      ],
    };

    render(<Message group={group} />);

    expect(screen.getByTestId('message-action-bar')).toHaveTextContent('a1');
  });
});
