import { Box, Typography } from '@mui/material';
import { useCallback } from 'react';
import type {
  TurnItem,
  TurnGroup,
  ToolCallState,
  ApprovalRequestState,
  ClarificationState,
  ReviewDecision,
} from '@/types';
import { useMessageStore } from '@/stores/messageStore';
import { dismissTurnError as dismissTurnErrorApi } from '@/services/api';
import { AgentAvatar } from './shared/AgentAvatar';
import { UserAvatar } from './shared/UserAvatar';
import { StreamdownRenderer } from './shared/StreamdownRenderer';
import { FileChip } from './FileChip';
import { ThinkingPanel } from './agent/ThinkingPanel';
import { WebSearchCard } from './agent/WebSearchCard';
import { McpToolCallCard } from './agent/McpToolCallCard';
import { CodeExecutionBlock } from './agent/CodeExecutionBlock';
import { ApprovalRequestCard } from './agent/ApprovalRequestCard';
import { ElicitationRequest } from './ElicitationRequest';
import { useElicitationStore } from '@/stores/elicitationStore';
import { CodeDiffBlock } from './agent/CodeDiffBlock';
import { ClarificationCard } from './agent/ClarificationCard';
import { MessageActionBar } from './agent/MessageActionBar';
import { ErrorCard } from './ErrorCard';

interface MessageProps {
  group: TurnGroup;
  threadId?: string;
  toolCalls?: ToolCallState[];
  approvalRequests?: ApprovalRequestState[];
  clarifications?: ClarificationState[];
  onApprovalDecision?: (callId: string, decision: ReviewDecision) => void;
  onElicitationDecision?: (requestId: string, serverName: string, decision: 'accept' | 'decline' | 'cancel', content?: Record<string, unknown>) => void;
  isStreaming?: boolean;
}

