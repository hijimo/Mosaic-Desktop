import { useState, useRef, useCallback, useEffect } from 'react';
import { Box, Typography, IconButton } from '@mui/material';
import {
  Activity,
  Bell,
  Code,
  BarChart3,
  Globe,
  PenLine,
  Paperclip,
  FileText,
  Mic,
  Image,
  SendHorizonal,
  Zap,
  FileSearch,
  Languages,
  ChevronDown,
  FolderOpen,
  Plus,
  Loader2,
} from 'lucide-react';
import { useParams } from 'react-router-dom';
import { useThreadStore } from '@/stores/threadStore';
import { useMessageStore } from '@/stores/messageStore';
import { useThread } from '@/hooks/useThread';
import { useSubmitOp } from '@/hooks/useSubmitOp';
import type { TurnItem, UserInput } from '@/types';

interface SkillCard {
  icon: React.ReactNode;
  title: string;
  description: string;
  prompt: string;
}

const SKILL_CARDS: SkillCard[] = [
  { icon: <Code size={16} />, title: 'Code Review', description: 'Deep analysis of logic and performance.', prompt: 'Review my code for bugs and performance issues.' },
  { icon: <BarChart3 size={18} />, title: 'Data Analyst', description: 'Visualize trends from CSV or SQL.', prompt: 'Analyze the data and show me key trends.' },
  { icon: <Globe size={20} />, title: 'Browse Agent', description: 'Research real-time web content.', prompt: 'Search the web for the latest information on this topic.' },
  { icon: <PenLine size={16} />, title: 'Drafting', description: 'Refined copy for emails and docs.', prompt: 'Help me draft a professional email.' },
];

const SUGGESTIONS = [
  { icon: <Zap size={16} />, label: 'Optimize', prompt: 'Optimize this code for better performance.' },
  { icon: <FileSearch size={15} />, label: 'Summarize', prompt: 'Summarize the key points.' },
  { icon: <Languages size={16} />, label: 'Translate', prompt: 'Translate this to English.' },
];

function renderTurnItem(item: TurnItem): React.ReactNode {
  switch (item.type) {
    case 'UserMessage':
      return item.content
        .filter((c): c is UserInput & { type: 'text' } => c.type === 'text')
        .map((c, i) => (
          <Box key={i} sx={{ alignSelf: 'flex-end', bgcolor: '#e8f0fe', borderRadius: 3, px: 2, py: 1.5, maxWidth: '80%' }}>
            <Typography sx={{ fontSize: 14, color: '#191c1e', whiteSpace: 'pre-wrap' }}>{c.text}</Typography>
          </Box>
        ));
    case 'AgentMessage':
      return (
        <Box sx={{ alignSelf: 'flex-start', bgcolor: '#fff', borderRadius: 3, px: 2, py: 1.5, maxWidth: '80%', border: '1px solid #f1f5f9' }}>
          <Typography sx={{ fontSize: 14, color: '#191c1e', whiteSpace: 'pre-wrap' }}>
            {item.content.map((c) => c.text).join('')}
          </Typography>
        </Box>
      );
    default:
      return null;
  }
}

