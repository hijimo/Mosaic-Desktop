import { useEffect } from 'react';
import { useSkillStore } from '@/stores/skillStore';
import { listSkills } from '@/services/api';

/**
 * 加载指定 cwd 的 skill 列表。
 * 每次调用都 force_reload，确保拿到最新数据。
 */
export function useLoadSkills(cwd: string | null): void {
  const setSkillsForCwd = useSkillStore((s) => s.setSkillsForCwd);
  const setLoading = useSkillStore((s) => s.setLoading);

  useEffect(() => {
    if (!cwd) return;

    let cancelled = false;
    setLoading(true);
    listSkills(cwd, true)
      .then((skills) => {
        if (!cancelled) setSkillsForCwd(cwd, skills);
      })
      .catch(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [cwd]);
}
