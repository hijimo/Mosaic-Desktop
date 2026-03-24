import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { Message } from '@/components/chat/Message';
import type { TurnItem } from '@/types';

// Mock child components
vi.mock('@/components/chat/ToolCallDisplay', () => ({
  ToolCallDisplay: ({ toolCall }: { toolCall: { callId: string } }) => (
    <div data-testid={`tool-call-${toolCall.callId}`} />
  ),
}));
vi.mock('@/components/chat/ApprovalRequest', () => ({
  ApprovalRequest: ({ request }: { request: { callId: string } }) => (
    <div data-testid={`approval-${request.callId}`} />
  ),
}));

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

  it('renders agent message text', () => {
    const item: TurnItem = {
      type: 'AgentMessage',
      id: 'a1',
      content: [{ type: 'Text', text: 'Here is my response.' }],
    };
    render(<Message item={item} />);
    expect(screen.getByText('Here is my response.')).toBeInTheDocument();
  });

  it('renders action bar for agent messages', () => {
    const item: TurnItem = {
      type: 'AgentMessage',
      id: 'a2',
      content: [{ type: 'Text', text: 'Response' }],
    };
    render(<Message item={item} />);
    expect(screen.getByText('Helpful')).toBeInTheDocument();
    expect(screen.getByText('Not Helpful')).toBeInTheDocument();
    expect(screen.getByText('Regenerate')).toBeInTheDocument();
  });

  it('renders tool calls within agent message', () => {
    const item: TurnItem = {
      type: 'AgentMessage',
      id: 'a3',
      content: [{ type: 'Text', text: 'Running command...' }],
    };
    const toolCalls = [{ callId: 'tc1', type: 'exec' as const, status: 'running' as const, name: 'ls' }];
    render(<Message item={item} toolCalls={toolCalls} />);
    expect(screen.getByTestId('tool-call-tc1')).toBeInTheDocument();
  });

  it('renders approval requests within agent message', () => {
    const item: TurnItem = {
      type: 'AgentMessage',
      id: 'a4',
      content: [{ type: 'Text', text: 'Need approval' }],
    };
    const approvalRequests = [{ callId: 'ar1', turnId: 't1', type: 'exec' as const, command: ['rm', '-rf'] }];
    render(<Message item={item} approvalRequests={approvalRequests} />);
    expect(screen.getByTestId('approval-ar1')).toBeInTheDocument();
  });

  it('renders reasoning turn', () => {
    const item: TurnItem = {
      type: 'Reasoning',
      id: 'r1',
      summary_text: ['Thinking about the problem...'],
      raw_content: [],
    };
    render(<Message item={item} />);
    expect(screen.getByText('Reasoning')).toBeInTheDocument();
    expect(screen.getByText('Thinking about the problem...')).toBeInTheDocument();
  });

  it('renders plan turn', () => {
    const item: TurnItem = {
      type: 'Plan',
      id: 'p1',
      text: 'Step 1: Analyze\nStep 2: Implement',
    };
    render(<Message item={item} />);
    expect(screen.getByText('Plan')).toBeInTheDocument();
  });

  it('returns null for unknown turn types', () => {
    const item: TurnItem = { type: 'ContextCompaction', id: 'cc1' };
    const { container } = render(<Message item={item} />);
    expect(container.firstChild).toBeNull();
  });
});
