import { useState, useMemo, useCallback } from 'react';
import { Box, Typography, IconButton, InputBase } from '@mui/material';
import { Search, Check } from 'lucide-react';
import { useSkillStore, useSkills, type SkillInfo } from '@/stores/skillStore';

const MAX_SELECTED = 10;

/** 每个 skill 的图标背景色轮转 */
const ICON_BG_COLORS = ['#dbeafe', '#ccfbf1', '#fef3c7', '#f3e8ff', '#fce7f3', '#e0e7ff'];

interface SkillSelectorProps {
  onConfirm: () => void;
  onCancel: () => void;
}

export function SkillSelector({ onConfirm, onCancel }: SkillSelectorProps) {
  const skills = useSkills();
  const loading = useSkillStore((s) => s.loading);
  const selectedSkills = useSkillStore((s) => s.selectedSkills);
  const toggleSkill = useSkillStore((s) => s.toggleSkill);
  const setSelectedSkills = useSkillStore((s) => s.setSelectedSkills);

  const [search, setSearch] = useState('');

  // 记录打开时的快照，取消时恢复
  const [snapshot] = useState(() => [...selectedSkills]);

  const filtered = useMemo(
    () =>
      skills.filter(
        (s) =>
          s.name.toLowerCase().includes(search.toLowerCase()) ||
          s.description.toLowerCase().includes(search.toLowerCase()),
      ),
    [skills, search],
  );

  const isSelected = useCallback(
    (skill: SkillInfo) => selectedSkills.some((s) => s.name === skill.name),
    [selectedSkills],
  );

  const handleCancel = useCallback(() => {
    setSelectedSkills(snapshot);
    onCancel();
  }, [snapshot, setSelectedSkills, onCancel]);

  const selectedCount = selectedSkills.length;

  return (
    <Box
      sx={{
        width: 320,
        borderRadius: 6,
        border: '1px solid rgba(124,185,232,0.3)',
        bgcolor: 'rgba(124,185,232,0.15)',
        backdropFilter: 'blur(10px)',
        boxShadow: '0px 12px 40px 0px rgba(0,91,193,0.1)',
        overflow: 'hidden',
        display: 'flex',
        flexDirection: 'column',
      }}
    >
      {/* 搜索栏 */}
      <Box sx={{ borderBottom: '1px solid rgba(255,255,255,0.2)', px: 2, pt: 2, pb: 2 }}>
        <Box
          sx={{
            bgcolor: 'rgba(255,255,255,0.4)',
            borderRadius: 2,
            display: 'flex',
            alignItems: 'center',
            px: 1.5,
            py: 1,
            gap: 1,
          }}
        >
          <Search size={16} color="rgba(29,78,216,0.5)" />
          <InputBase
            autoFocus
            placeholder="搜索技能..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            sx={{
              flex: 1,
              fontSize: 14,
              fontFamily: 'Inter, sans-serif',
              color: '#1e293b',
              '& input::placeholder': { color: 'rgba(29,78,216,0.5)', opacity: 1 },
            }}
          />
        </Box>
      </Box>

      {/* 列表 */}
      <Box sx={{ maxHeight: 288, overflowY: 'auto', py: 1 }}>
        {filtered.map((skill, idx) => {
          const selected = isSelected(skill);
          const disabled = !selected && selectedCount >= MAX_SELECTED;
          return (
            <Box
              key={skill.name}
              onClick={() => !disabled && toggleSkill(skill)}
              sx={{
                display: 'flex',
                alignItems: 'center',
                gap: 1.5,
                px: 2,
                py: 1.5,
                cursor: disabled ? 'not-allowed' : 'pointer',
                opacity: disabled ? 0.5 : 1,
                '&:hover': disabled ? {} : { bgcolor: 'rgba(255,255,255,0.15)' },
                transition: 'background 0.15s',
              }}
            >
              {/* 图标 */}
              <Box
                sx={{
                  width: 40,
                  height: 40,
                  borderRadius: 2,
                  bgcolor: ICON_BG_COLORS[idx % ICON_BG_COLORS.length],
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  flexShrink: 0,
                  fontSize: 18,
                  fontWeight: 700,
                  color: '#1e293b',
                }}
              >
                {skill.name.charAt(0).toUpperCase()}
              </Box>

              {/* 文本 */}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Typography
                  sx={{
                    fontSize: 14,
                    fontWeight: 600,
                    color: '#1e293b',
                    lineHeight: '20px',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {skill.name}
                </Typography>
                <Typography
                  sx={{
                    fontSize: 10,
                    color: '#64748b',
                    lineHeight: '15px',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {skill.description}
                </Typography>
              </Box>

              {/* 勾选框 */}
              <Box
                sx={{
                  width: 22,
                  height: 22,
                  borderRadius: 1.5,
                  flexShrink: 0,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  ...(selected
                    ? { bgcolor: '#2563eb', border: '1px solid transparent' }
                    : { bgcolor: '#fff', border: '1px solid #93c5fd' }),
                }}
              >
                {selected && <Check size={14} color="#fff" strokeWidth={3} />}
              </Box>
            </Box>
          );
        })}
        {filtered.length === 0 && (
          <Typography sx={{ textAlign: 'center', py: 3, fontSize: 13, color: '#64748b' }}>
            {loading ? '加载中...' : '未找到技能'}
          </Typography>
        )}
      </Box>

      {/* 底部操作栏 */}
      <Box
        sx={{
          borderTop: '1px solid rgba(255,255,255,0.2)',
          bgcolor: 'rgba(255,255,255,0.2)',
          display: 'flex',
          justifyContent: 'flex-end',
          gap: 1,
          px: 1.5,
          py: 1.5,
        }}
      >
        <IconButton onClick={handleCancel} sx={{ borderRadius: 1, px: 2, py: 0.75 }}>
          <Typography sx={{ fontSize: 12, fontWeight: 700, color: '#1e40af' }}>取消</Typography>
        </IconButton>
        <Box
          onClick={onConfirm}
          sx={{
            bgcolor: '#2563eb',
            borderRadius: 1,
            px: 2,
            py: 0.75,
            cursor: 'pointer',
            boxShadow: '0px 1px 2px 0px rgba(0,0,0,0.05)',
            '&:hover': { bgcolor: '#1d4ed8' },
          }}
        >
          <Typography sx={{ fontSize: 12, fontWeight: 700, color: '#fff' }}>
            确认{selectedCount > 0 ? ` (${selectedCount})` : ''}
          </Typography>
        </Box>
      </Box>
    </Box>
  );
}
