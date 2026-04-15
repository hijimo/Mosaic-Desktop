import { useState, useMemo, useCallback, useRef, useEffect } from 'react';
import { Box, Typography, InputBase } from '@mui/material';
import { Search, Check } from 'lucide-react';
import { useAgentRoleStore, type AgentRoleInfo } from '@/stores/agentRoleStore';

const ICON_BG_COLORS = ['#dbeafe', '#ccfbf1', '#fef3c7', '#f3e8ff', '#fce7f3', '#e0e7ff'];

interface AgentRoleSelectorProps {
  onClose: () => void;
}

export function AgentRoleSelector({ onClose }: AgentRoleSelectorProps) {
  const roles = useAgentRoleStore((s) => s.roles);
  const loading = useAgentRoleStore((s) => s.loading);
  const activeRole = useAgentRoleStore((s) => s.activeRole);
  const setActiveRole = useAgentRoleStore((s) => s.setActiveRole);

  const [search, setSearch] = useState('');
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const timer = setTimeout(() => searchRef.current?.focus(), 50);
    return () => clearTimeout(timer);
  }, []);

  const filtered = useMemo(
    () =>
      roles.filter(
        (r) =>
          r.name.toLowerCase().includes(search.toLowerCase()) ||
          (r.description ?? '').toLowerCase().includes(search.toLowerCase()),
      ),
    [roles, search],
  );

  const handleSelect = useCallback(
    (role: AgentRoleInfo) => {
      // 单选切换：再次点击取消选中
      setActiveRole(activeRole?.name === role.name ? null : role);
      onClose();
    },
    [activeRole, setActiveRole, onClose],
  );

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
            inputRef={searchRef}
            autoFocus
            placeholder="搜索 Agent..."
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
        {filtered.map((role, idx) => {
          const selected = activeRole?.name === role.name;
          return (
            <Box
              key={role.name}
              onClick={() => handleSelect(role)}
              sx={{
                display: 'flex',
                alignItems: 'center',
                gap: 1.5,
                px: 2,
                py: 1.5,
                cursor: 'pointer',
                '&:hover': { bgcolor: 'rgba(255,255,255,0.15)' },
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
                {role.name.charAt(0).toUpperCase()}
              </Box>

              {/* 文本 */}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.75 }}>
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
                    {role.name}
                  </Typography>
                  {role.source === 'user' && (
                    <Typography
                      sx={{
                        fontSize: 9,
                        fontWeight: 600,
                        color: '#7c3aed',
                        bgcolor: '#ede9fe',
                        borderRadius: 1,
                        px: 0.75,
                        py: 0.125,
                        lineHeight: '14px',
                      }}
                    >
                      自定义
                    </Typography>
                  )}
                </Box>
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
                  {role.description ?? '无描述'}
                </Typography>
              </Box>

              {/* 选中指示 */}
              <Box
                sx={{
                  width: 22,
                  height: 22,
                  borderRadius: '50%',
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
            {loading ? '加载中...' : '未找到 Agent'}
          </Typography>
        )}
      </Box>
    </Box>
  );
}
