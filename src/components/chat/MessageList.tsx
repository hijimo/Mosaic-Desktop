import { useRef, useLayoutEffect, useCallback } from 'react';
import { Box, Typography } from '@mui/material';
import { Loader2 } from 'lucide-react';
import { useMessageStore } from '@/stores/messageStore';
import { Message } from './Message';
import type { ToolCallState, ApprovalRequestState } from '@/types';

interface MessageListProps {
  threadId: string;
  toolCalls?: Map<string, ToolCallState[]>;
  approvalRequests?: Map<string, ApprovalRequestState[]>;
  onApprovalDecision?: (callId: string, decision: 'approve' | 'deny') => void;
}

const EMPTY_MESSAGES: never[] = [];

export function MessageList({ threadId, toolCalls, approvalRequests, onApprovalDecision }: MessageListProps): React.ReactElement {
  const messages = useMessageStore((s) => s.messagesByThread.get(threadId) ?? EMPTY_MESSAGES);
  const streamingTurn = useMessageStore((s) => s.streamingTurn);

  const containerRef = useRef<HTMLDivElement>(null);
  const isAtBottomRef = useRef(true);

  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    const threshold = 50;
    isAtBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < threshold;
  }, []);

  // 使用 useLayoutEffect 在 DOM 更新后同步滚动，避免闪烁
  // 依赖 messages.length 和 streamingTurn?.agentText 来触发
  const msgLen = messages.length;
  const streamText = streamingTurn?.agentText ?? '';
  useLayoutEffect(() => {
    if (isAtBottomRef.current) {
      const el = containerRef.current;
      if (el) el.scrollTop = el.scrollHeight;
    }
  }, [msgLen, streamText]);

  return (
    <Box
      ref={containerRef}
      onScroll={handleScroll}
      sx={{ flex: 1, overflow: 'auto', px: 4, py: 5, display: 'flex', flexDirection: 'column', gap: 5 }}
    >
      {messages.map((item, i) => (
        <Message
          key={item.id ?? i}
          item={item}
          toolCalls={toolCalls?.get(item.id)}
          approvalRequests={approvalRequests?.get(item.id)}
          onApprovalDecision={onApprovalDecision}
        />
      ))}

      {streamingTurn?.isStreaming && (
        <Box sx={{ display: 'flex', gap: 3, alignItems: 'flex-start' }}>
          <Box sx={{
            width: 40, height: 40, borderRadius: '8px', flexShrink: 0,
            background: 'linear-gradient(135deg, #005bc1 0%, #8db2ff 100%)',
            boxShadow: '0px 10px 15px -3px rgba(0,0,0,0.1)',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
          }}>
            <Typography sx={{ fontSize: 14, fontWeight: 700, color: '#fff', lineHeight: 1 }}>M</Typography>
          </Box>
          <Box sx={{
            bgcolor: '#fff', border: '1px solid rgba(192,199,207,0.05)',
            borderRadius: '0 24px 24px 24px', boxShadow: '0px 8px 30px rgba(0,0,0,0.04)',
            p: '33px', maxWidth: 762, flex: 1,
          }}>
            {streamingTurn.agentText ? (
              <Typography sx={{ fontSize: 16, color: '#41484e', lineHeight: '26px', whiteSpace: 'pre-wrap' }}>
                {streamingTurn.agentText}
              </Typography>
            ) : (
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <Loader2 size={16} color="#005bc1" className="animate-spin" />
                <Typography sx={{ fontSize: 14, color: '#94a3b8' }}>Thinking...</Typography>
              </Box>
            )}
          </Box>
        </Box>
      )}
    </Box>
  );
}
