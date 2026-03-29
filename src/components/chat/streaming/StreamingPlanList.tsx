import { Box } from '@mui/material';
import { useMessageStore } from '@/stores/messageStore';
import { AgentAvatar } from '../shared/AgentAvatar';
import { StreamdownRenderer } from '../shared/StreamdownRenderer';

export function StreamingPlanList(): React.ReactElement | null {
  const viewItems = useMessageStore((s) => s.streamingView?.items);
  const items = Array.from(viewItems?.values() ?? []).filter(
    (item) => item.itemType === 'Plan' && Boolean(item.planText),
  );

  if (items.length === 0) return null;

  return (
    <>
      {items.map((item) => (
        <Box
          key={item.itemId}
          sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}
        >
          <AgentAvatar />
          <Box sx={{ flex: 1, fontSize: 14, color: '#334155' }}>
            <StreamdownRenderer isStreaming>{item.planText}</StreamdownRenderer>
          </Box>
        </Box>
      ))}
    </>
  );
}
