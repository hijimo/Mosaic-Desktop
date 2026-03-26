import { Box, Typography } from '@mui/material';

export function UserAvatar(): React.ReactElement {
  return (
    <Box sx={{
      width: 40, height: 40, borderRadius: '8px', flexShrink: 0,
      bgcolor: '#fff',
      border: '1px solid rgba(192,199,207,0.1)',
      boxShadow: '0px 1px 2px rgba(0,0,0,0.05)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
    }}>
      <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#005bc1', lineHeight: 1 }}>U</Typography>
    </Box>
  );
}
