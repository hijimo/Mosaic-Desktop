import { Box, Typography } from '@mui/material';
import { ThumbsUp, ThumbsDown, RefreshCw } from 'lucide-react';
import type { TurnItem, UserInput, ToolCallState, ApprovalRequestState } from '@/types';
import { ToolCallDisplay } from './ToolCallDisplay';
import { ApprovalRequest } from './ApprovalRequest';

interface MessageProps {
  item: TurnItem;
  toolCalls?: ToolCallState[];
  approvalRequests?: ApprovalRequestState[];
  onApprovalDecision?: (callId: string, decision: 'approve' | 'deny') => void;
}

const userBubbleSx = {
  bgcolor: 'rgba(141,178,255,0.2)',
  borderRadius: '16px 16px 16px 0',
  px: 2.5,
  py: 2.5,
  maxWidth: '80%',
  boxShadow: '0px 1px 2px rgba(0,0,0,0.05)',
} as const;

const agentBubbleSx = {
  bgcolor: '#fff',
  border: '1px solid rgba(192,199,207,0.05)',
  borderRadius: '0 24px 24px 24px',
  boxShadow: '0px 8px 30px rgba(0,0,0,0.04)',
  p: '33px',
  maxWidth: '100%',
  display: 'flex',
  flexDirection: 'column',
  gap: 2,
} as const;

const avatarBaseSx = {
  width: 40,
  height: 40,
  borderRadius: '8px',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  flexShrink: 0,
} as const;

function UserAvatar(): React.ReactElement {
  return (
    <Box sx={{ ...avatarBaseSx, bgcolor: '#fff', border: '1px solid rgba(192,199,207,0.1)', boxShadow: '0px 1px 2px rgba(0,0,0,0.05)' }}>
      <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#005bc1', lineHeight: 1 }}>U</Typography>
    </Box>
  );
}

function AgentAvatar(): React.ReactElement {
  return (
    <Box sx={{ ...avatarBaseSx, background: 'linear-gradient(135deg, #005bc1 0%, #8db2ff 100%)', boxShadow: '0px 10px 15px -3px rgba(0,0,0,0.1)' }}>
      <Typography sx={{ fontSize: 14, fontWeight: 700, color: '#fff', lineHeight: 1 }}>M</Typography>
    </Box>
  );
}

function ActionBar(): React.ReactElement {
  const btnSx = { display: 'flex', alignItems: 'center', gap: 0.75, cursor: 'pointer', '&:hover': { opacity: 0.7 } } as const;
  const labelSx = { fontSize: 10, fontWeight: 600, color: '#94a3b8', textTransform: 'uppercase', letterSpacing: '0.5px' } as const;

  return (
    <Box sx={{ display: 'flex', gap: 2, px: 2, pt: 1 }}>
      <Box sx={btnSx}><ThumbsUp size={12} color="#94a3b8" /><Typography sx={labelSx}>Helpful</Typography></Box>
      <Box sx={btnSx}><ThumbsDown size={12} color="#94a3b8" /><Typography sx={labelSx}>Not Helpful</Typography></Box>
      <Box sx={btnSx}><RefreshCw size={10} color="#94a3b8" /><Typography sx={labelSx}>Regenerate</Typography></Box>
    </Box>
  );
}

export function Message({ item, toolCalls, approvalRequests, onApprovalDecision }: MessageProps): React.ReactElement | null {
  switch (item.type) {
    case 'UserMessage':
      return (
        <Box sx={{ display: 'flex', gap: 3, justifyContent: 'flex-end' }}>
          <Box sx={userBubbleSx}>
            {item.content
              .filter((c): c is UserInput & { type: 'text' } => c.type === 'text')
              .map((c, i) => (
                <Typography key={i} sx={{ fontSize: 16, color: '#41484e', lineHeight: '26px', whiteSpace: 'pre-wrap' }}>{c.text}</Typography>
              ))}
          </Box>
          <UserAvatar />
        </Box>
      );

    case 'AgentMessage':
      return (
        <Box sx={{ display: 'flex', gap: 3, alignItems: 'flex-start' }}>
          <AgentAvatar />
          <Box sx={{ display: 'flex', flexDirection: 'column', gap: 3, maxWidth: 762, flex: 1 }}>
            <Box sx={agentBubbleSx}>
              <Typography sx={{ fontSize: 16, color: '#41484e', lineHeight: '26px', whiteSpace: 'pre-wrap' }}>
                {item.content.map((c) => c.text).join('')}
              </Typography>
              {toolCalls?.map((tc) => <ToolCallDisplay key={tc.callId} toolCall={tc} />)}
              {approvalRequests?.map((ar) => (
                <ApprovalRequest key={ar.callId} request={ar} onDecision={onApprovalDecision} />
              ))}
            </Box>
            <ActionBar />
          </Box>
        </Box>
      );

    case 'Reasoning':
      return (
        <Box sx={{ display: 'flex', gap: 3, alignItems: 'flex-start' }}>
          <AgentAvatar />
          <Box sx={{ ...agentBubbleSx, bgcolor: '#f7f9fb', maxWidth: 762 }}>
            <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#005bc1', textTransform: 'uppercase', letterSpacing: '1.2px' }}>
              Reasoning
            </Typography>
            <Typography sx={{ fontSize: 14, color: '#41484e', lineHeight: '22px', whiteSpace: 'pre-wrap' }}>
              {item.summary_text.join('\n')}
            </Typography>
          </Box>
        </Box>
      );

    case 'Plan':
      return (
        <Box sx={{ display: 'flex', gap: 3, alignItems: 'flex-start' }}>
          <AgentAvatar />
          <Box sx={{ ...agentBubbleSx, maxWidth: 762 }}>
            <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#005bc1', textTransform: 'uppercase', letterSpacing: '1.2px' }}>
              Plan
            </Typography>
            <Typography sx={{ fontSize: 14, color: '#41484e', lineHeight: '22px', whiteSpace: 'pre-wrap' }}>
              {item.text}
            </Typography>
          </Box>
        </Box>
      );

    default:
      return null;
  }
}
