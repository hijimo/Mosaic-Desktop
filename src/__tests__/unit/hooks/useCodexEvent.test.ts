import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useCodexEvent } from '@/hooks/useCodexEvent';
import { useThreadStore } from '@/stores/threadStore';
import { useMessageStore } from '@/stores/messageStore';
import type { CodexEventPayload } from '@/types';

vi.mock('@tauri-apps/api/event');
vi.mock('@tauri-apps/api/core');

describe('useCodexEvent', () => {
  let capturedHandler: ((event: { payload: CodexEventPayload }) => void) | undefined;

  beforeEach(() => {
    vi.clearAllMocks();
    useThreadStore.setState({ threads: new Map(), activeThreadId: null });
    useMessageStore.setState({
      messagesByThread: new Map(),
      streamingTurn: null,
      streamingBuffer: null,
      streamingView: null,
      streamingItemOrder: new Map(),
    });

    // Add a thread so updateThread has something to patch
    useThreadStore.getState().addThread({
      thread_id: 't1',
      cwd: '/',
      model: null,
      model_provider_id: null,
      name: null,
      created_at: '',
      forked_from: null,
    });

    vi.mocked(listen).mockImplementation(async (_name, handler) => {
      capturedHandler = handler as typeof capturedHandler;
      return () => {};
    });
  });

  function emit(threadId: string, msg: CodexEventPayload['event']['msg']): void {
    act(() => {
      capturedHandler!({ payload: { thread_id: threadId, event: { id: 'e1', msg } } });
    });
  }

  it('registers listener on mount', () => {
    renderHook(() => useCodexEvent());
    expect(listen).toHaveBeenCalledWith('codex-event', expect.any(Function));
  });

  it('handles session_configured by updating thread model', () => {
    renderHook(() => useCodexEvent());
    emit('t1', {
      type: 'session_configured',
      session_id: 's1',
      model: 'gpt-4o',
      model_provider_id: 'openai',
      cwd: '/',
      history_log_id: 0,
      history_entry_count: 0,
      mode: 'Default',
      can_append: true,
    });

    const thread = useThreadStore.getState().threads.get('t1');
    expect(thread?.model).toBe('gpt-4o');
    expect(thread?.model_provider_id).toBe('openai');
  });

  it('handles thread_name_updated', () => {
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'thread_name_updated', thread_id: 't1', thread_name: 'My Chat' });

    expect(useThreadStore.getState().threads.get('t1')?.name).toBe('My Chat');
  });

  it('handles task_started by starting streaming buffer and view state', () => {
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });

    const state = useMessageStore.getState();
    expect(state.streamingBuffer).toMatchObject({
      turnId: 'turn-1',
      isStreaming: true,
    });
    expect(state.streamingView).toMatchObject({
      turnId: 'turn-1',
      isStreaming: true,
      revision: 0,
    });
  });

  it('buffers agent deltas instead of writing visible text immediately', () => {
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', { type: 'item_started', thread_id: 't1', turn_id: 'turn-1', item: { type: 'AgentMessage', id: 'a1', content: [] } });
    emit('t1', { type: 'agent_message_content_delta', thread_id: 't1', turn_id: 'turn-1', item_id: 'a1', delta: 'Hello' });
    emit('t1', { type: 'agent_message_content_delta', thread_id: 't1', turn_id: 'turn-1', item_id: 'a1', delta: ' world' });

    const buffered = useMessageStore.getState().streamingBuffer?.items.get('a1');
    expect(buffered?.pendingAgentText).toBe('Hello world');
    expect(useMessageStore.getState().streamingView?.items.get('a1')?.agentText ?? '').toBe('');

    act(() => {
      useMessageStore.getState().flushVisibleStreaming();
    });

    expect(useMessageStore.getState().streamingView?.items.get('a1')?.agentText).toBe('Hello world');
  });

  it('handles task_complete by stopping streaming', () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      {
        turn_id: 'turn-1',
        items: [
          {
            type: 'McpToolCall',
            id: 'tool-1',
            server: 'filesystem',
            tool: 'read_file',
            status: 'Completed',
            arguments: { path: '/tmp/demo.txt' },
          },
          {
            type: 'AgentMessage',
            id: 'a1',
            content: [{ type: 'Text', text: 'final response' }],
          },
        ],
      },
    ]);
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', { type: 'task_complete', turn_id: 'turn-1' });

    expect(useMessageStore.getState().streamingTurn?.isStreaming).toBe(false);
    expect(invoke).toHaveBeenCalledWith('thread_get_messages', { threadId: 't1' });
  });

  it('handles item_completed by appending message', () => {
    renderHook(() => useCodexEvent());
    // Start streaming first so completeStreamingItem works properly
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', {
      type: 'item_completed',
      thread_id: 't1',
      turn_id: 'turn-1',
      item: { type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: 'response' }] },
    });

    const groups = useMessageStore.getState().messagesByThread.get('t1');
    expect(groups).toHaveLength(1);
    expect(groups![0].turn_id).toBe('turn-1');
    expect(groups![0].items[0].type).toBe('AgentMessage');
  });

  it('handles error by stopping streaming and logging', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', { type: 'error', message: 'something broke' });

    expect(useMessageStore.getState().streamingTurn?.isStreaming).toBe(false);
    expect(consoleSpy).toHaveBeenCalledWith('[codex] something broke');
    consoleSpy.mockRestore();
  });
});
