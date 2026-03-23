import { useEffect } from 'react';
import { listenCodexEvent } from '@/services/api';
import { useThreadStore } from '@/stores/threadStore';
import { useMessageStore } from '@/stores/messageStore';
import type { CodexEventPayload } from '@/types';

/**
 * Listens to all codex-event emissions and dispatches to stores.
 * Mount once at the app root level.
 */
export function useCodexEvent(): void {
  const updateThread = useThreadStore((s) => s.updateThread);
  const { appendMessage, updateStreamingDelta, startStreaming, stopStreaming } =
    useMessageStore();

  useEffect(() => {
    const unlistenPromise = listenCodexEvent((payload: CodexEventPayload) => {
      const { thread_id, event } = payload;
      const msg = event.msg;
      console.debug('[codex-event]', msg.type, msg);

      switch (msg.type) {
        case 'session_configured':
          updateThread(thread_id, {
            model: msg.model,
            model_provider_id: msg.model_provider_id,
          });
          break;

        case 'thread_name_updated':
          updateThread(thread_id, { name: msg.thread_name ?? null });
          break;

        case 'task_started':
          startStreaming(msg.turn_id);
          break;

        case 'task_complete':
          stopStreaming();
          break;

        case 'turn_aborted':
          stopStreaming();
          break;

        case 'agent_message_delta':
          updateStreamingDelta(msg.delta);
          break;

        case 'item_completed':
          appendMessage(thread_id, msg.item);
          break;

        case 'error':
        case 'stream_error':
          console.error(`[codex] ${msg.message}`);
          stopStreaming();
          break;
      }
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [updateThread, appendMessage, updateStreamingDelta, startStreaming, stopStreaming]);
}
