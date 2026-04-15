import { Box } from '@mui/material';
import { useMessageStore } from '@/stores/messageStore';
import { StreamdownRenderer } from '../shared/StreamdownRenderer';

interface StreamingPlanListProps {
  threadId: string;
}

export function StreamingPlanList({ threadId }: StreamingPlanListProps): React.ReactElement | null {
  const viewRevision = useMessageStore(
    (s) => s.streamingByThread.get(threadId)?.streamingView.revision ?? -1,
  );
  const viewItems = viewRevision >= 0
    ? useMessageStore.getState().streamingByThread.get(threadId)?.streamingView.items
    : undefined;
  const items = Array.from(viewItems?.values() ?? []).filter(
    (item) => item.itemType === 'Plan' && Boolean(item.planText),
  );

  if (items.length === 0) return null;

  return (
    <>
      {items.map((item) => (
        <Box key={item.itemId} sx={{ flex: 1, fontSize: 14, color: '#334155' }}>
          <StreamdownRenderer isStreaming>{item.planText}</StreamdownRenderer>
        </Box>
      ))}
    </>
  );
}
