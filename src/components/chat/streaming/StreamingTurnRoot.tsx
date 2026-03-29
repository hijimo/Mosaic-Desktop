import { useCallback, useEffect, useRef } from 'react';
import { useMessageStore } from '@/stores/messageStore';
import { useToolCallStore } from '@/stores/toolCallStore';
import { useApprovalStore } from '@/stores/approvalStore';
import { useClarificationStore } from '@/stores/clarificationStore';
import type { TurnGroup, TurnItem } from '@/types';
import { Message } from '../Message';
import { TaskStartedIndicator } from '../indicators/TaskStartedIndicator';
import { TaskCompletedIndicator } from '../indicators/TaskCompletedIndicator';

interface StreamingTurnRootProps {
  threadId: string;
  onApprovalDecision?: (callId: string, decision: 'approve' | 'deny') => void;
}

const EMPTY_GROUPS: never[] = [];

export function StreamingTurnRoot({
  threadId,
  onApprovalDecision,
}: StreamingTurnRootProps): React.ReactElement {
  const turnGroups = useMessageStore(
    (s) => s.messagesByThread.get(threadId) ?? EMPTY_GROUPS,
  );
  const streamingView = useMessageStore((s) => s.streamingView);
  const hasPendingStreamingBuffer = useMessageStore(
    (s) => (s.streamingBuffer?.dirtyItemCount ?? 0) > 0,
  );
  const flushVisibleStreaming = useMessageStore((s) => s.flushVisibleStreaming);
  const toolCallsMap = useToolCallStore((s) => s.toolCalls);
  const approvalsMap = useApprovalStore((s) => s.approvals);
  const clarificationsMap = useClarificationStore((s) => s.requests);
  const isStreaming = streamingView?.isStreaming ?? false;
  const isComplete = !isStreaming && turnGroups.length > 0;
  const frameRef = useRef<number | null>(null);
  const streamingTurnId = streamingView?.turnId ?? null;
  const currentStreamingGroup =
    isStreaming && streamingTurnId
      ? turnGroups.find((group) => group.turn_id === streamingTurnId) ?? null
      : null;
  const visibleTurnGroups =
    currentStreamingGroup === null
      ? turnGroups
      : turnGroups.filter((group) => group.turn_id !== currentStreamingGroup.turn_id);
  const streamingGroup = buildStreamingGroup(streamingView, currentStreamingGroup);
  const activeToolCalls = Array.from(toolCallsMap.values());
  const approvals = Array.from(approvalsMap.values());
  const clarifications = Array.from(clarificationsMap.values());

  const ensureStreamingFlushScheduled = useCallback(() => {
    if (frameRef.current !== null) return;

    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      flushVisibleStreaming();
    });
  }, [flushVisibleStreaming]);

  useEffect(() => {
    if (!hasPendingStreamingBuffer) return;
    ensureStreamingFlushScheduled();
  }, [hasPendingStreamingBuffer, ensureStreamingFlushScheduled]);

  useEffect(() => () => {
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
    }
  }, []);

  return (
    <>
      {(isStreaming || turnGroups.length > 0) && <TaskStartedIndicator />}

      {visibleTurnGroups.map((group, index) => (
        <Message
          key={`${group.turn_id}-${index}`}
          group={group}
          onApprovalDecision={onApprovalDecision}
        />
      ))}

      {isStreaming && streamingGroup ? (
        <Message
          group={streamingGroup}
          toolCalls={activeToolCalls}
          approvalRequests={approvals}
          clarifications={clarifications}
          onApprovalDecision={onApprovalDecision}
          isStreaming
        />
      ) : null}

      {isComplete && <TaskCompletedIndicator />}
    </>
  );
}

function buildStreamingGroup(
  streamingView: ReturnType<typeof useMessageStore.getState>['streamingView'],
  currentStreamingGroup: TurnGroup | null,
): TurnGroup | null {
  if (!streamingView && !currentStreamingGroup) return null;

  const baseItems = currentStreamingGroup?.items ?? [];
  const baseItemIds = new Set(baseItems.map((item) => item.id));
  const baseAgentTexts = new Set(
    baseItems
      .filter((item): item is Extract<TurnItem, { type: 'AgentMessage' }> => item.type === 'AgentMessage')
      .map((item) => item.content.map((content) => content.text).join(''))
      .filter(Boolean),
  );
  const streamingItems = Array.from(streamingView?.items.values() ?? []).map<TurnItem>((item) => {
    switch (item.itemType) {
      case 'Reasoning':
        return {
          type: 'Reasoning',
          id: item.itemId,
          summary_text: [...item.reasoningSummary],
          raw_content: [...item.reasoningRaw],
        };
      case 'Plan':
        return {
          type: 'Plan',
          id: item.itemId,
          text: item.planText,
        };
      case 'AgentMessage':
      default:
        return {
          type: 'AgentMessage',
          id: item.itemId,
          content: [{ type: 'Text', text: item.agentText }],
        };
    }
  }).filter((item) => {
    if (baseItemIds.has(item.id)) return false;
    if (item.type !== 'AgentMessage') return true;

    const itemText = item.content.map((content) => content.text).join('');
    return !baseAgentTexts.has(itemText);
  });

  return {
    turn_id: streamingView?.turnId ?? currentStreamingGroup?.turn_id ?? 'streaming-turn',
    items: [...baseItems, ...streamingItems],
    status: 'InProgress',
  };
}
