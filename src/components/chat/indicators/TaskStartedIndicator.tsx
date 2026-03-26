import { Box, Typography } from '@mui/material';
import { Zap } from 'lucide-react';

export function TaskStartedIndicator(): React.ReactElement {
  return (
    <Box sx={{ display: 'flex', gap: 2, alignItems: 'center', justifyContent: 'center', py: 1 }}>
      <Box sx={{ flex: 1, height: '1px', bgcolor: 'rgba(192,199,207,0.2)' }} />
      <Box sx={{ display: 'flex', gap: 1, alignItems: 'center', px: 1.5, bgcolor: '#f7f9fb' }}>
        <Zap size={11.667} color="#94a3b8" />
        <Typography sx={{ fontSize: 10, fontWeight: 600, color: '#94a3b8', textTransform: 'uppercase', letterSpacing: '1px' }}>
          任务开始
        </Typography>
      </Box>
      <Box sx={{ flex: 1, height: '1px', bgcolor: 'rgba(192,199,207,0.2)' }} />
    </Box>
  );
}
