import { useEffect } from 'react';
import { useSkillStore } from '@/stores/skillStore';
import { listSkills } from '@/services/api';

/**
 * 加载指定 cwd 的 skill 列表。
 * cwd 变化时自动刷新；已缓存的 cwd 直接切换，不重复请求。
 */
export function useLoadSkills(cwd: string | null): void {
  const cache = useSkillStore((s) => s.cache);
  const setSkillsForCwd = useSkillStore((s) => s.setSkillsForCwd);
  const setActiveCwd = useSkillStore((s) => s.setActiveCwd);
  const setLoading = useSkillStore((s) => s.setLoading);

  useEffect(() => {
    if (!cwd) return;

    // 已缓存，直接切换
    if (cache[cwd]) {
      setActiveCwd(cwd);
      return;
    }

    // 未缓存，请求后端
    let cancelled = false;
    setLoading(true);
    listSkills(cwd)
      .then((skills) => {
        if (!cancelled) setSkillsForCwd(cwd, skills);
      })
      .catch(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [cwd]); // 只依赖 cwd 变化
}
