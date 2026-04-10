import { ApprovalRequest } from '../ApprovalRequest';
import type { ApprovalRequestState, ReviewDecision } from '@/types';

interface ApprovalRequestCardProps {
  request: ApprovalRequestState;
  onDecision?: (callId: string, decision: ReviewDecision) => void;
}

export function ApprovalRequestCard({ request, onDecision }: ApprovalRequestCardProps): React.ReactElement {
  return <ApprovalRequest request={request} onDecision={onDecision} />;
}
