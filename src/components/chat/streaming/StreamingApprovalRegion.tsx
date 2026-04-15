import { Box } from '@mui/material';
import { useApprovalStore } from '@/stores/approvalStore';
import { useElicitationStore } from '@/stores/elicitationStore';
import { ApprovalRequestCard } from '../agent/ApprovalRequestCard';
import { ElicitationRequest } from '../ElicitationRequest';

import type { ReviewDecision } from '@/types';

const EMPTY_MAP = new Map<string, never>();

interface StreamingApprovalRegionProps {
  threadId: string;
  onApprovalDecision?: (callId: string, decision: ReviewDecision) => void;
  onElicitationDecision?: (requestId: string, serverName: string, decision: 'accept' | 'decline' | 'cancel', content?: Record<string, unknown>) => void;
}

export function StreamingApprovalRegion({
  threadId,
  onApprovalDecision,
  onElicitationDecision,
}: StreamingApprovalRegionProps): React.ReactElement | null {
  const approvals = useApprovalStore((s) => s.byThread.get(threadId) ?? EMPTY_MAP);
  const elicitations = useElicitationStore((s) => s.byThread.get(threadId) ?? EMPTY_MAP);

  if (approvals.size === 0 && elicitations.size === 0) return null;

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1 }}>
      {Array.from(approvals.values()).map((request) => (
        <ApprovalRequestCard key={request.callId} request={request} onDecision={onApprovalDecision} />
      ))}
      {Array.from(elicitations.values()).map((request) => (
        <ElicitationRequest
          key={request.requestId}
          serverName={request.serverName}
          requestId={request.requestId}
          message={request.message}
          mode={request.mode}
          schema={request.schema}
          url={request.url}
          onDecision={onElicitationDecision}
        />
      ))}
    </Box>
  );
}
