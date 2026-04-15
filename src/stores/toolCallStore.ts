import { create } from 'zustand';
import type { ToolCallState } from '@/types';

interface ToolCallStoreState {
  /** Tool calls keyed by threadId → callId */
  byThread: Map<string, Map<string, ToolCallState>>;

  beginToolCall: (threadId: string, tc: ToolCallState) => void;
  updateToolCallOutput: (threadId: string, callId: string, delta: string) => void;
  endToolCall: (threadId: string, callId: string, updates: Partial<ToolCallState>) => void;
  clearThread: (threadId: string) => void;
}

export const useToolCallStore = create<ToolCallStoreState>((set) => ({
  byThread: new Map(),

  beginToolCall: (threadId, tc) =>
    set((state) => {
      const outer = new Map(state.byThread);
      const inner = new Map(outer.get(threadId) ?? []);
      inner.set(tc.callId, tc);
      outer.set(threadId, inner);
      return { byThread: outer };
    }),

  updateToolCallOutput: (threadId, callId, delta) =>
    set((state) => {
      const inner = state.byThread.get(threadId);
      const existing = inner?.get(callId);
      if (!existing) return state;
      const outer = new Map(state.byThread);
      const nextInner = new Map(inner);
      nextInner.set(callId, { ...existing, output: (existing.output ?? '') + delta });
      outer.set(threadId, nextInner);
      return { byThread: outer };
    }),

  endToolCall: (threadId, callId, updates) =>
    set((state) => {
      const inner = state.byThread.get(threadId);
      const existing = inner?.get(callId);
      if (!existing) return state;
      const outer = new Map(state.byThread);
      const nextInner = new Map(inner);
      nextInner.set(callId, { ...existing, ...updates });
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
