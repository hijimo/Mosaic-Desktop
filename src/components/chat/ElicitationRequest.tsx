import { useState, useCallback } from 'react';
import { Box, Typography, Button, TextField, MenuItem, Select, FormControl } from '@mui/material';
import { ShieldCheck, Link as LinkIcon } from 'lucide-react';
import type { ElicitationRequestState } from '@/types';

interface ElicitationRequestProps {
  serverName: string;
  requestId: string;
  message: string;
  mode?: ElicitationRequestState['mode'];
  schema?: ElicitationRequestState['schema'];
  url?: string;
  /** Already resolved: user's decision */
  responseAction?: string;
  /** Already resolved: user-submitted form data */
  responseContent?: Record<string, unknown>;
  onDecision?: (requestId: string, serverName: string, decision: 'accept' | 'decline' | 'cancel', content?: Record<string, unknown>) => void;
}

/** Render a single form field from a JSON Schema property definition. */
function SchemaField({ name, def, value, onChange }: {
  name: string;
  def: Record<string, unknown>;
  value: unknown;
  onChange: (name: string, value: unknown) => void;
}): React.ReactElement {
  const label = (def.title as string) ?? name;
  const description = def.description as string | undefined;
  const type = def.type as string;

  // Enum (string with enum or oneOf)
  if (type === 'string' && (def.enum || def.oneOf)) {
    const options: { value: string; label: string }[] = def.oneOf
      ? (def.oneOf as { const: string; title?: string }[]).map((o) => ({ value: o.const, label: o.title ?? o.const }))
      : (def.enum as string[]).map((e) => ({ value: e, label: e }));

    return (
      <FormControl fullWidth size="small">
        <Typography sx={{ fontSize: 10, fontWeight: 600, color: '#64748b', textTransform: 'uppercase', letterSpacing: '0.25px', mb: 0.5 }}>
          {label}
        </Typography>
        {description && <Typography sx={{ fontSize: 11, color: '#94a3b8', mb: 0.5 }}>{description}</Typography>}
        <Select
          value={(value as string) ?? (def.default as string) ?? ''}
          onChange={(e) => onChange(name, e.target.value)}
          sx={{ bgcolor: '#f8fafc', fontSize: 14, '& .MuiSelect-select': { py: '10px', px: '13px' } }}
        >
          {options.map((o) => (
            <MenuItem key={o.value} value={o.value}>{o.label}</MenuItem>
          ))}
        </Select>
      </FormControl>
    );
  }

  // Boolean
  if (type === 'boolean') {
    return (
      <FormControl fullWidth size="small">
        <Typography sx={{ fontSize: 10, fontWeight: 600, color: '#64748b', textTransform: 'uppercase', letterSpacing: '0.25px', mb: 0.5 }}>
          {label}
        </Typography>
        {description && <Typography sx={{ fontSize: 11, color: '#94a3b8', mb: 0.5 }}>{description}</Typography>}
        <Select
          value={String(value ?? def.default ?? false)}
          onChange={(e) => onChange(name, e.target.value === 'true')}
          sx={{ bgcolor: '#f8fafc', fontSize: 14, '& .MuiSelect-select': { py: '10px', px: '13px' } }}
        >
          <MenuItem value="true">是</MenuItem>
          <MenuItem value="false">否</MenuItem>
        </Select>
      </FormControl>
    );
  }

  // Number / Integer
  if (type === 'number' || type === 'integer') {
    return (
      <Box>
        <Typography sx={{ fontSize: 10, fontWeight: 600, color: '#64748b', textTransform: 'uppercase', letterSpacing: '0.25px', mb: 0.5 }}>
          {label}
        </Typography>
        {description && <Typography sx={{ fontSize: 11, color: '#94a3b8', mb: 0.5 }}>{description}</Typography>}
        <TextField
          fullWidth
          size="small"
          type="number"
          value={value ?? def.default ?? ''}
          onChange={(e) => onChange(name, e.target.value === '' ? undefined : Number(e.target.value))}
          inputProps={{ min: def.minimum as number, max: def.maximum as number }}
          sx={{ '& .MuiInputBase-root': { bgcolor: '#f8fafc', fontSize: 14 }, '& .MuiInputBase-input': { py: '10px', px: '13px' } }}
        />
      </Box>
    );
  }

  // Default: string
  return (
    <Box>
      <Typography sx={{ fontSize: 10, fontWeight: 600, color: '#64748b', textTransform: 'uppercase', letterSpacing: '0.25px', mb: 0.5 }}>
        {label}
      </Typography>
      {description && <Typography sx={{ fontSize: 11, color: '#94a3b8', mb: 0.5 }}>{description}</Typography>}
      <TextField
        fullWidth
        size="small"
        placeholder={`e.g. ${label}`}
        value={(value as string) ?? (def.default as string) ?? ''}
        onChange={(e) => onChange(name, e.target.value)}
        sx={{ '& .MuiInputBase-root': { bgcolor: '#f8fafc', fontSize: 14 }, '& .MuiInputBase-input': { py: '10px', px: '13px' } }}
      />
    </Box>
  );
}

