import { Box } from '@mui/material';
import { useClarificationStore } from '@/stores/clarificationStore';
import { ClarificationCard } from '../agent/ClarificationCard';

const EMPTY_MAP = new Map<string, never>();

interface StreamingClarificationRegionProps {
  threadId: string;
}

export function StreamingClarificationRegion({ threadId }: StreamingClarificationRegionProps): React.ReactElement | null {
  const clarifications = useClarificationStore((s) => s.byThread.get(threadId) ?? EMPTY_MAP);

  if (clarifications.size === 0) return null;

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1 }}>
      {Array.from(clarifications.values()).map((request) => (
        <ClarificationCard key={request.id} request={request} />
      ))}
    </Box>
  );
}
