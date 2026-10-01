//! One checkout's index, as the host holds it: the in-memory engines, the watcher, and the store thread.
//!
//! Opening an index starts the watcher first, so nothing written while the index loads is missed, then
//! loads the stored index (or builds one) on the store's own thread. Until that finishes, a query is
//! answered by the direct scan and says so with `"index":"none"`. After it, the first query's gate
//! reconciles the whole tree by size and time, because the files may have changed while no host was
//! running.
//!
//! The store's `Database` cannot leave the thread that opened it, so that thread owns it for the life of
//! the index and is sent each batch of changes the gates made, written in one short transaction.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::direct;
use crate::exact::{Exact, ExactAnswer, ExactRequest, FileRecord};
use crate::freshness::{Freshness, GateReport};
use crate::store::{self, Store};

/// A job for the store thread.
enum StoreJob {
    Changes { changed: Vec<(u32, FileRecord, Vec<u32>)>, removed: Vec<u32>, next_id: usize },
    Full,
}

/// What `search status` reports.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// Whether the index is loaded and answering, rather than the direct scan.
    pub ready: bool,
    /// How the index came to be: `loaded`, `built`, or `building`.
    pub origin: String,
    /// How long loading or building took.
    pub load_ms: u128,
    /// How many times a gate ran.
    pub gates: u64,
    /// How many files gates have read again.
    pub reindexed: u64,
    /// The last store error, if any.
    pub store_error: Option<String>,
    /// The index file.
    pub file: PathBuf,
}

/// One checkout's index.
pub struct Index {
    root: PathBuf,
    exact: Arc<RwLock<Option<Exact>>>,
    freshness: Freshness,
    gate_lock: Mutex<()>,
    store: Sender<StoreJob>,
    status: Arc<Mutex<Status>>,
}

impl Index {
    /// Opens the index of a root: starts the watcher, then loads or builds on the store thread. Once it
    /// is ready, a background gate reconciles the whole tree, so the first query does not pay for it.
    ///
    /// @param root - the checkout's root
    pub fn open(root: &Path) -> Arc<Index> {
        let index = Arc::new(Index::open_quietly(root));
        let warm = Arc::clone(&index);
        std::thread::Builder::new()
            .name("unluminous-index-warm".into())
            .spawn(move || {
                if warm.wait_ready(Duration::from_secs(3600)) {
                    warm.gate();
                }
            })
            .expect("the warm up thread starts");
        index
    }

    /// Opens the index without the background reconcile; the first gate does it instead.
    ///
    /// @param root - the checkout's root
    pub fn open_quietly(root: &Path) -> Index {
        let root = std::fs::canonicalize(root).map(|p| strip_verbatim(&p)).unwrap_or_else(|_| root.to_path_buf());
        let freshness = Freshness::start(&root);
        let exact = Arc::new(RwLock::new(None));
        let status = Arc::new(Mutex::new(Status { origin: "building".into(), file: store::index_folder(&root).join("index.rdb"), ..Status::default() }));
        let (tx, rx) = channel::<StoreJob>();
        let (thread_root, thread_exact, thread_status) = (root.clone(), Arc::clone(&exact), Arc::clone(&status));
        std::thread::Builder::new()
            .name("unluminous-index-store".into())
            .spawn(move || store_thread(&thread_root, &thread_exact, &thread_status, rx))
            .expect("the store thread starts");
        freshness.mark_everything();
        Index { root, exact, freshness, gate_lock: Mutex::new(()), store: tx, status }
    }

    /// The root this index covers.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A copy of the status.
    pub fn status(&self) -> Status {
        self.status.lock().expect("status").clone()
    }

    /// Blocks until the index is loaded or built, or the timeout passes. For tests and `search build`.
    ///
    /// @param timeout - how long to wait
    pub fn wait_ready(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.status().ready {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Runs the gate, so the index agrees with the files as they are now, and hands what changed to the
    /// store thread. Returns None while the index is not loaded yet.
    pub fn gate(&self) -> Option<GateReport> {
        let _one_at_a_time = self.gate_lock.lock().expect("gate");
        let mut guard = self.exact.write().expect("exact");
        let exact = guard.as_mut()?;
        let report = self.freshness.gate(exact);
        if !report.changed.is_empty() || !report.removed.is_empty() {
            let next_id = exact.files.len();
            let job = if exact.tombstones() > exact.live() / 4 + 1000 { StoreJob::Full } else {
                StoreJob::Changes { changed: report.changed.clone(), removed: report.removed.clone(), next_id }
            };
            let _ = self.store.send(job);
        }
        let mut status = self.status.lock().expect("status");
        status.gates += 1;
        status.reindexed += report.changed.len() as u64;
        Some(report)
    }

    /// An exact search: through the gate and the index when it is ready, by direct scan before then.
    /// The second value says which answered: `"trigram"` or `"none"`.
    ///
    /// @param request - the pattern, case and scope
    pub fn exact(&self, request: &ExactRequest) -> Result<(ExactAnswer, &'static str), String> {
        if self.gate().is_none() {
            return direct::scan(&self.root, request).map(|a| (a, "none"));
        }
        let guard = self.exact.read().expect("exact");
        match guard.as_ref() {
            Some(exact) => exact.search(&self.root, request).map(|a| (a, "trigram")),
            None => direct::scan(&self.root, request).map(|a| (a, "none")),
        }
    }

    /// Runs a closure over the loaded exact index, after the gate. None while it is not loaded.
    ///
    /// @param work - what to do with the index
    pub fn with_exact<T>(&self, work: impl FnOnce(&Exact) -> T) -> Option<T> {
        self.gate()?;
        let guard = self.exact.read().expect("exact");
        guard.as_ref().map(work)
    }
}

/// The store thread: opens the file, loads or builds the index, then writes each batch of changes.
///
/// @param root - the root
/// @param exact - where the loaded index goes
/// @param status - the status to update
/// @param jobs - batches of changes from the gates
fn store_thread(root: &Path, exact: &RwLock<Option<Exact>>, status: &Mutex<Status>, jobs: std::sync::mpsc::Receiver<StoreJob>) {
    let started = Instant::now();
    let store = Store::open(&store::index_folder(root));
    let (index, origin) = match &store {
        Ok(s) if s.exact_is_current() => match s.load_exact() {
            Ok(loaded) => (loaded, "loaded"),
            Err(_) => (Exact::build(root), "built"),
        },
        _ => (Exact::build(root), "built"),
    };
    if origin == "built" {
        if let Ok(s) = &store {
            if let Err(e) = s.save_exact(&index, root) {
                status.lock().expect("status").store_error = Some(e);
            }
        }
    }
    *exact.write().expect("exact") = Some(index);
    {
        let mut st = status.lock().expect("status");
        st.ready = true;
        st.origin = origin.into();
        st.load_ms = started.elapsed().as_millis();
        if let Err(e) = &store {
            st.store_error = Some(e.clone());
        }
    }
    let Ok(store) = store else { return };
    for job in jobs {
        let result = match job {
            StoreJob::Changes { changed, removed, next_id } => store.save_changes(&changed, &removed, next_id),
            StoreJob::Full => match exact.read().expect("exact").as_ref() {
                Some(index) => store.save_exact(index, root),
                None => Ok(()),
            },
        };
        if let Err(e) = result {
            status.lock().expect("status").store_error = Some(e);
        }
    }
}

/// A Windows path without its `\\?\` prefix, which `canonicalize` adds and other programs refuse.
///
/// @param path - a canonical path
pub fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string())
}
