import { useRef, useLayoutEffect, useCallback } from 'react';
import { Box, Typography } from '@mui/material';
import { Loader2 } from 'lucide-react';
import { useMessageStore } from '@/stores/messageStore';
import { useToolCallStore } from '@/stores/toolCallStore';
import { useApprovalStore } from '@/stores/approvalStore';
import { useClarificationStore } from '@/stores/clarificationStore';
import { Message } from './Message';
import { TaskStartedIndicator } from './indicators/TaskStartedIndicator';
import { TaskCompletedIndicator } from './indicators/TaskCompletedIndicator';
import { ThinkingPanel } from './agent/ThinkingPanel';
import { WebSearchCard } from './agent/WebSearchCard';
import { McpToolCallCard } from './agent/McpToolCallCard';
import { CodeExecutionBlock } from './agent/CodeExecutionBlock';
import { ApprovalRequestCard } from './agent/ApprovalRequestCard';
import { ClarificationCard } from './agent/ClarificationCard';
import { AgentAvatar } from './shared/AgentAvatar';
import { StreamdownRenderer } from './shared/StreamdownRenderer';

interface MessageListProps {
  threadId: string;
  onApprovalDecision?: (callId: string, decision: 'approve' | 'deny') => void;
}

const EMPTY_MESSAGES: never[] = [];

export function MessageList({ threadId, onApprovalDecision }: MessageListProps): React.ReactElement {
  const messages = useMessageStore((s) => s.messagesByThread.get(threadId) ?? EMPTY_MESSAGES);
  const streamingTurn = useMessageStore((s) => s.streamingTurn);
  const activeToolCalls = useToolCallStore((s) => s.toolCalls);
  const approvals = useApprovalStore((s) => s.approvals);
  const clarifications = useClarificationStore((s) => s.requests);

  const containerRef = useRef<HTMLDivElement>(null);
  const isAtBottomRef = useRef(true);

  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    isAtBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
  }, []);

  const msgLen = messages.length;
  const streamText = streamingTurn?.agentText ?? '';
  const streamItemCount = streamingTurn?.items.size ?? 0;
  const toolCallCount = activeToolCalls.size;
  const approvalCount = approvals.size;
  const clarificationCount = clarifications.size;

  useLayoutEffect(() => {
    if (isAtBottomRef.current) {
      const el = containerRef.current;
      if (el) el.scrollTop = el.scrollHeight;
    }
  }, [msgLen, streamText, streamItemCount, toolCallCount, approvalCount, clarificationCount]);

  const isStreaming = streamingTurn?.isStreaming ?? false;
  const isComplete = !isStreaming && msgLen > 0;

  return (
    <Box
      ref={containerRef}
      onScroll={handleScroll}
      sx={{ flex: 1, overflow: 'auto', px: 8, pt: 3, pb: 24, display: 'flex', flexDirection: 'column', gap: 4 }}
    >
      {/* Task Started indicator */}
      {(isStreaming || msgLen > 0) && <TaskStartedIndicator />}

      {/* Completed messages */}
      {messages.map((item, i) => (
        <Message
          key={item.id ?? i}
          item={item}
          onApprovalDecision={onApprovalDecision}
        />
      ))}

      {/* Streaming content */}
      {isStreaming && (
        <>
          {/* Streaming reasoning items */}
          {Array.from(streamingTurn!.items.values())
            .filter((si) => si.itemType === 'Reasoning' && si.reasoningSummary.some(Boolean))
            .map((si) => (
              <Box key={si.itemId} sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
                <AgentAvatar />
                <Box sx={{ flex: 1 }}>
                  <ThinkingPanel text={si.reasoningSummary.filter(Boolean).join('\n')} isStreaming />
                </Box>
              </Box>
            ))}

          {/* Streaming plan items */}
          {Array.from(streamingTurn!.items.values())
            .filter((si) => si.itemType === 'Plan' && si.planText)
            .map((si) => (
              <Box key={si.itemId} sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
                <AgentAvatar />
                <Box sx={{ flex: 1, fontSize: 14, color: '#334155' }}>
                  <StreamdownRenderer isStreaming>{si.planText}</StreamdownRenderer>
                </Box>
              </Box>
            ))}

          {/* Active tool calls */}
          {activeToolCalls.size > 0 && (
            <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
              <AgentAvatar />
              <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1 }}>
                {Array.from(activeToolCalls.values()).map((tc) => {
                  switch (tc.type) {
                    case 'web_search': return <WebSearchCard key={tc.callId} toolCall={tc} />;
                    case 'mcp': return <McpToolCallCard key={tc.callId} toolCall={tc} />;
                    case 'exec': return <CodeExecutionBlock key={tc.callId} toolCall={tc} />;
                    default: return null;
                  }
                })}
              </Box>
            </Box>
          )}

          {/* Active approval requests */}
          {approvals.size > 0 && (
            <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
              <AgentAvatar />
              <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1 }}>
                {Array.from(approvals.values()).map((ar) => (
                  <ApprovalRequestCard key={ar.callId} request={ar} onDecision={onApprovalDecision} />
                ))}
              </Box>
            </Box>
          )}

          {/* Active clarification requests */}
          {clarifications.size > 0 && (
            <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
              <AgentAvatar />
              <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1 }}>
                {Array.from(clarifications.values()).map((cr) => (
                  <ClarificationCard key={cr.id} request={cr} />
                ))}
              </Box>
            </Box>
          )}

          {/* Streaming agent message */}
          <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
            <AgentAvatar />
            <Box sx={{
              flex: 1, bgcolor: '#fff',
              border: '1px solid rgba(192,199,207,0.05)',
              borderRadius: '0 24px 24px 24px',
              boxShadow: '0px 8px 30px rgba(0,0,0,0.04)',
              p: '33px',
            }}>
              {streamingTurn!.agentText ? (
                <Box sx={{ fontSize: 16, color: '#41484e', lineHeight: '26px' }}>
                  <StreamdownRenderer isStreaming>{streamingTurn!.agentText}</StreamdownRenderer>
                </Box>
              ) : (
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                  <Loader2 size={16} color="#005bc1" className="animate-spin" />
                  <Typography sx={{ fontSize: 14, color: '#94a3b8' }}>思考中...</Typography>
                </Box>
              )}
            </Box>
          </Box>
        </>
      )}

      {/* Task Completed indicator */}
      {isComplete && <TaskCompletedIndicator />}
    </Box>
  );
}
