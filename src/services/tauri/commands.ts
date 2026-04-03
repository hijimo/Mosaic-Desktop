import { invoke } from '@tauri-apps/api/core';
import type { ThreadMeta, SubmitOpParams, TurnGroup } from '@/types';
import type { Op } from '@/types';
import type { ShareMessagePayload } from '@/components/chat/agent/messageShareContent';

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

export async function threadGetMessages(threadId: string): Promise<TurnGroup[]> {
  return invoke<TurnGroup[]>('thread_get_messages', { threadId });
}

export async function submitOp(threadId: string, id: string, op: Op): Promise<void> {
  return invoke<void>('submit_op', { threadId, id, op } satisfies SubmitOpParams);
}

export async function getCwd(): Promise<string> {
  return invoke<string>('get_cwd');
}

export async function getHomeDir(): Promise<string> {
  return invoke<string>('get_home_dir');
}

export async function listCwds(): Promise<string[]> {
  return invoke<string[]>('list_cwds');
}

export async function pickFolder(): Promise<string | null> {
  return invoke<string | null>('pick_folder');
}

export async function pickFiles(filters?: string[]): Promise<string[]> {
  return invoke<string[]>('pick_files', { filters });
}

export async function getConfig(): Promise<unknown> {
  return invoke<unknown>('get_config');
}

export interface ShareMessageResult {
  url: string;
}

export async function shareMessage(payload: ShareMessagePayload): Promise<ShareMessageResult> {
  return invoke<ShareMessageResult>('share_message', { payload });
}
