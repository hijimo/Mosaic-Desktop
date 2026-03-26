import { Box, Typography } from '@mui/material';

type Status = 'running' | 'completed' | 'failed' | 'pending';

const statusConfig: Record<Status, { bg: string; color: string; label: string }> = {
  completed: { bg: 'rgba(119,220,122,0.2)', color: '#006e20', label: '已完成' },
  running: { bg: 'transparent', color: '#3b82f6', label: '运行中' },
  pending: { bg: 'transparent', color: '#94a3b8', label: '等待中' },
  failed: { bg: 'rgba(220,38,38,0.1)', color: '#dc2626', label: '失败' },
};

interface StatusBadgeProps {
  status: Status;
}

export function StatusBadge({ status }: StatusBadgeProps): React.ReactElement {
  const cfg = statusConfig[status];

  if (status === 'running') {
    return (
      <Box sx={{ display: 'flex', gap: '6px', alignItems: 'center' }}>
        <Box sx={{ width: 8, height: 8, borderRadius: '50%', bgcolor: cfg.color }} />
        <Typography sx={{ fontSize: 10, fontWeight: 600, color: cfg.color, textTransform: 'uppercase' }}>
          {cfg.label}
        </Typography>
      </Box>
    );
  }

  return (
    <Box sx={{ bgcolor: cfg.bg, borderRadius: '12px', px: 1, py: '2px' }}>
      <Typography sx={{ fontSize: 10, fontWeight: 600, color: cfg.color, textTransform: 'uppercase' }}>
        {cfg.label}
      </Typography>
    </Box>
  );
}
