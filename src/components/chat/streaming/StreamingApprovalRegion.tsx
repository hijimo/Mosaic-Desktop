import { Box } from '@mui/material';
import { useApprovalStore } from '@/stores/approvalStore';
import { ApprovalRequestCard } from '../agent/ApprovalRequestCard';

interface StreamingApprovalRegionProps {
  onApprovalDecision?: (callId: string, decision: 'approve' | 'deny') => void;
}

export function StreamingApprovalRegion({
  onApprovalDecision,
}: StreamingApprovalRegionProps): React.ReactElement | null {
  const approvals = useApprovalStore((s) => s.approvals);

  if (approvals.size === 0) return null;

  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        gap: 2,
        flex: 1,
      }}
    >
      {Array.from(approvals.values()).map((request) => (
        <ApprovalRequestCard
          key={request.callId}
          request={request}
          onDecision={onApprovalDecision}
        />
      ))}
    </Box>
  );
}
