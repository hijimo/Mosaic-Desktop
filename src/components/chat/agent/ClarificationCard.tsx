import { Box, Typography } from '@mui/material';
import { HelpCircle } from 'lucide-react';
import type { ClarificationState } from '@/types';

interface ClarificationCardProps {
  request: ClarificationState;
}

export function ClarificationCard({ request }: ClarificationCardProps): React.ReactElement {
  return (
    <Box sx={{
      bgcolor: '#f0f7ff', border: '2px solid rgba(124,185,232,0.3)', borderRadius: 4,
      boxShadow: '0px 8px 20px rgba(124,185,232,0.15)',
      p: '26px', position: 'relative', overflow: 'hidden',
    }}>
      {/* Decorative corner */}
      <Box sx={{
        position: 'absolute', top: 0, right: 0, width: 82, height: 82, opacity: 0.1,
        background: 'radial-gradient(circle at top right, #7cb9e8, transparent 70%)',
      }} />

      <Box sx={{ display: 'flex', gap: 1, alignItems: 'center', mb: 1 }}>
        <HelpCircle size={15} color="#005bc1" />
        <Typography sx={{ fontSize: 14, fontWeight: 600, color: '#005bc1' }}>
          需要澄清
        </Typography>
      </Box>
      <Typography sx={{ fontSize: 14, color: '#334155', lineHeight: '20px' }}>
        {request.message}
      </Typography>
    </Box>
  );
}
