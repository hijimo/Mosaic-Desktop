import { Box, Typography } from '@mui/material';
import { Loader2 } from 'lucide-react';
import { useMessageStore } from '@/stores/messageStore';
import { AgentAvatar } from '../shared/AgentAvatar';
import { StreamdownRenderer } from '../shared/StreamdownRenderer';

export function StreamingAgentBody(): React.ReactElement | null {
  const streamingView = useMessageStore((s) => s.streamingView);
  const isStreaming = streamingView?.isStreaming ?? false;
  const items = Array.from(streamingView?.items.values() ?? []).filter(
    (item) => item.itemType === 'AgentMessage',
  );
  const agentText = items.map((item) => item.agentText).join('');

  if (!isStreaming && !agentText) return null;

  return (
    <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
      <AgentAvatar />
      <Box
        sx={{
          flex: 1,
          bgcolor: '#fff',
          border: '1px solid rgba(192,199,207,0.05)',
          borderRadius: '0 24px 24px 24px',
          boxShadow: '0px 8px 30px rgba(0,0,0,0.04)',
          p: '33px',
        }}
      >
        {agentText ? (
          <Box sx={{ fontSize: 16, color: '#41484e', lineHeight: '26px' }}>
            <StreamdownRenderer
              isStreaming={isStreaming}
              mode={isStreaming ? 'streaming-stable' : 'final'}
            >
              {agentText}
            </StreamdownRenderer>
          </Box>
        ) : (
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
            <Loader2 size={16} color='#005bc1' className='animate-spin' />
            <Typography sx={{ fontSize: 14, color: '#94a3b8' }}>
              思考中...
            </Typography>
          </Box>
        )}
      </Box>
    </Box>
  );
}
