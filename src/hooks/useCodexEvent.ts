import { useEffect } from 'react';
import { listenCodexEvent } from '@/services/api';
import { useThreadStore } from '@/stores/threadStore';
import { useMessageStore } from '@/stores/messageStore';
import { useToolCallStore } from '@/stores/toolCallStore';
import { useApprovalStore } from '@/stores/approvalStore';
import { useClarificationStore } from '@/stores/clarificationStore';
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
  const { addApproval, clearAll: clearApprovals } = useApprovalStore();
  const { addRequest: addClarification, clearAll: clearClarifications } = useClarificationStore();

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
          clearApprovals();
          clearClarifications();
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

        case 'exec_approval_request':
          addApproval({
            callId: msg.call_id,
            turnId: msg.turn_id,
            type: 'exec',
            command: msg.command,
            cwd: msg.cwd,
            reason: msg.reason,
          });
          break;

        case 'apply_patch_approval_request':
          addApproval({
            callId: msg.call_id,
            turnId: msg.turn_id,
            type: 'patch',
            reason: msg.reason,
            changes: msg.changes as Record<string, unknown>,
          });
          break;

        case 'request_user_input':
          addClarification({
            id: msg.id,
            message: msg.message,
            schema: msg.schema,
          });
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
    addApproval,
    clearApprovals,
    addClarification,
    clearClarifications,
  ]);
}
