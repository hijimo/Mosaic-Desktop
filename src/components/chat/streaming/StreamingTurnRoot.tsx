import { useMessageStore } from '@/stores/messageStore';
import { useToolCallStore } from '@/stores/toolCallStore';
import { useApprovalStore } from '@/stores/approvalStore';
import { useClarificationStore } from '@/stores/clarificationStore';
import type { ToolCallState, TurnGroup, TurnItem, ReviewDecision } from '@/types';
import { Message } from '../Message';

interface StreamingTurnRootProps {
  threadId: string;
  onApprovalDecision?: (callId: string, decision: ReviewDecision) => void;
  onElicitationDecision?: (requestId: string, serverName: string, decision: 'accept' | 'decline' | 'cancel', content?: Record<string, unknown>) => void;
}

const EMPTY_GROUPS: never[] = [];
const EMPTY_MAP = new Map<string, never>();

export function StreamingTurnRoot({
  threadId,
  onApprovalDecision,
  onElicitationDecision,
}: StreamingTurnRootProps): React.ReactElement {
  const turnGroups = useMessageStore(
    (s) => s.messagesByThread.get(threadId) ?? EMPTY_GROUPS,
  );

  // Subscribe to the revision number (primitive) — guarantees re-render on every view update
  const viewRevision = useMessageStore(
    (s) => s.streamingByThread.get(threadId)?.streamingView.revision ?? -1,
  );

  // Read the full streaming state imperatively during render (driven by revision change)
  const threadStreaming = viewRevision >= 0
    ? useMessageStore.getState().streamingByThread.get(threadId)
    : undefined;
  const streamingView = threadStreaming?.streamingView ?? null;
  const streamingItemOrder = threadStreaming?.streamingItemOrder ?? EMPTY_MAP;

  const toolCallsMap = useToolCallStore((s) => s.byThread.get(threadId) ?? EMPTY_MAP);
  const approvalsMap = useApprovalStore((s) => s.byThread.get(threadId) ?? EMPTY_MAP);
  const clarificationsMap = useClarificationStore((s) => s.byThread.get(threadId) ?? EMPTY_MAP);

  const isStreaming = streamingView?.isStreaming ?? false;
  const streamingTurnId = streamingView?.turnId ?? null;

  const currentStreamingGroup =
    isStreaming && streamingTurnId
      ? turnGroups.find((group) => group.turn_id === streamingTurnId) ?? null
      : null;
  const visibleTurnGroups =
    currentStreamingGroup === null
      ? turnGroups
      : turnGroups.filter((group) => group.turn_id !== currentStreamingGroup.turn_id);

  const activeToolCalls = Array.from(toolCallsMap.values());
  const approvals = Array.from(approvalsMap.values());
  const clarifications = Array.from(clarificationsMap.values());

  const streamingGroup = buildStreamingGroup(
    streamingView, currentStreamingGroup, streamingItemOrder, activeToolCalls,
  );

  return (
    <>
      {visibleTurnGroups.map((group, index) => (
        <Message
          key={`${group.turn_id}-${index}`}
          group={group}
          threadId={threadId}
          onApprovalDecision={onApprovalDecision}
          onElicitationDecision={onElicitationDecision}
        />
      ))}
      {isStreaming && streamingGroup ? (
        <Message
          group={streamingGroup}
          threadId={threadId}
          toolCalls={activeToolCalls.filter((tc) => tc.type === 'patch')}
          approvalRequests={approvals}
          clarifications={clarifications}
          onApprovalDecision={onApprovalDecision}
          onElicitationDecision={onElicitationDecision}
          isStreaming
        />
      ) : null}
    </>
  );
}

// ── Helpers ──

interface ViewItem {
  itemId: string;
  order: number;
  itemType: 'AgentMessage' | 'Reasoning' | 'Plan';
  agentText: string;
  reasoningSummary: string[];
  reasoningRaw: string[];
  planText: string;
}

interface ViewTurn {
  turnId: string;
  isStreaming: boolean;
  items: Map<string, ViewItem>;
}

function buildStreamingGroup(
  streamingView: ViewTurn | null,
  currentStreamingGroup: TurnGroup | null,
  streamingItemOrder: Map<string, number>,
  activeToolCalls: ToolCallState[],
): TurnGroup | null {
  if (!streamingView && !currentStreamingGroup) return null;

  const baseItems = currentStreamingGroup?.items ?? [];
  const baseItemIds = new Set(baseItems.map((item) => item.id));
  const baseAgentTexts = new Set(
    baseItems
      .filter((item): item is Extract<TurnItem, { type: 'AgentMessage' }> => item.type === 'AgentMessage')
      .map((item) => item.content.map((c) => c.text).join(''))
      .filter(Boolean),
  );

  const streamingItems = Array.from(streamingView?.items.values() ?? [])
    .map((item) => {
      let turnItem: TurnItem;
      switch (item.itemType) {
        case 'Reasoning':
          turnItem = { type: 'Reasoning', id: item.itemId, summary_text: [...item.reasoningSummary], raw_content: [...item.reasoningRaw] };
          break;
        case 'Plan':
          turnItem = { type: 'Plan', id: item.itemId, text: item.planText };
          break;
        case 'AgentMessage':
        default:
          turnItem = { type: 'AgentMessage', id: item.itemId, content: [{ type: 'Text', text: item.agentText }] };
          break;
      }
      return { item: turnItem, order: streamingItemOrder.get(item.itemId) ?? item.order };
    })
    .filter(({ item }) => {
      if (baseItemIds.has(item.id)) return false;
      if (item.type !== 'AgentMessage') return true;
      return !baseAgentTexts.has(item.content.map((c) => c.text).join(''));
    });

  const activeToolItems = activeToolCalls
    .map((tc) => buildToolCallTurnItem(tc))
    .filter((entry): entry is { item: TurnItem; order: number } => entry !== null);

  const orderedItems = [
    ...baseItems.map((item, index) => ({ item, order: streamingItemOrder.get(item.id) ?? index })),
    ...activeToolItems,
    ...streamingItems,
  ]
    .sort((a, b) => a.order - b.order)
    .map(({ item }) => item);

  return {
    turn_id: streamingView?.turnId ?? currentStreamingGroup?.turn_id ?? 'streaming-turn',
    items: orderedItems,
    status: 'InProgress',
  };
}

function buildToolCallTurnItem(tc: ToolCallState): { item: TurnItem; order: number } | null {
  const order = tc.order ?? Number.MAX_SAFE_INTEGER;
  switch (tc.type) {
    case 'mcp':
      return { order, item: {
        type: 'McpToolCall', id: tc.callId, server: tc.serverName ?? '', tool: tc.toolName ?? tc.name,
        status: tc.status === 'completed' ? 'Completed' : tc.status === 'failed' ? 'Failed' : 'InProgress',
        arguments: tc.arguments, result: undefined,
        error: tc.status === 'failed' ? { message: String((tc.result as { error?: unknown } | undefined)?.error ?? 'Tool call failed') } : undefined,
      }};
    case 'exec':
      return { order, item: {
        type: 'CommandExecution', id: tc.callId, command: tc.command?.join(' ') ?? tc.name, cwd: tc.cwd ?? '',
        status: tc.status === 'completed' ? 'Completed' : tc.status === 'failed' ? 'Failed' : 'InProgress',
        command_actions: [], aggregated_output: tc.output, exit_code: tc.exitCode ?? null,
      }};
    case 'web_search':
      return { order, item: { type: 'WebSearch', id: tc.callId, query: tc.name } };
    default:
      return null;
  }
}
