import { create } from 'zustand';
import type { SkillHubMeta, HubInstalledSkill, InstallResult } from '@/services/tauri/skillsHub';
import * as hubApi from '@/services/tauri/skillsHub';

interface SkillHubState {
  // Search
  searchQuery: string;
  searchResults: SkillHubMeta[];
  searchLoading: boolean;
  searchError: string | null;

  // Installed
  installed: HubInstalledSkill[];
  installedLoading: boolean;

  // Install flow
  installing: string | null; // identifier being installed
  lastInstallResult: InstallResult | null;

  // Actions
  setSearchQuery: (query: string) => void;
  search: (query: string, sourceFilter?: string) => Promise<void>;
  loadInstalled: () => Promise<void>;
  install: (identifier: string, category?: string, force?: boolean) => Promise<InstallResult>;
  uninstall: (name: string) => Promise<void>;
  clearSearch: () => void;
}

export const useSkillHubStore = create<SkillHubState>((set, get) => ({
  searchQuery: '',
  searchResults: [],
  searchLoading: false,
  searchError: null,
  installed: [],
  installedLoading: false,
  installing: null,
  lastInstallResult: null,

  setSearchQuery: (query) => set({ searchQuery: query }),

  search: async (query, sourceFilter) => {
    set({ searchLoading: true, searchError: null });
    try {
      const results = await hubApi.skillsHubSearch(query, sourceFilter);
      set({ searchResults: results, searchLoading: false });
    } catch (e) {
      set({ searchError: String(e), searchLoading: false });
    }
  },

  loadInstalled: async () => {
    set({ installedLoading: true });
    try {
      const list = await hubApi.skillsHubList();
      set({ installed: list, installedLoading: false });
    } catch {
      set({ installedLoading: false });
    }
  },

  install: async (identifier, category, force) => {
    set({ installing: identifier });
    try {
      const result = await hubApi.skillsHubInstall(identifier, category, force);
      set({ installing: null, lastInstallResult: result });
      // Refresh installed list
      get().loadInstalled();
      return result;
    } catch (e) {
      set({ installing: null });
      throw e;
    }
  },

  uninstall: async (name) => {
    await hubApi.skillsHubUninstall(name);
    get().loadInstalled();
  },

  clearSearch: () => set({ searchResults: [], searchQuery: '', searchError: null }),
}));
