/**
 * E2E smoke test: run inside Tauri window to verify the full
 * thread_start → submit_op(user_turn) → codex-event pipeline.
 *
 * Usage: import and call `runSmoke()` from browser console or a temp button.
 * Results are logged to console and returned as a summary object.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { CodexEventPayload } from '@/types';

export async function runSmoke(): Promise<{
  success: boolean;
  threadId: string | null;
  events: string[];
  error: string | null;
}> {
  const events: string[] = [];
  let error: string | null = null;
  let threadId: string | null = null;

  try {
    // 1. Listen for codex events
    const unlisten = await listen<CodexEventPayload>('codex-event', (e) => {
      const type = e.payload.event.msg.type;
      events.push(type);
      console.log(`[smoke] event: ${type}`, e.payload.event.msg);
    });

    // 2. Start a thread
    const cwd = await invoke<string>('get_cwd');
    console.log(`[smoke] cwd: ${cwd}`);

    threadId = await invoke<string>('thread_start', { cwd });
    console.log(`[smoke] thread started: ${threadId}`);

    // 3. Wait for session_configured
    await new Promise<void>((resolve) => {
      const check = setInterval(() => {
        if (events.includes('session_configured')) {
          clearInterval(check);
          resolve();
        }
      }, 100);
      setTimeout(() => {
        clearInterval(check);
        resolve();
      }, 10000);
    });

    console.log(`[smoke] session configured, sending user_turn...`);

    // 4. Submit a user_turn
    await invoke('submit_op', {
      threadId: threadId,
      id: crypto.randomUUID(),
      op: {
        type: 'user_turn',
        items: [
          { type: 'text', text: 'Say hello in one word.', text_elements: [] },
        ],
        cwd,
        model: '',
        approval_policy: 'on-request',
        sandbox_policy: { type: 'danger-full-access' },
      },
    });

    console.log(`[smoke] user_turn submitted, waiting for response...`);

    // 5. Wait for task_complete or error (up to 30s)
    await new Promise<void>((resolve) => {
      const check = setInterval(() => {
        if (
          events.includes('task_complete') ||
          events.includes('error') ||
          events.includes('stream_error')
        ) {
          clearInterval(check);
          resolve();
        }
      }, 200);
      setTimeout(() => {
        clearInterval(check);
        resolve();
      }, 30000);
    });

    // 6. Cleanup: shutdown thread
    await invoke('submit_op', {
      threadId: threadId,
      id: crypto.randomUUID(),
      op: { type: 'shutdown' },
    });

    unlisten();
  } catch (e) {
    error = String(e);
    console.error('[smoke] error:', e);
  }

  const success = events.includes('task_complete') && !events.includes('error');
  console.log(`[smoke] result:`, { success, threadId, events, error });
  return { success, threadId, events, error };
}

// Expose to window for console access
if (typeof window !== 'undefined') {
  (window as unknown as Record<string, unknown>).__runSmoke = runSmoke;
}
