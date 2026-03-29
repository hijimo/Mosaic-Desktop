import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { ThemeProvider } from '@mui/material';
import { theme } from '@/styles/theme';
import { MainLayout } from '@/layouts/MainLayout';
import { IndexPage } from '@/pages/index';

import type { ThreadMeta } from '@/types';

const { mockInvoke } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
}));

function resetInvokeMock(): void {
  mockInvoke.mockImplementation(async (command: string) => {
    switch (command) {
      case 'thread_list':
        return [] as ThreadMeta[];
      case 'get_cwd':
        return '/test/project';
      case 'thread_start':
        return 'mock-thread-id';
      case 'submit_op':
        return undefined;
      default:
        return undefined;
    }
  });
}

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

function renderApp(): void {
  render(
    <ThemeProvider theme={theme}>
      <MemoryRouter initialEntries={['/']}>
        <Routes>
          <Route path="/" element={<MainLayout />}>
            <Route index element={<IndexPage />} />
          </Route>
        </Routes>
      </MemoryRouter>
    </ThemeProvider>,
  );
}

describe('IndexPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetInvokeMock();
  });

  it('renders welcome heading', () => {
    renderApp();
    expect(screen.getByText('How can I help you today?')).toBeInTheDocument();
  });

  it('renders welcome subtitle', () => {
    renderApp();
    expect(screen.getByText(/Select a project/)).toBeInTheDocument();
  });

  it('renders project selector', () => {
    renderApp();
    expect(screen.getByText('Current Context')).toBeInTheDocument();
    expect(screen.getByText('Q4 Marketing Strategy')).toBeInTheDocument();
  });

  it('renders input textarea with placeholder', () => {
    renderApp();
    expect(screen.getByPlaceholderText(/Ask anything/)).toBeInTheDocument();
  });

  it('renders suggestion buttons', () => {
    renderApp();
    expect(screen.getByText('Optimize')).toBeInTheDocument();
    expect(screen.getByText('Summarize')).toBeInTheDocument();
    expect(screen.getByText('Translate')).toBeInTheDocument();
  });

  it('renders skill cards', () => {
    renderApp();
    expect(screen.getByText('Code Review')).toBeInTheDocument();
    expect(screen.getByText('Data Analyst')).toBeInTheDocument();
    expect(screen.getByText('Browse Agent')).toBeInTheDocument();
    expect(screen.getByText('Drafting')).toBeInTheDocument();
  });

  it('allows typing in the textarea', async () => {
    renderApp();
    const textarea = screen.getByPlaceholderText(/Ask anything/);
    await userEvent.type(textarea, 'Hello AI');
    expect(textarea).toHaveValue('Hello AI');
  });
});

describe('Sidebar', () => {
  it('renders brand name', () => {
    renderApp();
    expect(screen.getByText('Aether AI')).toBeInTheDocument();
  });

  it('renders navigation items', () => {
    renderApp();
    expect(screen.getByText('New Chat')).toBeInTheDocument();
    expect(screen.getByText('Automation')).toBeInTheDocument();
    expect(screen.getByText('Skills')).toBeInTheDocument();
    expect(screen.getByText('Agents')).toBeInTheDocument();
  });

  it('renders recent chats section', () => {
    renderApp();
    expect(screen.getByText('Recent Chats')).toBeInTheDocument();
  });

  it('renders settings button', () => {
    renderApp();
    expect(screen.getByText('Settings')).toBeInTheDocument();
  });

  it('renders header with Live Nodes', () => {
    renderApp();
    expect(screen.getByText('Live Nodes')).toBeInTheDocument();
  });
});
