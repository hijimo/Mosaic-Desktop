import { useState, useEffect, useCallback } from 'react';
import {
  Box, TextField, Button, Table, TableBody, TableCell, TableContainer,
  TableHead, TableRow, Paper, Chip, Typography, CircularProgress,
  Dialog, DialogTitle, DialogContent, DialogActions, Alert, IconButton,
  Tabs, Tab, InputAdornment, Snackbar,
} from '@mui/material';
import { Search, Download, Trash2, Shield, ArrowLeft } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useSkillHubStore } from '@/stores/skillHubStore';
import { skillsHubInspect, skillsHubAudit } from '@/services/tauri/skillsHub';
import type { SkillHubMeta, InstallResult } from '@/services/tauri/skillsHub';

const TRUST_COLORS: Record<string, 'info' | 'success' | 'warning' | 'default'> = {
  builtin: 'info',
  trusted: 'success',
  community: 'warning',
};

type SnackMsg = { text: string; severity: 'success' | 'error' | 'info' };

export function SkillsHubPage(): React.ReactElement {
  const [tab, setTab] = useState(0);
  const [snack, setSnack] = useState<SnackMsg | null>(null);
  const navigate = useNavigate();
  const notify = useCallback((text: string, severity: SnackMsg['severity'] = 'info') => setSnack({ text, severity }), []);

  return (
    <Box sx={{ height: '100%', display: 'flex', flexDirection: 'column', p: 2, gap: 2 }}>
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
        <IconButton size="small" onClick={() => navigate('/')}>
          <ArrowLeft size={18} />
        </IconButton>
        <Typography variant="h6">Skills Hub</Typography>
      </Box>
      <Tabs value={tab} onChange={(_, v) => setTab(v)} sx={{ minHeight: 36 }}>
        <Tab label="搜索市场" sx={{ minHeight: 36, py: 0 }} />
        <Tab label="已安装" sx={{ minHeight: 36, py: 0 }} />
      </Tabs>
      {tab === 0 ? <SearchPanel notify={notify} /> : <InstalledPanel notify={notify} />}
      <Snackbar open={!!snack} autoHideDuration={6000} onClose={() => setSnack(null)} anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}>
        <Alert severity={snack?.severity ?? 'info'} onClose={() => setSnack(null)} sx={{ maxWidth: 500, wordBreak: 'break-word' }}>
          {snack?.text}
        </Alert>
      </Snackbar>
    </Box>
  );
}

// ---------------------------------------------------------------------------
// Search Panel
// ---------------------------------------------------------------------------

