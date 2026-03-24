import { Box, Typography } from '@mui/material';
import { Terminal, Globe, Wrench, FileCode, Loader2 } from 'lucide-react';
import type { ToolCallState } from '@/types';

interface ToolCallDisplayProps {
  toolCall: ToolCallState;
}

const iconMap: Record<ToolCallState['type'], React.ReactNode> = {
  exec: <Terminal size={14} />,
  mcp: <Wrench size={14} />,
  web_search: <Globe size={14} />,
  patch: <FileCode size={14} />,
};

const labelMap: Record<ToolCallState['type'], string> = {
  exec: 'Command Execution',
  mcp: 'MCP Tool',
  web_search: 'Web Search',
  patch: 'Apply Patch',
};

const statusColor: Record<ToolCallState['status'], string> = {
  pending: '#94a3b8',
  running: '#005bc1',
  completed: '#006e20',
  failed: '#dc2626',
};

export function ToolCallDisplay({ toolCall }: ToolCallDisplayProps): React.ReactElement {
  const isRunning = toolCall.status === 'running' || toolCall.status === 'pending';

  return (
    <Box sx={{ bgcolor: '#f2f4f6', border: '1px solid rgba(192,199,207,0.1)', borderRadius: 2, overflow: 'hidden' }}>
      {/* Header */}
      <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', px: 2, py: 1.5 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Box sx={{ color: statusColor[toolCall.status], display: 'flex' }}>{iconMap[toolCall.type]}</Box>
          <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#005bc1', textTransform: 'uppercase', letterSpacing: '1.2px' }}>
            {labelMap[toolCall.type]}
          </Typography>
          {isRunning && <Loader2 size={12} color="#005bc1" className="animate-spin" />}
        </Box>
        <Typography sx={{ fontSize: 10, fontWeight: 600, color: statusColor[toolCall.status], textTransform: 'uppercase' }}>
          {toolCall.status}
        </Typography>
      </Box>

      {/* Command / Tool name */}
      {toolCall.command && (
        <Box sx={{ bgcolor: '#0f172a', px: 2, py: 1.5, fontFamily: '"Liberation Mono", monospace', fontSize: 12, color: '#93c5fd' }}>
          $ {toolCall.command.join(' ')}
        </Box>
      )}
      {toolCall.toolName && (
        <Box sx={{ px: 2, py: 1 }}>
          <Typography sx={{ fontSize: 12, color: '#41484e' }}>
            {toolCall.serverName && <>{toolCall.serverName} → </>}{toolCall.toolName}
          </Typography>
        </Box>
      )}

      {/* Output */}
      {toolCall.output && (
        <Box sx={{ bgcolor: '#0f172a', px: 2, py: 1.5, maxHeight: 200, overflow: 'auto' }}>
          <Typography component="pre" sx={{ fontFamily: '"Liberation Mono", monospace', fontSize: 12, color: '#93c5fd', whiteSpace: 'pre-wrap', m: 0 }}>
            {toolCall.output}
          </Typography>
        </Box>
      )}
    </Box>
  );
}
