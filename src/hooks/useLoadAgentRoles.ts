import { useEffect } from 'react';
import { useAgentRoleStore } from '@/stores/agentRoleStore';
import { listAgentRoles } from '@/services/api';

/** 加载 agent role 列表，仅首次调用时请求后端。 */
export function useLoadAgentRoles(): void {
  const setRoles = useAgentRoleStore((s) => s.setRoles);
  const setLoading = useAgentRoleStore((s) => s.setLoading);

  useEffect(() => {
    if (useAgentRoleStore.getState().roles.length > 0) return;

    let cancelled = false;
    setLoading(true);
    listAgentRoles()
      .then((r) => { if (!cancelled) setRoles(r); })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [setRoles, setLoading]);
}
