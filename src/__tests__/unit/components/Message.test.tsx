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
});
