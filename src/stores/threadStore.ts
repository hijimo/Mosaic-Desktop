import { create } from 'zustand';
import type { ThreadMeta } from '@/types';

interface ThreadState {
  threads: Map<string, ThreadMeta>;
  activeThreadId: string | null;

  setActiveThread: (id: string | null) => void;
  addThread: (meta: ThreadMeta) => void;
  removeThread: (id: string) => void;
  updateThread: (id: string, patch: Partial<ThreadMeta>) => void;
}

export const useThreadStore = create<ThreadState>((set) => ({
  threads: new Map(),
  activeThreadId: null,

  setActiveThread: (id) => set({ activeThreadId: id }),

  addThread: (meta) =>
    set((state) => {
      const next = new Map(state.threads);
      next.set(meta.thread_id, meta);
      return { threads: next };
    }),

  removeThread: (id) =>
    set((state) => {
      const next = new Map(state.threads);
      next.delete(id);
      return {
        threads: next,
        activeThreadId: state.activeThreadId === id ? null : state.activeThreadId,
      };
    }),

  updateThread: (id, patch) =>
    set((state) => {
      const existing = state.threads.get(id);
      if (!existing) return state;
      const next = new Map(state.threads);
      next.set(id, { ...existing, ...patch });
      return { threads: next };
    }),
}));
