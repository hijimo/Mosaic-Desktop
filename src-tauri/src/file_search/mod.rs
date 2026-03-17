use crossbeam_channel::{Receiver, Sender, after, never, select, unbounded};
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use nucleo::{Config, Injector, Matcher, Nucleo, Utf32String};
use nucleo::pattern::{CaseMatching, Normalization};
use serde::Serialize;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::Duration;

/// A single match result returned from the search.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileMatch {
    pub score: u32,
    pub path: PathBuf,
    pub root: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indices: Option<Vec<u32>>,
}

impl FileMatch {
    pub fn full_path(&self) -> PathBuf {
        self.root.join(&self.path)
    }
}

/// Returns the final path component for a matched path, falling back to the full path.
pub fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

#[derive(Debug)]
pub struct FileSearchResults {
    pub matches: Vec<FileMatch>,
    pub total_match_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct FileSearchSnapshot {
    pub query: String,
    pub matches: Vec<FileMatch>,
    pub total_match_count: usize,
    pub scanned_file_count: usize,
    pub walk_complete: bool,
}

#[derive(Debug, Clone)]
pub struct FileSearchOptions {
    pub limit: NonZero<usize>,
    pub exclude: Vec<String>,
    pub threads: NonZero<usize>,
    pub compute_indices: bool,
    pub respect_gitignore: bool,
}

impl Default for FileSearchOptions {
    fn default() -> Self {
        Self {
            #[allow(clippy::unwrap_used)]
            limit: NonZero::new(20).unwrap(),
            exclude: Vec::new(),
            #[allow(clippy::unwrap_used)]
            threads: NonZero::new(2).unwrap(),
            compute_indices: false,
            respect_gitignore: true,
        }
    }
}

pub trait SessionReporter: Send + Sync + 'static {
    fn on_update(&self, snapshot: &FileSearchSnapshot);
    fn on_complete(&self);
}

pub struct FileSearchSession {
    inner: Arc<SessionInner>,
}

impl FileSearchSession {
    pub fn update_query(&self, pattern_text: &str) {
        let _ = self
            .inner
            .work_tx
            .send(WorkSignal::QueryUpdated(pattern_text.to_string()));
    }
}

impl Drop for FileSearchSession {
    fn drop(&mut self) {
        self.inner.shutdown.store(true, Ordering::Relaxed);
        let _ = self.inner.work_tx.send(WorkSignal::Shutdown);
    }
}