export function Message({
  group,
  threadId,
  toolCalls,
  approvalRequests,
  clarifications,
  onApprovalDecision,
  onElicitationDecision,
  isStreaming,
}: MessageProps): React.ReactElement | null {
  const { items } = group;
  const dismissTurnError = useMessageStore((s) => s.dismissTurnError);
  const elicitations = useElicitationStore((s) => s.requests);

  const handleDismiss = useCallback(async () => {
    if (!threadId) return;
    await dismissTurnErrorApi(threadId, group.turn_id);
    dismissTurnError(threadId, group.turn_id);
  }, [threadId, group.turn_id, dismissTurnError]);
  const hasExternalAgentContent = Boolean(
    toolCalls?.length || approvalRequests?.length || clarifications?.length || isStreaming,
  );
  if (items.length === 0 && !hasExternalAgentContent && !group.error && group.status !== 'Dismissed') return null;

  // Separate user messages and agent-side items
  const userItems = items.filter(
    (i): i is TurnItem & { type: 'UserMessage' } => i.type === 'UserMessage',
  );
  const agentItems = items.filter((i) => i.type !== 'UserMessage');
  const firstAgentMessage = items.find(
    (item): item is Extract<TurnItem, { type: 'AgentMessage' }> => item.type === 'AgentMessage',
  );
  const shouldRenderStreamingPlaceholder =
    Boolean(isStreaming) && agentItems.length === 0;
  const hasAgentTurnContent =
    agentItems.length > 0 ||
    Boolean(toolCalls?.length) ||
    Boolean(approvalRequests?.length) ||
    Boolean(clarifications?.length) ||
    shouldRenderStreamingPlaceholder ||
    Boolean(group.error) ||
    group.status === 'Dismissed';

  const renderAgentItem = (item: Exclude<TurnItem, { type: 'UserMessage' }>): React.ReactNode => {
    switch (item.type) {
      case 'AgentMessage': {
        const text = item.content.map((c) => c.text).join('');
        return text ? (
          <Box
            key={item.id}
            data-testid='agent-message-segment'
            sx={{
              fontSize: 16,
              color: '#41484e',
              lineHeight: '26px',
            }}
          >
            <StreamdownRenderer isStreaming={isStreaming}>
              {text}
            </StreamdownRenderer>
          </Box>
        ) : null;
      }
      case 'Reasoning':
        return (
          <ThinkingPanel
            key={item.id}
            text={item.summary_text.join('\n')}
          />
        );
      case 'Plan':
        return (
          <Box
            key={item.id}
            sx={{
              fontSize: 14,
              color: '#334155',
              lineHeight: '22.75px',
            }}
          >
            <StreamdownRenderer>{item.text}</StreamdownRenderer>
          </Box>
        );
      case 'CommandExecution':
        return (
          <CodeExecutionBlock
            key={item.id}
            toolCall={{
              callId: item.id,
              type: 'exec',
              status: item.status === 'Completed' ? 'completed' : item.status === 'Failed' || item.status === 'Declined' ? 'failed' : 'running',
              name: item.command.split(' ')[0] ?? 'command',
              command: item.command.split(' '),
              cwd: item.cwd,
              output: item.aggregated_output,
              exitCode: item.exit_code ?? undefined,
            }}
          />
        );
      case 'McpToolCall':
        return (
          <McpToolCallCard
            key={item.id}
            toolCall={{
              callId: item.id,
              type: 'mcp',
              status: item.status === 'Completed' ? 'completed' : item.status === 'Failed' ? 'failed' : 'running',
              name: item.tool,
              serverName: item.server,
              toolName: item.tool,
              arguments: item.arguments as Record<string, unknown> | undefined,
              result: item.error ? { error: item.error.message } : undefined,
            }}
          />
        );
      case 'DynamicToolCall':
        return (
          <McpToolCallCard
            key={item.id}
            toolCall={{
              callId: item.id,
              type: 'mcp',
              status: item.status === 'Completed' ? 'completed' : item.status === 'Failed' ? 'failed' : 'running',
              name: item.tool,
              serverName: 'dynamic',
              toolName: item.tool,
              arguments: item.arguments as Record<string, unknown> | undefined,
            }}
          />
        );
      case 'FileChange':
        return item.changes.map((change) => (
          <CodeDiffBlock
            key={`${item.id}-${change.path}`}
            filename={change.path}
            patch={change.diff}
          />
        ));
      case 'WebSearch':
        return (
          <WebSearchCard
            key={item.id}
            toolCall={{
              callId: item.id,
              type: 'web_search',
              status: 'completed',
              name: item.query || 'Web Search',
            }}
          />
        );
      case 'ContextCompaction':
        return null;
      case 'ImageView':
        return (
          <Box key={item.id} sx={{ fontSize: 14, color: '#64748b' }}>
            📷 {item.path}
          </Box>
        );
      case 'EnteredReviewMode':
      case 'ExitedReviewMode':
        return (
          <Box key={item.id} sx={{ fontSize: 14, color: '#64748b', fontStyle: 'italic' }}>
            {item.review}
          </Box>
        );
      case 'CollabToolCall':
        return (
          <Box key={item.id} sx={{ fontSize: 14, color: '#64748b' }}>
            🤝 {item.tool} → {item.receiver_thread_ids.join(', ')} [{item.status}]
          </Box>
        );
      case 'Elicitation':
        return (
          <ElicitationRequest
            key={item.id}
            serverName={item.server_name}
            requestId={item.id}
            message={item.message}
            mode={item.mode as 'form' | 'url' | undefined}
            schema={item.schema}
            url={item.url}
            responseAction={item.response_action}
            responseContent={item.response_content}
          />
        );
      default:
        return null;
    }
  };

  return (
    <>
      {/* Render user messages */}
      {userItems.map((item) => (
        <Box
          key={item.id}
          sx={{ display: 'flex', gap: 2, justifyContent: 'flex-end' }}
        >
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
            {item.content.map((c, i) => {
              if (c.type === 'text') {
                return (
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
                );
              }
              return null;
            })}
            {/* Attached files container */}
            {item.content.some((c) => c.type === 'attached_file' || c.type === 'local_image') && (
              <Box sx={{ display: 'flex', flexWrap: 'wrap', gap: 1, mt: 1 }}>
                {item.content.map((c, i) => {
                  if (c.type === 'attached_file' || c.type === 'local_image') {
                    const name = c.type === 'attached_file' ? c.name : (c.path.split(/[\\/]/).pop() ?? c.path);
                    const ext = name.includes('.') ? name.split('.').pop()!.toLowerCase() : '';
                    return <FileChip key={i} file={{ id: `${item.id}-${i}`, name, ext }} />;
                  }
                  return null;
                })}
              </Box>
            )}
          </Box>
          <UserAvatar />
        </Box>
      ))}

      {/* Render agent-side items as a single block */}
      {hasAgentTurnContent && (
        <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
          <AgentAvatar />
          <Box
            data-testid='agent-turn-content'
            sx={{
              display: 'flex',
              flexDirection: 'column',
              gap: 2,
              flex: 1,
              bgcolor: '#fff',
              border: '1px solid rgba(192,199,207,0.05)',
              borderRadius: '0 24px 24px 24px',
              boxShadow: '0px 8px 30px rgba(0,0,0,0.04)',
              p: '16px',
            }}
          >
            {/* Tool calls */}
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

            {/* Elicitation requests */}
            {isStreaming && Array.from(elicitations.values()).map((er) => (
              <ElicitationRequest
                key={er.requestId}
                serverName={er.serverName}
                requestId={er.requestId}
                message={er.message}
                mode={er.mode}
                schema={er.schema}
                url={er.url}
                onDecision={onElicitationDecision}
              />
            ))}

            {/* Clarification requests */}
            {clarifications?.map((cr) => (
              <ClarificationCard key={cr.id} request={cr} />
            ))}

            {/* All agent-side items in order */}
            {agentItems.map((item) => renderAgentItem(item))}

            {shouldRenderStreamingPlaceholder ? (
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <Typography sx={{ fontSize: 14, color: '#94a3b8' }}>
                  思考中...
                </Typography>
              </Box>
            ) : null}

            {firstAgentMessage ? (
              <MessageActionBar
                group={group}
                messageId={firstAgentMessage.id}
              />
            ) : null}

            {group.error && group.status === 'Failed' && (
              <ErrorCard
                message={group.error.message}
                onDismiss={handleDismiss}
              />
            )}
            {group.status === 'Dismissed' && (
              <Typography sx={{ fontSize: 12, color: '#991b1b', fontStyle: 'italic' }}>
                该消息已被忽略
              </Typography>
            )}
          </Box>
        </Box>
      )}
    </>
  );
}
