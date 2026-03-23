import { createTheme } from '@mui/material/styles';

export const theme = createTheme({
  palette: {
    mode: 'light',
    primary: { main: '#7cb9e8' },
    secondary: { main: '#8db2ff' },
    background: {
      default: '#f7f9fb',
      paper: '#ffffff',
    },
    text: {
      primary: '#191c1e',
      secondary: '#41484e',
    },
  },
  typography: {
    fontFamily: 'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
  },
  components: {
    MuiButton: {
      styleOverrides: {
        root: { textTransform: 'none', cursor: 'pointer' },
      },
    },
    MuiIconButton: {
      styleOverrides: {
        root: { cursor: 'pointer' },
      },
    },
  },
});
