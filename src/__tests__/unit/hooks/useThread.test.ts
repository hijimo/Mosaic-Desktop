import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useThread } from '@/hooks/useThread';
import { useThreadStore } from '@/stores/threadStore';

vi.mock('@tauri-apps/api/core');

describe('useThread', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useThreadStore.setState({ threads: new Map(), activeThreadId: null });
  });

  it('createThread calls thread_start and updates store', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce('/test/cwd')   // getCwd
      .mockResolvedValueOnce('new-tid');     // threadStart

    const { result } = renderHook(() => useThread());

    let tid: string = '';
    await act(async () => {
      tid = await result.current.createThread();
    });

    expect(tid).toBe('new-tid');
    expect(invoke).toHaveBeenCalledWith('get_cwd');
    expect(invoke).toHaveBeenCalledWith('thread_start', { cwd: '/test/cwd' });

    const state = useThreadStore.getState();
    expect(state.activeThreadId).toBe('new-tid');
    expect(state.threads.has('new-tid')).toBe(true);
  });

  it('archiveThread calls thread_archive and removes from store', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    useThreadStore.getState().addThread({
      thread_id: 't1',
      cwd: '/',
      model: null,
      model_provider_id: null,
      name: null,
      created_at: '',
      forked_from: null,
    });

    const { result } = renderHook(() => useThread());

    await act(async () => {
      await result.current.archiveThread('t1');
    });

    expect(invoke).toHaveBeenCalledWith('thread_archive', { thread_id: 't1' });
    expect(useThreadStore.getState().threads.has('t1')).toBe(false);
  });
});
