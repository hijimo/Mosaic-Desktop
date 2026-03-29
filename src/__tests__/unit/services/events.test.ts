import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { listenCodexEvent } from '@/services/tauri/events';

vi.mock('@tauri-apps/api/event');

describe('tauri/events', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('listenCodexEvent registers listener on codex-event channel', async () => {
    const unlisten = vi.fn();
    vi.mocked(listen).mockResolvedValue(unlisten);

    const callback = vi.fn();
    const result = await listenCodexEvent(callback);

    expect(listen).toHaveBeenCalledWith('codex-event', expect.any(Function));
    expect(result).toBe(unlisten);
  });

  it('listenCodexEvent forwards payload to callback', async () => {
    let capturedHandler: ((event: unknown) => void) | undefined;
    vi.mocked(listen).mockImplementation(async (_name, handler) => {
      capturedHandler = handler as (event: unknown) => void;
      return () => {};
    });

    const callback = vi.fn();
    await listenCodexEvent(callback);

    const mockPayload = { thread_id: 't1', event: { id: 'e1', msg: { type: 'shutdown_complete' } } };
    capturedHandler!({ payload: mockPayload });

    expect(callback).toHaveBeenCalledWith(mockPayload);
  });
});
