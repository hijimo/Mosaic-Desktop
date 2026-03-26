import { create } from 'zustand';
import type { ClarificationState } from '@/types';

interface ClarificationStoreState {
  requests: Map<string, ClarificationState>;
  addRequest: (req: ClarificationState) => void;
  removeRequest: (id: string) => void;
  clearAll: () => void;
}

export const useClarificationStore = create<ClarificationStoreState>((set) => ({
  requests: new Map(),

  addRequest: (req) =>
    set((state) => {
      const next = new Map(state.requests);
      next.set(req.id, req);
      return { requests: next };
    }),

  removeRequest: (id) =>
    set((state) => {
      const next = new Map(state.requests);
      next.delete(id);
      return { requests: next };
    }),

  clearAll: () => set({ requests: new Map() }),
}));
