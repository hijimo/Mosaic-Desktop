import { Box, Typography } from '@mui/material';
import {
  MessageSquarePlus,
  Workflow,
  Sparkles,
  Bot,
  Settings,
} from 'lucide-react';
import { RecentChats } from './RecentChats';
import { useThread } from '@/hooks/useThread';
import { useThreadStore } from '@/stores/threadStore';
import { useNavigate } from 'react-router-dom';

interface NavItem {
  icon: React.ReactNode;
  label: string;
  key: string;
}

const NAV_ITEMS: NavItem[] = [
  { icon: <MessageSquarePlus size={20} />, label: '新对话', key: 'new-chat' },
  { icon: <Workflow size={20} />, label: '自动化', key: 'automation' },
  { icon: <Sparkles size={20} />, label: '技能', key: 'skills' },
  { icon: <Bot size={20} />, label: '智能体', key: 'agents' },
];

export function Sidebar(): React.ReactElement {
  const { createThread } = useThread();
  const setActiveThread = useThreadStore((s) => s.setActiveThread);
  const navigate = useNavigate();

  const handleNav = (key: string) => {
    if (key === 'new-chat') {
      setActiveThread(null);
      createThread();
    } else if (key === 'skills') {
      navigate('/skills-hub');
    }
  };

  return (
    <Box
      sx={{
        width: 256,
        height: '100vh',
        bgcolor: '#f2f4f6',
        borderRight: '1px solid rgba(226,232,240,0.3)',
        backdropFilter: 'blur(12px)',
        display: 'flex',
        flexDirection: 'column',
        justifyContent: 'space-between',
        px: 2,
        py: 3,
        flexShrink: 0,
      }}
    >
      <Box sx={{ display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        {/* Brand */}
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.5, px: 1, mb: 5 }}>
          <Box
            sx={{
              width: 40,
              height: 40,
              borderRadius: 1,
              bgcolor: '#8db2ff',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            <Sparkles size={18} color="#fff" />
          </Box>
          <Box>
            <Typography
              sx={{
                fontFamily: 'Manrope, sans-serif',
                fontWeight: 700,
                fontSize: 16,
                color: '#7cb9e8',
                lineHeight: '20px',
              }}
            >
              Aether AI
            </Typography>
            <Typography
              sx={{
                fontWeight: 600,
                fontSize: 10,
                color: '#64748b',
                textTransform: 'uppercase',
                letterSpacing: '0.5px',
                lineHeight: '15px',
              }}
            >
              灵感工作台
            </Typography>
          </Box>
        </Box>

        {/* Navigation */}
        <Box sx={{ display: 'flex', flexDirection: 'column', gap: 0.5 }}>
          {NAV_ITEMS.map((item) => (
            <Box
              key={item.key}
              onClick={() => handleNav(item.key)}
              sx={{
                display: 'flex',
                alignItems: 'center',
                gap: 1.5,
                px: 2,
                py: 1.25,
                borderRadius: 1,
                color: '#475569',
                fontSize: 14,
                cursor: 'pointer',
                '&:hover': { bgcolor: 'rgba(0,0,0,0.04)' },
              }}
            >
              {item.icon}
              <Typography sx={{ fontSize: 14, fontWeight: 'inherit', color: 'inherit' }}>
                {item.label}
              </Typography>
            </Box>
          ))}
        </Box>

        {/* Recent Chats — scrollable */}
        <Box sx={{ flex: 1, overflow: 'auto', mt: 1 }}>
          <RecentChats />
        </Box>
      </Box>

      {/* Footer */}
      <Box sx={{ borderTop: '1px solid rgba(192,199,207,0.1)', pt: 2, flexShrink: 0 }}>
        <Box
          sx={{
            display: 'flex',
            alignItems: 'center',
            gap: 1.5,
            px: 2,
            py: 1.25,
            borderRadius: 1,
            cursor: 'pointer',
            '&:hover': { bgcolor: 'rgba(0,0,0,0.04)' },
          }}
        >
          <Settings size={20} color="#475569" />
          <Typography sx={{ fontSize: 14, color: '#475569' }}>设置</Typography>
        </Box>
      </Box>
    </Box>
  );
}