export function ElicitationRequest({ serverName, requestId, message, mode, schema, url, responseAction, responseContent, onDecision }: ElicitationRequestProps): React.ReactElement {
  const [formData, setFormData] = useState<Record<string, unknown>>({});

  const handleFieldChange = useCallback((name: string, value: unknown) => {
    setFormData((prev) => ({ ...prev, [name]: value }));
  }, []);

  const isResolved = !!responseAction;
  const isUrlMode = mode === 'url';
  const hasSchema = !isUrlMode && schema?.properties && Object.keys(schema.properties as Record<string, unknown>).length > 0;

  const handleAccept = useCallback(() => {
    if (isUrlMode) {
      // Open URL in external browser (only allow http/https to prevent javascript: etc.)
      if (url) {
        try {
          const parsed = new URL(url);
          if (parsed.protocol === 'http:' || parsed.protocol === 'https:') {
            window.open(url, '_blank', 'noopener,noreferrer');
          }
        } catch {
          // invalid URL, skip opening
        }
      }
      onDecision?.(requestId, serverName, 'accept');
    } else if (hasSchema) {
      onDecision?.(requestId, serverName, 'accept', formData);
    } else {
      onDecision?.(requestId, serverName, 'accept');
    }
  }, [isUrlMode, hasSchema, url, formData, requestId, serverName, onDecision]);

  // Determine button labels
  const acceptLabel = isUrlMode ? '打开链接' : hasSchema ? '提交' : '确认';

  // URL mode: white card with blue border + shadow
  if (isUrlMode) {
    return (
      <Box sx={{
        bgcolor: '#fff', border: '1px solid #d4e6ff', borderRadius: '16px',
        boxShadow: '0px 1px 2px rgba(0,0,0,0.05)', p: '25px',
        display: 'flex', flexDirection: 'column', gap: '20px',
      }}>
        {/* Header with icon */}
        <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
          <Box sx={{ bgcolor: '#f0f7ff', borderRadius: '8px', width: 40, height: 40, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0 }}>
            <LinkIcon size={20} color="#3b82f6" />
          </Box>
          <Box sx={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 0.5 }}>
            <Typography sx={{ fontSize: 14, fontWeight: 600, color: '#1e293b', lineHeight: '20px' }}>
              {serverName} 需要外部认证
            </Typography>
            <Typography sx={{ fontSize: 12, color: '#64748b', lineHeight: '16px' }}>
              {message}
            </Typography>
          </Box>
        </Box>

        {/* URL display */}
        {url && (
          <Box sx={{ bgcolor: '#f8fafc', border: '1px solid #e2e8f0', borderRadius: '4px', p: '13px', display: 'flex', alignItems: 'center', gap: 1.5 }}>
            <LinkIcon size={12} color="#475569" />
            <Typography sx={{ fontFamily: '"Liberation Mono", monospace', fontSize: 12, color: '#475569', lineHeight: '16px', wordBreak: 'break-all' }}>
              {url}
            </Typography>
          </Box>
        )}

        {/* Buttons (only when pending) */}
        {!isResolved && (
          <ActionButtons acceptLabel={acceptLabel} onAccept={handleAccept} onReject={() => onDecision?.(requestId, serverName, 'decline')} onCancel={() => onDecision?.(requestId, serverName, 'cancel')} gradient />
        )}
        {isResolved && <ResolvedBadge action={responseAction} />}
      </Box>
    );
  }

  // Form mode with schema: render form fields
  if (hasSchema) {
    const properties = schema!.properties as Record<string, Record<string, unknown>>;
    return (
      <Box sx={{
        bgcolor: '#fff', border: '1px solid #d4e6ff', borderRadius: '16px',
        boxShadow: '0px 1px 2px rgba(0,0,0,0.05)', p: '25px',
        display: 'flex', flexDirection: 'column', gap: '20px',
      }}>
        {/* Header */}
        <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
          <Box sx={{ bgcolor: '#f0f7ff', borderRadius: '8px', width: 40, height: 40, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0 }}>
            <ShieldCheck size={20} color="#3b82f6" />
          </Box>
          <Box sx={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 0.5 }}>
            <Typography sx={{ fontSize: 14, fontWeight: 600, color: '#1e293b', lineHeight: '20px' }}>
              {serverName} 请求信息
            </Typography>
            <Typography sx={{ fontSize: 12, color: '#64748b', lineHeight: '16px' }}>
              {message}
            </Typography>
          </Box>
        </Box>

        {/* Form fields */}
        <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
          {isResolved
            ? <ReadOnlyFields properties={properties} values={responseContent ?? {}} />
            : Object.entries(properties).map(([name, def]) => (
                <SchemaField key={name} name={name} def={def} value={formData[name]} onChange={handleFieldChange} />
              ))
          }
        </Box>

        {!isResolved && (
          <ActionButtons acceptLabel={acceptLabel} onAccept={handleAccept} onReject={() => onDecision?.(requestId, serverName, 'decline')} onCancel={() => onDecision?.(requestId, serverName, 'cancel')} flexAccept />
        )}
        {isResolved && <ResolvedBadge action={responseAction} />}
      </Box>
    );
  }

  // Pure confirmation mode: glassmorphism card
  return (
    <Box sx={{
      backdropFilter: 'blur(2px)', bgcolor: 'rgba(255,255,255,0.8)',
      border: '1px solid rgba(124,185,232,0.3)', borderRadius: '16px',
      boxShadow: '0px 4px 6px -1px rgba(0,0,0,0.1), 0px 2px 4px -2px rgba(0,0,0,0.1)',
      p: '25px', display: 'flex', flexDirection: 'column', gap: 2,
    }}>
      {/* Header */}
      <Box sx={{ display: 'flex', gap: 1.5, alignItems: 'center' }}>
        <Box sx={{ bgcolor: '#eff6ff', borderRadius: '4px', width: 32, height: 32, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0 }}>
          <ShieldCheck size={16} color="#3b82f6" />
        </Box>
        <Typography sx={{ fontSize: 14, fontWeight: 600, color: '#1e293b', lineHeight: '20px' }}>
          {serverName} 请求确认
        </Typography>
      </Box>

      {/* Message */}
      <Typography sx={{ fontSize: 14, color: '#475569', lineHeight: '20px' }}>
        {message}
      </Typography>

      {/* Buttons */}
      {!isResolved && (
        <ActionButtons acceptLabel={acceptLabel} onAccept={handleAccept} onReject={() => onDecision?.(requestId, serverName, 'decline')} onCancel={() => onDecision?.(requestId, serverName, 'cancel')} />
      )}
      {isResolved && <ResolvedBadge action={responseAction} />}
    </Box>
  );
}

