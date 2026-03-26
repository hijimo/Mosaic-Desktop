import { Box, Typography } from '@mui/material';
import { Wrench } from 'lucide-react';
import { StatusBadge } from '../shared/StatusBadge';
import type { ToolCallState } from '@/types';

interface McpToolCallCardProps {
  toolCall: ToolCallState;
}

export function McpToolCallCard({ toolCall }: McpToolCallCardProps): React.ReactElement {
  return (
    <Box sx={{
      bgcolor: '#fff', border: '1px solid rgba(192,199,207,0.2)', borderRadius: 2,
      boxShadow: '0px 1px 2px rgba(0,0,0,0.05)',
      display: 'flex', gap: 1.5, alignItems: 'center', p: '13px',
    }}>
      <Box sx={{ bgcolor: '#fff7ed', borderRadius: 1, width: 32, height: 32, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0 }}>
        <Wrench size={15} color="#f97316" />
      </Box>
      <Box sx={{ flex: 1, minWidth: 0 }}>
        <Typography sx={{ fontSize: 11, fontWeight: 600, color: '#94a3b8', textTransform: 'uppercase', letterSpacing: '-0.275px' }}>
          MCP 工具调用
        </Typography>
        <Typography sx={{ fontSize: 12, fontWeight: 500, color: '#334155', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {toolCall.toolName ? `${toolCall.serverName ? toolCall.serverName + ' → ' : ''}${toolCall.toolName}` : toolCall.name}
          {toolCall.arguments ? `(${JSON.stringify(toolCall.arguments)})` : ''}
        </Typography>
      </Box>
      <StatusBadge status={toolCall.status} />
    </Box>
  );
}
