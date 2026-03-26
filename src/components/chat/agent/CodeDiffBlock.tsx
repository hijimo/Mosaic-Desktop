import { Box, Typography } from '@mui/material';

interface DiffLine {
  type: 'add' | 'remove' | 'context';
  content: string;
}

interface CodeDiffBlockProps {
  filename: string;
  patch?: string;
}

function parseDiff(patch: string): DiffLine[] {
  return patch.split('\n').map((line) => {
    if (line.startsWith('+') && !line.startsWith('+++')) return { type: 'add', content: line };
    if (line.startsWith('-') && !line.startsWith('---')) return { type: 'remove', content: line };
    return { type: 'context', content: line };
  });
}

export function CodeDiffBlock({ filename, patch }: CodeDiffBlockProps): React.ReactElement {
  const lines = patch ? parseDiff(patch) : [];

  return (
    <Box sx={{ bgcolor: '#f2f4f6', border: '1px solid rgba(192,199,207,0.2)', borderRadius: 2, overflow: 'hidden' }}>
      {/* Header */}
      <Box sx={{
        bgcolor: '#eceef0', borderBottom: '1px solid rgba(192,199,207,0.2)',
        display: 'flex', alignItems: 'center', justifyContent: 'space-between', px: 2, py: 1,
      }}>
        <Typography sx={{ fontSize: 10, fontWeight: 600, color: '#64748b', textTransform: 'uppercase', letterSpacing: '1px' }}>
          {filename} — 差异
        </Typography>
        <Box sx={{ bgcolor: '#d1fae5', borderRadius: '2px', px: 1, py: '2px' }}>
          <Typography sx={{ fontSize: 9, fontWeight: 600, color: '#047857', textTransform: 'uppercase' }}>
            更新
          </Typography>
        </Box>
      </Box>

      {/* Diff content */}
      <Box sx={{ p: 2, fontFamily: '"Liberation Mono", monospace', fontSize: 12, display: 'flex', flexDirection: 'column', gap: '2px' }}>
        {lines.map((line, i) => {
          const bgMap = { add: '#ecfdf5', remove: '#fef2f2', context: 'transparent' };
          const colorMap = { add: '#047857', remove: '#b91c1c', context: '#94a3b8' };
          return (
            <Box key={i} sx={{ bgcolor: bgMap[line.type], px: 0.5, opacity: line.type === 'context' ? 1 : 0.5 }}>
              <Typography sx={{ fontFamily: 'inherit', fontSize: 'inherit', color: colorMap[line.type], lineHeight: '16px', whiteSpace: 'pre-wrap' }}>
                {line.content}
              </Typography>
            </Box>
          );
        })}
      </Box>
    </Box>
  );
}
