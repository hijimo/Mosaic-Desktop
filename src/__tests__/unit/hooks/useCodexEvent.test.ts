import { renderHook } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { useCodexEvent } from '@/hooks/useCodexEvent';
import { useThreadStore } from '@/stores/threadStore';
import { useMessageStore } from '@/stores/messageStore';
import type { CodexEventPayload } from '@/types';

vi.mock('@tauri-apps/api/event');

describe('useCodexEvent', () => {
  let capturedHandler: ((event: { payload: CodexEventPayload }) => void) | undefined;

  beforeEach(() => {
    vi.clearAllMocks();
    useThreadStore.setState({ threads: new Map(), activeThreadId: null });
    useMessageStore.setState({ messagesByThread: new Map(), streamingTurn: null });

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
      return vi.fn();
    });
  });

  function emit(threadId: string, msg: CodexEventPayload['event']['msg']): void {
    capturedHandler!({ payload: { thread_id: threadId, event: { id: 'e1', msg } } });
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

  it('handles task_started by starting streaming', () => {
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });

    expect(useMessageStore.getState().streamingTurn).toEqual({
      turnId: 'turn-1',
      agentText: '',
      isStreaming: true,
    });
  });

  it('handles agent_message_delta by accumulating text', () => {
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', { type: 'agent_message_delta', delta: 'Hello' });
    emit('t1', { type: 'agent_message_delta', delta: ' world' });

    expect(useMessageStore.getState().streamingTurn?.agentText).toBe('Hello world');
  });

  it('handles task_complete by stopping streaming', () => {
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', { type: 'task_complete', turn_id: 'turn-1' });

    expect(useMessageStore.getState().streamingTurn).toBeNull();
  });

  it('handles item_completed by appending message', () => {
    renderHook(() => useCodexEvent());
    emit('t1', {
      type: 'item_completed',
      thread_id: 't1',
      turn_id: 'turn-1',
      item: { type: 'AgentMessage', id: 'a1', content: [{ type: 'Text', text: 'response' }] },
    });

    const msgs = useMessageStore.getState().messagesByThread.get('t1');
    expect(msgs).toHaveLength(1);
    expect(msgs![0].type).toBe('AgentMessage');
  });

  it('handles error by stopping streaming and logging', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', { type: 'error', message: 'something broke' });

    expect(useMessageStore.getState().streamingTurn).toBeNull();
    expect(consoleSpy).toHaveBeenCalledWith('[codex] something broke');
    consoleSpy.mockRestore();
  });
});
