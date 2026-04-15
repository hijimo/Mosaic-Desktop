import { create } from 'zustand';
import type { ClarificationState } from '@/types';

interface ClarificationStoreState {
  byThread: Map<string, Map<string, ClarificationState>>;

  addRequest: (threadId: string, req: ClarificationState) => void;
  removeRequest: (threadId: string, id: string) => void;
  clearThread: (threadId: string) => void;
}

export const useClarificationStore = create<ClarificationStoreState>((set) => ({
  byThread: new Map(),

  addRequest: (threadId, req) =>
    set((state) => {
      const outer = new Map(state.byThread);
      const inner = new Map(outer.get(threadId) ?? []);
      inner.set(req.id, req);
      outer.set(threadId, inner);
      return { byThread: outer };
    }),

  removeRequest: (threadId, id) =>
    set((state) => {
      const inner = state.byThread.get(threadId);
      if (!inner?.has(id)) return state;
      const outer = new Map(state.byThread);
      const nextInner = new Map(inner);
      nextInner.delete(id);
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
