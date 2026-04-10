import { Box, Typography, Button } from '@mui/material';
import { ShieldAlert, Terminal, FileCode } from 'lucide-react';
import type { ApprovalRequestState, ReviewDecision } from '@/types';

interface ApprovalRequestProps {
  request: ApprovalRequestState;
  onDecision?: (callId: string, decision: ReviewDecision) => void;
}

function getDecisionLabel(decision: ReviewDecision, type: 'exec' | 'patch'): string {
  if (decision === 'approved') return '是，继续执行';
  if (decision === 'approved_for_session') {
    return type === 'exec'
      ? '是，本次会话中不再询问此命令'
      : '是，不再询问这些文件的变更';
  }
  if (decision === 'denied') return '否，跳过此操作继续';
  if (decision === 'abort') return '否，告诉 AI 换一种方式';
  if (typeof decision === 'object') {
    if ('approved_execpolicy_amendment' in decision) {
      const cmd = decision.approved_execpolicy_amendment.proposed_execpolicy_amendment.join(' ');
      return `是，以后不再询问以 \`${cmd}\` 开头的命令`;
    }
    if ('network_policy_amendment' in decision) {
      const { host, action } = decision.network_policy_amendment.network_policy_amendment;
      return action === 'allow'
        ? `是，以后允许访问 ${host}`
        : `否，以后禁止访问 ${host}`;
    }
  }
  return String(decision);
}

function isRejectDecision(decision: ReviewDecision): boolean {
  if (decision === 'denied' || decision === 'abort') return true;
  if (typeof decision === 'object' && 'network_policy_amendment' in decision) {
    return decision.network_policy_amendment.network_policy_amendment.action === 'deny';
  }
  return false;
}

export function ApprovalRequest({ request, onDecision }: ApprovalRequestProps): React.ReactElement {
  const isExec = request.type === 'exec';
  const Icon = isExec ? Terminal : FileCode;

  const decisions: ReviewDecision[] = request.availableDecisions ?? (isExec
    ? ['approved', 'abort']
    : ['approved', 'approved_for_session', 'abort']);

  return (
    <Box sx={{ border: '1px solid rgba(220,38,38,0.2)', borderRadius: 2, overflow: 'hidden', bgcolor: 'rgba(220,38,38,0.02)' }}>
      {/* Header */}
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, px: 2, py: 1.5, borderBottom: '1px solid rgba(220,38,38,0.1)' }}>
        <ShieldAlert size={14} color="#dc2626" />
        <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#dc2626', letterSpacing: '1px' }}>
          需要授权
        </Typography>
      </Box>

      {/* Content */}
      <Box sx={{ px: 2, py: 1.5, display: 'flex', flexDirection: 'column', gap: 1 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Icon size={14} color="#41484e" />
          <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#191c1e' }}>
            {isExec ? '执行命令' : '应用补丁'}
          </Typography>
        </Box>

        {request.command && (
          <Box sx={{ bgcolor: '#0f172a', borderRadius: 1, px: 1.5, py: 1 }}>
            <Typography component="pre" sx={{ fontFamily: '"Liberation Mono", monospace', fontSize: 12, color: '#93c5fd', m: 0 }}>
              $ {request.command.join(' ')}
            </Typography>
          </Box>
        )}

        {request.reason && (
          <Typography sx={{ fontSize: 12, color: '#41484e', lineHeight: '18px' }}>{request.reason}</Typography>
        )}

        {/* Dynamic decision buttons */}
        <Box sx={{ display: 'flex', flexDirection: 'column', gap: 0.5, pt: 0.5 }}>
          {decisions.map((decision) => {
            const reject = isRejectDecision(decision);
            const color = reject ? '#dc2626' : '#006e20';
            const key = typeof decision === 'string' ? decision : JSON.stringify(decision);
            return (
              <Button
                key={key}
                size="small"
                variant={reject ? 'outlined' : 'contained'}
                onClick={() => onDecision?.(request.callId, decision)}
                sx={{
                  ...(reject
                    ? { borderColor: color, color }
                    : { bgcolor: color, color: '#fff', '&:hover': { bgcolor: reject ? undefined : '#005a1a' } }),
                  fontSize: 11,
                  px: 2,
                  py: 0.5,
                  justifyContent: 'flex-start',
                  textTransform: 'none',
                }}
              >
                {getDecisionLabel(decision, request.type)}
              </Button>
            );
          })}
        </Box>
      </Box>
    </Box>
  );
}
