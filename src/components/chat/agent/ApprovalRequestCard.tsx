import { Box, Typography, Button } from '@mui/material';
import { ShieldAlert } from 'lucide-react';
import type { ApprovalRequestState } from '@/types';

interface ApprovalRequestCardProps {
  request: ApprovalRequestState;
  onDecision?: (callId: string, decision: 'approve' | 'deny') => void;
}

export function ApprovalRequestCard({ request, onDecision }: ApprovalRequestCardProps): React.ReactElement {
  return (
    <Box sx={{
      bgcolor: '#fffbeb', border: '2px solid rgba(253,230,138,0.5)', borderRadius: 4,
      boxShadow: '0px 1px 2px rgba(0,0,0,0.05)', p: '22px',
    }}>
      <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
        <Box sx={{ bgcolor: '#fef3c7', borderRadius: 2, width: 40, height: 40, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0 }}>
          <ShieldAlert size={20} color="#d97706" />
        </Box>
        <Box sx={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 0.5 }}>
          <Typography sx={{ fontSize: 14, fontWeight: 600, color: '#78350f' }}>
            需要执行审批
          </Typography>
          {request.reason && (
            <Typography sx={{ fontSize: 12, color: '#92400e', lineHeight: '19.5px' }}>
              {request.reason}
            </Typography>
          )}
          {request.command && (
            <Box sx={{ bgcolor: '#0f172a', borderRadius: 1, px: 1.5, py: 1, mt: 0.5 }}>
              <Typography component="pre" sx={{ fontFamily: '"Liberation Mono", monospace', fontSize: 12, color: '#bfdbfe', m: 0 }}>
                $ {request.command.join(' ')}
              </Typography>
            </Box>
          )}
          <Box sx={{ display: 'flex', gap: 1, pt: 1.5 }}>
            <Button
              size="small"
              variant="contained"
              onClick={() => onDecision?.(request.callId, 'approve')}
              sx={{ bgcolor: '#d97706', fontSize: 12, fontWeight: 600, px: 2, py: '6.5px', borderRadius: 1, boxShadow: '0px 1px 2px rgba(0,0,0,0.05)', '&:hover': { bgcolor: '#b45309' } }}
            >
              批准执行
            </Button>
            <Button
              size="small"
              variant="outlined"
              onClick={() => onDecision?.(request.callId, 'deny')}
              sx={{ borderColor: '#fde68a', color: '#b45309', fontSize: 12, fontWeight: 600, px: 2, py: '6.5px', borderRadius: 1, bgcolor: '#fff' }}
            >
              拒绝
            </Button>
          </Box>
        </Box>
      </Box>
    </Box>
  );
}
