import { useState, useCallback, useEffect } from 'react';
import { Box, IconButton } from '@mui/material';
import { Paperclip, Mic, Image, SendHorizonal, Plus } from 'lucide-react';
import { v4 as uuidv4 } from 'uuid';
import { pickFiles } from '@/services/api';
import { useFileUploadStore, type AttachedFile } from '@/stores/fileUploadStore';
import { FileChip } from './FileChip';

interface InputAreaProps {
  value: string;
  onChange: (value: string) => void;
  onSend: (text: string, files: AttachedFile[]) => void;
  /** 是否为欢迎页大输入框样式 */
  variant?: 'default' | 'welcome';
}

const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp'];

function extOf(path: string): string {
  const dot = path.lastIndexOf('.');
  return dot === -1 ? '' : path.slice(dot + 1).toLowerCase();
}

function nameOf(path: string): string {
  const sep = path.lastIndexOf('/');
  const sep2 = path.lastIndexOf('\\');
  return path.slice(Math.max(sep, sep2) + 1);
}

function toAttachedFiles(paths: string[]): AttachedFile[] {
  return paths.map((p) => ({
    id: uuidv4(),
    name: nameOf(p),
    path: p,
    ext: extOf(p),
  }));
}

export function InputArea({ value, onChange, onSend, variant = 'default' }: InputAreaProps): React.ReactElement {
  const [files, setFiles] = useState<AttachedFile[]>([]);
  const pending = useFileUploadStore((s) => s.pending);
  const consumePending = useFileUploadStore((s) => s.consumePending);

  const isWelcome = variant === 'welcome';

  // 消费外部工具/skill 注入的 pending 文件
  useEffect(() => {
    if (pending.length > 0) {
      setFiles((prev) => [...prev, ...consumePending()]);
    }
  }, [pending, consumePending]);

  const addFiles = useCallback((newFiles: AttachedFile[]) => {
    setFiles((prev) => [...prev, ...newFiles]);
  }, []);

  const removeFile = useCallback((id: string) => {
    setFiles((prev) => prev.filter((f) => f.id !== id));
  }, []);

  const handlePickFiles = useCallback(async () => {
    const paths = await pickFiles().catch(() => [] as string[]);
    if (paths.length > 0) addFiles(toAttachedFiles(paths));
  }, [addFiles]);

  const handlePickImages = useCallback(async () => {
    const paths = await pickFiles(IMAGE_EXTS).catch(() => [] as string[]);
    if (paths.length > 0) addFiles(toAttachedFiles(paths));
  }, [addFiles]);

  const handleSend = useCallback(() => {
    if (!value.trim() && files.length === 0) return;
    onSend(value.trim(), files);
    setFiles([]);
  }, [value, files, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  return (
    <Box sx={{
      bgcolor: '#fff',
      borderRadius: isWelcome ? 6 : 4,
      p: isWelcome ? 3 : 2,
      border: isWelcome ? '1px solid #fff' : '1px solid #e2e8f0',
      boxShadow: isWelcome ? '0px 25px 50px -12px rgba(124,185,232,0.1)' : undefined,
      display: 'flex',
      flexDirection: 'column',
    }}>
      {/* File Preview Area */}
      {files.length > 0 && (
        <Box sx={{
          display: 'flex',
          flexWrap: 'wrap',
          gap: 1,
          mb: 1.5,
          px: isWelcome ? 1.5 : 0,
        }}>
          {files.map((f) => (
            <FileChip key={f.id} file={f} onRemove={removeFile} />
          ))}
          <IconButton
            size="small"
            onClick={handlePickFiles}
            sx={{
              width: 32,
              height: 32,
              border: '1px dashed rgba(192,199,207,0.3)',
              borderRadius: 2,
            }}
          >
            <Plus size={10} color="#64748b" />
          </IconButton>
        </Box>
      )}

      {/* Textarea */}
      <Box sx={{ minHeight: isWelcome ? 96 : undefined, px: isWelcome ? 1.5 : 0, py: isWelcome ? 1 : 0 }}>
        <Box
          component="textarea"
          value={value}
          onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => onChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={isWelcome ? 'Ask anything or use @ and / for tools...' : '输入消息...'}
          rows={isWelcome ? 3 : 1}
          sx={{
            width: '100%', border: 'none', outline: 'none', resize: 'none',
            fontSize: isWelcome ? 18 : 14, fontFamily: 'Inter, sans-serif',
            color: '#191c1e', bgcolor: 'transparent',
            '&::placeholder': { color: 'rgba(65,72,78,0.4)' },
          }}
        />
      </Box>

      {/* Actions Bar */}
      <Box sx={{
        borderTop: isWelcome ? '1px solid rgba(192,199,207,0.1)' : undefined,
        pt: isWelcome ? 2 : 0,
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        ...(isWelcome ? {} : { mt: 0.5 }),
      }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
          <IconButton size="small" sx={{ p: 1 }} onClick={handlePickFiles}>
            <Paperclip size={isWelcome ? 20 : 16} color="#64748b" />
          </IconButton>
          <Box sx={{ width: 1, height: 24, bgcolor: 'rgba(192,199,207,0.2)', mx: 0.5 }} />
          <IconButton size="small" sx={{ p: 1 }}><Mic size={isWelcome ? 18 : 14} color="#64748b" /></IconButton>
          <IconButton size="small" sx={{ p: 1 }} onClick={handlePickImages}>
            <Image size={isWelcome ? 18 : 14} color="#64748b" />
          </IconButton>
        </Box>
        <Box
          onClick={handleSend}
          sx={{
            width: isWelcome ? 48 : 40, height: isWelcome ? 48 : 40,
            borderRadius: isWelcome ? 3 : 2.5,
            background: (value.trim() || files.length > 0)
              ? 'linear-gradient(135deg, #7cb9e8 0%, #8db2ff 100%)'
              : isWelcome ? 'linear-gradient(135deg, #7cb9e8 0%, #8db2ff 100%)' : '#e2e8f0',
            boxShadow: isWelcome ? '0px 10px 15px -3px rgba(124,185,232,0.3)' : undefined,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            cursor: (value.trim() || files.length > 0) ? 'pointer' : 'default', flexShrink: 0,
          }}
        >
          <SendHorizonal size={isWelcome ? 18 : 16} color="#fff" />
        </Box>
      </Box>
    </Box>
  );
}
