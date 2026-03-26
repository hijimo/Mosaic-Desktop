import { useState } from 'react';
import { Box, Typography } from '@mui/material';
import { Brain, ChevronDown } from 'lucide-react';

interface ThinkingPanelProps {
  text: string;
  isStreaming?: boolean;
}

export function ThinkingPanel({ text, isStreaming }: ThinkingPanelProps): React.ReactElement {
  const [open, setOpen] = useState(false);

  return (
    <Box sx={{ bgcolor: '#f8fafc', border: '1px solid #e2e8f0', borderRadius: 2, overflow: 'hidden' }}>
      <Box
        onClick={() => setOpen(!open)}
        sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', px: 2, py: 1, cursor: 'pointer' }}
      >
        <Box sx={{ display: 'flex', gap: 1, alignItems: 'center' }}>
          <Brain size={11.667} color="#64748b" />
          <Typography sx={{ fontSize: 12, fontWeight: 500, color: '#64748b' }}>
            {isStreaming ? '思考中...' : '思考过程'}
          </Typography>
        </Box>
        <ChevronDown
          size={12}
          color="#64748b"
          style={{ transform: open ? 'rotate(180deg)' : 'rotate(0deg)', transition: 'transform 0.2s' }}
        />
      </Box>
      {open && text && (
        <Box sx={{ px: 2, pb: 1.5 }}>
          <Typography sx={{ fontSize: 12, color: '#64748b', lineHeight: '19.5px', whiteSpace: 'pre-wrap' }}>
            {text}
          </Typography>
        </Box>
      )}
    </Box>
  );
}
