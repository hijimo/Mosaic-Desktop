import { useState, useCallback, useEffect, useRef } from 'react';
import { Box, IconButton, Typography, Popover } from '@mui/material';
import { Zap, Paperclip, Mic, SendHorizonal, CircleStop, Plus } from 'lucide-react';
import { v4 as uuidv4 } from 'uuid';
import { pickFiles } from '@/services/api';
import { useFileUploadStore, type AttachedFile } from '@/stores/fileUploadStore';
import { useSkillStore } from '@/stores/skillStore';
import { useAgentRoleStore } from '@/stores/agentRoleStore';
import { FileChip } from './FileChip';
import { SkillSelector } from './SkillSelector';
import { ActiveAgentBar } from './ActiveAgentBar';

interface InputAreaProps {
  value: string;
  onChange: (value: string) => void;
  onSend: (text: string, files: AttachedFile[]) => void;
  variant?: 'default' | 'welcome';
  isStreaming?: boolean;
  onStop?: () => void;
}

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

export function InputArea({ value, onChange, onSend, variant = 'default', isStreaming, onStop }: InputAreaProps): React.ReactElement {
  const [files, setFiles] = useState<AttachedFile[]>([]);
  const pending = useFileUploadStore((s) => s.pending);
  const consumePending = useFileUploadStore((s) => s.consumePending);
  const selectedSkills = useSkillStore((s) => s.selectedSkills);
  const activeRole = useAgentRoleStore((s) => s.activeRole);

  const isWelcome = variant === 'welcome';

  const [skillAnchor, setSkillAnchor] = useState<HTMLElement | null>(null);
  const skillOpen = Boolean(skillAnchor);
  const zapRef = useRef<HTMLButtonElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

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

  const canSend = !!(value.trim() || files.length > 0);

  const handleSend = useCallback(() => {
    if (!value.trim() && files.length === 0) return;
    onSend(value.trim(), files);
    setFiles([]);
  }, [value, files, onSend]);

  const openSkillSelector = useCallback((anchor: HTMLElement) => {
    setSkillAnchor(anchor);
  }, []);

  const closeSkillSelector = useCallback(() => {
    setSkillAnchor(null);
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
        return;
      }
      if (e.key === '\x24' && !e.ctrlKey && !e.metaKey && !e.altKey) {
        if (textareaRef.current && !skillOpen) {
          e.preventDefault();
          openSkillSelector(textareaRef.current);
        }
      }
    },
    [handleSend, skillOpen, openSkillSelector],
  );

  const handleZapClick = useCallback(() => {
    if (zapRef.current) openSkillSelector(zapRef.current);
  }, [openSkillSelector]);

  const hasSelectedSkills = selectedSkills.length > 0;

  return (
    <Box sx={{
      bgcolor: '#fff',
      borderRadius: isWelcome ? 6 : 4,
      p: isWelcome ? 3 : 2,
      border: isWelcome ? '1px solid #fff' : '1px solid #e2e8f0',
      boxShadow: isWelcome ? '0px 25px 50px -12px rgba(124,185,232,0.1)' : undefined,
      display: 'flex',
      flexDirection: 'column',
      gap: isWelcome ? 2.5 : 1,
    }}>
      {(isWelcome || hasSelectedSkills || activeRole) && (
        <ActiveAgentBar />
      )}

      {files.length > 0 && (
        <Box sx={{ display: 'flex', flexWrap: 'wrap', gap: 1, px: isWelcome ? 1.5 : 0 }}>
          {files.map((f) => (
            <FileChip key={f.id} file={f} onRemove={removeFile} />
          ))}
          <IconButton size="small" onClick={handlePickFiles} sx={{ width: 32, height: 32, border: '1px dashed rgba(192,199,207,0.3)', borderRadius: 2 }}>
            <Plus size={10} color="#64748b" />
          </IconButton>
        </Box>
      )}

      <Box sx={{ minHeight: isWelcome ? 96 : undefined, px: isWelcome ? 1.5 : 0, py: isWelcome ? 1 : 0 }}>
        <Box
          component="textarea"
          ref={textareaRef}
          value={value}
          onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => onChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={isWelcome ? 'Ask anything...' : '输入消息...'}
          rows={isWelcome ? 3 : 1}
          sx={{
            width: '100%', border: 'none', outline: 'none', resize: 'none',
            fontSize: isWelcome ? 18 : 14, fontFamily: 'Inter, sans-serif',
            color: '#191c1e', bgcolor: 'transparent',
            '&::placeholder': { color: 'rgba(65,72,78,0.4)' },
          }}
        />
      </Box>

      <Box sx={{
        borderTop: isWelcome ? '1px solid rgba(192,199,207,0.1)' : undefined,
        pt: isWelcome ? 2 : 0,
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
          <IconButton ref={zapRef} size="small" sx={{ p: 1 }} onClick={handleZapClick}>
            <Zap size={isWelcome ? 20 : 16} color={hasSelectedSkills ? '#2563eb' : '#41484e'} />
          </IconButton>
          <Box sx={{ width: 1, height: 24, bgcolor: 'rgba(192,199,207,0.2)', mx: 0.5 }} />
          <IconButton size="small" sx={{ p: 1 }} onClick={handlePickFiles}>
            <Paperclip size={isWelcome ? 18 : 14} color="#41484e" />
          </IconButton>
          <IconButton size="small" sx={{ p: 1 }}>
            <Mic size={isWelcome ? 18 : 14} color="#41484e" />
          </IconButton>
        </Box>
        {isStreaming ? (
          <Box onClick={onStop} sx={{
            height: isWelcome ? 48 : 40, borderRadius: 2,
            background: 'linear-gradient(135deg, #ef4444 0%, #dc2626 100%)',
            boxShadow: '0px 4px 6px -1px rgba(239,68,68,0.2), 0px 2px 4px -2px rgba(239,68,68,0.2)',
            display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 1.25,
            cursor: 'pointer', flexShrink: 0, px: 3, position: 'relative', overflow: 'hidden', transition: 'all 0.3s ease',
          }}>
            <Box sx={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', opacity: 0.2, pointerEvents: 'none' }}>
              <Box sx={{ width: 96, height: 96, border: '2px dashed #fff', borderRadius: 3, flexShrink: 0 }} />
            </Box>
            <CircleStop size={15} color="#fff" style={{ position: 'relative' }} />
            <Typography sx={{ fontSize: 11, fontWeight: 600, color: '#fff', textTransform: 'uppercase', letterSpacing: '1.65px', whiteSpace: 'nowrap', position: 'relative' }}>
              停止
            </Typography>
          </Box>
        ) : (
          <Box onClick={canSend ? handleSend : undefined} sx={{
            height: isWelcome ? 48 : 40, borderRadius: 2,
            background: canSend ? 'linear-gradient(135deg, #7cb9e8 0%, #8db2ff 100%)' : '#e0e3e5',
            boxShadow: canSend ? '0px 4px 6px -1px rgba(124,185,232,0.2), 0px 2px 4px -2px rgba(124,185,232,0.2)' : 'none',
            display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 1,
            cursor: canSend ? 'pointer' : 'default', flexShrink: 0, px: 3, transition: 'all 0.3s ease',
          }}>
            <Typography sx={{ fontSize: 11, fontWeight: 600, color: canSend ? '#fff' : '#9ca3af', textTransform: 'uppercase', letterSpacing: '1.1px', whiteSpace: 'nowrap', transition: 'color 0.3s ease' }}>
              发送
            </Typography>
            <SendHorizonal size={14} color={canSend ? '#fff' : '#9ca3af'} />
          </Box>
        )}
      </Box>

      <Popover
        open={skillOpen} anchorEl={skillAnchor} onClose={closeSkillSelector}
        anchorOrigin={{ vertical: 'top', horizontal: 'left' }}
        transformOrigin={{ vertical: 'bottom', horizontal: 'left' }}
        slotProps={{ paper: { sx: { bgcolor: 'transparent', boxShadow: 'none', overflow: 'visible', mb: 1 } } }}
      >
        <SkillSelector onConfirm={closeSkillSelector} onCancel={closeSkillSelector} />
      </Popover>
    </Box>
  );
}
