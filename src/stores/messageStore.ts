import { create } from 'zustand';
import type { TurnItem, TurnGroup } from '@/types';

// ── Streaming data structures ──

interface StreamingViewItem {
  threadId: string;
  turnId: string;
  itemId: string;
  order: number;
  itemType: 'AgentMessage' | 'Reasoning' | 'Plan';
  agentText: string;
  reasoningSummary: string[];
  reasoningRaw: string[];
  planText: string;
}

interface StreamingViewTurn {
  turnId: string;
  isStreaming: boolean;
  items: Map<string, StreamingViewItem>;
  revision: number;
}

// ── Per-thread streaming state ──

interface ThreadStreamingState {
  streamingView: StreamingViewTurn;
  streamingItemOrder: Map<string, number>;
}

// ── Store interface ──

interface MessageState {
  messagesByThread: Map<string, TurnGroup[]>;
  streamingByThread: Map<string, ThreadStreamingState>;

  appendMessage: (threadId: string, turnId: string, item: TurnItem) => void;
  setMessages: (threadId: string, groups: TurnGroup[]) => void;
  dismissTurnError: (threadId: string, turnId: string) => void;
  clearThread: (threadId: string) => void;

  startStreaming: (threadId: string, turnId: string) => void;
  stopStreaming: (threadId: string) => void;
  startStreamingItem: (threadId: string, turnId: string, item: TurnItem, order?: number) => void;
  appendAgentContentDelta: (threadId: string, itemId: string, delta: string) => void;
  appendReasoningContentDelta: (threadId: string, itemId: string, delta: string, summaryIndex: number) => void;
  appendReasoningRawContentDelta: (threadId: string, itemId: string, delta: string, contentIndex: number) => void;
  appendPlanDelta: (threadId: string, itemId: string, delta: string) => void;
  completeStreamingItem: (threadId: string, turnId: string, item: TurnItem) => void;
}

