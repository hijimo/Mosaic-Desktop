import { Box } from '@mui/material';
import { useToolCallStore } from '@/stores/toolCallStore';
import { WebSearchCard } from '../agent/WebSearchCard';
import { McpToolCallCard } from '../agent/McpToolCallCard';
import { CodeExecutionBlock } from '../agent/CodeExecutionBlock';

export function StreamingToolRegion(): React.ReactElement | null {
  const activeToolCalls = useToolCallStore((s) => s.toolCalls);

  if (activeToolCalls.size === 0) return null;

  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        gap: 2,
        flex: 1,
      }}
    >
      {Array.from(activeToolCalls.values()).map((tc) => {
        switch (tc.type) {
          case 'web_search':
            return <WebSearchCard key={tc.callId} toolCall={tc} />;
          case 'mcp':
            return <McpToolCallCard key={tc.callId} toolCall={tc} />;
          case 'exec':
            return <CodeExecutionBlock key={tc.callId} toolCall={tc} />;
          default:
            return null;
        }
      })}
    </Box>
  );
}
