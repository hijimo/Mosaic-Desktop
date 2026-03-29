import { Box } from '@mui/material';
import { useMessageStore } from '@/stores/messageStore';
import { ThinkingPanel } from '../agent/ThinkingPanel';

export function StreamingReasoningList(): React.ReactElement | null {
  const viewItems = useMessageStore((s) => s.streamingView?.items);
  const items = Array.from(viewItems?.values() ?? []).filter(
      (item) =>
        item.itemType === 'Reasoning' && item.reasoningSummary.some(Boolean),
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
