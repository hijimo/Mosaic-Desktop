import { create } from 'zustand';
import type { TurnItem } from '@/types';

interface StreamingTurn {
  turnId: string;
  agentText: string;
  isStreaming: boolean;
}

interface MessageState {
  messagesByThread: Map<string, TurnItem[]>;
  streamingTurn: StreamingTurn | null;

  appendMessage: (threadId: string, item: TurnItem) => void;
  updateStreamingDelta: (delta: string) => void;
  startStreaming: (turnId: string) => void;
  stopStreaming: () => void;
  clearThread: (threadId: string) => void;
}

export const useMessageStore = create<MessageState>((set) => ({
  messagesByThread: new Map(),
  streamingTurn: null,

  appendMessage: (threadId, item) =>
    set((state) => {
      const next = new Map(state.messagesByThread);
      const msgs = next.get(threadId) ?? [];
      next.set(threadId, [...msgs, item]);
      return { messagesByThread: next };
    }),

  updateStreamingDelta: (delta) =>
    set((state) => {
      if (!state.streamingTurn) return state;
      return {
        streamingTurn: {
          ...state.streamingTurn,
          agentText: state.streamingTurn.agentText + delta,
        },
      };
    }),

  startStreaming: (turnId) =>
    set({ streamingTurn: { turnId, agentText: '', isStreaming: true } }),

  stopStreaming: () =>
    set((state) => {
      if (!state.streamingTurn) return state;
      return {
        streamingTurn: { ...state.streamingTurn, isStreaming: false },
      };
    }),

  clearThread: (threadId) =>
    set((state) => {
      const next = new Map(state.messagesByThread);
      next.delete(threadId);
      return { messagesByThread: next };
    }),
}));
