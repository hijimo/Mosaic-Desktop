import { create } from 'zustand';
import type { ElicitationRequestState } from '@/types';

interface ElicitationStoreState {
  requests: Map<string, ElicitationRequestState>;
  addRequest: (req: ElicitationRequestState) => void;
  removeRequest: (requestId: string) => void;
  clearAll: () => void;
}

export const useElicitationStore = create<ElicitationStoreState>((set) => ({
  requests: new Map(),

  addRequest: (req) =>
    set((state) => {
      const next = new Map(state.requests);
      next.set(req.requestId, req);
      return { requests: next };
    }),

  removeRequest: (requestId) =>
    set((state) => {
      const next = new Map(state.requests);
      next.delete(requestId);
      return { requests: next };
    }),

  clearAll: () => set({ requests: new Map() }),
}));
