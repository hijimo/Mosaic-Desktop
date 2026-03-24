import { Box, Typography, Button } from '@mui/material';
import { ShieldAlert, Terminal, FileCode } from 'lucide-react';
import type { ApprovalRequestState } from '@/types';

interface ApprovalRequestProps {
  request: ApprovalRequestState;
  onDecision?: (callId: string, decision: 'approve' | 'deny') => void;
}

export function ApprovalRequest({ request, onDecision }: ApprovalRequestProps): React.ReactElement {
  const isExec = request.type === 'exec';
  const Icon = isExec ? Terminal : FileCode;

  return (
    <Box sx={{ border: '1px solid rgba(220,38,38,0.2)', borderRadius: 2, overflow: 'hidden', bgcolor: 'rgba(220,38,38,0.02)' }}>
      {/* Header */}
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, px: 2, py: 1.5, borderBottom: '1px solid rgba(220,38,38,0.1)' }}>
        <ShieldAlert size={14} color="#dc2626" />
        <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#dc2626', textTransform: 'uppercase', letterSpacing: '1px' }}>
          Approval Required
        </Typography>
      </Box>

      {/* Content */}
      <Box sx={{ px: 2, py: 1.5, display: 'flex', flexDirection: 'column', gap: 1 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Icon size={14} color="#41484e" />
          <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#191c1e' }}>
            {isExec ? 'Execute Command' : 'Apply Patch'}
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

        {/* Actions */}
        <Box sx={{ display: 'flex', gap: 1, pt: 0.5 }}>
          <Button
            size="small"
            variant="contained"
            onClick={() => onDecision?.(request.callId, 'approve')}
            sx={{ bgcolor: '#006e20', fontSize: 11, px: 2, py: 0.5, '&:hover': { bgcolor: '#005a1a' } }}
          >
            Approve
          </Button>
          <Button
            size="small"
            variant="outlined"
            onClick={() => onDecision?.(request.callId, 'deny')}
            sx={{ borderColor: '#dc2626', color: '#dc2626', fontSize: 11, px: 2, py: 0.5 }}
          >
            Deny
          </Button>
        </Box>
      </Box>
    </Box>
  );
}
