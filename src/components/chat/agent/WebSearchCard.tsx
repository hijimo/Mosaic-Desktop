import { Box, Typography } from '@mui/material';
import { Globe } from 'lucide-react';
import { StatusBadge } from '../shared/StatusBadge';
import type { ToolCallState } from '@/types';

interface WebSearchCardProps {
  toolCall: ToolCallState;
}

export function WebSearchCard({ toolCall }: WebSearchCardProps): React.ReactElement {
  return (
    <Box sx={{
      bgcolor: '#fff', border: '1px solid rgba(192,199,207,0.2)', borderRadius: 2,
      boxShadow: '0px 1px 2px rgba(0,0,0,0.05)',
      display: 'flex', gap: 1.5, alignItems: 'center', p: '13px',
    }}>
      <Box sx={{ bgcolor: '#eff6ff', borderRadius: 1, width: 32, height: 32, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0 }}>
        <Globe size={15} color="#3b82f6" />
      </Box>
      <Box sx={{ flex: 1, minWidth: 0 }}>
        <Typography sx={{ fontSize: 11, fontWeight: 600, color: '#94a3b8', textTransform: 'uppercase', letterSpacing: '-0.275px' }}>
          网络搜索
        </Typography>
        <Typography sx={{ fontSize: 12, fontWeight: 500, color: '#334155', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {toolCall.name || '搜索中...'}
        </Typography>
      </Box>
      <StatusBadge status={toolCall.status} />
    </Box>
  );
}
