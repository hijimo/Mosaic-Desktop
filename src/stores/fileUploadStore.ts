import { create } from 'zustand';

export interface AttachedFile {
  id: string;
  name: string;
  path: string;
  /** 文件扩展名（小写，不含点） */
  ext: string;
}

/**
 * 全局文件上传 store —— 供外部工具/skill 向输入框注入附件。
 * InputArea 内部维护自己的 files state，但会监听此 store 的 pending 队列。
 */
interface FileUploadState {
  /** 外部注入的待消费文件（工具/skill 调用 addPending 后由 InputArea 消费并清空） */
  pending: AttachedFile[];
  addPending: (files: AttachedFile[]) => void;
  consumePending: () => AttachedFile[];
}

export const useFileUploadStore = create<FileUploadState>((set, get) => ({
  pending: [],
  addPending: (files) =>
    set((s) => ({ pending: [...s.pending, ...files] })),
  consumePending: () => {
    const p = get().pending;
    if (p.length > 0) set({ pending: [] });
    return p;
  },
}));
