import { useLayoutEffect } from 'react';
import { Box } from '@mui/material';
import { useMessageStore } from '@/stores/messageStore';
import { useBottomLockScroll } from '@/hooks/useBottomLockScroll';
import { StreamingTurnRoot } from './streaming/StreamingTurnRoot';

interface MessageListProps {
  threadId: string;
  onApprovalDecision?: (callId: string, decision: 'approve' | 'deny') => void;
}

const EMPTY_GROUPS: never[] = [];

export function MessageList({
  threadId,
  onApprovalDecision,
}: MessageListProps): React.ReactElement {
  const turnGroups = useMessageStore(
    (s) => s.messagesByThread.get(threadId) ?? EMPTY_GROUPS,
  );
  const streamingView = useMessageStore((s) => s.streamingView);
  const { attachContainer, handleScroll, scheduleReconcile } =
    useBottomLockScroll();

  const msgLen = turnGroups.length;
  const streamRevision = streamingView?.revision ?? 0;

  useLayoutEffect(() => {
    scheduleReconcile();
  }, [msgLen, streamRevision, scheduleReconcile]);

  return (
    <Box
      ref={attachContainer}
      onScroll={handleScroll}
      sx={{
        flex: 1,
        overflow: 'auto',
        overflowAnchor: 'none',
        px: 8,
        pt: 3,
        pb: 24,
        display: 'flex',
        flexDirection: 'column',
        gap: 4,
      }}
    >
      <StreamingTurnRoot
        threadId={threadId}
        onApprovalDecision={onApprovalDecision}
      />
    </Box>
  );
}
