import { Outlet } from 'react-router-dom';
import { Box } from '@mui/material';
import { Sidebar } from '@/components/common/Sidebar';

export function MainLayout(): React.ReactElement {
  return (
    <Box sx={{ display: 'flex', height: '100vh', bgcolor: '#f7f9fb' }}>
      <Sidebar />
      <Box component="main" sx={{ flex: 1, overflow: 'hidden', position: 'relative' }}>
        <Outlet />
      </Box>
    </Box>
  );
}
