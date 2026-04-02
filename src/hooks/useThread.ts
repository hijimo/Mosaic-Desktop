import { useCallback } from 'react';
import { threadStart, threadArchive, threadResume, threadGetMessages, getCwd } from '@/services/api';
import { useThreadStore } from '@/stores/threadStore';
import { useMessageStore } from '@/stores/messageStore';
import type { ThreadMeta } from '@/types';

/**
 * Thread lifecycle management: create, archive, and resume threads.
 */
export function useThread(): {
  createThread: (overrideCwd?: string) => Promise<string>;
  archiveThread: (id: string) => Promise<void>;
  resumeThread: (id: string) => Promise<string>;
} {
  const addThread = useThreadStore((s) => s.addThread);
  const setActiveThread = useThreadStore((s) => s.setActiveThread);
  const removeThread = useThreadStore((s) => s.removeThread);
  const setMessages = useMessageStore((s) => s.setMessages);

  const createThread = useCallback(async (overrideCwd?: string) => {
    const cwd = overrideCwd ?? await getCwd();
    const threadId = await threadStart(cwd);
    const meta: ThreadMeta = {
      thread_id: threadId,
      cwd,
      model: null,
      model_provider_id: null,
      name: null,
      created_at: new Date().toISOString(),
      forked_from: null,
    };
    addThread(meta);
    setActiveThread(threadId);
    return threadId;
  }, [addThread, setActiveThread]);

  const archiveThread = useCallback(
    async (id: string) => {
      await threadArchive(id);
      removeThread(id);
    },
    [removeThread],
  );

  const resumeThread = useCallback(
    async (id: string) => {
      const meta = await threadResume(id);
      addThread(meta);
      const messages = await threadGetMessages(id);
      setMessages(id, messages);
      setActiveThread(id);
      return id;
    },
    [addThread, setActiveThread, setMessages],
  );

  return { createThread, archiveThread, resumeThread };
}
