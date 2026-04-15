import { render } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ClarificationCard } from '@/components/chat/agent/ClarificationCard';
import { useClarificationStore } from '@/stores/clarificationStore';

const submitOpMock = vi.fn();

vi.mock('@/hooks/useSubmitOp', () => ({
  useSubmitOp: () => submitOpMock,
}));

describe('ClarificationCard', () => {
  beforeEach(() => {
    submitOpMock.mockReset();
    submitOpMock.mockResolvedValue(undefined);
    useClarificationStore.setState({ byThread: new Map() });
  });

  it('renders clarification request from per-thread store', () => {
    useClarificationStore.getState().addRequest('thread-1', {
      id: 'clarify-1',
      message: '请选择部署环境',
      schema: undefined,
    });

    const threadRequests = useClarificationStore.getState().byThread.get('thread-1')!;
    const request = threadRequests.get('clarify-1')!;
    render(<ClarificationCard request={request} />);
    expect(threadRequests.has('clarify-1')).toBe(true);
  });
});
