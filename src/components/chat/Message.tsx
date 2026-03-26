import { Box, Typography } from '@mui/material';
import type {
  TurnItem,
  UserInput,
  ToolCallState,
  ApprovalRequestState,
  ClarificationState,
} from '@/types';
import { AgentAvatar } from './shared/AgentAvatar';
import { UserAvatar } from './shared/UserAvatar';
import { StreamdownRenderer } from './shared/StreamdownRenderer';
import { ThinkingPanel } from './agent/ThinkingPanel';
import { WebSearchCard } from './agent/WebSearchCard';
import { McpToolCallCard } from './agent/McpToolCallCard';
import { CodeExecutionBlock } from './agent/CodeExecutionBlock';
import { ApprovalRequestCard } from './agent/ApprovalRequestCard';
import { CodeDiffBlock } from './agent/CodeDiffBlock';
import { ClarificationCard } from './agent/ClarificationCard';

interface MessageProps {
  item: TurnItem;
  toolCalls?: ToolCallState[];
  approvalRequests?: ApprovalRequestState[];
  clarifications?: ClarificationState[];
  onApprovalDecision?: (callId: string, decision: 'approve' | 'deny') => void;
  isStreaming?: boolean;
}

export function Message({
  item,
  toolCalls,
  approvalRequests,
  clarifications,
  onApprovalDecision,
  isStreaming,
}: MessageProps): React.ReactElement | null {
  switch (item.type) {
    case 'UserMessage':
      return (
        <Box sx={{ display: 'flex', gap: 2, justifyContent: 'flex-end' }}>
          <Box
            sx={{
              bgcolor: '#f0f7ff',
              border: '1px solid #d4e6ff',
              borderRadius: '16px 16px 16px 0',
              p: '17px',
              maxWidth: '80%',
              boxShadow: '0px 1px 2px rgba(0,0,0,0.05)',
            }}
          >
            {item.content
              .filter(
                (c): c is UserInput & { type: 'text' } => c.type === 'text',
              )
              .map((c, i) => (
                <Typography
                  key={i}
                  sx={{
                    fontSize: 14,
                    fontWeight: 500,
                    color: '#334155',
                    lineHeight: '22.75px',
                    whiteSpace: 'pre-wrap',
                  }}
                >
                  {c.text}
                </Typography>
              ))}
          </Box>
          <UserAvatar />
        </Box>
      );

    case 'AgentMessage': {
      const agentText = item.content.map((c) => c.text).join('');
      return (
        <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
          <AgentAvatar />
          <Box
            sx={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1 }}
          >
            {/* Render tool calls by type */}
            {toolCalls?.map((tc) => {
              switch (tc.type) {
                case 'web_search':
                  return <WebSearchCard key={tc.callId} toolCall={tc} />;
                case 'mcp':
                  return <McpToolCallCard key={tc.callId} toolCall={tc} />;
                case 'exec':
                  return <CodeExecutionBlock key={tc.callId} toolCall={tc} />;
                case 'patch': {
                  const changes = tc.result as
                    | Record<string, { patch?: string }>
                    | undefined;
                  const firstFile = changes
                    ? Object.keys(changes)[0]
                    : undefined;
                  const patch =
                    firstFile && changes
                      ? changes[firstFile]?.patch
                      : undefined;
                  return patch ? (
                    <CodeDiffBlock
                      key={tc.callId}
                      filename={firstFile!}
                      patch={patch}
                    />
                  ) : null;
                }
                default:
                  return null;
              }
            })}

            {/* Approval requests */}
            {approvalRequests?.map((ar) => (
              <ApprovalRequestCard
                key={ar.callId}
                request={ar}
                onDecision={onApprovalDecision}
              />
            ))}

            {/* Clarification requests */}
            {clarifications?.map((cr) => (
              <ClarificationCard key={cr.id} request={cr} />
            ))}

            {/* Agent message content with Streamdown */}
            {agentText && (
              <Box
                sx={{
                  bgcolor: '#fff',
                  border: '1px solid rgba(192,199,207,0.05)',
                  borderRadius: '0 24px 24px 24px',
                  boxShadow: '0px 8px 30px rgba(0,0,0,0.04)',
                  p: '16px',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 2,
                }}
              >
                <Box
                  sx={{ fontSize: 16, color: '#41484e', lineHeight: '26px' }}
                >
                  <StreamdownRenderer isStreaming={isStreaming}>
                    {agentText}
                  </StreamdownRenderer>
                </Box>
              </Box>
            )}
          </Box>
        </Box>
      );
    }

    case 'Reasoning':
      return (
        <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
          <AgentAvatar />
          <Box sx={{ flex: 1 }}>
            <ThinkingPanel text={item.summary_text.join('\n')} />
          </Box>
        </Box>
      );

    case 'Plan':
      return (
        <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
          <AgentAvatar />
          <Box
            sx={{
              flex: 1,
              fontSize: 14,
              color: '#334155',
              lineHeight: '22.75px',
            }}
          >
            <StreamdownRenderer>{item.text}</StreamdownRenderer>
          </Box>
        </Box>
      );

    default:
      return null;
  }
}
