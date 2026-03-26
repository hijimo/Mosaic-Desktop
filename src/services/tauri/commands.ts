import { invoke } from '@tauri-apps/api/core';
import type { ThreadMeta, SubmitOpParams, TurnItem } from '@/types';
import type { Op } from '@/types';

export async function threadStart(cwd?: string): Promise<string> {
  return invoke<string>('thread_start', { cwd });
}

export async function threadList(): Promise<ThreadMeta[]> {
  return invoke<ThreadMeta[]>('thread_list');
}

export async function threadGetInfo(threadId: string): Promise<ThreadMeta> {
  return invoke<ThreadMeta>('thread_get_info', { threadId });
}

export async function threadArchive(threadId: string): Promise<void> {
  return invoke<void>('thread_archive', { threadId });
}

export async function threadResume(threadId: string): Promise<ThreadMeta> {
  return invoke<ThreadMeta>('thread_resume', { threadId });
}

export async function threadGetMessages(threadId: string): Promise<TurnItem[]> {
  return invoke<TurnItem[]>('thread_get_messages', { threadId });
}

export async function submitOp(threadId: string, id: string, op: Op): Promise<void> {
  return invoke<void>('submit_op', { threadId, id, op } satisfies SubmitOpParams);
}

export async function getCwd(): Promise<string> {
  return invoke<string>('get_cwd');
}

export async function getConfig(): Promise<unknown> {
  return invoke<unknown>('get_config');
}