pub fn create_session(
    search_directories: Vec<PathBuf>,
    options: FileSearchOptions,
    reporter: Arc<dyn SessionReporter>,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> anyhow::Result<FileSearchSession> {
    let FileSearchOptions {
        limit,
        exclude,
        threads,
        compute_indices,
        respect_gitignore,
    } = options;

    let Some(primary_search_directory) = search_directories.first() else {
        anyhow::bail!("at least one search directory is required");
    };
    let override_matcher = build_override_matcher(primary_search_directory, &exclude)?;
    let (work_tx, work_rx) = unbounded();

    let notify_tx = work_tx.clone();
    let notify = Arc::new(move || {
        let _ = notify_tx.send(WorkSignal::NucleoNotify);
    });
    let nucleo = Nucleo::new(
        Config::DEFAULT.match_paths(),
        notify,
        Some(threads.get()),
        1,
    );
    let injector = nucleo.injector();
    let cancelled = cancel_flag.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

    let inner = Arc::new(SessionInner {
        search_directories,
        limit: limit.get(),
        threads: threads.get(),
        compute_indices,
        respect_gitignore,
        cancelled,
        shutdown: Arc::new(AtomicBool::new(false)),
        reporter,
        work_tx: work_tx.clone(),
    });

    let matcher_inner = inner.clone();
    thread::spawn(move || matcher_worker(matcher_inner, work_rx, nucleo));

    let walker_inner = inner.clone();
    thread::spawn(move || walker_worker(walker_inner, override_matcher, injector));

    Ok(FileSearchSession { inner })
}

/// Returns a comparator closure that orders items by descending score then ascending path.
pub fn cmp_by_score_desc_then_path_asc<T, FScore, FPath>(
    score_of: FScore,
    path_of: FPath,
) -> impl FnMut(&T, &T) -> std::cmp::Ordering
where
    FScore: Fn(&T) -> u32,
    FPath: Fn(&T) -> &str,
{
    use std::cmp::Ordering;
    move |a, b| match score_of(b).cmp(&score_of(a)) {
        Ordering::Equal => path_of(a).cmp(path_of(b)),
        other => other,
    }
}

/// Synchronous one-shot search.
pub fn run(
    pattern_text: &str,
    roots: Vec<PathBuf>,
    options: FileSearchOptions,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> anyhow::Result<FileSearchResults> {
    let reporter = Arc::new(RunReporter::default());
    let session = create_session(roots, options, reporter.clone(), cancel_flag)?;
    session.update_query(pattern_text);
    let snapshot = reporter.wait_for_complete();
    Ok(FileSearchResults {
        matches: snapshot.matches,
        total_match_count: snapshot.total_match_count,
    })
}

// ── Internals ────────────────────────────────────────────────────

struct SessionInner {
    search_directories: Vec<PathBuf>,
    limit: usize,
    threads: usize,
    compute_indices: bool,
    respect_gitignore: bool,
    cancelled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    reporter: Arc<dyn SessionReporter>,
    work_tx: Sender<WorkSignal>,
}

enum WorkSignal {
    QueryUpdated(String),
    NucleoNotify,
    WalkComplete,
    Shutdown,
}

fn build_override_matcher(
    search_directory: &Path,
    exclude: &[String],
) -> anyhow::Result<Option<ignore::overrides::Override>> {
    if exclude.is_empty() {
        return Ok(None);
    }
    let mut builder = OverrideBuilder::new(search_directory);
    for pat in exclude {
        builder.add(&format!("!{pat}"))?;
    }
    Ok(Some(builder.build()?))
}

fn get_file_path<'a>(path: &'a Path, search_directories: &[PathBuf]) -> Option<(usize, &'a str)> {
    let mut best: Option<(usize, &Path)> = None;
    for (idx, root) in search_directories.iter().enumerate() {
        if let Ok(rel) = path.strip_prefix(root) {
            let depth = root.components().count();
            match best {
                Some((bi, _)) if search_directories[bi].components().count() >= depth => {}
                _ => best = Some((idx, rel)),
            }
        }
    }
    let (root_idx, rel) = best?;
    rel.to_str().map(|p| (root_idx, p))
}

fn walker_worker(
    inner: Arc<SessionInner>,
    override_matcher: Option<ignore::overrides::Override>,
    injector: Injector<Arc<str>>,
) {
    let Some(first_root) = inner.search_directories.first() else {
        let _ = inner.work_tx.send(WorkSignal::WalkComplete);
        return;
    };

    let mut walk_builder = WalkBuilder::new(first_root);
    for root in inner.search_directories.iter().skip(1) {
        walk_builder.add(root);
    }
    walk_builder
        .threads(inner.threads)
        .hidden(false)
        .follow_links(true)
        .require_git(true);
    if !inner.respect_gitignore {
        walk_builder
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .ignore(false)
            .parents(false);
    }
    if let Some(m) = override_matcher {
        walk_builder.overrides(m);
    }

    walk_builder.build_parallel().run(|| {
        const CHECK_INTERVAL: usize = 1024;
        let mut n = 0;
        let dirs = inner.search_directories.clone();
        let inj = injector.clone();
        let cancelled = inner.cancelled.clone();
        let shutdown = inner.shutdown.clone();

        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => return ignore::WalkState::Continue,
            };
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                return ignore::WalkState::Continue;
            }
            let path = entry.path();
            let Some(full_path) = path.to_str() else {
                return ignore::WalkState::Continue;
            };
            if let Some((_, relative)) = get_file_path(path, &dirs) {
                inj.push(Arc::from(full_path), |_, cols| {
                    cols[0] = Utf32String::from(relative);
                });
            }
            n += 1;
            if n >= CHECK_INTERVAL {
                if cancelled.load(Ordering::Relaxed) || shutdown.load(Ordering::Relaxed) {
                    return ignore::WalkState::Quit;
                }
                n = 0;
            }
            ignore::WalkState::Continue
        })
    });
    let _ = inner.work_tx.send(WorkSignal::WalkComplete);
}

