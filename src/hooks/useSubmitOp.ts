import { useCallback } from 'react';
import { v4 as uuidv4 } from 'uuid';
import { submitOp } from '@/services/api';
import type { Op } from '@/types';

/**
 * Returns a function to submit an Op to a given thread.
 */
export function useSubmitOp(): (threadId: string, op: Op) => Promise<void> {
  return useCallback(async (threadId: string, op: Op) => {
    await submitOp(threadId, uuidv4(), op);
  }, []);
}
