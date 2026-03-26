import { useEffect } from 'react';
import { listenCodexEvent } from '@/services/api';
import { useThreadStore } from '@/stores/threadStore';
import { useMessageStore } from '@/stores/messageStore';
import { useToolCallStore } from '@/stores/toolCallStore';
import type { CodexEventPayload } from '@/types';

/**
 * Listens to all codex-event emissions and dispatches to stores.
 * Mount once at the app root level.
 */
export function useCodexEvent(): void {
  const updateThread = useThreadStore((s) => s.updateThread);
  const {
    appendMessage,
    startStreaming,
    stopStreaming,
    startStreamingItem,
    updateAgentContentDelta,
    updateReasoningContentDelta,
    updateReasoningRawContentDelta,
    updatePlanDelta,
    completeStreamingItem,
  } = useMessageStore();
  const { beginToolCall, updateToolCallOutput, endToolCall, clearAll } =
    useToolCallStore();

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
          clearAll();
          startStreaming(msg.turn_id);
          break;

        case 'task_complete':
          stopStreaming();
          break;

        case 'turn_aborted':
          stopStreaming();
          break;

        // ── v2 Structured item events ──
        case 'item_started':
          startStreamingItem(msg.thread_id, msg.turn_id, msg.item);
          break;

        case 'item_completed':
          completeStreamingItem(thread_id, msg.item);
          break;

        case 'agent_message_content_delta':
          updateAgentContentDelta(msg.item_id, msg.delta);
          break;

        case 'reasoning_content_delta':
          updateReasoningContentDelta(msg.item_id, msg.delta, msg.summary_index);
          break;

        case 'reasoning_raw_content_delta':
          updateReasoningRawContentDelta(msg.item_id, msg.delta, msg.content_index);
          break;

        case 'plan_delta':
          updatePlanDelta(msg.item_id, msg.delta);
          break;

        // ── Tool call events ──
        case 'mcp_tool_call_begin':
          beginToolCall({
            callId: msg.call_id,
            type: 'mcp',
            status: 'running',
            name: msg.invocation.tool,
            serverName: msg.invocation.server,
            toolName: msg.invocation.tool,
            arguments: msg.invocation.arguments,
          });
          break;

        case 'mcp_tool_call_end':
          endToolCall(msg.call_id, {
            status: 'completed',
            result: msg.result,
          });
          break;

        case 'exec_command_begin':
          beginToolCall({
            callId: msg.call_id,
            type: 'exec',
            status: 'running',
            name: msg.command?.[0] ?? 'command',
            command: typeof msg.command === 'string' ? [msg.command] : msg.command,
            cwd: msg.cwd,
          });
          break;

        case 'exec_command_output_delta':
          updateToolCallOutput(msg.call_id, msg.delta);
          break;

        case 'exec_command_end':
          endToolCall(msg.call_id, {
            status: msg.exit_code === 0 ? 'completed' : 'failed',
            exitCode: msg.exit_code,
            output: [msg.stdout, msg.stderr].filter(Boolean).join('\n') || undefined,
          });
          break;

        case 'web_search_begin':
          beginToolCall({
            callId: msg.call_id,
            type: 'web_search',
            status: 'running',
            name: 'Web Search',
          });
          break;

        case 'web_search_end':
          endToolCall(msg.call_id, {
            status: 'completed',
            name: msg.query || 'Web Search',
          });
          break;

        case 'patch_apply_begin':
          beginToolCall({
            callId: msg.call_id,
            type: 'patch',
            status: 'running',
            name: 'Apply Patch',
          });
          break;

        case 'patch_apply_end':
          endToolCall(msg.call_id, {
            status: msg.success ? 'completed' : 'failed',
          });
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
  }, [
    updateThread,
    appendMessage,
    startStreaming,
    stopStreaming,
    startStreamingItem,
    updateAgentContentDelta,
    updateReasoningContentDelta,
    updateReasoningRawContentDelta,
    updatePlanDelta,
    completeStreamingItem,
    beginToolCall,
    updateToolCallOutput,
    endToolCall,
    clearAll,
  ]);
}
