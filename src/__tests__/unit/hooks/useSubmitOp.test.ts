import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useSubmitOp } from '@/hooks/useSubmitOp';
import type { Op } from '@/types';

vi.mock('@tauri-apps/api/core');
vi.mock('uuid', () => ({ v4: () => 'mock-uuid' }));

describe('useSubmitOp', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('submits op with generated uuid', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSubmitOp());
    const op: Op = { type: 'interrupt' };

    await act(async () => {
      await result.current('t1', op);
    });

    expect(invoke).toHaveBeenCalledWith('submit_op', {
      thread_id: 't1',
      id: 'mock-uuid',
      op: { type: 'interrupt' },
    });
  });

  it('submits user_turn op correctly', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSubmitOp());
    const op: Op = {
      type: 'user_turn',
      items: [{ type: 'text', text: 'hello', text_elements: [] }],
      cwd: '.',
      model: 'gpt-4o',
      approval_policy: 'on-request',
      sandbox_policy: { type: 'danger-full-access' },
    };

    await act(async () => {
      await result.current('t1', op);
    });

    expect(invoke).toHaveBeenCalledWith('submit_op', {
      thread_id: 't1',
      id: 'mock-uuid',
      op,
    });
  });
});