export const useMessageStore = create<MessageState>((set) => ({
  messagesByThread: new Map(),
  streamingByThread: new Map(),

  // ── Message CRUD ──

  appendMessage: (threadId, turnId, item) =>
    set((state) => ({
      messagesByThread: appendItemToThread(state.messagesByThread, threadId, turnId, item),
    })),

  setMessages: (threadId, groups) =>
    set((state) => {
      const next = new Map(state.messagesByThread);
      next.set(threadId, groups);
      return { messagesByThread: next };
    }),

  dismissTurnError: (threadId, turnId) =>
    set((state) => {
      const groups = state.messagesByThread.get(threadId);
      if (!groups) return state;
      const next = new Map(state.messagesByThread);
      next.set(
        threadId,
        groups.map((g) =>
          g.turn_id === turnId ? { ...g, status: 'Dismissed' as const } : g,
        ),
      );
      return { messagesByThread: next };
    }),

  clearThread: (threadId) =>
    set((state) => {
      const nextMessages = new Map(state.messagesByThread);
      nextMessages.delete(threadId);
      const nextStreaming = new Map(state.streamingByThread);
      nextStreaming.delete(threadId);
      return { messagesByThread: nextMessages, streamingByThread: nextStreaming };
    }),

  // ── Streaming lifecycle ──

  startStreaming: (threadId, turnId) =>
    set((state) => {
      const next = new Map(state.streamingByThread);
      next.set(threadId, {
        streamingView: { turnId, isStreaming: true, items: new Map(), revision: 0 },
        streamingItemOrder: new Map(),
      });
      return { streamingByThread: next };
    }),

  stopStreaming: (threadId) =>
    set((state) => {
      const ts = state.streamingByThread.get(threadId);
      if (!ts) return state;
      const next = new Map(state.streamingByThread);
      next.set(threadId, {
        ...ts,
        streamingView: { ...ts.streamingView, isStreaming: false },
      });
      return { streamingByThread: next };
    }),

  startStreamingItem: (threadId, turnId, item, order = 0) =>
    set((state) => {
      const ts = state.streamingByThread.get(threadId);
      if (!ts) return state;

      const itemId = getItemId(item);
      const itemType = getStreamingItemType(item);
      if (!itemId) return state;

      const nextViewItems = new Map(ts.streamingView.items);
      nextViewItems.set(itemId, {
        threadId, turnId, itemId, order, itemType,
        agentText: '', reasoningSummary: [], reasoningRaw: [], planText: '',
      });

      const nextItemOrder = new Map(ts.streamingItemOrder);
      nextItemOrder.set(itemId, order);

      const next = new Map(state.streamingByThread);
      next.set(threadId, {
        streamingView: { ...ts.streamingView, items: nextViewItems, revision: ts.streamingView.revision + 1 },
        streamingItemOrder: nextItemOrder,
      });
      return { streamingByThread: next };
    }),

  // ── Delta: write directly to view ──

  appendAgentContentDelta: (threadId, itemId, delta) =>
    set((state) => {
      const ts = state.streamingByThread.get(threadId);
      if (!ts) return state;
      const existing = ts.streamingView.items.get(itemId);
      if (!existing) return state;

      const nextItems = new Map(ts.streamingView.items);
      nextItems.set(itemId, { ...existing, agentText: existing.agentText + delta });

      const next = new Map(state.streamingByThread);
      next.set(threadId, {
        ...ts,
        streamingView: { ...ts.streamingView, items: nextItems, revision: ts.streamingView.revision + 1 },
      });
      return { streamingByThread: next };
    }),

  appendReasoningContentDelta: (threadId, itemId, delta, summaryIndex) =>
    set((state) => {
      const ts = state.streamingByThread.get(threadId);
      if (!ts) return state;
      const existing = ts.streamingView.items.get(itemId);
      if (!existing) return state;

      const nextSummary = [...existing.reasoningSummary];
      while (nextSummary.length <= summaryIndex) nextSummary.push('');
      nextSummary[summaryIndex] += delta;

      const nextItems = new Map(ts.streamingView.items);
      nextItems.set(itemId, { ...existing, reasoningSummary: nextSummary });

      const next = new Map(state.streamingByThread);
      next.set(threadId, {
        ...ts,
        streamingView: { ...ts.streamingView, items: nextItems, revision: ts.streamingView.revision + 1 },
      });
      return { streamingByThread: next };
    }),

  appendReasoningRawContentDelta: (threadId, itemId, delta, contentIndex) =>
    set((state) => {
      const ts = state.streamingByThread.get(threadId);
      if (!ts) return state;
      const existing = ts.streamingView.items.get(itemId);
      if (!existing) return state;

      const nextRaw = [...existing.reasoningRaw];
      while (nextRaw.length <= contentIndex) nextRaw.push('');
      nextRaw[contentIndex] += delta;

      const nextItems = new Map(ts.streamingView.items);
      nextItems.set(itemId, { ...existing, reasoningRaw: nextRaw });

      const next = new Map(state.streamingByThread);
      next.set(threadId, {
        ...ts,
        streamingView: { ...ts.streamingView, items: nextItems, revision: ts.streamingView.revision + 1 },
      });
      return { streamingByThread: next };
    }),

  appendPlanDelta: (threadId, itemId, delta) =>
    set((state) => {
      const ts = state.streamingByThread.get(threadId);
      if (!ts) return state;
      const existing = ts.streamingView.items.get(itemId);
      if (!existing) return state;

      const nextItems = new Map(ts.streamingView.items);
      nextItems.set(itemId, { ...existing, planText: existing.planText + delta });

      const next = new Map(state.streamingByThread);
      next.set(threadId, {
        ...ts,
        streamingView: { ...ts.streamingView, items: nextItems, revision: ts.streamingView.revision + 1 },
      });
      return { streamingByThread: next };
    }),

  // ── Complete item: move from streaming → messagesByThread ──

  completeStreamingItem: (threadId, turnId, item) => {
    set((state) => {
      const nextMessages = appendCompletedStreamingItem(state.messagesByThread, threadId, turnId, item);
      const ts = state.streamingByThread.get(threadId);
      if (!ts) return { messagesByThread: nextMessages };

      const itemId = getItemId(item);
      const nextViewItems = new Map(ts.streamingView.items);
      nextViewItems.delete(itemId);

      const next = new Map(state.streamingByThread);
      next.set(threadId, {
        ...ts,
        streamingView: { ...ts.streamingView, items: nextViewItems },
      });

      return { messagesByThread: nextMessages, streamingByThread: next };
    });
  },
}));

// ── Helpers ──

function appendItemToThread(
  messagesByThread: Map<string, TurnGroup[]>,
  threadId: string, turnId: string, item: TurnItem,
): Map<string, TurnGroup[]> {
  const next = new Map(messagesByThread);
  const groups = next.get(threadId) ?? [];
  const last = groups[groups.length - 1];
  if (last && last.turn_id === turnId) {
    next.set(threadId, [...groups.slice(0, -1), { ...last, items: [...last.items, item] }]);
  } else {
    next.set(threadId, [...groups, { turn_id: turnId, items: [item] }]);
  }
  return next;
}

function appendCompletedStreamingItem(
  messagesByThread: Map<string, TurnGroup[]>,
  threadId: string, turnId: string, item: TurnItem,
): Map<string, TurnGroup[]> {
  const next = new Map(messagesByThread);
  const groups = next.get(threadId) ?? [];
  const itemId = getItemId(item);
  const lastGroup = groups[groups.length - 1];
  if (lastGroup && lastGroup.turn_id === turnId) {
    if (!lastGroup.items.some((existing) => getItemId(existing) === itemId)) {
      next.set(threadId, [...groups.slice(0, -1), { ...lastGroup, items: [...lastGroup.items, item] }]);
    }
  } else {
    next.set(threadId, [...groups, { turn_id: turnId, items: [item] }]);
  }
  return next;
}


function getStreamingItemType(item: TurnItem): 'AgentMessage' | 'Reasoning' | 'Plan' {
  if (item.type === 'Reasoning') return 'Reasoning';
  if (item.type === 'Plan') return 'Plan';
  return 'AgentMessage';
}

function getItemId(item: TurnItem): string {
  return item.id ?? '';
}
