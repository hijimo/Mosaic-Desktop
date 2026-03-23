import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { CodexEventPayload } from '@/types';

export function listenCodexEvent(
  callback: (payload: CodexEventPayload) => void,
): Promise<UnlistenFn> {
  return listen<CodexEventPayload>('codex-event', (event) => {
    callback(event.payload);
  });
}
