import { create } from 'zustand';

export type SkillScope = 'Repo' | 'User' | 'System' | 'Admin';

export interface SkillInfo {
  name: string;
  description: string;
  scope: SkillScope;
  path: string;
}

const MAX_SELECTED_SKILLS = 10;

/** 全局 scope（不依赖工作区） */
const GLOBAL_SCOPES: SkillScope[] = ['User', 'System', 'Admin'];

interface SkillState {
  /** 按 cwd 缓存的 skill 列表 */
  cache: Record<string, SkillInfo[]>;
  /** 当前生效的 cwd */
  activeCwd: string | null;
  loading: boolean;
  selectedSkills: SkillInfo[];

  setSkillsForCwd: (cwd: string, skills: SkillInfo[]) => void;
  setActiveCwd: (cwd: string | null) => void;
  setLoading: (loading: boolean) => void;
  toggleSkill: (skill: SkillInfo) => void;
  removeSelectedSkill: (name: string) => void;
  clearSelectedSkills: () => void;
  setSelectedSkills: (skills: SkillInfo[]) => void;
}

export const useSkillStore = create<SkillState>((set) => ({
  cache: {},
  activeCwd: null,
  loading: false,
  selectedSkills: [],

  setSkillsForCwd: (cwd, skills) =>
    set((state) => ({
      cache: { ...state.cache, [cwd]: skills },
      activeCwd: cwd,
      loading: false,
    })),

  setActiveCwd: (cwd) => set({ activeCwd: cwd }),

  setLoading: (loading) => set({ loading }),

  toggleSkill: (skill) =>
    set((state) => {
      const exists = state.selectedSkills.some((s) => s.name === skill.name);
      if (exists) return { selectedSkills: state.selectedSkills.filter((s) => s.name !== skill.name) };
      if (state.selectedSkills.length >= MAX_SELECTED_SKILLS) return state;
      return { selectedSkills: [...state.selectedSkills, skill] };
    }),

  removeSelectedSkill: (name) =>
    set((state) => ({ selectedSkills: state.selectedSkills.filter((s) => s.name !== name) })),

  clearSelectedSkills: () => set({ selectedSkills: [] }),

  setSelectedSkills: (skills) => set({ selectedSkills: skills.slice(0, MAX_SELECTED_SKILLS) }),
}));

/** 从 store 中获取当前应展示的 skills 列表 */
export function useSkills(): SkillInfo[] {
  const cache = useSkillStore((s) => s.cache);
  const activeCwd = useSkillStore((s) => s.activeCwd);

  if (activeCwd && cache[activeCwd]) return cache[activeCwd];

  // 回退到全局 skills
  const firstEntry = Object.values(cache)[0];
  if (!firstEntry) return [];
  return firstEntry.filter((s) => GLOBAL_SCOPES.includes(s.scope));
}
