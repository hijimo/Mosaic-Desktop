//! File watcher that monitors directories for changes and broadcasts events.
//!
//! Uses `notify` for cross-platform filesystem notifications with throttling
//! to coalesce bursts of changes into a single event per interval.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{sleep_until, Instant};
use tracing::warn;

/// Events emitted by the file watcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileWatcherEvent {
    /// One or more files under watched roots changed.
    Changed { paths: Vec<PathBuf> },
}

const THROTTLE_INTERVAL: Duration = Duration::from_secs(10);

// ── Throttle helper ──────────────────────────────────────────────

struct ThrottledPaths {
    pending: HashSet<PathBuf>,
    next_allowed: Instant,
}

impl ThrottledPaths {
    fn new(now: Instant) -> Self {
        Self {
            pending: HashSet::new(),
            next_allowed: now,
        }
    }

    fn add(&mut self, paths: Vec<PathBuf>) {
        self.pending.extend(paths);
    }

    fn take_ready(&mut self, now: Instant) -> Option<Vec<PathBuf>> {
        if self.pending.is_empty() || now < self.next_allowed {
            return None;
        }
        Some(self.drain(now))
    }

    fn take_pending(&mut self, now: Instant) -> Option<Vec<PathBuf>> {
        if self.pending.is_empty() {
            return None;
        }
        Some(self.drain(now))
    }

    fn next_deadline(&self, now: Instant) -> Option<Instant> {
        (!self.pending.is_empty() && now < self.next_allowed).then_some(self.next_allowed)
    }

    fn drain(&mut self, now: Instant) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.pending.drain().collect();
        paths.sort_unstable();
        self.next_allowed = now + THROTTLE_INTERVAL;
        paths
    }
}

// ── FileWatcher ──────────────────────────────────────────────────

struct WatcherInner {
    watcher: RecommendedWatcher,
    watched: HashMap<PathBuf, RecursiveMode>,
}

struct WatchState {
    root_ref_counts: HashMap<PathBuf, usize>,
}

/// Watches directories for file changes and broadcasts coarse-grained events.
pub struct FileWatcher {
    inner: Option<Mutex<WatcherInner>>,
    state: Arc<RwLock<WatchState>>,
    tx: broadcast::Sender<FileWatcherEvent>,
}

/// RAII guard that unregisters watched roots on drop.
pub struct WatchRegistration {
    watcher: std::sync::Weak<FileWatcher>,
    roots: Vec<PathBuf>,
}

impl Drop for WatchRegistration {
    fn drop(&mut self) {
        if let Some(fw) = self.watcher.upgrade() {
            fw.unregister_roots(&self.roots);
        }
    }
}

impl FileWatcher {
    /// Create a new file watcher backed by the OS notification system.
    pub fn new() -> notify::Result<Self> {
        let (raw_tx, raw_rx) = mpsc::unbounded_channel();
        let sender = raw_tx;
        let watcher = notify::recommended_watcher(move |res| {
            let _ = sender.send(res);
        })?;
        let inner = WatcherInner {
            watcher,
            watched: HashMap::new(),
        };
        let (tx, _) = broadcast::channel(128);
        let state = Arc::new(RwLock::new(WatchState {
            root_ref_counts: HashMap::new(),
        }));
        let fw = Self {
            inner: Some(Mutex::new(inner)),
            state: Arc::clone(&state),
            tx: tx.clone(),
        };
        fw.spawn_event_loop(raw_rx, state, tx);
        Ok(fw)
    }

    /// Create a no-op watcher (for tests or when FS watching is unavailable).
    pub fn noop() -> Self {
        let (tx, _) = broadcast::channel(1);
        Self {
            inner: None,
            state: Arc::new(RwLock::new(WatchState {
                root_ref_counts: HashMap::new(),
            })),
            tx,
        }
    }

    /// Subscribe to file change events.
    pub fn subscribe(&self) -> broadcast::Receiver<FileWatcherEvent> {
        self.tx.subscribe()
    }

    /// Register a directory root for recursive watching.
    pub fn register_root(self: &Arc<Self>, root: PathBuf) -> WatchRegistration {
        self.add_root(root.clone());
        WatchRegistration {
            watcher: Arc::downgrade(self),
            roots: vec![root],
        }
    }

    /// Register multiple roots at once.
    pub fn register_roots(self: &Arc<Self>, roots: Vec<PathBuf>) -> WatchRegistration {
        for r in &roots {
            self.add_root(r.clone());
        }
        WatchRegistration {
            watcher: Arc::downgrade(self),
            roots,
        }
    }

    fn add_root(&self, root: PathBuf) {
        let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
        let count = state.root_ref_counts.entry(root.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            self.watch_path(&root, RecursiveMode::Recursive);
        }
    }

