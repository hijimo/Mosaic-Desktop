import { useState, useCallback, useRef } from 'react';
import { Box, Typography, Popover } from '@mui/material';
import { Bot, ChevronDown, X } from 'lucide-react';
import { useSkillStore } from '@/stores/skillStore';
import { useAgentRoleStore } from '@/stores/agentRoleStore';
import { SkillTag } from './SkillTag';
import { AgentRoleSelector } from './AgentRoleSelector';

export function ActiveAgentBar() {
  const selectedSkills = useSkillStore((s) => s.selectedSkills);
  const removeSelectedSkill = useSkillStore((s) => s.removeSelectedSkill);
  const activeRole = useAgentRoleStore((s) => s.activeRole);
  const setActiveRole = useAgentRoleStore((s) => s.setActiveRole);

  const agentName = activeRole?.name ?? 'default';

  const pillRef = useRef<HTMLDivElement>(null);
  const [anchorEl, setAnchorEl] = useState<HTMLElement | null>(null);
  const open = Boolean(anchorEl);

  const handlePillClick = useCallback(() => {
    if (pillRef.current) setAnchorEl(pillRef.current);
  }, []);

  const handleClose = useCallback(() => {
    setAnchorEl(null);
  }, []);

  return (
    <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, minHeight: 34, overflow: 'hidden' }}>
      {/* Agent Pill */}
      <Box
        ref={pillRef}
        onClick={handlePillClick}
        sx={{
          display: 'flex',
          alignItems: 'center',
          gap: 1,
          bgcolor: activeRole ? 'rgba(37,99,235,0.1)' : 'rgba(124,185,232,0.1)',
          border: activeRole ? '1px solid rgba(37,99,235,0.2)' : '1px solid rgba(124,185,232,0.2)',
          borderRadius: 3,
          px: 1.5,
          py: 0.75,
          flexShrink: 0,
          cursor: 'pointer',
          '&:hover': { bgcolor: activeRole ? 'rgba(37,99,235,0.15)' : 'rgba(124,185,232,0.15)' },
          transition: 'background 0.15s',
        }}
      >
        <Bot size={12} color={activeRole ? '#2563eb' : '#7cb9e8'} />
        <Typography
          sx={{
            fontSize: 12,
            fontWeight: 600,
            color: activeRole ? '#2563eb' : '#7cb9e8',
            textTransform: 'uppercase',
            letterSpacing: '0.6px',
            lineHeight: '16px',
          }}
        >
          Agent:
        </Typography>
        <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#191c1e', lineHeight: '16px' }}>
          {agentName}
        </Typography>
        {activeRole ? (
          <Box
            onClick={(e) => { e.stopPropagation(); setActiveRole(null); }}
            sx={{
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              p: 0.25,
              borderRadius: 3,
              '&:hover': { bgcolor: 'rgba(0,0,0,0.06)' },
            }}
          >
            <X size={10} color="#64748b" />
          </Box>
        ) : (
          <ChevronDown size={10} color="#7cb9e8" />
        )}
      </Box>

      {/* Divider */}
      {selectedSkills.length > 0 && (
        <Box sx={{ width: '1px', height: 16, bgcolor: 'rgba(192,199,207,0.3)', flexShrink: 0 }} />
      )}

      {/* Skill Tags */}
      {selectedSkills.map((skill) => (
        <SkillTag key={skill.name} skill={skill} onRemove={removeSelectedSkill} />
      ))}

      {/* Agent Role Selector Popover */}
      <Popover
        open={open}
        anchorEl={anchorEl}
        onClose={handleClose}
        anchorOrigin={{ vertical: 'bottom', horizontal: 'left' }}
        transformOrigin={{ vertical: 'top', horizontal: 'left' }}
        slotProps={{
          paper: {
            sx: {
              bgcolor: 'transparent',
              boxShadow: 'none',
              overflow: 'visible',
              mt: 1,
            },
          },
        }}
      >
        <AgentRoleSelector onClose={handleClose} />
      </Popover>
    </Box>
  );
}
