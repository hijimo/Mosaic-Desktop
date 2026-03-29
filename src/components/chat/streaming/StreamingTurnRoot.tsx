import { useCallback, useEffect, useRef } from 'react';
import { Box } from '@mui/material';
import { useMessageStore } from '@/stores/messageStore';
import { Message } from '../Message';
import { TaskStartedIndicator } from '../indicators/TaskStartedIndicator';
import { TaskCompletedIndicator } from '../indicators/TaskCompletedIndicator';
import { AgentAvatar } from '../shared/AgentAvatar';
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
        <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
          <AgentAvatar />
          <Box
            data-testid='streaming-agent-turn-content'
            sx={{
              display: 'flex',
              flexDirection: 'column',
              gap: 2,
              flex: 1,
              bgcolor: '#fff',
              border: '1px solid rgba(192,199,207,0.05)',
              borderRadius: '0 24px 24px 24px',
              boxShadow: '0px 8px 30px rgba(0,0,0,0.04)',
              p: '16px',
            }}
          >
            <StreamingReasoningList />
            <StreamingPlanList />
            <StreamingToolRegion />
            <StreamingApprovalRegion onApprovalDecision={onApprovalDecision} />
            <StreamingClarificationRegion />
            <StreamingAgentBody />
          </Box>
        </Box>
      ) : null}

      {isComplete && <TaskCompletedIndicator />}
    </>
  );
}
