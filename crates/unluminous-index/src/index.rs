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
use crate::passages::{self, PassageHit};
use crate::store::{self, Store};
use crate::symbols::{self, SymbolTable};

/// A job for the store thread.
enum StoreJob {
    Changes { changed: Vec<(u32, FileRecord, Vec<Vec<u32>>)>, removed: Vec<u32>, removed_paths: Vec<String>, next_id: usize },
    Full { removed_paths: Vec<String>, changed: Vec<FileRecord> },
    Search { question: String, k: usize, kind: Option<String>, reply: Sender<Result<Vec<PassageHit>, String>> },
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
    /// How many chunks the passage table holds, once it is built.
    pub passages: usize,
    /// Whether the passage table is built, so passage search can answer.
    pub passages_ready: bool,
    /// The index file.
    pub file: PathBuf,
}

/// One checkout's index.
pub struct Index {
    root: PathBuf,
    exact: Arc<RwLock<Option<Exact>>>,
    symbols: Arc<RwLock<Option<SymbolTable>>>,
    freshness: Freshness,
    gate_lock: Mutex<()>,
    last_gate: Mutex<Duration>,
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
        let symbols = Arc::new(RwLock::new(None));
        let (tx, rx) = channel::<StoreJob>();
        let (thread_root, thread_exact, thread_status, thread_symbols) = (root.clone(), Arc::clone(&exact), Arc::clone(&status), Arc::clone(&symbols));
        std::thread::Builder::new()
            .name("unluminous-index-store".into())
            .spawn(move || store_thread(&thread_root, &thread_exact, &thread_symbols, &thread_status, rx))
            .expect("the store thread starts");
        freshness.mark_everything();
        Index { root, exact, symbols, freshness, gate_lock: Mutex::new(()), last_gate: Mutex::new(Duration::ZERO), store: tx, status }
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
        if let Some(table) = self.symbols.write().expect("symbols").as_mut() {
            for path in &report.removed_paths {
                table.remove_file(path);
            }
            for (_, record, _) in &report.changed {
                table.set_file(&record.rel, symbols::definitions_of(record).unwrap_or_default());
            }
        }
        if !report.changed.is_empty() || !report.removed.is_empty() {
            let next_id = exact.files.len();
            let job = if exact.tombstones() > exact.live() / 4 + 1000 {
                StoreJob::Full { removed_paths: report.removed_paths.clone(), changed: report.changed.iter().map(|c| c.1.clone()).collect() }
            } else {
                StoreJob::Changes { changed: report.changed.clone(), removed: report.removed.clone(), removed_paths: report.removed_paths.clone(), next_id }
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
        let gate_started = Instant::now();
        let gated = self.gate();
        *self.last_gate.lock().expect("last gate") = gate_started.elapsed();
        if gated.is_none() {
            return direct::scan(&self.root, request).map(|a| (a, "none"));
        }
        let guard = self.exact.read().expect("exact");
        match guard.as_ref() {
            Some(exact) => exact.search(&self.root, request).map(|a| (a, "trigram")),
            None => direct::scan(&self.root, request).map(|a| (a, "none")),
        }
    }

    /// How long the last exact search's gate took, which `search find` reports beside the search time.
    pub fn last_gate(&self) -> Duration {
        *self.last_gate.lock().expect("last gate")
    }

    /// The chunks that best answer a question, best first, after the gate. Answered on the store thread,
    /// which owns the passage table.
    ///
    /// @param question - the question
    /// @param k - how many to retrieve
    /// @param kind - only chunks of this kind, if given
    pub fn passages(&self, question: &str, k: usize, kind: Option<&str>) -> Result<Vec<PassageHit>, String> {
        if self.gate().is_none() {
            return Err("the index is still being built".into());
        }
        let (tx, rx) = channel();
        self.store.send(StoreJob::Search { question: question.to_owned(), k, kind: kind.map(str::to_owned), reply: tx }).map_err(|_| "the index's store has stopped".to_owned())?;
        rx.recv_timeout(Duration::from_secs(30)).map_err(|_| "the passage search did not answer".to_owned())?
    }

    /// Runs a closure over the symbol table and the exact index, after the gate. None while either is
    /// not built yet.
    ///
    /// @param work - what to do with them
    pub fn with_symbols<T>(&self, work: impl FnOnce(&SymbolTable, &Exact) -> T) -> Option<T> {
        self.gate()?;
        let exact = self.exact.read().expect("exact");
        let symbols = self.symbols.read().expect("symbols");
        Some(work(symbols.as_ref()?, exact.as_ref()?))
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
/// @param symbol_table - where the symbol table built from it goes
/// @param status - the status to update
/// @param jobs - batches of changes from the gates
fn store_thread(root: &Path, exact: &RwLock<Option<Exact>>, symbol_table: &RwLock<Option<SymbolTable>>, status: &Mutex<Status>, jobs: std::sync::mpsc::Receiver<StoreJob>) {
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
    let table = SymbolTable::build(&index);
    *exact.write().expect("exact") = Some(index);
    *symbol_table.write().expect("symbols") = Some(table);
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
    build_passages(&store, exact, status, origin == "built");
    for job in jobs {
        let result = match job {
            StoreJob::Changes { changed, removed, removed_paths, next_id } => {
                let records: Vec<&FileRecord> = changed.iter().map(|c| &c.1).collect();
                store.save_changes(&changed, &removed, next_id).and_then(|()| passages::update(&store.session(), &removed_paths, &records))
            }
            StoreJob::Full { removed_paths, changed } => match exact.read().expect("exact").as_ref() {
                Some(index) => {
                    let records: Vec<&FileRecord> = changed.iter().collect();
                    store.save_exact(index, root).and_then(|()| passages::update(&store.session(), &removed_paths, &records))
                }
                None => Ok(()),
            },
            StoreJob::Search { question, k, kind, reply } => {
                let _ = reply.send(passages::search(&store.session(), &question, k, kind.as_deref()));
                Ok(())
            }
        };
        if let Err(e) = result {
            status.lock().expect("status").store_error = Some(e);
        }
    }
}

/// Builds the passage table when it was never built, was built by another version, or the exact index
/// was just built from scratch; otherwise keeps the one in the file.
///
/// @param store - the store
/// @param exact - the exact index
/// @param status - the status to update
/// @param rebuilt - whether the exact index was just built rather than loaded
fn build_passages(store: &Store, exact: &RwLock<Option<Exact>>, status: &Mutex<Status>, rebuilt: bool) {
    let current = store.meta("passage_version").as_deref() == Some(passages::PASSAGE_VERSION);
    let session = store.session();
    let built = if current && !rebuilt {
        passages::create(&session).and_then(|()| {
            let rows = session.query("SELECT count(*) FROM chunk", &[], 1).map_err(|e| e.to_string())?;
            Ok(match rows.value(0, 0) { Some(inillucent_driver::Value::Integer(n)) => *n as usize, _ => 0 })
        })
    } else {
        let guard = exact.read().expect("exact");
        match guard.as_ref() {
            Some(index) => passages::rebuild(&session, index),
            None => Ok(0),
        }
    };
    let mut st = status.lock().expect("status");
    match built {
        Ok(count) => {
            st.passages = count;
            st.passages_ready = true;
            let _ = store.set_meta("passage_version", passages::PASSAGE_VERSION);
        }
        Err(e) => st.store_error = Some(format!("the passage table: {e}")),
    }
}

/// A Windows path without its `\\?\` prefix, which `canonicalize` adds and other programs refuse.
///
/// @param path - a canonical path
pub fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string())
}
