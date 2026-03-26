import { useState, useCallback } from 'react';
import { Box, Typography, Popover, Button } from '@mui/material';
import { ChevronDown, ChevronRight, Sparkles, Archive } from 'lucide-react';
import useSWR from 'swr';
import { useThreadStore } from '@/stores/threadStore';
import { threadList, threadResume, threadGetMessages, threadArchive } from '@/services/api';
import { useMessageStore } from '@/stores/messageStore';
import type { ThreadMeta } from '@/types';

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'now';
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  return `${days}d`;
}

function projectName(cwd: string): string {
  const parts = cwd.replace(/\/+$/, '').split('/');
  return parts[parts.length - 1] || cwd;
}

function groupByCwd(threads: ThreadMeta[]): Map<string, ThreadMeta[]> {
  const sorted = [...threads].sort(
    (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );
  const groups = new Map<string, ThreadMeta[]>();
  for (const t of sorted) {
    const list = groups.get(t.cwd) ?? [];
    list.push(t);
    groups.set(t.cwd, list);
  }
  return groups;
}

const sectionTitle = {
  fontWeight: 600,
  fontSize: 11,
  color: 'rgba(65,72,78,0.5)',
  textTransform: 'uppercase',
  letterSpacing: '0.55px',
  lineHeight: '20px',
} as const;

const projectLabel = {
  fontWeight: 600,
  fontSize: 14,
  color: '#41484e',
  letterSpacing: '-0.35px',
  lineHeight: '20px',
} as const;

const chatItem = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  px: '32px',
  py: '6px',
  borderRadius: '4px',
  cursor: 'pointer',
} as const;

const chatName = {
  fontSize: 14,
  letterSpacing: '-0.35px',
  lineHeight: '20px',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
} as const;

const timeLabel = {
  fontSize: 10,
  letterSpacing: '-0.35px',
  lineHeight: '20px',
  flexShrink: 0,
  ml: 1,
} as const;

