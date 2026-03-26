import { create } from 'zustand';
import type { ToolCallState } from '@/types';

interface ToolCallStoreState {
  /** Active tool calls keyed by call_id */
  toolCalls: Map<string, ToolCallState>;

  beginToolCall: (tc: ToolCallState) => void;
  updateToolCallOutput: (callId: string, delta: string) => void;
  endToolCall: (callId: string, updates: Partial<ToolCallState>) => void;
  clearAll: () => void;
}

export const useToolCallStore = create<ToolCallStoreState>((set) => ({
  toolCalls: new Map(),

  beginToolCall: (tc) =>
    set((state) => {
      const next = new Map(state.toolCalls);
      next.set(tc.callId, tc);
      return { toolCalls: next };
    }),

  updateToolCallOutput: (callId, delta) =>
    set((state) => {
      const existing = state.toolCalls.get(callId);
      if (!existing) return state;
      const next = new Map(state.toolCalls);
      next.set(callId, { ...existing, output: (existing.output ?? '') + delta });
      return { toolCalls: next };
    }),

  endToolCall: (callId, updates) =>
    set((state) => {
      const existing = state.toolCalls.get(callId);
      if (!existing) return state;
      const next = new Map(state.toolCalls);
      next.set(callId, { ...existing, ...updates });
      return { toolCalls: next };
    }),

  clearAll: () => set({ toolCalls: new Map() }),
}));
