import { Box, Typography } from '@mui/material';
import errorAlertIcon from '@/assets/icons/error-alert.svg';
import retryIcon from '@/assets/icons/retry.svg';

interface ErrorCardProps {
  message: string;
  onRetry?: () => void;
  onDismiss?: () => void;
}

export function ErrorCard({ message, onRetry, onDismiss }: ErrorCardProps): React.ReactElement {
  return (
    <Box
      sx={{
        bgcolor: '#fff5f5',
        border: '2px solid #fee2e2',
        borderRadius: 4,
        p: '22px',
        boxShadow: '0px 1px 2px rgba(0,0,0,0.05)',
      }}
    >
      <Box sx={{ display: 'flex', gap: 2, alignItems: 'flex-start' }}>
        <Box
          sx={{
            bgcolor: '#fee2e2',
            borderRadius: 2,
            width: 40,
            height: 40,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
          }}
        >
          <Box component="img" src={errorAlertIcon} alt="" sx={{ width: 20, height: 20 }} />
        </Box>
        <Box sx={{ flex: 1, minWidth: 0 }}>
          <Typography sx={{ fontSize: 14, fontWeight: 600, color: '#7f1d1d', lineHeight: '20px' }}>
            响应生成失败
          </Typography>
          <Typography sx={{ fontSize: 12, color: '#991b1b', lineHeight: '19.5px', mt: 0.5 }}>
            {message}
          </Typography>
          <Box sx={{ display: 'flex', gap: 1, pt: 1.5 }}>
            {onRetry && (
              <Box
                onClick={onRetry}
                sx={{
                  bgcolor: '#dc2626',
                  borderRadius: 1,
                  px: 2,
                  py: '6.5px',
                  display: 'flex',
                  alignItems: 'center',
                  gap: 1,
                  cursor: 'pointer',
                  boxShadow: '0px 1px 2px rgba(0,0,0,0.05)',
                }}
              >
                <Box component="img" src={retryIcon} alt="" sx={{ width: 8, height: 8 }} />
                <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#fff', lineHeight: '16px' }}>
                  重新生成
                </Typography>
              </Box>
            )}
            {onDismiss && (
              <Box
                onClick={onDismiss}
                sx={{
                  bgcolor: '#fff',
                  border: '1px solid #fecaca',
                  borderRadius: 1,
                  px: 2,
                  py: '7px',
                  cursor: 'pointer',
                }}
              >
                <Typography sx={{ fontSize: 12, fontWeight: 600, color: '#b91c1c', lineHeight: '16px' }}>
                  忽略
                </Typography>
              </Box>
            )}
          </Box>
        </Box>
      </Box>
    </Box>
  );
}
