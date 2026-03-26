import { create } from 'zustand';
import type { TurnItem } from '@/types';

interface StreamingItem {
  threadId: string;
  turnId: string;
  itemId: string;
  itemType: 'AgentMessage' | 'Reasoning' | 'Plan';
  agentText: string;
  reasoningSummary: string[];
  reasoningRaw: string[];
  planText: string;
}

interface StreamingTurn {
  turnId: string;
  agentText: string;
  isStreaming: boolean;
  /** Active streaming items keyed by item_id */
  items: Map<string, StreamingItem>;
}

interface MessageState {
  messagesByThread: Map<string, TurnItem[]>;
  streamingTurn: StreamingTurn | null;

  appendMessage: (threadId: string, item: TurnItem) => void;
  setMessages: (threadId: string, items: TurnItem[]) => void;
  startStreaming: (turnId: string) => void;
  stopStreaming: () => void;
  clearThread: (threadId: string) => void;

  /** v2 structured item tracking */
  startStreamingItem: (threadId: string, turnId: string, item: TurnItem) => void;
  updateAgentContentDelta: (itemId: string, delta: string) => void;
  updateReasoningContentDelta: (itemId: string, delta: string, summaryIndex: number) => void;
  updateReasoningRawContentDelta: (itemId: string, delta: string, contentIndex: number) => void;
  updatePlanDelta: (itemId: string, delta: string) => void;
  completeStreamingItem: (threadId: string, item: TurnItem) => void;
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

  setMessages: (threadId, items) =>
    set((state) => {
      const next = new Map(state.messagesByThread);
      next.set(threadId, items);
      return { messagesByThread: next };
    }),

  startStreaming: (turnId) =>
    set({ streamingTurn: { turnId, agentText: '', isStreaming: true, items: new Map() } }),

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

  // ── v2 structured item methods ──

  startStreamingItem: (threadId, turnId, item) =>
    set((state) => {
      if (!state.streamingTurn) return state;
      const items = new Map(state.streamingTurn.items);
      const itemId = getItemId(item);
      const itemType = item.type === 'AgentMessage' ? 'AgentMessage'
        : item.type === 'Reasoning' ? 'Reasoning'
        : item.type === 'Plan' ? 'Plan'
        : 'AgentMessage';
      items.set(itemId, {
        threadId,
        turnId,
        itemId,
        itemType: itemType as StreamingItem['itemType'],
        agentText: '',
        reasoningSummary: [],
        reasoningRaw: [],
        planText: '',
      });
      return {
        streamingTurn: { ...state.streamingTurn, items },
      };
    }),

  updateAgentContentDelta: (itemId, delta) =>
    set((state) => {
      if (!state.streamingTurn) return state;
      const items = new Map(state.streamingTurn.items);
      const existing = items.get(itemId);
      if (!existing) return state;
      items.set(itemId, { ...existing, agentText: existing.agentText + delta });
      return {
        streamingTurn: {
          ...state.streamingTurn,
          // Also update the top-level agentText for backward compatibility
          agentText: state.streamingTurn.agentText + delta,
          items,
        },
      };
    }),

  updateReasoningContentDelta: (itemId, delta, summaryIndex) =>
    set((state) => {
      if (!state.streamingTurn) return state;
      const items = new Map(state.streamingTurn.items);
      const existing = items.get(itemId);
      if (!existing) return state;
      const summary = [...existing.reasoningSummary];
      while (summary.length <= summaryIndex) {
        summary.push('');
      }
      summary[summaryIndex] += delta;
      items.set(itemId, { ...existing, reasoningSummary: summary });
      return {
        streamingTurn: { ...state.streamingTurn, items },
      };
    }),

  updateReasoningRawContentDelta: (itemId, delta, contentIndex) =>
    set((state) => {
      if (!state.streamingTurn) return state;
      const items = new Map(state.streamingTurn.items);
      const existing = items.get(itemId);
      if (!existing) return state;
      const raw = [...existing.reasoningRaw];
      while (raw.length <= contentIndex) {
        raw.push('');
      }
      raw[contentIndex] += delta;
      items.set(itemId, { ...existing, reasoningRaw: raw });
      return {
        streamingTurn: { ...state.streamingTurn, items },
      };
    }),

  updatePlanDelta: (itemId, delta) =>
    set((state) => {
      if (!state.streamingTurn) return state;
      const items = new Map(state.streamingTurn.items);
      const existing = items.get(itemId);
      if (!existing) return state;
      items.set(itemId, { ...existing, planText: existing.planText + delta });
      return {
        streamingTurn: { ...state.streamingTurn, items },
      };
    }),

  completeStreamingItem: (threadId, item) =>
    set((state) => {
      // Append the completed item to the message list
      const next = new Map(state.messagesByThread);
      const msgs = next.get(threadId) ?? [];
      // Deduplicate: don't add if an item with the same id already exists
      const itemId = getItemId(item);
      if (!msgs.some((m) => getItemId(m) === itemId)) {
        next.set(threadId, [...msgs, item]);
      }
      // Remove from streaming items
      if (state.streamingTurn) {
        const items = new Map(state.streamingTurn.items);
        items.delete(itemId);
        return {
          messagesByThread: next,
          streamingTurn: { ...state.streamingTurn, items },
        };
      }
      return { messagesByThread: next };
    }),
}));

function getItemId(item: TurnItem): string {
  return item.id ?? '';
}