export function RecentChats(): React.ReactElement {
  const threads = useThreadStore((s) => s.threads);
  const activeThreadId = useThreadStore((s) => s.activeThreadId);
  const setActiveThread = useThreadStore((s) => s.setActiveThread);
  const addThread = useThreadStore((s) => s.addThread);
  const setMessages = useMessageStore((s) => s.setMessages);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [resuming, setResuming] = useState<string | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [confirmAnchor, setConfirmAnchor] = useState<HTMLElement | null>(null);
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const removeThread = useThreadStore((s) => s.removeThread);

  const handleArchiveClick = useCallback((e: React.MouseEvent<HTMLElement>, threadId: string) => {
    e.stopPropagation();
    setConfirmAnchor(e.currentTarget);
    setConfirmId(threadId);
  }, []);

  const handleArchiveConfirm = useCallback(async () => {
    if (!confirmId) return;
    setConfirmAnchor(null);
    setConfirmId(null);
    try {
      await threadArchive(confirmId);
    } catch {
      // thread may not be active on backend, still remove from UI
    }
    removeThread(confirmId);
  }, [confirmId, removeThread]);

  const handleArchiveCancel = useCallback(() => {
    setConfirmAnchor(null);
    setConfirmId(null);
  }, []);

  useSWR('thread_list', threadList, {
    onSuccess: (list) => {
      for (const meta of list) {
        if (!threads.has(meta.thread_id)) {
          addThread(meta);
        }
      }
    },
  });

  const handleSelectThread = async (threadId: string): Promise<void> => {
    if (resuming) return;
    setResuming(threadId);
    try {
      const meta = await threadResume(threadId);
      addThread(meta);
      const messages = await threadGetMessages(threadId);
      setMessages(threadId, messages);
      setActiveThread(threadId);
    } catch (err) {
      console.error('Failed to resume thread:', err);
    } finally {
      setResuming(null);
    }
  };

  const allThreads = Array.from(threads.values());
  const grouped = groupByCwd(allThreads);

  const toggle = (cwd: string): void => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      next.has(cwd) ? next.delete(cwd) : next.add(cwd);
      return next;
    });
  };

  return (
    <Box sx={{ pt: '24px', height: '100%', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      {/* Section header */}
      <Box sx={{ px: '12px' }}>
        <Typography sx={sectionTitle}>Recent Chats</Typography>
      </Box>

      {allThreads.length === 0 && (
        <Box sx={{ px: '12px', py: 1 }}>
          <Typography sx={{ fontSize: 12, color: 'rgba(65,72,78,0.5)', fontStyle: 'italic' }}>
            No conversations yet
          </Typography>
        </Box>
      )}

      {/* Grouped list */}
      <Box sx={{ pt: '16px', flex: 1, overflow: 'auto', pr: '4px', display: 'flex', flexDirection: 'column', gap: '16px' }}>
        {Array.from(grouped.entries()).map(([cwd, chats]) => {
          const isOpen = !collapsed.has(cwd);
          const hasActive = chats.some((c) => c.thread_id === activeThreadId);

          return (
            <Box key={cwd} sx={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
              {/* Project header */}
              <Box
                onClick={() => toggle(cwd)}
                sx={{
                  px: '12px', py: '4px',
                  display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                  cursor: 'pointer',
                }}
              >
                <Box sx={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  {isOpen
                    ? <ChevronDown size={9} color="#41484e" />
                    : <ChevronRight size={9} color="#41484e" />
                  }
                  <Typography sx={projectLabel}>{projectName(cwd)}</Typography>
                </Box>
                {hasActive && (
                  <Box sx={{ width: 8, height: 8, borderRadius: '50%', bgcolor: '#006e20', boxShadow: '0px 1px 2px rgba(0,110,32,0.4)' }} />
                )}
              </Box>

              {/* Chat items */}
              {isOpen && (
                <Box sx={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
                  {chats.map((chat) => {
                    const isActive = chat.thread_id === activeThreadId;
                    const isHovered = hoveredId === chat.thread_id;
                    return (
                      <Box
                        key={chat.thread_id}
                        onClick={() => handleSelectThread(chat.thread_id)}
                        onMouseEnter={() => setHoveredId(chat.thread_id)}
                        onMouseLeave={() => setHoveredId(null)}
                        sx={{
                          ...chatItem,
                          ...(isActive
                            ? {
                                bgcolor: 'rgba(216,226,255,0.3)',
                                borderLeft: '2px solid #005bc1',
                                pl: '34px',
                              }
                            : {
                                '&:hover': { bgcolor: 'rgba(0,0,0,0.04)' },
                              }),
                        }}
                      >
                        <Box sx={{ display: 'flex', alignItems: 'center', overflow: 'hidden', minWidth: 0 }}>
                          {isActive && <Sparkles size={13} color="#001a41" style={{ flexShrink: 0 }} />}
                          <Typography
                            sx={{
                              ...chatName,
                              fontWeight: isActive ? 500 : 400,
                              color: isActive ? '#001a41' : 'rgba(65,72,78,0.8)',
                              ml: isActive ? '8px' : 0,
                            }}
                          >
                            {chat.name || 'New Chat'}
                          </Typography>
                        </Box>
                        {isHovered || confirmId === chat.thread_id ? (
                          <Box
                            onClick={(e) => handleArchiveClick(e, chat.thread_id)}
                            sx={{ flexShrink: 0, ml: 1, display: 'flex', cursor: 'pointer', p: '2px', borderRadius: 1, '&:hover': { bgcolor: 'rgba(0,0,0,0.08)' } }}
                          >
                            <Archive size={13} color="#94a3b8" />
                          </Box>
                        ) : (
                          <Typography
                            sx={{
                              ...timeLabel,
                              color: isActive ? 'rgba(0,91,193,0.6)' : 'rgba(113,120,127,0.6)',
                            }}
                          >
                            {relativeTime(chat.created_at)}
                          </Typography>
                        )}
                      </Box>
                    );
                  })}
                </Box>
              )}
            </Box>
          );
        })}
      </Box>

      {/* Archive confirm popover */}
      <Popover
        open={Boolean(confirmAnchor)}
        anchorEl={confirmAnchor}
        onClose={handleArchiveCancel}
        anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}
        transformOrigin={{ vertical: 'top', horizontal: 'center' }}
      >
        <Box sx={{ p: 2, display: 'flex', flexDirection: 'column', gap: 1.5, maxWidth: 200 }}>
          <Typography sx={{ fontSize: 13, fontWeight: 500, color: '#191c1e' }}>确定归档此会话？</Typography>
          <Box sx={{ display: 'flex', gap: 1, justifyContent: 'flex-end' }}>
            <Button size="small" onClick={handleArchiveCancel} sx={{ fontSize: 12, color: '#64748b' }}>取消</Button>
            <Button size="small" variant="contained" onClick={handleArchiveConfirm} sx={{ fontSize: 12, bgcolor: '#dc2626', '&:hover': { bgcolor: '#b91c1c' } }}>归档</Button>
          </Box>
        </Box>
      </Popover>
    </Box>
  );
}
