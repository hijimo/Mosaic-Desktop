import { Box, Typography } from '@mui/material';
import { Bot, ChevronDown } from 'lucide-react';
import { useSkillStore } from '@/stores/skillStore';
import { SkillTag } from './SkillTag';

interface ActiveAgentBarProps {
  agentName?: string;
}

export function ActiveAgentBar({ agentName = 'Mosaic' }: ActiveAgentBarProps) {
  const selectedSkills = useSkillStore((s) => s.selectedSkills);
  const removeSelectedSkill = useSkillStore((s) => s.removeSelectedSkill);

  return (
    <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, minHeight: 34, overflow: 'hidden' }}>
      {/* Agent Pill */}
      <Box
        sx={{
          display: 'flex',
          alignItems: 'center',
          gap: 1,
          bgcolor: 'rgba(124,185,232,0.1)',
          border: '1px solid rgba(124,185,232,0.2)',
          borderRadius: 3,
          px: 1.5,
          py: 0.75,
          flexShrink: 0,
        }}
      >
        <Bot size={12} color="#7cb9e8" />
        <Typography
          sx={{
            fontSize: 12,
            fontWeight: 600,
            color: '#7cb9e8',
            textTransform: 'uppercase',
            letterSpacing: '0.6px',
            lineHeight: '16px',
          }}
        >
          Active Agent:
        </Typography>
        <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#191c1e', lineHeight: '16px' }}>
          {agentName}
        </Typography>
        <ChevronDown size={10} color="#7cb9e8" />
      </Box>

      {/* Divider — 竖线分隔符 */}
      {selectedSkills.length > 0 && (
        <Box sx={{ width: '1px', height: 16, bgcolor: 'rgba(192,199,207,0.3)', flexShrink: 0 }} />
      )}

      {/* Skill Tags — 同一行，溢出隐藏 */}
      {selectedSkills.map((skill) => (
        <SkillTag key={skill.name} skill={skill} onRemove={removeSelectedSkill} />
      ))}
    </Box>
  );
}