function SearchPanel({ notify }: { notify: (text: string, severity?: SnackMsg['severity']) => void }): React.ReactElement {
  const { searchQuery, searchResults, searchLoading, searchError, setSearchQuery, search, loadInstalled } = useSkillHubStore();
  const [inspecting, setInspecting] = useState<SkillHubMeta | null>(null);
  const [installResult, setInstallResult] = useState<InstallResult | null>(null);

  // Load installed list on mount so we can mark installed skills
  useEffect(() => { loadInstalled(); }, [loadInstalled]);

  const handleSearch = useCallback(() => {
    if (searchQuery.trim()) search(searchQuery);
  }, [searchQuery, search]);

  return (
    <>
      <Box sx={{ display: 'flex', gap: 1 }}>
        <TextField
          size="small" fullWidth placeholder="搜索技能（如 kubernetes, react, testing...）"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
          InputProps={{
            startAdornment: <InputAdornment position="start"><Search size={16} /></InputAdornment>,
          }}
        />
        <Button variant="contained" size="small" onClick={handleSearch} disabled={searchLoading}>
          {searchLoading ? <CircularProgress size={18} /> : '搜索'}
        </Button>
      </Box>

      {searchError && <Alert severity="error">{searchError}</Alert>}

      {searchResults.length > 0 && (
        <TableContainer component={Paper} variant="outlined" sx={{ flex: 1, overflow: 'auto' }}>
          <Table size="small" stickyHeader>
            <TableHead>
              <TableRow>
                <TableCell>名称</TableCell>
                <TableCell>描述</TableCell>
                <TableCell>来源</TableCell>
                <TableCell>信任</TableCell>
                <TableCell align="right">操作</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {searchResults.map((skill) => (
                <SearchResultRow
                  key={skill.identifier}
                  skill={skill}
                  onInspect={() => setInspecting(skill)}
                  onInstalled={(r) => setInstallResult(r)}
                  notify={notify}
                />
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      )}

      {inspecting && (
        <InspectDialog skill={inspecting} onClose={() => setInspecting(null)} />
      )}
      {installResult && (
        <InstallResultDialog result={installResult} onClose={() => setInstallResult(null)} />
      )}
    </>
  );
}

function SearchResultRow({ skill, onInspect, onInstalled, notify }: {
  skill: SkillHubMeta;
  onInspect: () => void;
  onInstalled: (r: InstallResult) => void;
  notify: (text: string, severity?: SnackMsg['severity']) => void;
}): React.ReactElement {
  const install = useSkillHubStore((s) => s.install);
  const installing = useSkillHubStore((s) => s.installing);
  const installed = useSkillHubStore((s) => s.installed);
  const isInstalling = installing === skill.identifier;
  const isInstalled = installed.some((s) => s.name === skill.name);

  const handleInstall = async (): Promise<void> => {
    try {
      const result = await install(skill.identifier);
      onInstalled(result);
    } catch (e) {
      notify(String(e), 'error');
    }
  };

  return (
    <TableRow hover sx={{ cursor: 'pointer' }} onClick={onInspect}>
      <TableCell>
        <Typography variant="body2" fontWeight={600}>{skill.name}</Typography>
      </TableCell>
      <TableCell>
        <Typography variant="body2" color="text.secondary" noWrap sx={{ maxWidth: 300 }}>
          {skill.description}
        </Typography>
      </TableCell>
      <TableCell>
        <Typography variant="caption" color="text.secondary">{skill.source}</Typography>
      </TableCell>
      <TableCell>
        <Chip
          label={skill.trustLevel === 'builtin' ? 'official' : skill.trustLevel}
          size="small"
          color={TRUST_COLORS[skill.trustLevel] ?? 'default'}
          variant="outlined"
        />
      </TableCell>
      <TableCell align="right" onClick={(e) => e.stopPropagation()}>
        {isInstalled ? (
          <Chip label="已安装" size="small" color="success" variant="outlined" />
        ) : (
          <Button
            size="small" variant="outlined" startIcon={isInstalling ? <CircularProgress size={14} /> : <Download size={14} />}
            disabled={isInstalling}
            onClick={handleInstall}
          >
            安装
          </Button>
        )}
      </TableCell>
    </TableRow>
  );
}

// ---------------------------------------------------------------------------
// Installed Panel
// ---------------------------------------------------------------------------

function InstalledPanel({ notify }: { notify: (text: string, severity?: SnackMsg['severity']) => void }): React.ReactElement {
  const { installed, installedLoading, loadInstalled, uninstall } = useSkillHubStore();
  const [pendingUninstall, setPendingUninstall] = useState<string | null>(null);

  useEffect(() => { loadInstalled(); }, [loadInstalled]);

  const handleUninstall = async (): Promise<void> => {
    if (!pendingUninstall) return;
    const name = pendingUninstall;
    setPendingUninstall(null);
    try { await uninstall(name); } catch (e) { notify(String(e), 'error'); }
  };

  if (installedLoading) return <Box sx={{ display: 'flex', justifyContent: 'center', py: 4 }}><CircularProgress /></Box>;
  if (installed.length === 0) return <Typography color="text.secondary" sx={{ py: 4, textAlign: 'center' }}>暂无已安装技能</Typography>;

  return (
    <>
    <TableContainer component={Paper} variant="outlined" sx={{ flex: 1, overflow: 'auto' }}>
      <Table size="small" stickyHeader>
        <TableHead>
          <TableRow>
            <TableCell>名称</TableCell>
            <TableCell>描述</TableCell>
            <TableCell>类型</TableCell>
            <TableCell>状态</TableCell>
            <TableCell align="right">操作</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {installed.map((s) => (
            <TableRow key={s.name}>
              <TableCell><Typography variant="body2" fontWeight={600}>{s.name}</Typography></TableCell>
              <TableCell><Typography variant="body2" color="text.secondary" noWrap sx={{ maxWidth: 250 }}>{s.description}</Typography></TableCell>
              <TableCell>
                <Chip
                  label={s.sourceType === 'hub' ? `hub (${s.hubSource ?? ''})` : s.sourceType}
                  size="small" variant="outlined"
                  color={s.sourceType === 'builtin' ? 'info' : s.sourceType === 'hub' ? 'success' : 'default'}
                />
              </TableCell>
              <TableCell>
                {s.scanVerdict && (
                  <Chip label={s.scanVerdict} size="small" variant="outlined"
                    color={s.scanVerdict === 'SAFE' ? 'success' : s.scanVerdict === 'CAUTION' ? 'warning' : 'error'} />
                )}
                {!s.enabled && <Chip label="已禁用" size="small" color="default" sx={{ ml: 0.5 }} />}
              </TableCell>
              <TableCell align="right">
                {s.sourceType === 'hub' && (
                  <>
                    <IconButton size="small" title="重新扫描" onClick={async () => {
                      try { const r = await skillsHubAudit(s.name); notify(r.report, 'info'); } catch (e) { notify(String(e), 'error'); }
                    }}><Shield size={14} /></IconButton>
                    <IconButton size="small" title="卸载" color="error" onClick={() => setPendingUninstall(s.name)}>
                      <Trash2 size={14} />
                    </IconButton>
                  </>
                )}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
    <Dialog open={!!pendingUninstall} onClose={() => setPendingUninstall(null)}>
      <DialogTitle>确认卸载</DialogTitle>
      <DialogContent>确定卸载 {pendingUninstall}？</DialogContent>
      <DialogActions>
        <Button onClick={() => setPendingUninstall(null)}>取消</Button>
        <Button color="error" onClick={handleUninstall}>卸载</Button>
      </DialogActions>
    </Dialog>
    </>
  );
}
// ---------------------------------------------------------------------------

function InspectDialog({ skill, onClose }: { skill: SkillHubMeta; onClose: () => void }): React.ReactElement {
  const [detail, setDetail] = useState<SkillHubMeta | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let stale = false;
    setLoading(true);
    skillsHubInspect(skill.identifier).then((d) => { if (!stale) { setDetail(d ?? skill); setLoading(false); } }).catch(() => { if (!stale) { setDetail(skill); setLoading(false); } });
    return () => { stale = true; };
  }, [skill]);

  const s = detail ?? skill;

  return (
    <Dialog open onClose={onClose} maxWidth="sm" fullWidth>
      <DialogTitle>{s.name}</DialogTitle>
      <DialogContent dividers>
        {loading ? <CircularProgress /> : (
          <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
            <Typography variant="body2"><b>描述：</b>{s.description}</Typography>
            <Typography variant="body2"><b>来源：</b>{s.source}</Typography>
            <Typography variant="body2"><b>标识：</b><code>{s.identifier}</code></Typography>
            <Typography variant="body2"><b>信任等级：</b>
              <Chip label={s.trustLevel} size="small" color={TRUST_COLORS[s.trustLevel] ?? 'default'} variant="outlined" sx={{ ml: 0.5 }} />
            </Typography>
            {s.repo && <Typography variant="body2"><b>仓库：</b>{s.repo}</Typography>}
            {s.tags.length > 0 && (
              <Box sx={{ display: 'flex', gap: 0.5, flexWrap: 'wrap' }}>
                <Typography variant="body2"><b>标签：</b></Typography>
                {s.tags.map((t) => <Chip key={t} label={t} size="small" />)}
              </Box>
            )}
          </Box>
        )}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>关闭</Button>
      </DialogActions>
    </Dialog>
  );
}

function InstallResultDialog({ result, onClose }: { result: InstallResult; onClose: () => void }): React.ReactElement {
  return (
    <Dialog open onClose={onClose} maxWidth="sm" fullWidth>
      <DialogTitle>安装完成 — {result.name}</DialogTitle>
      <DialogContent dividers>
        <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
          <Alert severity={result.scanVerdict === 'SAFE' ? 'success' : result.scanVerdict === 'CAUTION' ? 'warning' : 'error'}>
            扫描结果：{result.scanVerdict}（{result.scanFindings} 个发现）
          </Alert>
          <Typography variant="body2"><b>安装路径：</b>{result.installPath}</Typography>
          <Typography variant="caption" component="pre" sx={{ whiteSpace: 'pre-wrap', bgcolor: 'grey.50', p: 1, borderRadius: 1, maxHeight: 200, overflow: 'auto' }}>
            {result.scanReport}
          </Typography>
        </Box>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} variant="contained">确定</Button>
      </DialogActions>
    </Dialog>
  );
}