/** Shared button group for all three modes. */
function ActionButtons({ acceptLabel, onAccept, onReject, onCancel, gradient, flexAccept }: {
  acceptLabel: string;
  onAccept: () => void;
  onReject: () => void;
  onCancel: () => void;
  gradient?: boolean;
  flexAccept?: boolean;
}): React.ReactElement {
  return (
    <Box sx={{ display: 'flex', gap: 1, pt: 1 }}>
      <Button
        size="small"
        variant="contained"
        onClick={onAccept}
        sx={{
          ...(gradient
            ? { background: 'linear-gradient(to right, #7cb9e8, #005bc1)' }
            : { bgcolor: '#005bc1', '&:hover': { bgcolor: '#004a9e' } }),
          ...(flexAccept ? { flex: 1 } : {}),
          color: '#fff', fontSize: 12, fontWeight: 600, px: '20px', py: '8.5px',
          borderRadius: '8px', textTransform: 'none',
          boxShadow: '0px 1px 2px rgba(0,0,0,0.05)',
        }}
      >
        {acceptLabel}
      </Button>
      <Button
        size="small"
        variant="outlined"
        onClick={onReject}
        sx={{
          borderColor: 'rgba(192,199,207,0.3)', color: '#475569', bgcolor: '#fff',
          fontSize: 12, fontWeight: 600, px: '21px', py: '9px',
          borderRadius: '8px', textTransform: 'none',
        }}
      >
        拒绝
      </Button>
      <Button
        size="small"
        onClick={onCancel}
        sx={{
          color: '#94a3b8', fontSize: 12, fontWeight: 600, px: '20px', py: '8.5px',
          textTransform: 'none',
        }}
      >
        取消
      </Button>
    </Box>
  );
}

const ACTION_LABELS: Record<string, string> = {
  accept: '已确认',
  decline: '已拒绝',
  cancel: '已取消',
};

/** Small badge showing the resolved status. */
function ResolvedBadge({ action }: { action?: string }): React.ReactElement {
  const label = ACTION_LABELS[action ?? ''] ?? action ?? '已完成';
  const isAccept = action === 'accept';
  return (
    <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, pt: 0.5 }}>
      <Typography sx={{
        fontSize: 11, fontWeight: 600, px: 1.5, py: 0.5, borderRadius: '4px',
        bgcolor: isAccept ? '#f0fdf4' : '#fef2f2',
        color: isAccept ? '#16a34a' : '#dc2626',
      }}>
        {label}
      </Typography>
    </Box>
  );
}

/** Read-only display of submitted form values. */
function ReadOnlyFields({ properties, values }: {
  properties: Record<string, Record<string, unknown>>;
  values: Record<string, unknown>;
}): React.ReactElement {
  return (
    <>
      {Object.entries(properties).map(([name, def]) => {
        const label = (def.title as string) ?? name;
        const val = values[name];
        return (
          <Box key={name}>
            <Typography sx={{ fontSize: 10, fontWeight: 600, color: '#64748b', textTransform: 'uppercase', letterSpacing: '0.25px', mb: 0.5 }}>
              {label}
            </Typography>
            <Box sx={{ bgcolor: '#f8fafc', border: '1px solid #e2e8f0', borderRadius: '4px', px: '13px', py: '10px' }}>
              <Typography sx={{ fontSize: 14, color: '#1e293b' }}>
                {val === undefined || val === null || val === '' ? '—' : String(val)}
              </Typography>
            </Box>
          </Box>
        );
      })}
    </>
  );
}