export function IndexPage(): React.ReactElement {
  const { threadId: routeThreadId } = useParams<{ threadId?: string }>();
  const [inputText, setInputText] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const activeThreadId = useThreadStore((s) => s.activeThreadId);
  const messages = useMessageStore((s) => s.messagesByThread);
  const streamingTurn = useMessageStore((s) => s.streamingTurn);
  const { createThread } = useThread();
  const submitOp = useSubmitOp();

  const currentThreadId = routeThreadId ?? activeThreadId;
  const threadMessages = currentThreadId ? messages.get(currentThreadId) ?? [] : [];
  const hasMessages = threadMessages.length > 0 || streamingTurn?.isStreaming;

  const handleSend = useCallback(
    async (text?: string) => {
      const msg = (text ?? inputText).trim();
      if (!msg) return;

      let tid = currentThreadId;
      if (!tid) {
        tid = await createThread();
      }

      setInputText('');

      const userInput: UserInput = {
        type: 'text',
        text: msg,
        text_elements: [],
      };

      await submitOp(tid, {
        type: 'user_turn',
        items: [userInput],
        cwd: '.',
        model: '',
        approval_policy: 'on-request',
        sandbox_policy: { type: 'danger-full-access' },
      });
    },
    [inputText, currentThreadId, createThread, submitOp],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  // Auto-scroll on new messages
  const messagesEndRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [threadMessages.length, streamingTurn?.agentText]);

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
              <Activity size={16} color="#41484e" />
              <Typography sx={{ fontSize: 10, fontWeight: 500, color: '#41484e', textTransform: 'uppercase', letterSpacing: '1px' }}>
                Live Nodes
              </Typography>
            </Box>
            <Bell size={20} color="#41484e" />
          </Box>
        </Box>

        {/* Messages */}
        <Box sx={{ flex: 1, overflow: 'auto', px: 4, py: 3, display: 'flex', flexDirection: 'column', gap: 2 }}>
          {threadMessages.map((item, i) => (
            <Box key={i}>{renderTurnItem(item)}</Box>
          ))}
          {streamingTurn?.isStreaming && (
            <Box sx={{ alignSelf: 'flex-start', bgcolor: '#fff', borderRadius: 3, px: 2, py: 1.5, maxWidth: '80%', border: '1px solid #f1f5f9' }}>
              <Typography sx={{ fontSize: 14, color: '#191c1e', whiteSpace: 'pre-wrap' }}>
                {streamingTurn.agentText || <Loader2 size={16} className="animate-spin" />}
              </Typography>
            </Box>
          )}
          <div ref={messagesEndRef} />
        </Box>

        {/* Input */}
        <Box sx={{ px: 4, pb: 3, pt: 1, flexShrink: 0 }}>
          <Box
            sx={{
              bgcolor: '#fff',
              borderRadius: 4,
              p: 2,
              border: '1px solid #e2e8f0',
              display: 'flex',
              alignItems: 'flex-end',
              gap: 1.5,
            }}
          >
            <Box
              component="textarea"
              ref={textareaRef}
              value={inputText}
              onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => setInputText(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Type a message..."
              rows={1}
              sx={{
                flex: 1,
                border: 'none',
                outline: 'none',
                resize: 'none',
                fontSize: 14,
                fontFamily: 'Inter, sans-serif',
                color: '#191c1e',
                bgcolor: 'transparent',
                '&::placeholder': { color: 'rgba(65,72,78,0.4)' },
              }}
            />
            <Box
              onClick={() => handleSend()}
              sx={{
                width: 40,
                height: 40,
                borderRadius: 2.5,
                background: inputText.trim()
                  ? 'linear-gradient(135deg, #7cb9e8 0%, #8db2ff 100%)'
                  : '#e2e8f0',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                cursor: inputText.trim() ? 'pointer' : 'default',
                flexShrink: 0,
              }}
            >
              <SendHorizonal size={16} color="#fff" />
            </Box>
          </Box>
        </Box>
      </Box>
    );
  }

  // ── Welcome view (no messages) ──
  return (
    <Box sx={{ height: '100%', position: 'relative', overflow: 'hidden' }}>
      {/* Background blurs */}
      <Box sx={{ position: 'absolute', top: '25%', left: '25%', right: '37.5%', bottom: '37.5%', bgcolor: 'rgba(124,185,232,0.05)', filter: 'blur(50px)', borderRadius: 3 }} />
      <Box sx={{ position: 'absolute', top: '37.5%', left: '37.5%', right: '25%', bottom: '25%', bgcolor: 'rgba(212,230,229,0.2)', filter: 'blur(50px)', borderRadius: 3 }} />

      {/* Header */}
      <Box
        sx={{
          position: 'absolute', top: 0, left: 0, right: 0, height: 56,
          backdropFilter: 'blur(6px)', bgcolor: 'rgba(255,255,255,0.7)',
          borderBottom: '1px solid #f1f5f9',
          display: 'flex', alignItems: 'center', justifyContent: 'flex-end', px: 4, zIndex: 10,
        }}
      >
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 3 }}>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
            <Activity size={16} color="#41484e" />
            <Typography sx={{ fontSize: 10, fontWeight: 500, color: '#41484e', textTransform: 'uppercase', letterSpacing: '1px' }}>
              Live Nodes
            </Typography>
          </Box>
          <Bell size={20} color="#41484e" />
        </Box>
      </Box>

      {/* Welcome */}
      <Box sx={{ position: 'absolute', top: 84, left: 0, right: 0, display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 2, pb: 6 }}>
        <Typography
          sx={{ fontFamily: 'Manrope, sans-serif', fontWeight: 800, fontSize: 48, color: '#191c1e', textAlign: 'center', letterSpacing: '-1.2px', lineHeight: '48px' }}
        >
          How can I help you today?
        </Typography>
        <Typography sx={{ fontSize: 16, color: '#41484e', textAlign: 'center', lineHeight: '24px' }}>
          Select a project and launch your next automated<br />workspace mission.
        </Typography>
      </Box>

      {/* Interaction Container */}
      <Box sx={{ position: 'absolute', top: '50%', left: 128, right: 128, maxWidth: 768, mx: 'auto', transform: 'translateY(-50%)', display: 'flex', flexDirection: 'column', gap: 3 }}>
        {/* Project Selector */}
        <Box sx={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 1.5 }}>
          <Typography sx={{ fontSize: 11, fontWeight: 600, color: 'rgba(65,72,78,0.6)', textTransform: 'uppercase', letterSpacing: '2.2px' }}>
            Current Context
          </Typography>
          <Box sx={{ bgcolor: '#eceef0', borderRadius: 3, p: 0.5, display: 'flex', alignItems: 'center', gap: 1 }}>
            <Box
              sx={{
                bgcolor: '#fff', borderRadius: 3, px: 2.5, py: 1,
                display: 'flex', alignItems: 'center', gap: 1,
                boxShadow: '0px 1px 2px rgba(0,0,0,0.05)', border: '1px solid rgba(192,199,207,0.1)', cursor: 'pointer',
              }}
            >
              <FolderOpen size={14} color="#191c1e" />
              <Typography sx={{ fontSize: 14, fontWeight: 600, color: '#191c1e' }}>Q4 Marketing Strategy</Typography>
              <ChevronDown size={10} color="#191c1e" />
            </Box>
            <IconButton size="small" sx={{ p: 1 }}>
              <Plus size={14} color="#64748b" />
            </IconButton>
          </Box>
        </Box>

        {/* Input Area */}
        <Box
          sx={{
            bgcolor: '#fff', borderRadius: 6, p: 3,
            border: '1px solid #fff',
            boxShadow: '0px 25px 50px -12px rgba(124,185,232,0.1)',
          }}
        >
          {/* Textarea */}
          <Box sx={{ minHeight: 96, px: 1.5, py: 1 }}>
            <Box
              component="textarea"
              ref={textareaRef}
              value={inputText}
              onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => setInputText(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Ask anything or use @ and / for tools..."
              rows={3}
              sx={{
                width: '100%', border: 'none', outline: 'none', resize: 'none',
                fontSize: 18, fontFamily: 'Inter, sans-serif', color: '#191c1e', bgcolor: 'transparent',
                '&::placeholder': { color: 'rgba(65,72,78,0.4)' },
              }}
            />
          </Box>

          {/* Actions Bar */}
          <Box sx={{ borderTop: '1px solid rgba(192,199,207,0.1)', pt: 2, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
              <IconButton size="small" sx={{ p: 1 }}><Paperclip size={20} color="#64748b" /></IconButton>
              <IconButton size="small" sx={{ p: 1 }}><FileText size={18} color="#64748b" /></IconButton>
              <Box sx={{ width: 1, height: 24, bgcolor: 'rgba(192,199,207,0.2)', mx: 0.5 }} />
              <IconButton size="small" sx={{ p: 1 }}><Mic size={18} color="#64748b" /></IconButton>
              <IconButton size="small" sx={{ p: 1 }}><Image size={18} color="#64748b" /></IconButton>
            </Box>
            <Box
              onClick={() => handleSend()}
              sx={{
                width: 48, height: 48, borderRadius: 3,
                background: inputText.trim()
                  ? 'linear-gradient(135deg, #7cb9e8 0%, #8db2ff 100%)'
                  : 'linear-gradient(135deg, #7cb9e8 0%, #8db2ff 100%)',
                boxShadow: '0px 10px 15px -3px rgba(124,185,232,0.3)',
                display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer',
              }}
            >
              <SendHorizonal size={18} color="#fff" />
            </Box>
          </Box>
        </Box>

        {/* Suggestions */}
        <Box sx={{ display: 'flex', justifyContent: 'center', gap: 3 }}>
          {SUGGESTIONS.map((s) => (
            <Box
              key={s.label}
              onClick={() => handleSend(s.prompt)}
              sx={{
                display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 0.5,
                opacity: 0.4, cursor: 'pointer', '&:hover': { opacity: 0.7 },
              }}
            >
              {s.icon}
              <Typography sx={{ fontSize: 10, fontWeight: 600, textTransform: 'uppercase', letterSpacing: '0.5px', color: '#191c1e' }}>
                {s.label}
              </Typography>
            </Box>
          ))}
        </Box>
      </Box>

      {/* Skill Cards */}
      <Box sx={{ position: 'absolute', bottom: 48, left: 64, right: 64, maxWidth: 896, mx: 'auto' }}>
        <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 2, px: 2 }}>
          {SKILL_CARDS.map((card) => (
            <Box
              key={card.title}
              onClick={() => handleSend(card.prompt)}
              sx={{
                bgcolor: 'rgba(255,255,255,0.4)', borderRadius: 4, p: 2, height: 122,
                border: '1px solid rgba(0,0,0,0)', cursor: 'pointer',
                '&:hover': { bgcolor: 'rgba(255,255,255,0.7)' },
              }}
            >
              <Box sx={{ color: '#191c1e', mb: 2 }}>{card.icon}</Box>
              <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#191c1e', textTransform: 'uppercase', letterSpacing: '-0.3px', mb: 0.5 }}>
                {card.title}
              </Typography>
              <Typography sx={{ fontSize: 10, color: '#41484e', lineHeight: '16px' }}>
                {card.description}
              </Typography>
            </Box>
          ))}
        </Box>
      </Box>
    </Box>
  );
}
