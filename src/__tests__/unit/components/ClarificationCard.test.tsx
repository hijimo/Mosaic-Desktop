import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
    useClarificationStore.setState({ requests: new Map() });
  });

  it('submits selected clarification answers as user_input_answer', async () => {
    const user = userEvent.setup();

    useClarificationStore.getState().addRequest({
      id: 'clarify-1',
      threadId: 'thread-1',
      message: '请选择部署环境',
      questions: [
        {
          text: '部署环境',
          options: ['staging', 'production'],
          is_other: true,
        },
      ],
    });

    render(
      <ClarificationCard
        request={useClarificationStore.getState().requests.get('clarify-1')!}
      />,
    );

    await user.click(screen.getByRole('button', { name: 'staging' }));
    await user.click(screen.getByRole('button', { name: '提交答案' }));

    await waitFor(() => {
      expect(submitOpMock).toHaveBeenCalledWith('thread-1', {
        type: 'user_input_answer',
        id: 'clarify-1',
        response: {
          answers: [
            {
              question: '部署环境',
              choice: 'staging',
              source: 'option',
            },
          ],
        },
      });
    });

    expect(useClarificationStore.getState().requests.has('clarify-1')).toBe(false);
  });
});
