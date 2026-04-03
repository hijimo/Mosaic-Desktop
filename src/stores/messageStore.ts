import { create } from 'zustand';
import type { TurnItem, TurnGroup } from '@/types';

interface StreamingItem {
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

interface StreamingTurn {
  turnId: string;
  agentText: string;
  isStreaming: boolean;
  items: Map<string, StreamingItem>;
}

interface StreamingBufferItem {
  threadId: string;
  turnId: string;
  itemId: string;
  order: number;
  itemType: 'AgentMessage' | 'Reasoning' | 'Plan';
  pendingAgentText: string;
  pendingReasoningSummary: string[];
  pendingReasoningRaw: string[];
  pendingPlanText: string;
  dirty: boolean;
}

interface StreamingBufferTurn {
  turnId: string;
  isStreaming: boolean;
  items: Map<string, StreamingBufferItem>;
  dirtyItemCount: number;
}

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

interface MessageState {
  messagesByThread: Map<string, TurnGroup[]>;
  streamingTurn: StreamingTurn | null;
  streamingBuffer: StreamingBufferTurn | null;
  streamingView: StreamingViewTurn | null;
  streamingItemOrder: Map<string, number>;

  appendMessage: (threadId: string, turnId: string, item: TurnItem) => void;
  setMessages: (threadId: string, groups: TurnGroup[]) => void;
  dismissTurnError: (threadId: string, turnId: string) => void;
  startStreaming: (turnId: string) => void;
  stopStreaming: () => void;
  clearThread: (threadId: string) => void;

  startStreamingItem: (threadId: string, turnId: string, item: TurnItem, order?: number) => void;
  bufferAgentContentDelta: (itemId: string, delta: string) => void;
  bufferReasoningContentDelta: (itemId: string, delta: string, summaryIndex: number) => void;
  bufferReasoningRawContentDelta: (itemId: string, delta: string, contentIndex: number) => void;
  bufferPlanDelta: (itemId: string, delta: string) => void;
  flushVisibleStreaming: () => void;

