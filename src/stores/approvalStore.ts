import { create } from 'zustand';
import type { ApprovalRequestState } from '@/types';

interface ApprovalStoreState {
  byThread: Map<string, Map<string, ApprovalRequestState>>;

  addApproval: (threadId: string, approval: ApprovalRequestState) => void;
  removeApproval: (threadId: string, callId: string) => void;
  clearThread: (threadId: string) => void;
}

export const useApprovalStore = create<ApprovalStoreState>((set) => ({
  byThread: new Map(),

  addApproval: (threadId, approval) =>
    set((state) => {
      const outer = new Map(state.byThread);
      const inner = new Map(outer.get(threadId) ?? []);
      inner.set(approval.callId, approval);
      outer.set(threadId, inner);
      return { byThread: outer };
    }),

  removeApproval: (threadId, callId) =>
    set((state) => {
      const inner = state.byThread.get(threadId);
      if (!inner?.has(callId)) return state;
      const outer = new Map(state.byThread);
      const nextInner = new Map(inner);
      nextInner.delete(callId);
      outer.set(threadId, nextInner);
      return { byThread: outer };
    }),

  clearThread: (threadId) =>
    set((state) => {
      if (!state.byThread.has(threadId)) return state;
      const outer = new Map(state.byThread);
      outer.delete(threadId);
      return { byThread: outer };
    }),
}));
