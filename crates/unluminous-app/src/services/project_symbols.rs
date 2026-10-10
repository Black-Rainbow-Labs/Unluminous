//! The project's definitions, from the code index (Atrius) held in this process. `task-2231` §5.2.
//!
//! This replaces the table the window used to build on a thread of its own (`services::symbol_index`,
//! a name, a five way kind and a path for each of up to 200,000 definitions, read again whenever a file
//! was saved). Atrius already reads every file the project's search covers, keeps the definitions
//! fresh from its own file watcher, and since `task-2231` knows each definition's container,
//! parameters, return type, field and variable types, visibility and documentation, and every file's
//! imports. So the window opens the index and reads it.
//!
//! **Two ways to open it, and which is the window's choice.** The released window is the checkout's
//! index host when nothing else is (`atrius_index::host::in_process`): its definitions are loaded from
//! the index file on open, so the first keystroke answers from the whole project, and an agent's
//! `search` commands are answered from the same index. When another process is already the host, or
//! when the window was built by a test, the index is held in memory alone (`Index::open_in_memory`),
//! which reads the same files and writes nothing, because a test must not write into the person's own
//! cache and two processes must not write one index file.
//!
//! **Nothing here waits.** [`ProjectSymbols::read`] tries the table's lock and answers `None` while a
//! gate is writing to it, which a keystroke treats as "the project has nothing to add this time"; the
//! table is kept fresh by a thread of Atrius's own (`Index::keep_fresh`), never by a frame.
//!
//! The ownership rule of `task-1675` §3.3 stands: an open tab's definitions come from its live text,
//! and Atrius's copy of an open file is never offered beside it. The callers skip open paths.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use atrius_index::host::Host;
use atrius_index::index::Index;
use atrius_index::symbols::SymbolTable;

/// How often Atrius is asked whether the watcher saw a change, so a file written by another program
/// is in the next keystroke's answer within this long.
const FRESHNESS: Duration = Duration::from_millis(300);

/// The project's index, as the window holds it.
pub struct ProjectSymbols {
    index: Arc<Index>,
    root: PathBuf,
    /// The host, kept so the index lives as long as the window when the window is the host.
    _host: Option<Arc<Host>>,
    /// Whether the project is a git repository, whose `.gitignore` decides what is read.
    repository: bool,
}

impl ProjectSymbols {
    /// Opens the project's index: as its host when `on_disk` and nothing else hosts it, and in memory
    /// otherwise. Cheap: the reading happens on Atrius's own threads.
    ///
    /// @param root - the project folder
    /// @param on_disk - whether the window may be the index host and write the index file
    pub fn open(root: &Path, on_disk: bool) -> ProjectSymbols {
        let canonical = atrius_index::host::canonical(root);
        let host = match on_disk {
            true => atrius_index::host::in_process(&canonical),
            false => None,
        };
        let index = match &host {
            Some(host) => Arc::clone(&host.index),
            None => Index::open_in_memory(&canonical),
        };
        Index::keep_fresh(&index, FRESHNESS);
        let repository = canonical.join(".git").exists();
        ProjectSymbols { index, root: canonical, _host: host, repository }
    }

    /// True until the definitions have been read or loaded.
    pub fn is_building(&self) -> bool {
        !self.index.symbols_ready()
    }

    /// True when the table holds no definitions at all, or is not built yet.
    pub fn is_empty(&self) -> bool {
        self.index.try_symbols(|table| table.is_empty()).unwrap_or(true)
    }

    /// Whether the window is the index host, which `status` reports.
    pub fn is_host(&self) -> bool {
        self._host.is_some()
    }

    /// Runs a closure over the definitions, or answers `None` without waiting while they are being
    /// written or are not built yet.
    ///
    /// @param work - what to do with the table
    pub fn read<T>(&self, work: impl FnOnce(&SymbolTable) -> T) -> Option<T> {
        self.index.try_symbols(work)
    }

    /// A project file's path as the index writes it: relative to the project, with forward slashes.
    /// `None` for a path outside the project.
    ///
    /// @param path - the file
    pub fn relative(&self, path: &Path) -> Option<String> {
        let canonical = atrius_index::host::canonical(path);
        let rest = canonical.strip_prefix(&self.root).ok()?;
        Some(rest.to_string_lossy().replace('\\', "/"))
    }

    /// A path the index wrote, as the file it names.
    ///
    /// @param rel - the path relative to the project
    pub fn absolute(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// The project folder, as the index holds it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether a project file is one the window offers names from.
    ///
    /// Inside a repository the code index reads what ripgrep reads, and the project's own `.gitignore`
    /// has decided. Outside one the window's own rule (`FileTree::all_files`) has always left the three
    /// build folders out (`services::ignore::BUILD_OUTPUT`), so a generated file is never offered beside
    /// the source it was generated from.
    ///
    /// @param rel - the file, relative to the project
    pub fn offers(&self, rel: &str) -> bool {
        self.repository
            || !rel.split('/').any(|part| crate::services::ignore::BUILD_OUTPUT.contains(&part))
    }
}

/// Converts a byte range in a file's text on the disk, which may have `\r\n` line breaks, into the
/// range a `Document`, which holds `\n` alone, has for the same bytes.
///
/// @param text - the file's text as it is on the disk
/// @param range - the range in it
pub fn document_range(text: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let end = range.end.min(text.len());
    let start = range.start.min(end);
    let before_start = text.get(..start).map_or(0, |t| t.matches("\r\n").count());
    let before_end = text.get(..end).map_or(0, |t| t.matches("\r\n").count());
    start - before_start..end - before_end
}