  updateAgentContentDelta: (itemId: string, delta: string) => void;
  updateReasoningContentDelta: (itemId: string, delta: string, summaryIndex: number) => void;
  updateReasoningRawContentDelta: (itemId: string, delta: string, contentIndex: number) => void;
  updatePlanDelta: (itemId: string, delta: string) => void;
  completeStreamingItem: (threadId: string, turnId: string, item: TurnItem) => void;
}

export const useMessageStore = create<MessageState>((set, get) => ({
  messagesByThread: new Map(),
  streamingTurn: null,
  streamingBuffer: null,
  streamingView: null,
  streamingItemOrder: new Map(),

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
          g.turn_id === turnId
            ? { ...g, error: undefined, status: 'Completed' as const }
            : g,
        ),
      );
      return { messagesByThread: next };
    }),

  startStreaming: (turnId) =>
    set({
      streamingBuffer: {
        turnId,
        isStreaming: true,
        items: new Map(),
        dirtyItemCount: 0,
      },
      streamingView: {
        turnId,
        isStreaming: true,
        items: new Map(),
        revision: 0,
      },
      streamingTurn: {
        turnId,
        agentText: '',
        isStreaming: true,
        items: new Map(),
      },
      streamingItemOrder: new Map(),
    }),

  stopStreaming: () => {
    get().flushVisibleStreaming();
    set((state) => {
      if (!state.streamingView && !state.streamingTurn && !state.streamingBuffer) {
        return state;
      }

      const nextView = state.streamingView
        ? { ...state.streamingView, isStreaming: false }
        : null;
      const nextTurn = nextView
        ? buildLegacyTurn(nextView)
        : state.streamingTurn
          ? { ...state.streamingTurn, isStreaming: false }
          : null;
      const nextBuffer = state.streamingBuffer
        ? { ...state.streamingBuffer, isStreaming: false }
        : null;

      return {
        streamingView: nextView,
        streamingTurn: nextTurn,
        streamingBuffer: nextBuffer,
      };
    });
  },

  clearThread: (threadId) =>
    set((state) => {
      const next = new Map(state.messagesByThread);
      next.delete(threadId);
      return { messagesByThread: next };
    }),

  startStreamingItem: (threadId, turnId, item, order = 0) =>
    set((state) => {
      if (!state.streamingBuffer || !state.streamingView) return state;

      const itemId = getItemId(item);
      const itemType = getStreamingItemType(item);
      if (!itemId) return state;

      const nextBufferItems = new Map(state.streamingBuffer.items);
      nextBufferItems.set(itemId, {
        threadId,
        turnId,
        itemId,
        order,
        itemType,
        pendingAgentText: '',
        pendingReasoningSummary: [],
        pendingReasoningRaw: [],
        pendingPlanText: '',
        dirty: false,
      });

      const nextViewItems = new Map(state.streamingView.items);
      nextViewItems.set(itemId, {
        threadId,
        turnId,
        itemId,
        order,
        itemType,
        agentText: '',
        reasoningSummary: [],
        reasoningRaw: [],
        planText: '',
      });

      const nextItemOrder = new Map(state.streamingItemOrder);
      nextItemOrder.set(itemId, order);

      const nextView: StreamingViewTurn = {
        ...state.streamingView,
        items: nextViewItems,
      };

      return {
        streamingBuffer: {
          ...state.streamingBuffer,
          items: nextBufferItems,
          dirtyItemCount: state.streamingBuffer.dirtyItemCount,
        },
        streamingView: nextView,
        streamingTurn: buildLegacyTurn(nextView),
        streamingItemOrder: nextItemOrder,
      };
    }),

  bufferAgentContentDelta: (itemId, delta) =>
    set((state) => {
      if (!state.streamingBuffer) return state;
      const buffered = state.streamingBuffer.items.get(itemId);
      if (!buffered) return state;

      const nextItems = new Map(state.streamingBuffer.items);
      const nextDirtyItemCount = buffered.dirty
        ? state.streamingBuffer.dirtyItemCount
        : state.streamingBuffer.dirtyItemCount + 1;
      nextItems.set(itemId, {
        ...buffered,
        pendingAgentText: buffered.pendingAgentText + delta,
        dirty: true,
      });

      return {
        streamingBuffer: {
          ...state.streamingBuffer,
          items: nextItems,
          dirtyItemCount: nextDirtyItemCount,
        },
      };
    }),

  bufferReasoningContentDelta: (itemId, delta, summaryIndex) =>
    set((state) => {
      if (!state.streamingBuffer) return state;
      const buffered = state.streamingBuffer.items.get(itemId);
      if (!buffered) return state;

      const nextSummary = [...buffered.pendingReasoningSummary];
      while (nextSummary.length <= summaryIndex) {
        nextSummary.push('');
      }
      nextSummary[summaryIndex] += delta;

      const nextItems = new Map(state.streamingBuffer.items);
      const nextDirtyItemCount = buffered.dirty
        ? state.streamingBuffer.dirtyItemCount
        : state.streamingBuffer.dirtyItemCount + 1;
      nextItems.set(itemId, {
        ...buffered,
        pendingReasoningSummary: nextSummary,
        dirty: true,
      });

      return {
        streamingBuffer: {
          ...state.streamingBuffer,
          items: nextItems,
          dirtyItemCount: nextDirtyItemCount,
        },
      };
    }),

  bufferReasoningRawContentDelta: (itemId, delta, contentIndex) =>
    set((state) => {
      if (!state.streamingBuffer) return state;
      const buffered = state.streamingBuffer.items.get(itemId);
      if (!buffered) return state;

      const nextRaw = [...buffered.pendingReasoningRaw];
      while (nextRaw.length <= contentIndex) {
        nextRaw.push('');
      }
      nextRaw[contentIndex] += delta;

      const nextItems = new Map(state.streamingBuffer.items);
      const nextDirtyItemCount = buffered.dirty
        ? state.streamingBuffer.dirtyItemCount
        : state.streamingBuffer.dirtyItemCount + 1;
      nextItems.set(itemId, {
        ...buffered,
        pendingReasoningRaw: nextRaw,
        dirty: true,
      });

      return {
        streamingBuffer: {
          ...state.streamingBuffer,
          items: nextItems,
          dirtyItemCount: nextDirtyItemCount,
        },
      };
    }),

  bufferPlanDelta: (itemId, delta) =>
    set((state) => {
      if (!state.streamingBuffer) return state;
      const buffered = state.streamingBuffer.items.get(itemId);
      if (!buffered) return state;

      const nextItems = new Map(state.streamingBuffer.items);
      const nextDirtyItemCount = buffered.dirty
        ? state.streamingBuffer.dirtyItemCount
        : state.streamingBuffer.dirtyItemCount + 1;
      nextItems.set(itemId, {
        ...buffered,
        pendingPlanText: buffered.pendingPlanText + delta,
        dirty: true,
      });

      return {
        streamingBuffer: {
          ...state.streamingBuffer,
          items: nextItems,
          dirtyItemCount: nextDirtyItemCount,
        },
      };
    }),

  flushVisibleStreaming: () =>
    set((state) => {
      if (!state.streamingBuffer || !state.streamingView) return state;
      if (state.streamingBuffer.dirtyItemCount === 0) return state;

      const nextBufferItems = new Map<string, StreamingBufferItem>();
      const nextViewItems = new Map(state.streamingView.items);

      for (const [itemId, buffered] of state.streamingBuffer.items) {
        const previousView = nextViewItems.get(itemId) ?? {
          threadId: buffered.threadId,
          turnId: buffered.turnId,
          itemId,
          order: buffered.order,
          itemType: buffered.itemType,
          agentText: '',
          reasoningSummary: [],
          reasoningRaw: [],
          planText: '',
        };

        if (buffered.dirty) {
          nextViewItems.set(itemId, {
            ...previousView,
            agentText: previousView.agentText + buffered.pendingAgentText,
            reasoningSummary: mergeTextArrays(
              previousView.reasoningSummary,
              buffered.pendingReasoningSummary,
            ),
            reasoningRaw: mergeTextArrays(
              previousView.reasoningRaw,
              buffered.pendingReasoningRaw,
            ),
            planText: previousView.planText + buffered.pendingPlanText,
          });
        }

        nextBufferItems.set(itemId, resetBufferedItem(buffered));
      }

      const nextView: StreamingViewTurn = {
        ...state.streamingView,
        items: nextViewItems,
        revision: state.streamingView.revision + 1,
      };

      return {
        streamingBuffer: {
          ...state.streamingBuffer,
          items: nextBufferItems,
          dirtyItemCount: 0,
        },
        streamingView: nextView,
        streamingTurn: buildLegacyTurn(nextView),
      };
    }),

  updateAgentContentDelta: (itemId, delta) => {
    get().bufferAgentContentDelta(itemId, delta);
    get().flushVisibleStreaming();
  },

  updateReasoningContentDelta: (itemId, delta, summaryIndex) => {
    get().bufferReasoningContentDelta(itemId, delta, summaryIndex);
    get().flushVisibleStreaming();
  },

  updateReasoningRawContentDelta: (itemId, delta, contentIndex) => {
    get().bufferReasoningRawContentDelta(itemId, delta, contentIndex);
    get().flushVisibleStreaming();
  },

  updatePlanDelta: (itemId, delta) => {
    get().bufferPlanDelta(itemId, delta);
    get().flushVisibleStreaming();
  },

  completeStreamingItem: (threadId, turnId, item) => {
    get().flushVisibleStreaming();

    set((state) => {
      const nextMessages = appendCompletedStreamingItem(
        state.messagesByThread,
        threadId,
        turnId,
        item,
      );
      const itemId = getItemId(item);

      const nextBufferItems = state.streamingBuffer
        ? new Map(state.streamingBuffer.items)
        : null;
      const removedBufferedItem = nextBufferItems?.get(itemId) ?? null;
      nextBufferItems?.delete(itemId);

      const nextViewItems = state.streamingView
        ? new Map(state.streamingView.items)
        : null;
      nextViewItems?.delete(itemId);

      const nextView = state.streamingView && nextViewItems
        ? {
            ...state.streamingView,
            items: nextViewItems,
          }
        : null;

      return {
        messagesByThread: nextMessages,
        streamingBuffer: state.streamingBuffer && nextBufferItems
          ? {
              ...state.streamingBuffer,
              items: nextBufferItems,
              dirtyItemCount: Math.max(
                0,
                state.streamingBuffer.dirtyItemCount -
                  (removedBufferedItem?.dirty ? 1 : 0),
              ),
            }
          : state.streamingBuffer,
        streamingView: nextView,
        streamingTurn: nextView ? buildLegacyTurn(nextView) : state.streamingTurn,
      };
    });
  },
}));

