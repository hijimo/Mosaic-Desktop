import { ThemeProvider, CssBaseline, Snackbar, Alert } from '@mui/material';
import { createBrowserRouter, RouterProvider } from 'react-router-dom';
import { SWRConfig } from 'swr';
import { useState, useEffect, useCallback } from 'react';
import { theme } from '@/styles/theme';
import { MainLayout } from '@/layouts/MainLayout';
import { IndexPage } from '@/pages/index';
import { SkillsHubPage } from '@/pages/skills-hub';
import { useCodexEvent } from '@/hooks/useCodexEvent';
import '@/styles/global.css';

const router = createBrowserRouter([
  {
    path: '/',
    element: <MainLayout />,
    children: [
      { index: true, element: <IndexPage /> },
      { path: 'thread/:threadId', element: <IndexPage /> },
      { path: 'skills-hub', element: <SkillsHubPage /> },
    ],
  },
]);

function AppInner(): React.ReactElement {
  useCodexEvent();
  return <RouterProvider router={router} />;
}

function GlobalErrorToast(): React.ReactElement {
  const [error, setError] = useState<string | null>(null);

  const handleError = useCallback((e: PromiseRejectionEvent) => {
    const msg = e.reason instanceof Error ? e.reason.message : String(e.reason ?? 'Unknown error');
    setError(msg);
  }, []);

  useEffect(() => {
    window.addEventListener('unhandledrejection', handleError);
    return () => window.removeEventListener('unhandledrejection', handleError);
  }, [handleError]);

  return (
    <Snackbar
      open={!!error}
      autoHideDuration={8000}
      onClose={() => setError(null)}
      anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
      sx={{ top: '150px !important' }}
    >
      <Alert severity="error" onClose={() => setError(null)} sx={{ maxWidth: 600, wordBreak: 'break-word' }}>
        {error}
      </Alert>
    </Snackbar>
  );
}

export default function App(): React.ReactElement {
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <GlobalErrorToast />
      <SWRConfig value={{ revalidateOnFocus: false, dedupingInterval: 2000 }}>
        <AppInner />
      </SWRConfig>
    </ThemeProvider>
  );
}
