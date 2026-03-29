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
      streamingBuffer: null,
      streamingView: null,
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

  it('startStreaming sets streaming buffer and view state', () => {
    useMessageStore.getState().startStreaming('turn-1');
    const state = useMessageStore.getState();
    expect(state.streamingBuffer).toMatchObject({ turnId: 'turn-1' });
    expect(state.streamingView).toMatchObject({
      turnId: 'turn-1',
      isStreaming: true,
      revision: 0,
    });
    expect(state.streamingTurn).toMatchObject({ turnId: 'turn-1', agentText: '', isStreaming: true });
  });

  it('buffers multiple agent deltas until flushVisibleStreaming runs', () => {
    const {
      startStreaming,
      startStreamingItem,
      bufferAgentContentDelta,
      flushVisibleStreaming,
    } = useMessageStore.getState();
    startStreaming('turn-1');
    startStreamingItem('t1', 'turn-1', { type: 'AgentMessage', id: 'a1', content: [] });
    bufferAgentContentDelta('a1', 'Hello');
    bufferAgentContentDelta('a1', ' world');

    let state = useMessageStore.getState();
    expect(state.streamingView?.items.get('a1')?.agentText ?? '').toBe('');
    expect(state.streamingTurn?.agentText).toBe('');

    flushVisibleStreaming();

    state = useMessageStore.getState();
    expect(state.streamingView?.items.get('a1')?.agentText).toBe('Hello world');
    expect(state.streamingTurn?.agentText).toBe('Hello world');
    expect(state.streamingView?.revision).toBe(1);
  });

  it('bufferAgentContentDelta is no-op when not streaming', () => {
    useMessageStore.getState().bufferAgentContentDelta('a1', 'ignored');
    const state = useMessageStore.getState();
    expect(state.streamingTurn).toBeNull();
    expect(state.streamingBuffer).toBeNull();
    expect(state.streamingView).toBeNull();
  });

  it('flushes buffered content before stopStreaming finalizes the visible turn', () => {
    const {
      startStreaming,
      startStreamingItem,
      bufferAgentContentDelta,
      stopStreaming,
    } = useMessageStore.getState();
    startStreaming('turn-1');
    startStreamingItem('t1', 'turn-1', { type: 'AgentMessage', id: 'a1', content: [] });
    bufferAgentContentDelta('a1', 'done');

    stopStreaming();

    const state = useMessageStore.getState();
    expect(state.streamingView?.items.get('a1')?.agentText).toBe('done');
    expect(state.streamingView?.isStreaming).toBe(false);
    expect(state.streamingTurn?.isStreaming).toBe(false);
  });

  it('clearThread removes messages for a thread', () => {
    useMessageStore.getState().appendMessage('t1', 'turn-1', userMsg);
    useMessageStore.getState().clearThread('t1');
    expect(useMessageStore.getState().messagesByThread.has('t1')).toBe(false);
  });
});
