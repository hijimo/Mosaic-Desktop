import { useState, useCallback, useEffect } from 'react';
import {
  Box,
  Typography,
  IconButton,
  Menu,
  MenuItem,
  ListItemIcon,
  ListItemText,
} from '@mui/material';
import {
  Activity,
  Bell,
  Code,
  BarChart3,
  Globe,
  PenLine,
  Zap,
  FileSearch,
  Languages,
  ChevronDown,
  FolderOpen,
  Plus,
  Check,
} from 'lucide-react';
import { useParams } from 'react-router-dom';
import { useThreadStore } from '@/stores/threadStore';
import { useMessageStore } from '@/stores/messageStore';
import { useThread } from '@/hooks/useThread';
import { useSubmitOp } from '@/hooks/useSubmitOp';
import { useLoadSkills } from '@/hooks/useLoadSkills';
import { useLoadAgentRoles } from '@/hooks/useLoadAgentRoles';
import { MessageList } from '@/components/chat/MessageList';
import { InputArea } from '@/components/chat/InputArea';
import { getHomeDir, listCwds, pickFolder, getConfig } from '@/services/api';
import type { AttachedFile } from '@/stores/fileUploadStore';
import type { UserInput, ReviewDecision } from '@/types';
import { useApprovalStore } from '@/stores/approvalStore';
import { useElicitationStore } from '@/stores/elicitationStore';

interface SkillCard {
  icon: React.ReactNode;
  title: string;
  description: string;
  prompt: string;
}

const SKILL_CARDS: SkillCard[] = [
  {
    icon: <Code size={16} />,
    title: '代码审查',
    description: '深度分析逻辑与性能问题。',
    prompt: '审查我的代码，检查 bug 和性能问题。',
  },
  {
    icon: <BarChart3 size={18} />,
    title: '数据分析',
    description: '从 CSV 或 SQL 中可视化趋势。',
    prompt: '分析数据并展示关键趋势。',
  },
  {
    icon: <Globe size={20} />,
    title: '浏览代理',
    description: '实时搜索网络内容。',
    prompt: '搜索网络上关于这个主题的最新信息。',
  },
  {
    icon: <PenLine size={16} />,
    title: '文案撰写',
    description: '为邮件和文档润色文案。',
    prompt: '帮我撰写一封专业的邮件。',
  },
];

const SUGGESTIONS = [
  {
    icon: <Zap size={16} />,
    label: '优化',
    prompt: '优化这段代码以提升性能。',
  },
  { icon: <FileSearch size={15} />, label: '总结', prompt: '总结要点。' },
  {
    icon: <Languages size={16} />,
    label: '翻译',
    prompt: '将这段内容翻译成英文。',
  },
];

