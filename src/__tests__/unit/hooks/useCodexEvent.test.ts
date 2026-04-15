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
    useMessageStore.setState({ messagesByThread: new Map(), streamingByThread: new Map() });
    useThreadStore.getState().addThread({
      thread_id: 't1', cwd: '/', model: null, model_provider_id: null,
      name: null, created_at: '', forked_from: null,
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

  it('handles session_configured', () => {
    renderHook(() => useCodexEvent());
    emit('t1', {
      type: 'session_configured', session_id: 's1', model: 'gpt-4o', model_provider_id: 'openai',
      cwd: '/', history_log_id: 0, history_entry_count: 0, mode: 'Default', can_append: true,
    });
    expect(useThreadStore.getState().threads.get('t1')?.model).toBe('gpt-4o');
  });

  it('handles task_started by starting per-thread streaming', () => {
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    const ts = useMessageStore.getState().streamingByThread.get('t1');
    expect(ts).toBeDefined();
    expect(ts!.streamingView.isStreaming).toBe(true);
  });

  it('writes agent deltas directly to view', () => {
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', { type: 'item_started', thread_id: 't1', turn_id: 'turn-1', item: { type: 'AgentMessage', id: 'a1', content: [] } });
    emit('t1', { type: 'agent_message_content_delta', thread_id: 't1', turn_id: 'turn-1', item_id: 'a1', delta: 'Hello' });
    emit('t1', { type: 'agent_message_content_delta', thread_id: 't1', turn_id: 'turn-1', item_id: 'a1', delta: ' world' });

    const ts = useMessageStore.getState().streamingByThread.get('t1')!;
    expect(ts.streamingView.items.get('a1')?.agentText).toBe('Hello world');
  });

  it('handles task_complete by stopping streaming', () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', { type: 'task_complete', turn_id: 'turn-1' });

    const ts = useMessageStore.getState().streamingByThread.get('t1')!;
    expect(ts.streamingView.isStreaming).toBe(false);
    expect(invoke).toHaveBeenCalledWith('thread_get_messages', { threadId: 't1' });
  });

  it('handles error by stopping streaming', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    renderHook(() => useCodexEvent());
    emit('t1', { type: 'task_started', turn_id: 'turn-1', collaboration_mode_kind: 'Default' });
    emit('t1', { type: 'error', message: 'something broke' });

    const ts = useMessageStore.getState().streamingByThread.get('t1')!;
    expect(ts.streamingView.isStreaming).toBe(false);
    consoleSpy.mockRestore();
  });
});
