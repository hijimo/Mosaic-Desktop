import { describe, it, expect, beforeEach } from 'vitest';
import { useThreadStore } from '@/stores/threadStore';
import type { ThreadMeta } from '@/types';

const makeMeta = (id: string, overrides?: Partial<ThreadMeta>): ThreadMeta => ({
  thread_id: id,
  cwd: '/tmp',
  model: null,
  model_provider_id: null,
  name: null,
  created_at: '2026-01-01T00:00:00Z',
  forked_from: null,
  ...overrides,
});

describe('threadStore', () => {
  beforeEach(() => {
    useThreadStore.setState({ threads: new Map(), activeThreadId: null });
  });

  it('addThread inserts a thread', () => {
    const meta = makeMeta('t1');
    useThreadStore.getState().addThread(meta);
    expect(useThreadStore.getState().threads.get('t1')).toEqual(meta);
  });

  it('setActiveThread updates activeThreadId', () => {
    useThreadStore.getState().setActiveThread('t1');
    expect(useThreadStore.getState().activeThreadId).toBe('t1');
  });

  it('removeThread deletes thread and clears activeThreadId if matched', () => {
    const meta = makeMeta('t1');
    useThreadStore.getState().addThread(meta);
    useThreadStore.getState().setActiveThread('t1');
    useThreadStore.getState().removeThread('t1');

    expect(useThreadStore.getState().threads.has('t1')).toBe(false);
    expect(useThreadStore.getState().activeThreadId).toBeNull();
  });

  it('removeThread keeps activeThreadId if different', () => {
    useThreadStore.getState().addThread(makeMeta('t1'));
    useThreadStore.getState().addThread(makeMeta('t2'));
    useThreadStore.getState().setActiveThread('t2');
    useThreadStore.getState().removeThread('t1');

    expect(useThreadStore.getState().activeThreadId).toBe('t2');
  });

  it('updateThread patches existing thread', () => {
    useThreadStore.getState().addThread(makeMeta('t1'));
    useThreadStore.getState().updateThread('t1', { model: 'gpt-4o', name: 'My Chat' });

    const updated = useThreadStore.getState().threads.get('t1');
    expect(updated?.model).toBe('gpt-4o');
    expect(updated?.name).toBe('My Chat');
    expect(updated?.cwd).toBe('/tmp'); // unchanged
  });

  it('updateThread is a no-op for non-existent thread', () => {
    useThreadStore.getState().updateThread('nope', { model: 'x' });
    expect(useThreadStore.getState().threads.size).toBe(0);
  });
});