export function IndexPage(): React.ReactElement {
  const { threadId: routeThreadId } = useParams<{ threadId?: string }>();
  const [inputText, setInputText] = useState('');

  const activeThreadId = useThreadStore((s) => s.activeThreadId);
  const messages = useMessageStore((s) => s.messagesByThread);
  const streamingTurn = useMessageStore((s) => s.streamingTurn);
  const { createThread, resumeThread } = useThread();
  const submitOp = useSubmitOp();
  const removeApproval = useApprovalStore((s) => s.removeApproval);

  const handleApproval = useCallback(
    (callId: string, decision: ReviewDecision) => {
      if (!activeThreadId) return;
      const approval = useApprovalStore.getState().approvals.get(callId);
      if (!approval) return;

      const op =
        approval.type === 'exec'
          ? { type: 'exec_approval' as const, id: callId, decision }
          : { type: 'patch_approval' as const, id: callId, decision };

      submitOp(activeThreadId, op);
      removeApproval(callId);
    },
    [activeThreadId, submitOp, removeApproval],
  );

  const removeElicitation = useElicitationStore((s) => s.removeRequest);
  const handleElicitation = useCallback(
    (
      requestId: string,
      serverName: string,
      decision: 'accept' | 'decline' | 'cancel',
      content?: Record<string, unknown>,
    ) => {
      if (!activeThreadId) return;
      submitOp(activeThreadId, {
        type: 'resolve_elicitation',
        server_name: serverName,
        request_id: requestId,
        decision,
        ...(content ? { content } : {}),
      });
      removeElicitation(requestId);
    },
    [activeThreadId, submitOp, removeElicitation],
  );

  // ── Workspace selector state ──
  const [selectedCwd, setSelectedCwd] = useState<string | null>(null);
  const [cwdList, setCwdList] = useState<string[]>([]);
  const [anchorEl, setAnchorEl] = useState<null | HTMLElement>(null);

  useEffect(() => {
    (async () => {
      const [cwds, home] = await Promise.all([listCwds(), getHomeDir()]);
      const set = new Set(cwds);
      const list = [...cwds];
      if (!set.has(home)) list.push(home);
      setSelectedCwd(home);
      setCwdList(list);
    })().catch(console.error);
  }, []);

  const folderName = (path: string): string => {
    const parts = path.replace(/[/\\]+$/, '').split(/[/\\]/);
    return parts[parts.length - 1] || path;
  };

  const handlePickFolder = useCallback(async () => {
    const picked = await pickFolder();
    if (picked) {
      setSelectedCwd(picked);
      setCwdList((prev) => (prev.includes(picked) ? prev : [picked, ...prev]));
    }
  }, []);

  const currentThreadId = routeThreadId ?? activeThreadId;
  const threadMessages = currentThreadId
    ? (messages.get(currentThreadId) ?? [])
    : [];
  const hasMessages = threadMessages.length > 0 || streamingTurn?.isStreaming;

  // ── Skills: 有效 cwd = 当前 thread 的 cwd（有会话时） 或 selectedCwd（新会话页面） ──
  const threads = useThreadStore((s) => s.threads);
  const threadCwd = currentThreadId
    ? (threads.get(currentThreadId)?.cwd ?? null)
    : null;
  const effectiveCwd = threadCwd ?? selectedCwd;
  useLoadSkills(effectiveCwd);
  useLoadAgentRoles();

  // ── Read config for approval/sandbox policies ──
  const [approvalPolicy, setApprovalPolicy] =
    useState<import('@/types/events').ApprovalPolicy>('on-request');
  const [sandboxPolicy, setSandboxPolicy] = useState<
    import('@/types/events').SandboxPolicy
  >({ type: 'danger-full-access' });

  useEffect(() => {
    (async () => {
      try {
        const config = (await getConfig()) as Record<string, unknown>;

        // approval_policy is already resolved by Rust (profile merged)
        if (
          config.approval_policy &&
          typeof config.approval_policy === 'string'
        ) {
          setApprovalPolicy(
            config.approval_policy as import('@/types/events').ApprovalPolicy,
          );
        }

        const mode = config.sandbox_mode as string | undefined;
        const wsWrite = config.sandbox_workspace_write as
          | { writable_roots?: string[]; network_access?: boolean }
          | undefined;
        if (mode === 'workspace-write') {
          setSandboxPolicy({
            type: 'workspace-write',
            writable_roots: wsWrite?.writable_roots ?? [],
            read_only_access: { type: 'full-access' },
            network_access: wsWrite?.network_access ?? false,
          });
        } else if (mode === 'read-only') {
          setSandboxPolicy({
            type: 'read-only',
            access: { type: 'full-access' },
          });
        }
      } catch (e) {
        console.warn('failed to read config, using defaults', e);
      }
    })();
  }, []);

  const handleSend = useCallback(
    async (text?: string, files: AttachedFile[] = []) => {
      const msg = (text ?? inputText).trim();
      if (!msg && files.length === 0) return;

      let tid = currentThreadId;
      if (!tid) {
        tid = await createThread(selectedCwd ?? undefined);
      } else {
        await resumeThread(tid);
      }

      setInputText('');

      const items: UserInput[] = [];
      if (msg) {
        items.push({ type: 'text', text: msg, text_elements: [] });
      }
      for (const f of files) {
        items.push({ type: 'attached_file', name: f.name, path: f.path });
      }

      await submitOp(tid, {
        type: 'user_turn',
        items,
        cwd: '.',
        model: '',
        approval_policy: approvalPolicy,
        sandbox_policy: sandboxPolicy,
      });
    },
    [
      inputText,
      currentThreadId,
      selectedCwd,
      createThread,
      resumeThread,
      submitOp,
      approvalPolicy,
      sandboxPolicy,
    ],
  );

  // ── Chat view (has messages) ──
  if (hasMessages) {
    return (
      <Box sx={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
        {/* Header */}
        <Box
          sx={{
            height: 56,
            backdropFilter: 'blur(6px)',
            bgcolor: 'rgba(255,255,255,0.7)',
            borderBottom: '1px solid #f1f5f9',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'flex-end',
            px: 4,
            flexShrink: 0,
          }}
        >
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 3 }}>
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
              <Activity size={16} color='#41484e' />
              <Typography
                sx={{
                  fontSize: 10,
                  fontWeight: 500,
                  color: '#41484e',
                  textTransform: 'uppercase',
                  letterSpacing: '1px',
                }}
              >
                实时节点
              </Typography>
            </Box>
            <Bell size={20} color='#41484e' />
          </Box>
        </Box>

        {/* Messages */}
        <MessageList
          threadId={currentThreadId!}
          onApprovalDecision={handleApproval}
          onElicitationDecision={handleElicitation}
        />

        {/* Input */}
        <Box sx={{ px: 4, pb: 3, pt: 1, flexShrink: 0 }}>
          <InputArea
            value={inputText}
            onChange={setInputText}
            onSend={(text, files) => handleSend(text, files)}
            isStreaming={streamingTurn?.isStreaming}
            onStop={() =>
              activeThreadId && submitOp(activeThreadId, { type: 'interrupt' })
            }
          />
        </Box>
      </Box>
    );
  }

  // ── Welcome view (no messages) ──
  return (
    <Box sx={{ height: '100%', position: 'relative', overflow: 'hidden' }}>
      {/* Background blurs */}
      <Box
        sx={{
          position: 'absolute',
          top: '25%',
          left: '25%',
          right: '37.5%',
          bottom: '37.5%',
          bgcolor: 'rgba(124,185,232,0.05)',
          filter: 'blur(50px)',
          borderRadius: 3,
        }}
      />
      <Box
        sx={{
          position: 'absolute',
          top: '37.5%',
          left: '37.5%',
          right: '25%',
          bottom: '25%',
          bgcolor: 'rgba(212,230,229,0.2)',
          filter: 'blur(50px)',
          borderRadius: 3,
        }}
      />

      {/* Header */}
      <Box
        sx={{
          position: 'absolute',
          top: 0,
          left: 0,
          right: 0,
          height: 56,
          backdropFilter: 'blur(6px)',
          bgcolor: 'rgba(255,255,255,0.7)',
          borderBottom: '1px solid #f1f5f9',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'flex-end',
          px: 4,
          zIndex: 10,
        }}
      >
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 3 }}>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
            <Activity size={16} color='#41484e' />
            <Typography
              sx={{
                fontSize: 10,
                fontWeight: 500,
                color: '#41484e',
                textTransform: 'uppercase',
                letterSpacing: '1px',
              }}
            >
              实时节点
            </Typography>
          </Box>
          <Bell size={20} color='#41484e' />
        </Box>
      </Box>

      {/* Welcome */}
      <Box
        sx={{
          position: 'absolute',
          top: 84,
          left: 0,
          right: 0,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: 2,
          pb: 6,
        }}
      >
        <Typography
          sx={{
            fontFamily: 'Manrope, sans-serif',
            fontWeight: 800,
            fontSize: 48,
            color: '#191c1e',
            textAlign: 'center',
            letterSpacing: '-1.2px',
            lineHeight: '48px',
          }}
        >
          今天我能帮你做什么？
        </Typography>
        <Typography
          sx={{
            fontSize: 16,
            color: '#41484e',
            textAlign: 'center',
            lineHeight: '24px',
          }}
        >
          选择一个项目，开启你的下一个
          <br />
          自动化工作任务。
        </Typography>
      </Box>

      {/* Interaction Container */}
      <Box
        sx={{
          position: 'absolute',
          top: '50%',
          left: 128,
          right: 128,
          maxWidth: 768,
          mx: 'auto',
          transform: 'translateY(-50%)',
          display: 'flex',
          flexDirection: 'column',
          gap: 3,
        }}
      >
        {/* Project Selector */}
        <Box
          sx={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            gap: 1.5,
          }}
        >
          <Box
            sx={{
              bgcolor: '#eceef0',
              borderRadius: 3,
              p: 0.5,
              display: 'flex',
              alignItems: 'center',
              gap: 1,
            }}
          >
            <Box
              onClick={(e: React.MouseEvent<HTMLDivElement>) =>
                setAnchorEl(e.currentTarget)
              }
              sx={{
                bgcolor: '#fff',
                borderRadius: 3,
                px: 2.5,
                py: 1,
                display: 'flex',
                alignItems: 'center',
                gap: 1,
                boxShadow: '0px 1px 2px rgba(0,0,0,0.05)',
                border: '1px solid rgba(192,199,207,0.1)',
                cursor: 'pointer',
              }}
            >
              <FolderOpen size={14} color='#191c1e' />
              <Typography
                sx={{ fontSize: 14, fontWeight: 600, color: '#191c1e' }}
              >
                {selectedCwd ? folderName(selectedCwd) : '...'}
              </Typography>
              <ChevronDown size={10} color='#191c1e' />
            </Box>
            <Menu
              anchorEl={anchorEl}
              open={Boolean(anchorEl)}
              onClose={() => setAnchorEl(null)}
              slotProps={{ paper: { sx: { maxHeight: 300, minWidth: 220 } } }}
            >
              {cwdList.map((cwd) => (
                <MenuItem
                  key={cwd}
                  selected={cwd === selectedCwd}
                  onClick={() => {
                    setSelectedCwd(cwd);
                    setAnchorEl(null);
                  }}
                >
                  <ListItemIcon sx={{ minWidth: 28 }}>
                    {cwd === selectedCwd ? (
                      <Check size={14} />
                    ) : (
                      <FolderOpen size={14} />
                    )}
                  </ListItemIcon>
                  <ListItemText
                    primary={folderName(cwd)}
                    secondary={cwd}
                    slotProps={{
                      primary: { sx: { fontSize: 14, fontWeight: 600 } },
                      secondary: { sx: { fontSize: 11, opacity: 0.6 } },
                    }}
                  />
                </MenuItem>
              ))}
            </Menu>
            <IconButton size='small' sx={{ p: 1 }} onClick={handlePickFolder}>
              <Plus size={14} color='#64748b' />
            </IconButton>
          </Box>
        </Box>

        {/* Input Area */}
        <InputArea
          value={inputText}
          onChange={setInputText}
          onSend={(text, files) => handleSend(text, files)}
          variant='welcome'
        />

        {/* Suggestions */}
        <Box sx={{ display: 'flex', justifyContent: 'center', gap: 3 }}>
          {SUGGESTIONS.map((s) => (
            <Box
              key={s.label}
              onClick={() => handleSend(s.prompt)}
              sx={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: 0.5,
                opacity: 0.4,
                cursor: 'pointer',
                '&:hover': { opacity: 0.7 },
              }}
            >
              {s.icon}
              <Typography
                sx={{
                  fontSize: 10,
                  fontWeight: 600,
                  textTransform: 'uppercase',
                  letterSpacing: '0.5px',
                  color: '#191c1e',
                }}
              >
                {s.label}
              </Typography>
            </Box>
          ))}
        </Box>
      </Box>

      {/* Skill Cards */}
      <Box
        sx={{
          position: 'absolute',
          bottom: 48,
          left: 64,
          right: 64,
          maxWidth: 896,
          mx: 'auto',
        }}
      >
        <Box
          sx={{
            display: 'grid',
            gridTemplateColumns: 'repeat(4, 1fr)',
            gap: 2,
            px: 2,
          }}
        >
          {SKILL_CARDS.map((card) => (
            <Box
              key={card.title}
              onClick={() => handleSend(card.prompt)}
              sx={{
                bgcolor: 'rgba(255,255,255,0.4)',
                borderRadius: 4,
                p: 2,
                height: 122,
                border: '1px solid rgba(0,0,0,0)',
                cursor: 'pointer',
                '&:hover': { bgcolor: 'rgba(255,255,255,0.7)' },
              }}
            >
              <Box sx={{ color: '#191c1e', mb: 2 }}>{card.icon}</Box>
              <Typography
                sx={{
                  fontSize: 12,
                  fontWeight: 600,
                  color: '#191c1e',
                  textTransform: 'uppercase',
                  letterSpacing: '-0.3px',
                  mb: 0.5,
                }}
              >
                {card.title}
              </Typography>
              <Typography
                sx={{ fontSize: 10, color: '#41484e', lineHeight: '16px' }}
              >
                {card.description}
              </Typography>
            </Box>
          ))}
        </Box>
      </Box>
    </Box>
  );
}
