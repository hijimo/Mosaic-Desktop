import { Box } from '@mui/material';
import { useMessageStore } from '@/stores/messageStore';
import { ThinkingPanel } from '../agent/ThinkingPanel';

interface StreamingReasoningListProps {
  threadId: string;
}

export function StreamingReasoningList({ threadId }: StreamingReasoningListProps): React.ReactElement | null {
  const viewRevision = useMessageStore(
    (s) => s.streamingByThread.get(threadId)?.streamingView.revision ?? -1,
  );
  const viewItems = viewRevision >= 0
    ? useMessageStore.getState().streamingByThread.get(threadId)?.streamingView.items
    : undefined;
  const items = Array.from(viewItems?.values() ?? []).filter(
    (item) => item.itemType === 'Reasoning' && item.reasoningSummary.some(Boolean),
  );

  if (items.length === 0) return null;

  return (
    <>
      {items.map((item) => (
        <Box key={item.itemId} sx={{ flex: 1 }}>
          <ThinkingPanel
            text={item.reasoningSummary.filter(Boolean).join('\n')}
            isStreaming
          />
        </Box>
      ))}
    </>
  );
}
