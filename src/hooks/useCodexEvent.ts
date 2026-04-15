import { useEffect, useRef } from 'react';
import { listenCodexEvent, threadGetMessages } from '@/services/api';
import { useThreadStore } from '@/stores/threadStore';
import { useMessageStore } from '@/stores/messageStore';
import { useToolCallStore } from '@/stores/toolCallStore';
import { useApprovalStore } from '@/stores/approvalStore';
import { useClarificationStore } from '@/stores/clarificationStore';
import { useElicitationStore } from '@/stores/elicitationStore';
import type { CodexEventPayload } from '@/types';

/**
 * Listens to all codex-event emissions and dispatches to stores.
 * Every event carries a `thread_id` — all store mutations are scoped to that thread.
 */
export function useCodexEvent(): void {
  const updateThread = useThreadStore((s) => s.updateThread);
  const {
    setMessages,
    startStreaming,
    stopStreaming,
    startStreamingItem,
    appendAgentContentDelta,
    appendReasoningContentDelta,
    appendReasoningRawContentDelta,
    appendPlanDelta,
    completeStreamingItem,
  } = useMessageStore();
  const { beginToolCall, updateToolCallOutput, endToolCall, clearThread: clearToolCalls } =
    useToolCallStore();
  const { addApproval, clearThread: clearApprovals } = useApprovalStore();
  const { addRequest: addClarification, clearThread: clearClarifications } = useClarificationStore();
  const { addRequest: addElicitation, clearThread: clearElicitations } = useElicitationStore();

  const eventOrderByThread = useRef<Map<string, number>>(new Map());

  const nextEventOrder = (threadId: string): number => {
    const current = eventOrderByThread.current.get(threadId) ?? 0;
    const next = current + 1;
    eventOrderByThread.current.set(threadId, next);
    return next;
  };

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
          eventOrderByThread.current.set(thread_id, 0);
          clearToolCalls(thread_id);
          clearApprovals(thread_id);
          clearClarifications(thread_id);
          clearElicitations(thread_id);
          startStreaming(thread_id, msg.turn_id);
          break;

        case 'task_complete':
          stopStreaming(thread_id);
          void threadGetMessages(thread_id).then((messages) => {
            setMessages(thread_id, messages);
          });
          break;

        case 'turn_aborted':
          stopStreaming(thread_id);
          void threadGetMessages(thread_id).then((messages) => {
            setMessages(thread_id, messages);
          });
          break;

        case 'item_started':
          startStreamingItem(thread_id, msg.turn_id, msg.item, nextEventOrder(thread_id));
          break;

        case 'item_completed':
          completeStreamingItem(thread_id, msg.turn_id, msg.item);
          break;

        case 'agent_message_content_delta':
          appendAgentContentDelta(thread_id, msg.item_id, msg.delta);
          break;

        case 'reasoning_content_delta':
          appendReasoningContentDelta(thread_id, msg.item_id, msg.delta, msg.summary_index);
          break;

        case 'reasoning_raw_content_delta':
          appendReasoningRawContentDelta(thread_id, msg.item_id, msg.delta, msg.content_index);
          break;

        case 'plan_delta':
          appendPlanDelta(thread_id, msg.item_id, msg.delta);
          break;

        case 'mcp_tool_call_begin':
          beginToolCall(thread_id, {
            callId: msg.call_id, type: 'mcp', status: 'running', name: msg.invocation.tool,
            order: nextEventOrder(thread_id), serverName: msg.invocation.server,
            toolName: msg.invocation.tool, arguments: msg.invocation.arguments,
          });
          break;

        case 'mcp_tool_call_end':
          endToolCall(thread_id, msg.call_id, { status: 'completed', result: msg.result });
          break;

        case 'exec_command_begin':
          beginToolCall(thread_id, {
            callId: msg.call_id, type: 'exec', status: 'running',
            name: msg.command?.[0] ?? 'command', order: nextEventOrder(thread_id),
            command: typeof msg.command === 'string' ? [msg.command] : msg.command, cwd: msg.cwd,
          });
          break;

        case 'exec_command_output_delta':
          updateToolCallOutput(thread_id, msg.call_id, msg.delta);
          break;

        case 'exec_command_end':
          endToolCall(thread_id, msg.call_id, {
            status: msg.exit_code === 0 ? 'completed' : 'failed', exitCode: msg.exit_code,
            output: [msg.stdout, msg.stderr].filter(Boolean).join('\n') || undefined,
          });
          break;

        case 'web_search_begin':
          beginToolCall(thread_id, {
            callId: msg.call_id, type: 'web_search', status: 'running',
            name: 'Web Search', order: nextEventOrder(thread_id),
          });
          break;

        case 'web_search_end':
          endToolCall(thread_id, msg.call_id, { status: 'completed', name: msg.query || 'Web Search' });
          break;

        case 'patch_apply_begin':
          beginToolCall(thread_id, {
            callId: msg.call_id, type: 'patch', status: 'running',
            name: 'Apply Patch', order: nextEventOrder(thread_id),
          });
          break;

        case 'patch_apply_end':
          endToolCall(thread_id, msg.call_id, { status: msg.success ? 'completed' : 'failed' });
          break;

        case 'error':
        case 'stream_error':
          console.error(`[codex] ${msg.message}`);
          stopStreaming(thread_id);
          break;

        case 'exec_approval_request':
          addApproval(thread_id, {
            callId: msg.call_id, turnId: msg.turn_id, type: 'exec',
            order: nextEventOrder(thread_id), command: msg.command, cwd: msg.cwd,
            reason: msg.reason, availableDecisions: msg.available_decisions,
          });
          break;

        case 'apply_patch_approval_request':
          addApproval(thread_id, {
            callId: msg.call_id, turnId: msg.turn_id, type: 'patch',
            order: nextEventOrder(thread_id), reason: msg.reason,
            changes: msg.changes as Record<string, unknown>,
            availableDecisions: ['approved', 'approved_for_session', 'abort'],
          });
          break;

        case 'elicitation_request':
          addElicitation(thread_id, {
            serverName: msg.server_name, requestId: msg.request_id, message: msg.message,
            mode: (msg.mode as 'form' | 'url' | undefined) ?? 'form',
            schema: msg.schema as Record<string, unknown> | undefined,
            url: msg.url, order: nextEventOrder(thread_id),
          });
          break;

        case 'request_user_input':
          addClarification(thread_id, {
            id: msg.id, order: nextEventOrder(thread_id), message: msg.message, schema: msg.schema,
          });
          break;

        case 'list_skills_response':
          break;
      }
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [
    updateThread, setMessages, startStreaming, stopStreaming,
    startStreamingItem, appendAgentContentDelta, appendReasoningContentDelta,
    appendReasoningRawContentDelta, appendPlanDelta, completeStreamingItem,
    beginToolCall, updateToolCallOutput, endToolCall, clearToolCalls,
    addApproval, clearApprovals, addClarification, clearClarifications,
    addElicitation, clearElicitations,
  ]);
}
