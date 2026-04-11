import { create } from 'zustand';

export interface AgentRoleInfo {
  name: string;
  description: string | null;
  source: 'built-in' | 'user';
}

interface AgentRoleState {
  roles: AgentRoleInfo[];
  activeRole: AgentRoleInfo | null;
  loading: boolean;

  setRoles: (roles: AgentRoleInfo[]) => void;
  setActiveRole: (role: AgentRoleInfo | null) => void;
  setLoading: (loading: boolean) => void;
}

export const useAgentRoleStore = create<AgentRoleState>((set) => ({
  roles: [],
  activeRole: null,
  loading: false,

  setRoles: (roles) => set({ roles, loading: false }),
  setActiveRole: (role) => set({ activeRole: role }),
  setLoading: (loading) => set({ loading }),
}));
