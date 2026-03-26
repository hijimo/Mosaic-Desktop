import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { Message } from '@/components/chat/Message';
import type { TurnItem } from '@/types';

// Mock Streamdown to avoid Tailwind dependency in tests
vi.mock('streamdown', () => ({
  Streamdown: ({ children }: { children: string }) => <div>{children}</div>,
}));
vi.mock('@streamdown/code', () => ({ code: {} }));
vi.mock('@streamdown/cjk', () => ({ cjk: {} }));
vi.mock('streamdown/styles.css', () => ({}));

describe('Message', () => {
  it('renders user message text', () => {
    const item: TurnItem = {
      type: 'UserMessage',
      id: 'u1',
      content: [{ type: 'text', text: 'Hello, AI!', text_elements: [] }],
    };
    render(<Message item={item} />);
    expect(screen.getByText('Hello, AI!')).toBeInTheDocument();
  });

  it('renders agent message text via Streamdown', () => {
    const item: TurnItem = {
      type: 'AgentMessage',
      id: 'a1',
      content: [{ type: 'Text', text: 'Here is my response.' }],
    };
    render(<Message item={item} />);
    expect(screen.getByText('Here is my response.')).toBeInTheDocument();
  });

  it('renders tool calls within agent message', () => {
    const item: TurnItem = {
      type: 'AgentMessage',
      id: 'a3',
      content: [{ type: 'Text', text: 'Running command...' }],
    };
    const toolCalls = [{ callId: 'tc1', type: 'exec' as const, status: 'running' as const, name: 'ls', command: ['ls'] }];
    render(<Message item={item} toolCalls={toolCalls} />);
    // CodeExecutionBlock renders the terminal title with command name
    expect(screen.getByText(/bash/)).toBeInTheDocument();
    expect(screen.getByText('Running command...')).toBeInTheDocument();
  });

  it('renders approval requests within agent message', () => {
    const item: TurnItem = {
      type: 'AgentMessage',
      id: 'a4',
      content: [{ type: 'Text', text: 'Need approval' }],
    };
    const approvalRequests = [{ callId: 'ar1', turnId: 't1', type: 'exec' as const, command: ['rm', '-rf'] }];
    render(<Message item={item} approvalRequests={approvalRequests} />);
    // ApprovalRequestCard renders Chinese text
    expect(screen.getByText('需要执行审批')).toBeInTheDocument();
    expect(screen.getByText('批准执行')).toBeInTheDocument();
  });

  it('renders reasoning turn as ThinkingPanel', () => {
    const item: TurnItem = {
      type: 'Reasoning',
      id: 'r1',
      summary_text: ['Thinking about the problem...'],
      raw_content: [],
    };
    render(<Message item={item} />);
    // ThinkingPanel renders "思考过程" label
    expect(screen.getByText('思考过程')).toBeInTheDocument();
  });

  it('renders plan turn via Streamdown', () => {
    const item: TurnItem = {
      type: 'Plan',
      id: 'p1',
      text: 'Step 1: Analyze\nStep 2: Implement',
    };
    render(<Message item={item} />);
    // Plan text rendered through mocked Streamdown (preserves newlines in div)
    expect(screen.getByText(/Step 1: Analyze/)).toBeInTheDocument();
  });

  it('returns null for unknown turn types', () => {
    const item: TurnItem = { type: 'ContextCompaction', id: 'cc1' };
    const { container } = render(<Message item={item} />);
    expect(container.firstChild).toBeNull();
  });
});
