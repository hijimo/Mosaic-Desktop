import { Box } from '@mui/material';
import { useClarificationStore } from '@/stores/clarificationStore';
import { ClarificationCard } from '../agent/ClarificationCard';

export function StreamingClarificationRegion(): React.ReactElement | null {
  const clarifications = useClarificationStore((s) => s.requests);

  if (clarifications.size === 0) return null;

  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        gap: 2,
        flex: 1,
      }}
    >
      {Array.from(clarifications.values()).map((request) => (
        <ClarificationCard key={request.id} request={request} />
      ))}
    </Box>
  );
}
