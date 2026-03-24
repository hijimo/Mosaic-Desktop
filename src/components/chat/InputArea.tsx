import { useRef, useCallback } from 'react';
import { Box, IconButton } from '@mui/material';
import { Paperclip, FileText, Mic, Image, SendHorizonal } from 'lucide-react';

interface InputAreaProps {
  value: string;
  onChange: (value: string) => void;
  onSend: (text: string) => void;
  /** 是否为欢迎页大输入框样式 */
  variant?: 'default' | 'welcome';
}

export function InputArea({ value, onChange, onSend, variant = 'default' }: InputAreaProps): React.ReactElement {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        if (value.trim()) onSend(value.trim());
      }
    },
    [value, onSend],
  );

  const handleSend = useCallback(() => {
    if (value.trim()) onSend(value.trim());
  }, [value, onSend]);

  const isWelcome = variant === 'welcome';

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
      {/* Textarea */}
      <Box sx={{ minHeight: isWelcome ? 96 : undefined, px: isWelcome ? 1.5 : 0, py: isWelcome ? 1 : 0 }}>
        <Box
          component="textarea"
          ref={textareaRef}
          value={value}
          onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => onChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={isWelcome ? 'Ask anything or use @ and / for tools...' : 'Type a message...'}
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
          <IconButton size="small" sx={{ p: 1 }}><Paperclip size={isWelcome ? 20 : 16} color="#64748b" /></IconButton>
          <IconButton size="small" sx={{ p: 1 }}><FileText size={isWelcome ? 18 : 14} color="#64748b" /></IconButton>
          <Box sx={{ width: 1, height: 24, bgcolor: 'rgba(192,199,207,0.2)', mx: 0.5 }} />
          <IconButton size="small" sx={{ p: 1 }}><Mic size={isWelcome ? 18 : 14} color="#64748b" /></IconButton>
          <IconButton size="small" sx={{ p: 1 }}><Image size={isWelcome ? 18 : 14} color="#64748b" /></IconButton>
        </Box>
        <Box
          onClick={handleSend}
          sx={{
            width: isWelcome ? 48 : 40, height: isWelcome ? 48 : 40,
            borderRadius: isWelcome ? 3 : 2.5,
            background: value.trim()
              ? 'linear-gradient(135deg, #7cb9e8 0%, #8db2ff 100%)'
              : isWelcome ? 'linear-gradient(135deg, #7cb9e8 0%, #8db2ff 100%)' : '#e2e8f0',
            boxShadow: isWelcome ? '0px 10px 15px -3px rgba(124,185,232,0.3)' : undefined,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            cursor: value.trim() ? 'pointer' : 'default', flexShrink: 0,
          }}
        >
          <SendHorizonal size={isWelcome ? 18 : 16} color="#fff" />
        </Box>
      </Box>
    </Box>
  );
}
