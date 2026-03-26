import { Box, Typography } from '@mui/material';

export function AgentAvatar(): React.ReactElement {
  return (
    <Box sx={{
      width: 40, height: 40, borderRadius: '8px', flexShrink: 0,
      background: 'linear-gradient(135deg, #7cb9e8 0%, #005bc1 100%)',
      boxShadow: '0px 10px 15px -3px rgba(0,0,0,0.1), 0px 4px 6px -4px rgba(0,0,0,0.1)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
    }}>
      <Typography sx={{ fontSize: 14, fontWeight: 700, color: '#fff', lineHeight: 1 }}>M</Typography>
    </Box>
  );
}
