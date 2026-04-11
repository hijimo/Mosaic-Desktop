import { Box, Typography } from '@mui/material';
import { Zap, X } from 'lucide-react';
import type { SkillInfo } from '@/stores/skillStore';

interface SkillTagProps {
  skill: SkillInfo;
  onRemove: (name: string) => void;
}

export function SkillTag({ skill, onRemove }: SkillTagProps) {
  return (
    <Box
      sx={{
        display: 'flex',
        alignItems: 'center',
        gap: 0.75,
        bgcolor: 'rgba(216,226,255,0.5)',
        border: '1px solid rgba(124,185,232,0.1)',
        borderRadius: 2,
        px: 1.5,
        py: 0.75,
        boxShadow: '0px 1px 2px 0px rgba(0,0,0,0.05)',
        maxWidth: 180,
        flexShrink: 0,
      }}
    >
      <Zap size={12} color="#001a41" fill="#001a41" />
      <Typography
        sx={{
          fontSize: 12,
          fontWeight: 600,
          color: '#001a41',
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          maxWidth: 130,
          lineHeight: '16px',
        }}
      >
        {skill.name}
      </Typography>
      <Box
        onClick={() => onRemove(skill.name)}
        sx={{
          ml: 0.5,
          cursor: 'pointer',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          p: 0.25,
          borderRadius: 3,
          '&:hover': { bgcolor: 'rgba(0,0,0,0.06)' },
        }}
      >
        <X size={10} color="#64748b" />
      </Box>
    </Box>
  );
}
