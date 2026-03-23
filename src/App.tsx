import { ThemeProvider, CssBaseline } from '@mui/material';
import { createBrowserRouter, RouterProvider } from 'react-router-dom';
import { SWRConfig } from 'swr';
import { theme } from '@/styles/theme';
import { MainLayout } from '@/layouts/MainLayout';
import { IndexPage } from '@/pages/index';
import { useCodexEvent } from '@/hooks/useCodexEvent';
import '@/styles/global.css';

const router = createBrowserRouter([
  {
    path: '/',
    element: <MainLayout />,
    children: [
      { index: true, element: <IndexPage /> },
      { path: 'thread/:threadId', element: <IndexPage /> },
    ],
  },
]);

function AppInner(): React.ReactElement {
  useCodexEvent();
  return <RouterProvider router={router} />;
}

export default function App(): React.ReactElement {
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <SWRConfig value={{ revalidateOnFocus: false, dedupingInterval: 2000 }}>
        <AppInner />
      </SWRConfig>
    </ThemeProvider>
  );
}
