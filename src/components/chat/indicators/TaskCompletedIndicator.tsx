import { Box, Typography } from '@mui/material';
import { CheckCircle } from 'lucide-react';

export function TaskCompletedIndicator(): React.ReactElement {
  return (
    <Box sx={{ display: 'flex', gap: 2, alignItems: 'center', justifyContent: 'center', pt: 5, pb: 1 }}>
      <Box sx={{ flex: 1, height: '1px', bgcolor: '#d1fae5' }} />
      <Box sx={{ display: 'flex', gap: 1, alignItems: 'center', px: 1.5, bgcolor: '#f7f9fb' }}>
        <CheckCircle size={11.667} color="#10b981" />
        <Typography sx={{ fontSize: 10, fontWeight: 600, color: '#10b981', textTransform: 'uppercase', letterSpacing: '1px' }}>
          任务完成
        </Typography>
      </Box>
      <Box sx={{ flex: 1, height: '1px', bgcolor: '#d1fae5' }} />
    </Box>
  );
}
