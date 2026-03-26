import { Box, Typography } from '@mui/material';
import { Copy } from 'lucide-react';
import type { ToolCallState } from '@/types';

interface CodeExecutionBlockProps {
  toolCall: ToolCallState;
}

export function CodeExecutionBlock({ toolCall }: CodeExecutionBlockProps): React.ReactElement {
  const commandStr = toolCall.command?.join(' ') ?? '';
  const title = toolCall.name || commandStr.split('/').pop() || 'terminal';

  const handleCopy = (): void => {
    const text = toolCall.output || commandStr;
    navigator.clipboard.writeText(text).catch(() => {});
  };

  return (
    <Box sx={{
      bgcolor: '#0f172a', border: '1px solid #1e293b', borderRadius: 2, overflow: 'hidden',
      boxShadow: '0px 20px 25px -5px rgba(0,0,0,0.1), 0px 8px 10px -6px rgba(0,0,0,0.1)',
    }}>
      {/* Title bar */}
      <Box sx={{
        bgcolor: 'rgba(30,41,59,0.5)', borderBottom: '1px solid #334155',
        display: 'flex', alignItems: 'center', justifyContent: 'space-between', px: 2, py: 1,
      }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Box sx={{ width: 10, height: 10, borderRadius: '50%', bgcolor: 'rgba(239,68,68,0.8)' }} />
          <Box sx={{ width: 10, height: 10, borderRadius: '50%', bgcolor: 'rgba(234,179,8,0.8)' }} />
          <Box sx={{ width: 10, height: 10, borderRadius: '50%', bgcolor: 'rgba(34,197,94,0.8)' }} />
          <Typography sx={{ fontSize: 10, color: '#94a3b8', fontFamily: '"Liberation Mono", monospace', pl: 1 }}>
            bash — {title}
          </Typography>
        </Box>
        <Box onClick={handleCopy} sx={{ cursor: 'pointer', display: 'flex', '&:hover': { opacity: 0.7 } }}>
          <Copy size={10} color="#94a3b8" />
        </Box>
      </Box>

      {/* Content */}
      <Box sx={{ px: 2, py: 2, fontFamily: '"Liberation Mono", monospace', fontSize: 12 }}>
        {commandStr && (
          <Typography component="span" sx={{ fontFamily: 'inherit', fontSize: 'inherit', color: '#34d399' }}>
            {'$ '}
          </Typography>
        )}
        {commandStr && (
          <Typography component="span" sx={{ fontFamily: 'inherit', fontSize: 'inherit', color: '#bfdbfe' }}>
            {commandStr}
          </Typography>
        )}
        {toolCall.output && (
          <Box component="pre" sx={{ fontFamily: 'inherit', fontSize: 'inherit', color: '#bfdbfe', lineHeight: '19.5px', whiteSpace: 'pre-wrap', m: 0, mt: commandStr ? 0.5 : 0 }}>
            {toolCall.output}
          </Box>
        )}
        {toolCall.status === 'running' && (
          <Typography component="span" sx={{ fontFamily: 'inherit', fontSize: 'inherit', color: '#bfdbfe' }}>_</Typography>
        )}
      </Box>
    </Box>
  );
}
