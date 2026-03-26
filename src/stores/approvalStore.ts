import { create } from 'zustand';
import type { ApprovalRequestState } from '@/types';

interface ApprovalStoreState {
  approvals: Map<string, ApprovalRequestState>;
  addApproval: (approval: ApprovalRequestState) => void;
  removeApproval: (callId: string) => void;
  clearAll: () => void;
}

export const useApprovalStore = create<ApprovalStoreState>((set) => ({
  approvals: new Map(),

  addApproval: (approval) =>
    set((state) => {
      const next = new Map(state.approvals);
      next.set(approval.callId, approval);
      return { approvals: next };
    }),

  removeApproval: (callId) =>
    set((state) => {
      const next = new Map(state.approvals);
      next.delete(callId);
      return { approvals: next };
    }),

  clearAll: () => set({ approvals: new Map() }),
}));