fn matcher_worker(
    inner: Arc<SessionInner>,
    work_rx: Receiver<WorkSignal>,
    mut nucleo: Nucleo<Arc<str>>,
) -> anyhow::Result<()> {
    const TICK_MS: u64 = 10;
    let config = Config::DEFAULT.match_paths();
    let mut indices_matcher = inner.compute_indices.then(|| Matcher::new(config.clone()));
    let cancel_requested = || inner.cancelled.load(Ordering::Relaxed);
    let shutdown_requested = || inner.shutdown.load(Ordering::Relaxed);

    let mut last_query = String::new();
    let mut next_notify = never();
    let mut will_notify = false;
    let mut walk_complete = false;

    loop {
        select! {
            recv(work_rx) -> signal => {
                let Ok(signal) = signal else { break };
                match signal {
                    WorkSignal::QueryUpdated(query) => {
                        let append = query.starts_with(&last_query);
                        nucleo.pattern.reparse(0, &query, CaseMatching::Smart, Normalization::Smart, append);
                        last_query = query;
                        will_notify = true;
                        next_notify = after(Duration::from_millis(0));
                    }
                    WorkSignal::NucleoNotify => {
                        if !will_notify {
                            will_notify = true;
                            next_notify = after(Duration::from_millis(TICK_MS));
                        }
                    }
                    WorkSignal::WalkComplete => {
                        walk_complete = true;
                        if !will_notify {
                            will_notify = true;
                            next_notify = after(Duration::from_millis(0));
                        }
                    }
                    WorkSignal::Shutdown => break,
                }
            }
            recv(next_notify) -> _ => {
                will_notify = false;
                let status = nucleo.tick(TICK_MS);
                if status.changed {
                    let snap = nucleo.snapshot();
                    let limit = inner.limit.min(snap.matched_item_count() as usize);
                    let pattern = snap.pattern().column_pattern(0);
                    let matches: Vec<_> = (0..limit as u32).filter_map(|n| {
                        let item = snap.get_matched_item(n)?;
                        let full = item.data.as_ref();
                        let (root_idx, rel) = get_file_path(Path::new(full), &inner.search_directories)?;
                        let indices = if let Some(im) = indices_matcher.as_mut() {
                            let mut v = Vec::<u32>::new();
                            let _ = pattern.indices(item.matcher_columns[0].slice(..), im, &mut v);
                            v.sort_unstable();
                            v.dedup();
                            Some(v)
                        } else {
                            None
                        };
                        Some(FileMatch {
                            score: 0, // nucleo 0.5 doesn't expose per-item score via get_matched_item
                            path: PathBuf::from(rel),
                            root: inner.search_directories[root_idx].clone(),
                            indices,
                        })
                    }).collect();

                    inner.reporter.on_update(&FileSearchSnapshot {
                        query: last_query.clone(),
                        matches,
                        total_match_count: snap.matched_item_count() as usize,
                        scanned_file_count: snap.item_count() as usize,
                        walk_complete,
                    });
                }
                if !status.running && walk_complete {
                    inner.reporter.on_complete();
                }
            }
            default(Duration::from_millis(100)) => {}
        }
        if cancel_requested() || shutdown_requested() {
            break;
        }
    }
    inner.reporter.on_complete();
    Ok(())
}

#[derive(Default)]
struct RunReporter {
    snapshot: RwLock<FileSearchSnapshot>,
    completed: (Condvar, Mutex<bool>),
}

impl SessionReporter for RunReporter {
    fn on_update(&self, snapshot: &FileSearchSnapshot) {
        #[allow(clippy::unwrap_used)]
        let mut guard = self.snapshot.write().unwrap();
        *guard = snapshot.clone();
    }
    fn on_complete(&self) {
        let (cv, mutex) = &self.completed;
        let mut done = mutex.lock().unwrap();
        *done = true;
        cv.notify_all();
    }
}

impl RunReporter {
    fn wait_for_complete(&self) -> FileSearchSnapshot {
        let (cv, mutex) = &self.completed;
        let mut done = mutex.lock().unwrap();
        while !*done {
            done = cv.wait(done).unwrap();
        }
        self.snapshot.read().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_temp_tree(count: usize) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..count {
            fs::write(dir.path().join(format!("file-{i:04}.txt")), format!("contents {i}")).unwrap();
        }
        dir
    }

    #[test]
    fn run_returns_matches_for_query() {
        let dir = create_temp_tree(40);
        let results = run(
            "file-000",
            vec![dir.path().to_path_buf()],
            FileSearchOptions::default(),
            None,
        )
        .expect("run ok");
        assert!(!results.matches.is_empty());
        assert!(results.matches.iter().any(|m| m.path.to_string_lossy().contains("file-0000.txt")));
    }

    #[test]
    fn file_name_from_path_uses_basename() {
        assert_eq!(file_name_from_path("foo/bar.txt"), "bar.txt");
    }

    #[test]
    fn cancel_exits_run() {
        let dir = create_temp_tree(200);
        let cancel = Arc::new(AtomicBool::new(true));
        let results = run("file-", vec![dir.path().to_path_buf()], FileSearchOptions::default(), Some(cancel)).expect("run ok");
        assert_eq!(results.matches, Vec::new());
    }
}
