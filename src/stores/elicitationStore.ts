import { create } from 'zustand';
import type { ElicitationRequestState } from '@/types';

interface ElicitationStoreState {
  byThread: Map<string, Map<string, ElicitationRequestState>>;

  addRequest: (threadId: string, req: ElicitationRequestState) => void;
  removeRequest: (threadId: string, requestId: string) => void;
  clearThread: (threadId: string) => void;
}

export const useElicitationStore = create<ElicitationStoreState>((set) => ({
  byThread: new Map(),

  addRequest: (threadId, req) =>
    set((state) => {
      const outer = new Map(state.byThread);
      const inner = new Map(outer.get(threadId) ?? []);
      inner.set(req.requestId, req);
      outer.set(threadId, inner);
      return { byThread: outer };
    }),

  removeRequest: (threadId, requestId) =>
    set((state) => {
      const inner = state.byThread.get(threadId);
      if (!inner?.has(requestId)) return state;
      const outer = new Map(state.byThread);
      const nextInner = new Map(inner);
      nextInner.delete(requestId);
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
