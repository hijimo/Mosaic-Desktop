import { create } from 'zustand';

export type MessageReaction = 'none' | 'up' | 'down';
export type MessageShareState = 'idle' | 'preparing' | 'sharing' | 'success' | 'failed';

interface MessageActionState {
  reactions: Record<string, MessageReaction>;
  shareStates: Record<string, MessageShareState>;
  toggleReaction: (messageId: string, nextReaction: Exclude<MessageReaction, 'none'>) => void;
  setShareState: (messageId: string, state: MessageShareState) => void;
  reset: () => void;
}

export const useMessageActionStore = create<MessageActionState>((set) => ({
  reactions: {},
  shareStates: {},

  toggleReaction: (messageId, nextReaction) =>
    set((state) => {
      const currentReaction = state.reactions[messageId] ?? 'none';
      return {
        reactions: {
          ...state.reactions,
          [messageId]: currentReaction === nextReaction ? 'none' : nextReaction,
        },
      };
    }),

  setShareState: (messageId, nextState) =>
    set((state) => ({
      shareStates: {
        ...state.shareStates,
        [messageId]: nextState,
      },
    })),

  reset: () =>
    set({
      reactions: {},
      shareStates: {},
    }),
}));
