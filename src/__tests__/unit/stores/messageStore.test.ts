import { describe, it, expect, beforeEach } from 'vitest';
import { useMessageStore } from '@/stores/messageStore';
import type { TurnItem } from '@/types';

const userMsg: TurnItem = {
  type: 'UserMessage',
  id: 'u1',
  content: [{ type: 'text', text: 'hello', text_elements: [] }],
};

const agentMsg: TurnItem = {
  type: 'AgentMessage',
  id: 'a1',
  content: [{ type: 'Text', text: 'hi there' }],
};

describe('messageStore', () => {
  beforeEach(() => {
    useMessageStore.setState({
      messagesByThread: new Map(),
      streamingTurn: null,
    });
  });

  it('appendMessage adds to correct thread', () => {
    const { appendMessage } = useMessageStore.getState();
    appendMessage('t1', 'turn-1', userMsg);
    appendMessage('t1', 'turn-1', agentMsg);
    appendMessage('t2', 'turn-2', userMsg);

    const state = useMessageStore.getState();
    expect(state.messagesByThread.get('t1')).toHaveLength(1);
    expect(state.messagesByThread.get('t1')![0].items).toHaveLength(2);
    expect(state.messagesByThread.get('t2')).toHaveLength(1);
  });

  it('startStreaming sets streamingTurn', () => {
    useMessageStore.getState().startStreaming('turn-1');
    const st = useMessageStore.getState().streamingTurn;
    expect(st).toMatchObject({ turnId: 'turn-1', agentText: '', isStreaming: true });
    expect(st?.items).toBeInstanceOf(Map);
  });

  it('updateAgentContentDelta accumulates text via item tracking', () => {
    const { startStreaming, startStreamingItem, updateAgentContentDelta } = useMessageStore.getState();
    startStreaming('turn-1');
    startStreamingItem('t1', 'turn-1', { type: 'AgentMessage', id: 'a1', content: [] });
    updateAgentContentDelta('a1', 'Hello');
    updateAgentContentDelta('a1', ' world');

    const st = useMessageStore.getState().streamingTurn;
    expect(st?.agentText).toBe('Hello world');
    expect(st?.items.get('a1')?.agentText).toBe('Hello world');
  });

  it('updateAgentContentDelta is no-op when not streaming', () => {
    useMessageStore.getState().updateAgentContentDelta('a1', 'ignored');
    expect(useMessageStore.getState().streamingTurn).toBeNull();
  });

  it('stopStreaming clears streamingTurn', () => {
    useMessageStore.getState().startStreaming('turn-1');
    useMessageStore.getState().stopStreaming();
    expect(useMessageStore.getState().streamingTurn?.isStreaming).toBe(false);
  });

  it('clearThread removes messages for a thread', () => {
    useMessageStore.getState().appendMessage('t1', 'turn-1', userMsg);
    useMessageStore.getState().clearThread('t1');
    expect(useMessageStore.getState().messagesByThread.has('t1')).toBe(false);
  });
});
