import { Box, Typography } from '@mui/material';
import { Loader2 } from 'lucide-react';
import { useMessageStore } from '@/stores/messageStore';
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
    agentText ? (
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
    )
  );
}
