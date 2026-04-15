import { describe, it, expect, beforeEach } from 'vitest';
import { useMessageStore } from '@/stores/messageStore';
import type { TurnItem } from '@/types';

const userMsg: TurnItem = {
  type: 'UserMessage', id: 'u1',
  content: [{ type: 'text', text: 'hello', text_elements: [] }],
};
const agentMsg: TurnItem = {
  type: 'AgentMessage', id: 'a1',
  content: [{ type: 'Text', text: 'hi there' }],
};

describe('messageStore', () => {
  beforeEach(() => {
    useMessageStore.setState({ messagesByThread: new Map(), streamingByThread: new Map() });
  });

  it('appendMessage adds to correct thread', () => {
    const { appendMessage } = useMessageStore.getState();
    appendMessage('t1', 'turn-1', userMsg);
    appendMessage('t1', 'turn-1', agentMsg);
    appendMessage('t2', 'turn-2', userMsg);
    const state = useMessageStore.getState();
    expect(state.messagesByThread.get('t1')![0].items).toHaveLength(2);
    expect(state.messagesByThread.get('t2')).toHaveLength(1);
  });

  it('startStreaming sets per-thread streaming state', () => {
    useMessageStore.getState().startStreaming('t1', 'turn-1');
    const ts = useMessageStore.getState().streamingByThread.get('t1')!;
    expect(ts.streamingView).toMatchObject({ turnId: 'turn-1', isStreaming: true, revision: 0 });
  });

  it('appendAgentContentDelta writes directly to view', () => {
    const { startStreaming, startStreamingItem, appendAgentContentDelta } = useMessageStore.getState();
    startStreaming('t1', 'turn-1');
    startStreamingItem('t1', 'turn-1', { type: 'AgentMessage', id: 'a1', content: [] });
    appendAgentContentDelta('t1', 'a1', 'Hello');
    appendAgentContentDelta('t1', 'a1', ' world');

    const ts = useMessageStore.getState().streamingByThread.get('t1')!;
    expect(ts.streamingView.items.get('a1')?.agentText).toBe('Hello world');
    expect(ts.streamingView.revision).toBe(4); // start(1) + startItem(2) + 2 deltas
  });

  it('appendAgentContentDelta is no-op when not streaming', () => {
    useMessageStore.getState().appendAgentContentDelta('t1', 'a1', 'ignored');
    expect(useMessageStore.getState().streamingByThread.has('t1')).toBe(false);
  });

  it('stopStreaming sets isStreaming to false', () => {
    const { startStreaming, startStreamingItem, appendAgentContentDelta, stopStreaming } = useMessageStore.getState();
    startStreaming('t1', 'turn-1');
    startStreamingItem('t1', 'turn-1', { type: 'AgentMessage', id: 'a1', content: [] });
    appendAgentContentDelta('t1', 'a1', 'done');
    stopStreaming('t1');

    const ts = useMessageStore.getState().streamingByThread.get('t1')!;
    expect(ts.streamingView.items.get('a1')?.agentText).toBe('done');
    expect(ts.streamingView.isStreaming).toBe(false);
  });

  it('clearThread removes messages and streaming state', () => {
    useMessageStore.getState().appendMessage('t1', 'turn-1', userMsg);
    useMessageStore.getState().startStreaming('t1', 'turn-1');
    useMessageStore.getState().clearThread('t1');
    const state = useMessageStore.getState();
    expect(state.messagesByThread.has('t1')).toBe(false);
    expect(state.streamingByThread.has('t1')).toBe(false);
  });

  it('isolates streaming state between threads', () => {
    const { startStreaming, startStreamingItem, appendAgentContentDelta } = useMessageStore.getState();
    startStreaming('t1', 'turn-1');
    startStreaming('t2', 'turn-2');
    startStreamingItem('t1', 'turn-1', { type: 'AgentMessage', id: 'a1', content: [] });
    startStreamingItem('t2', 'turn-2', { type: 'AgentMessage', id: 'a2', content: [] });
    appendAgentContentDelta('t1', 'a1', 'thread1');
    appendAgentContentDelta('t2', 'a2', 'thread2');

    expect(useMessageStore.getState().streamingByThread.get('t1')!.streamingView.items.get('a1')?.agentText).toBe('thread1');
    expect(useMessageStore.getState().streamingByThread.get('t2')!.streamingView.items.get('a2')?.agentText).toBe('thread2');
  });
});
