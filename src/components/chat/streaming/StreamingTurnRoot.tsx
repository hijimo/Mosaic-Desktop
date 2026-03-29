import { useCallback, useEffect, useRef } from 'react';
import { useMessageStore } from '@/stores/messageStore';
import { Message } from '../Message';
import { TaskStartedIndicator } from '../indicators/TaskStartedIndicator';
import { TaskCompletedIndicator } from '../indicators/TaskCompletedIndicator';
import { StreamingReasoningList } from './StreamingReasoningList';
import { StreamingPlanList } from './StreamingPlanList';
import { StreamingToolRegion } from './StreamingToolRegion';
import { StreamingApprovalRegion } from './StreamingApprovalRegion';
import { StreamingClarificationRegion } from './StreamingClarificationRegion';
import { StreamingAgentBody } from './StreamingAgentBody';

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
  const isStreaming = streamingView?.isStreaming ?? false;
  const isComplete = !isStreaming && turnGroups.length > 0;
  const frameRef = useRef<number | null>(null);

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

      {turnGroups.map((group, index) => (
        <Message
          key={`${group.turn_id}-${index}`}
          group={group}
          onApprovalDecision={onApprovalDecision}
        />
      ))}

      {isStreaming ? (
        <>
          <StreamingReasoningList />
          <StreamingPlanList />
          <StreamingToolRegion />
          <StreamingApprovalRegion onApprovalDecision={onApprovalDecision} />
          <StreamingClarificationRegion />
          <StreamingAgentBody />
        </>
      ) : null}

      {isComplete && <TaskCompletedIndicator />}
    </>
  );
}