function appendItemToThread(
  messagesByThread: Map<string, TurnGroup[]>,
  threadId: string,
  turnId: string,
  item: TurnItem,
): Map<string, TurnGroup[]> {
  const next = new Map(messagesByThread);
  const groups = next.get(threadId) ?? [];
  const last = groups[groups.length - 1];

  if (last && last.turn_id === turnId) {
    const updated = [
      ...groups.slice(0, -1),
      { ...last, items: [...last.items, item] },
    ];
    next.set(threadId, updated);
  } else {
    next.set(threadId, [...groups, { turn_id: turnId, items: [item] }]);
  }

  return next;
}

function appendCompletedStreamingItem(
  messagesByThread: Map<string, TurnGroup[]>,
  threadId: string,
  turnId: string,
  item: TurnItem,
): Map<string, TurnGroup[]> {
  const next = new Map(messagesByThread);
  const groups = next.get(threadId) ?? [];
  const itemId = getItemId(item);
  const lastGroup = groups[groups.length - 1];

  if (lastGroup && lastGroup.turn_id === turnId) {
    if (!lastGroup.items.some((existing) => getItemId(existing) === itemId)) {
      const updated = [
        ...groups.slice(0, -1),
        { ...lastGroup, items: [...lastGroup.items, item] },
      ];
      next.set(threadId, updated);
    }
  } else {
    next.set(threadId, [...groups, { turn_id: turnId, items: [item] }]);
  }

  return next;
}

