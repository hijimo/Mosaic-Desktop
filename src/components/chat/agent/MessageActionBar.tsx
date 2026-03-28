import { useMemo, useState } from 'react';
import { Alert, Box, IconButton, Menu, MenuItem, Snackbar, Tooltip } from '@mui/material';
import { Copy, ChevronDown, Share2, ThumbsDown, ThumbsUp } from 'lucide-react';
import { share } from '@vnidrop/tauri-plugin-share';
import { shareMessage } from '@/services/api';
import { writeClipboardText } from '@/services/clipboard';
import type { TurnGroup } from '@/types';
import { useMessageActionStore } from '@/stores/messageActionStore';
import { buildCopyMarkdown, buildCopyText, buildSharePayload } from './messageShareContent';

interface MessageActionBarProps {
  group: TurnGroup;
  messageId: string;
}

interface NoticeState {
  open: boolean;
  message: string;
  severity: 'success' | 'error' | 'info' | 'warning';
}

function extractErrorMessage(error: unknown): string {
  const normalize = (message: string) => message.replace(/^share message failed:\s*/i, '').trim();

  if (error instanceof Error && error.message) {
    return normalize(error.message);
  }
  if (typeof error === 'string' && error.trim().length > 0) {
    return normalize(error);
  }
  return '未知错误';
}

export function MessageActionBar({
  group,
  messageId,
}: MessageActionBarProps): React.ReactElement {
  const [copyMenuAnchor, setCopyMenuAnchor] = useState<HTMLElement | null>(null);
  const [notice, setNotice] = useState<NoticeState>({
    open: false,
    message: '',
    severity: 'info',
  });

  const reaction = useMessageActionStore((state) => state.reactions[messageId] ?? 'none');
  const shareState = useMessageActionStore((state) => state.shareStates[messageId] ?? 'idle');
  const toggleReaction = useMessageActionStore((state) => state.toggleReaction);
  const setShareState = useMessageActionStore((state) => state.setShareState);

  const copyText = useMemo(() => buildCopyText(group), [group]);
  const copyMarkdown = useMemo(() => buildCopyMarkdown(group), [group]);

  const showNotice = (
    message: string,
    severity: NoticeState['severity'],
  ) => {
    setNotice({
      open: true,
      message,
      severity,
    });
  };

  const copyContent = async (content: string, successMessage: string) => {
    await writeClipboardText(content);
    showNotice(successMessage, 'success');
    setCopyMenuAnchor(null);
  };

  const handleShare = async () => {
    setShareState(messageId, 'preparing');

    try {
      const result = await shareMessage(buildSharePayload(group));
      setShareState(messageId, 'sharing');

      try {
        await share({ url: result.url });
        setShareState(messageId, 'success');
        showNotice('已调起系统分享', 'success');
      } catch (error) {
        console.warn('系统分享调起失败，回退为复制链接。', error);
        await writeClipboardText(result.url);
        setShareState(messageId, 'success');
        showNotice('已复制分享链接', 'success');
      }
    } catch (error) {
      const errorMessage = extractErrorMessage(error);
      console.error('消息分享失败。', errorMessage, error);
      setShareState(messageId, 'failed');
      showNotice(`分享失败：${errorMessage}`, 'error');
    }
  };

  const shareLabel = shareState === 'preparing'
    ? '准备分享中'
    : shareState === 'sharing'
      ? '正在调起分享'
      : '分享';

  return (
    <Box
      sx={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 1.5,
        borderTop: '1px solid rgba(148, 163, 184, 0.18)',
        pt: 1.5,
      }}
    >
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
        <Tooltip title='复制'>
          <IconButton
            aria-label='复制'
            size='small'
            onClick={() => {
              void copyContent(copyText, '已复制');
            }}
          >
            <Copy size={16} />
          </IconButton>
        </Tooltip>

        <Tooltip title='更多复制选项'>
          <IconButton
            aria-label='更多复制选项'
            size='small'
            onClick={(event) => setCopyMenuAnchor(event.currentTarget)}
          >
            <ChevronDown size={16} />
          </IconButton>
        </Tooltip>

        <Menu
          anchorEl={copyMenuAnchor}
          open={Boolean(copyMenuAnchor)}
          onClose={() => setCopyMenuAnchor(null)}
        >
          <MenuItem
            onClick={() => {
              void copyContent(copyText, '已复制');
            }}
          >
            复制
          </MenuItem>
          <MenuItem
            onClick={() => {
              void copyContent(copyMarkdown, '已复制 Markdown');
            }}
          >
            复制为 Markdown
          </MenuItem>
        </Menu>

        <Tooltip title='点赞'>
          <IconButton
            aria-label='点赞'
            size='small'
            color={reaction === 'up' ? 'primary' : 'default'}
            onClick={() => toggleReaction(messageId, 'up')}
          >
            <ThumbsUp size={16} />
          </IconButton>
        </Tooltip>

        <Tooltip title='点踩'>
          <IconButton
            aria-label='点踩'
            size='small'
            color={reaction === 'down' ? 'primary' : 'default'}
            onClick={() => toggleReaction(messageId, 'down')}
          >
            <ThumbsDown size={16} />
          </IconButton>
        </Tooltip>

        <Tooltip title={shareLabel}>
          <span>
            <IconButton
              aria-label='分享'
              size='small'
              color={shareState === 'success' ? 'primary' : 'default'}
              disabled={shareState === 'preparing' || shareState === 'sharing'}
              onClick={() => {
                void handleShare();
              }}
            >
              <Share2 size={16} />
            </IconButton>
          </span>
        </Tooltip>
      </Box>

      <Snackbar
        open={notice.open}
        autoHideDuration={3000}
        anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
        sx={{ top: '150px !important' }}
        onClose={(_, reason) => {
          if (reason === 'clickaway') {
            return;
          }
          setNotice((current) => ({ ...current, open: false }));
        }}
      >
        <Alert
          onClose={() => setNotice((current) => ({ ...current, open: false }))}
          severity={notice.severity}
          variant='filled'
          sx={{ width: '100%' }}
        >
          {notice.message}
        </Alert>
      </Snackbar>
    </Box>
  );
}