    fn unregister_roots(&self, roots: &[PathBuf]) {
        let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
        for root in roots {
            let should_unwatch = if let Some(c) = state.root_ref_counts.get_mut(root) {
                if *c > 1 {
                    *c -= 1;
                    false
                } else {
                    state.root_ref_counts.remove(root);
                    true
                }
            } else {
                false
            };
            if should_unwatch {
                if let Some(inner) = &self.inner {
                    let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                    if guard.watched.remove(root).is_some() {
                        let _ = guard.watcher.unwatch(root);
                    }
                }
            }
        }
    }

    fn watch_path(&self, path: &Path, mode: RecursiveMode) {
        let Some(inner) = &self.inner else { return };
        if !path.exists() {
            return;
        }
        let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard.watched.contains_key(path) {
            return;
        }
        if let Err(e) = guard.watcher.watch(path, mode) {
            warn!("failed to watch {}: {e}", path.display());
            return;
        }
        guard.watched.insert(path.to_path_buf(), mode);
    }

    fn spawn_event_loop(
        &self,
        mut raw_rx: mpsc::UnboundedReceiver<notify::Result<Event>>,
        state: Arc<RwLock<WatchState>>,
        tx: broadcast::Sender<FileWatcherEvent>,
    ) {
        if let Ok(handle) = Handle::try_current() {
            handle.spawn(async move {
                let now = Instant::now();
                let mut throttled = ThrottledPaths::new(now);

                loop {
                    let deadline = throttled
                        .next_deadline(Instant::now())
                        .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400));
                    let timer = sleep_until(deadline);
                    tokio::pin!(timer);

                    tokio::select! {
                        res = raw_rx.recv() => {
                            match res {
                                Some(Ok(event)) => {
                                    let paths = classify(&event, &state);
                                    throttled.add(paths);
                                    if let Some(p) = throttled.take_ready(Instant::now()) {
                                        let _ = tx.send(FileWatcherEvent::Changed { paths: p });
                                    }
                                }
                                Some(Err(e)) => warn!("file watcher error: {e}"),
                                None => {
                                    if let Some(p) = throttled.take_pending(Instant::now()) {
                                        let _ = tx.send(FileWatcherEvent::Changed { paths: p });
                                    }
                                    break;
                                }
                            }
                        }
                        _ = &mut timer => {
                            if let Some(p) = throttled.take_ready(Instant::now()) {
                                let _ = tx.send(FileWatcherEvent::Changed { paths: p });
                            }
                        }
                    }
                }
            });
        }
    }
}

fn classify(event: &Event, state: &RwLock<WatchState>) -> Vec<PathBuf> {
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return Vec::new();
    }
    let roots: HashSet<PathBuf> = state
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .root_ref_counts
        .keys()
        .cloned()
        .collect();

    event
        .paths
        .iter()
        .filter(|p| roots.iter().any(|r| p.starts_with(r)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_coalesces() {
        let start = Instant::now();
        let mut t = ThrottledPaths::new(start);
        t.add(vec![PathBuf::from("a")]);
        let first = t.take_ready(start).unwrap();
        assert_eq!(first, vec![PathBuf::from("a")]);

        t.add(vec![PathBuf::from("b"), PathBuf::from("c")]);
        assert!(t.take_ready(start).is_none()); // throttled

        let later = start + THROTTLE_INTERVAL;
        let second = t.take_ready(later).unwrap();
        assert_eq!(second, vec![PathBuf::from("b"), PathBuf::from("c")]);
    }

    #[test]
    fn flush_pending_on_shutdown() {
        let start = Instant::now();
        let mut t = ThrottledPaths::new(start);
        t.add(vec![PathBuf::from("a")]);
        let _ = t.take_ready(start);
        t.add(vec![PathBuf::from("b")]);
        assert!(t.take_ready(start).is_none());
        let flushed = t.take_pending(start).unwrap();
        assert_eq!(flushed, vec![PathBuf::from("b")]);
    }

    #[test]
    fn noop_watcher_subscribes() {
        let fw = FileWatcher::noop();
        let _rx = fw.subscribe();
    }

    #[test]
    fn register_dedupes() {
        let fw = Arc::new(FileWatcher::noop());
        fw.add_root(PathBuf::from("/tmp/a"));
        fw.add_root(PathBuf::from("/tmp/a"));
        fw.add_root(PathBuf::from("/tmp/b"));
        let state = fw.state.read().unwrap();
        assert_eq!(state.root_ref_counts.len(), 2);
        assert_eq!(state.root_ref_counts[&PathBuf::from("/tmp/a")], 2);
    }

    #[test]
    fn drop_registration_unregisters() {
        let fw = Arc::new(FileWatcher::noop());
        let reg = fw.register_root(PathBuf::from("/tmp/x"));
        assert_eq!(fw.state.read().unwrap().root_ref_counts.len(), 1);
        drop(reg);
        assert_eq!(fw.state.read().unwrap().root_ref_counts.len(), 0);
    }
}
