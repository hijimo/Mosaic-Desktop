import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  threadStart,
  threadList,
  threadGetInfo,
  threadArchive,
  submitOp,
  getCwd,
} from '@/services/tauri/commands';
import type { Op } from '@/types';

vi.mock('@tauri-apps/api/core');

describe('tauri/commands', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('threadStart calls invoke with correct args', async () => {
    vi.mocked(invoke).mockResolvedValue('tid-1');
    const result = await threadStart('/my/dir');
    expect(invoke).toHaveBeenCalledWith('thread_start', { cwd: '/my/dir' });
    expect(result).toBe('tid-1');
  });

  it('threadStart passes undefined cwd when omitted', async () => {
    vi.mocked(invoke).mockResolvedValue('tid-2');
    await threadStart();
    expect(invoke).toHaveBeenCalledWith('thread_start', { cwd: undefined });
  });

  it('threadList calls invoke', async () => {
    vi.mocked(invoke).mockResolvedValue(['a', 'b']);
    const result = await threadList();
    expect(invoke).toHaveBeenCalledWith('thread_list');
    expect(result).toEqual(['a', 'b']);
  });

  it('threadGetInfo calls invoke with thread_id', async () => {
    const meta = { thread_id: 't1', cwd: '/' };
    vi.mocked(invoke).mockResolvedValue(meta);
    const result = await threadGetInfo('t1');
    expect(invoke).toHaveBeenCalledWith('thread_get_info', { thread_id: 't1' });
    expect(result).toEqual(meta);
  });

  it('threadArchive calls invoke with thread_id', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await threadArchive('t1');
    expect(invoke).toHaveBeenCalledWith('thread_archive', { thread_id: 't1' });
  });

  it('submitOp calls invoke with serialized op', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const op: Op = { type: 'interrupt' };
    await submitOp('t1', 'op-1', op);
    expect(invoke).toHaveBeenCalledWith('submit_op', {
      thread_id: 't1',
      id: 'op-1',
      op: { type: 'interrupt' },
    });
  });

  it('getCwd calls invoke', async () => {
    vi.mocked(invoke).mockResolvedValue('/home/user');
    const result = await getCwd();
    expect(invoke).toHaveBeenCalledWith('get_cwd');
    expect(result).toBe('/home/user');
  });
});
