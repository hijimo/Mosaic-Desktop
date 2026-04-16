import { invoke } from '@tauri-apps/api/core';

// ---------------------------------------------------------------------------
// Types — mirrors Rust hub::models structs (camelCase from serde)
// ---------------------------------------------------------------------------

export type TrustLevel = 'builtin' | 'trusted' | 'community' | 'agent_created';

export interface SkillHubMeta {
  name: string;
  description: string;
  source: string;
  identifier: string;
  trustLevel: TrustLevel;
  repo?: string;
  path?: string;
  tags: string[];
  extra: Record<string, unknown>;
}

export interface HubInstalledSkill {
  name: string;
  description: string;
  scope: string;
  path: string;
  sourceType: 'hub' | 'builtin' | 'local';
  enabled: boolean;
  // Hub-specific fields (only present when sourceType === 'hub')
  hubSource?: string;
  hubIdentifier?: string;
  trustLevel?: string;
  scanVerdict?: string;
  installedAt?: string;
}

export interface InstallResult {
  name: string;
  installPath: string;
  scanVerdict: string;
  scanFindings: number;
  scanReport: string;
}

export interface AuditResult {
  scanResult: unknown;
  report: string;
}

// ---------------------------------------------------------------------------
// Service functions — invoke Tauri commands
// ---------------------------------------------------------------------------

export async function skillsHubSearch(
  query: string,
  sourceFilter?: string,
  limit?: number,
): Promise<SkillHubMeta[]> {
  return invoke<SkillHubMeta[]>('skills_hub_search', {
    query,
    sourceFilter: sourceFilter ?? 'all',
    limit: limit ?? 20,
  });
}

export async function skillsHubInspect(
  identifier: string,
): Promise<SkillHubMeta | null> {
  return invoke<SkillHubMeta | null>('skills_hub_inspect', { identifier });
}

export async function skillsHubInstall(
  identifier: string,
  category?: string,
  force?: boolean,
): Promise<InstallResult> {
  return invoke<InstallResult>('skills_hub_install', {
    identifier,
    category: category ?? '',
    force: force ?? false,
  });
}

export async function skillsHubList(cwd?: string): Promise<HubInstalledSkill[]> {
  return invoke<HubInstalledSkill[]>('skills_hub_list', { cwd });
}

export async function skillsHubUninstall(name: string): Promise<string> {
  return invoke<string>('skills_hub_uninstall', { name });
}

export async function skillsHubAudit(name: string): Promise<AuditResult> {
  return invoke<AuditResult>('skills_hub_audit', { name });
}

// ---------------------------------------------------------------------------
// Phase 4: Advanced features
// ---------------------------------------------------------------------------

export interface UpdateCheckResult {
  name: string;
  identifier: string;
  source: string;
  status: 'up_to_date' | 'update_available' | 'unavailable';
}

export interface TapEntry {
  repo: string;
  path: string;
}

export async function skillsHubCheckUpdates(): Promise<UpdateCheckResult[]> {
  return invoke<UpdateCheckResult[]>('skills_hub_check_updates');
}

export async function skillsHubTapsList(): Promise<TapEntry[]> {
  return invoke<TapEntry[]>('skills_hub_taps_list');
}

export async function skillsHubTapsAdd(repo: string, path?: string): Promise<boolean> {
  return invoke<boolean>('skills_hub_taps_add', { repo, path });
}

export async function skillsHubTapsRemove(repo: string): Promise<boolean> {
  return invoke<boolean>('skills_hub_taps_remove', { repo });
}

export async function skillsHubSnapshotExport(): Promise<unknown> {
  return invoke<unknown>('skills_hub_snapshot_export');
}

export async function skillsHubSnapshotImport(snapshot: unknown, force?: boolean): Promise<string> {
  return invoke<string>('skills_hub_snapshot_import', { snapshot, force });
}