function resetBufferedItem(item: StreamingBufferItem): StreamingBufferItem {
  return {
    ...item,
    pendingAgentText: '',
    pendingReasoningSummary: [],
    pendingReasoningRaw: [],
    pendingPlanText: '',
    dirty: false,
  };
}

function mergeTextArrays(current: string[], pending: string[]): string[] {
  const next = [...current];
  for (let index = 0; index < pending.length; index += 1) {
    if (pending[index] === undefined) continue;
    while (next.length <= index) {
      next.push('');
    }
    next[index] += pending[index];
  }
  return next;
}

function buildLegacyTurn(view: StreamingViewTurn): StreamingTurn {
  const items = new Map<string, StreamingItem>();
  let agentText = '';

  for (const [itemId, item] of view.items) {
        items.set(itemId, {
          threadId: item.threadId,
          turnId: item.turnId,
          itemId,
          order: item.order,
          itemType: item.itemType,
          agentText: item.agentText,
      reasoningSummary: [...item.reasoningSummary],
      reasoningRaw: [...item.reasoningRaw],
      planText: item.planText,
    });

    if (item.itemType === 'AgentMessage') {
      agentText += item.agentText;
    }
  }

  return {
    turnId: view.turnId,
    agentText,
    isStreaming: view.isStreaming,
    items,
  };
}

function getStreamingItemType(
  item: TurnItem,
): 'AgentMessage' | 'Reasoning' | 'Plan' {
  if (item.type === 'Reasoning') return 'Reasoning';
  if (item.type === 'Plan') return 'Plan';
  return 'AgentMessage';
}

function getItemId(item: TurnItem): string {
  return item.id ?? '';
}
