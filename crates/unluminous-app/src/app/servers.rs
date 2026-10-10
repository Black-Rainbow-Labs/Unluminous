//! The semantic tier of completion: a language server's answer, merged with the structural rows.
//! `task-2231` §5.3, §5.4, §6.1, §6.5 and §6.9.
//!
//! A plugin's manifest names the adapter that answers for its language (`language.server = lsp` for
//! Rust, `tsserver` for TypeScript and JavaScript), and `unluminous_lsp` speaks to it on a worker
//! thread. This module is the window's half:
//!
//! - **When a server runs.** One per adapter per project, started from the second frame on when a
//!   file whose plugin names one is showing and `editor.servers` is `automatic`, never before the
//!   first frame. Where the program is comes from `unluminous_lsp::find_program`; when there is none
//!   the server is `Absent` with what would install one, and the footer says so.
//! - **What it is told.** The showing document's text whenever its revision moves, which the worker
//!   turns into an incremental change.
//! - **What it is asked.** The gatherer asks it about the word at the caret, at most once for each
//!   place and revision, and offers the last answer for the word in the meantime, filtered by the stem
//!   as it grows, which is what every client does while `isIncomplete` answers are outstanding. Nothing
//!   waits: when the answer arrives the waker asks for a frame and the popup is worked out again.
//! - **What a row knows.** A server row is matched on its filter text, ordered by the server's own
//!   order among its equals, and carries the server's insertion and the import edit it resolves to.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use unluminous_core::completion::{self, Candidate, Info, Insert, Source};
use unluminous_lsp::{Adapter, Item, Reply, Server, ServerState, SignatureHelp, Trigger};

use crate::app::UnluminousApp;

/// One running server and what it has said.
pub struct Running {
    pub server: Server,
    /// The last completion answer for each file, with the place it was asked about.
    answers: HashMap<PathBuf, Answer>,
    /// The newest completion asked for each file: the ticket, the revision and the word start.
    asked: HashMap<PathBuf, (u64, u64, usize)>,
    /// The revision of each file the server was last told, so an unchanged file is not sent again.
    synced: HashMap<PathBuf, u64>,
    /// The newest signature help asked, and the answer once it arrives.
    signature: Option<SignatureAsked>,
    /// Resolutions asked, by ticket, with the handle of the row they were asked for.
    resolving: HashMap<u64, String>,
    /// Rows resolved, by handle.
    resolved: HashMap<String, Item>,
}

impl Running {
    /// A server that has been asked nothing yet.
    ///
    /// @param server - the server
    fn new(server: Server) -> Running {
        Running {
            server,
            answers: HashMap::new(),
            asked: HashMap::new(),
            synced: HashMap::new(),
            signature: None,
            resolving: HashMap::new(),
            resolved: HashMap::new(),
        }
    }
}

/// A completion answer, and the word start it was asked for.
#[derive(Debug, Clone)]
struct Answer {
    revision: u64,
    word_start: usize,
    incomplete: bool,
    items: Vec<Item>,
}

/// Signature help asked at a place, and what came back.
#[derive(Debug, Clone)]
struct SignatureAsked {
    ticket: u64,
    path: PathBuf,
    revision: u64,
    offset: usize,
    help: Option<SignatureHelp>,
    answered: bool,
}

/// Every server the window runs, by adapter and project.
#[derive(Default)]
pub struct Servers {
    running: HashMap<(Adapter, PathBuf), Running>,
    /// Whether this window may start servers at all. `UnluminousApp::allow_language_servers`.
    pub(crate) allowed: bool,
    /// A program and its arguments to run as every server in place of the one `find_program` finds.
    /// `UnluminousApp::use_this_language_server`, which is how a test runs a scripted server.
    pub(crate) program: Option<(PathBuf, Vec<String>)>,
}

impl Servers {
    /// Every server and its state, for the footer and `status --section servers`.
    pub fn states(&self) -> Vec<(String, ServerState, PathBuf)> {
        let mut out: Vec<(String, ServerState, PathBuf)> = self
            .running
            .iter()
            .map(|((_, root), r)| (r.server.label().to_owned(), r.server.state(), root.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Shuts every server down and waits for each to stop.
    pub fn stop_everything(&mut self) {
        for (_, running) in self.running.drain() {
            running.server.stop();
        }
    }
}

/// The language id a server is told for a file, from its extension.
///
/// @param path - the file
/// True when a server's import path names the project's own code rather than a dependency: a Rust
/// path through `crate`, `self` or `super`, or a relative module specifier.
///
/// @param path - the path the server would import
fn is_the_projects_own(path: &str) -> bool {
    ["crate::", "self::", "super::", "./", "../"].iter().any(|start| path.starts_with(start))
}

/// The name a server is shown by when no program says otherwise.
///
/// @param adapter - the adapter
fn server_label(adapter: Adapter) -> &'static str {
    match adapter {
        Adapter::Lsp => "rust-analyzer",
        Adapter::TsServer => "tsserver",
    }
}

fn language_id(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or_default() {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "typescriptreact",
        "jsx" => "javascriptreact",
        _ => "javascript",
    }
}

/// The edits a server gave, as completion edits.
///
/// @param edits - the server's edits
fn edits_of(edits: &[unluminous_lsp::TextEdit]) -> Vec<completion::Edit> {
    edits
        .iter()
        .map(|e| completion::Edit { range: e.range.clone(), text: e.text.clone() })
        .collect()
}

/// A server row as a candidate.
///
/// @param item - what the server answered
pub fn candidate_of_item(item: &Item) -> Candidate {
    let insert = match (&item.insertion, item.callable) {
        (Some(i), _) => Insert::Text {
            insert: i.insert.clone(),
            replace: i.replace.clone(),
            text: i.text.clone(),
            caret: i.caret,
        },
        (None, true) => Insert::Call { has_parameters: true },
        (None, false) => Insert::Name,
    };
    // A row the server would import from somewhere the file does not use yet is as far away as a
    // project name the file does not import: the least likely of everything in the list.
    // An import from the project's own code (`crate::`, `./`) is as far as the structure's own
    // needs-import rows; one from a dependency is farther, and a project row of the same name, which
    // the structure offers too, merges into the nearer of the two.
    let locality = match item.import.as_deref() {
        Some(path) if is_the_projects_own(path) => completion::Locality::NeedsImport,
        Some(_) => completion::Locality::Dependency,
        None => completion::Locality::Project,
    };
    let info = Info {
        filter: (item.filter != item.label).then(|| item.filter.clone()),
        signature: item.detail.clone(),
        doc: item.doc.clone(),
        insert,
        extra_edits: edits_of(&item.extra_edits),
        locality,
        expected_type: item.expected_type,
        server_order: Some(item.order),
        deprecated: item.deprecated,
        preselect: item.preselect,
        needs_resolve: item.needs_resolve,
        handle: Some(item.handle.clone()),
        ..Info::default()
    };
    Candidate {
        name: item.label.clone(),
        source: Source::Server,
        kind: item.kind,
        detail: item.import.as_ref().map(|path| format!("use {path}")).unwrap_or_default(),
        info,
    }
}

impl UnluminousApp {
    /// Lets this window start language servers. The released binary calls it, as it calls
    /// `index_on_disk`; a window a test builds starts none unless the test asks, because when a real
    /// server answers is not something a screenshot can know.
    pub fn allow_language_servers(&mut self) {
        self.servers.allowed = true;
    }

    /// Runs this program as every language server, and allows servers. A test uses it to run a
    /// scripted server, which answers the same way on every run.
    ///
    /// @param program - the program
    /// @param args - its arguments
    pub fn use_this_language_server(&mut self, program: PathBuf, args: Vec<String>) {
        self.servers.allowed = true;
        self.servers.program = Some((program, args));
    }

    /// The adapter and root a file's server runs with, when its plugin names one and servers are on.
    ///
    /// @param path - the file
    fn server_for(&self, path: &Path) -> Option<(Adapter, PathBuf)> {
        if !self.servers.allowed || !self.settings.servers.is_automatic() {
            return None;
        }
        let grammar = self.grammar_for(Some(path))?;
        let adapter = Adapter::parse(grammar.completion.server.as_deref()?)?;
        Some((adapter, atrius_index::host::canonical(self.tree.root())))
    }

    /// Starts the server for the file that is showing when it is not running, tells it the file's text
    /// when the text has moved, and takes every reply waiting. Called once a frame from the second on.
    pub(crate) fn keep_the_servers_running(&mut self) {
        self.take_the_server_replies();
        let Some(path) = self.files.active().path().map(Path::to_path_buf) else { return };
        let Some(key) = self.server_for(&path) else { return };
        if !self.servers.running.contains_key(&key) {
            self.start_a_server(&key, &path);
        }
        self.sync_the_showing_document(&key, &path);
    }

    /// Starts one server, or records it as absent with the reason no program was found.
    ///
    /// @param key - the adapter and the project
    /// @param path - the file that asked for it
    fn start_a_server(&mut self, key: &(Adapter, PathBuf), path: &Path) {
        let grammar = self.grammar_for(Some(path)).cloned().unwrap_or_default();
        let keys = &grammar.completion;
        let found = match &self.servers.program {
            Some((program, args)) => Ok(unluminous_lsp::ServerSpec {
                adapter: key.0,
                root: key.1.clone(),
                program: program.clone(),
                args: args.clone(),
                label: server_label(key.0).to_owned(),
            }),
            None => unluminous_lsp::find_program(
                key.0,
                &key.1,
                path,
                keys.server_command.as_deref(),
                &keys.server_args,
            ),
        };
        let server = match found {
            Ok(spec) => Server::start(spec, self.thread_waker()),
            Err(why) => Server::absent(server_label(key.0), &why),
        };
        self.servers.running.insert(key.clone(), Running::new(server));
    }

    /// Tells a server the showing document's text when its revision has moved since it was last told.
    ///
    /// @param key - the server
    /// @param path - the document's file
    fn sync_the_showing_document(&mut self, key: &(Adapter, PathBuf), path: &Path) {
        let revision = self.document().text_revision();
        let Some(running) = self.servers.running.get_mut(key) else { return };
        let state = running.server.state();
        let worth_it = state.answers() || state == ServerState::Starting;
        if running.synced.get(path) == Some(&revision) || !worth_it {
            return;
        }
        let text: Arc<str> = Arc::from(self.files.active().document.text().to_string());
        running.server.sync(path, language_id(path), revision, text);
        running.synced.insert(path.to_path_buf(), revision);
    }

    /// Takes every reply every server has waiting, and works the popup out again when a completion
    /// answer arrived.
    fn take_the_server_replies(&mut self) {
        let mut arrived = false;
        let mut resolved = false;
        for running in self.servers.running.values_mut() {
            for reply in running.server.poll() {
                match reply {
                    Reply::Completions { ticket, path, revision, incomplete, items } => {
                        let Some((newest, _, word_start)) = running.asked.get(&path).copied()
                        else {
                            continue;
                        };
                        if newest != ticket {
                            continue;
                        }
                        let answer = Answer { revision, word_start, incomplete, items };
                        running.answers.insert(path, answer);
                        arrived = true;
                    }
                    Reply::Resolved { ticket, item } => {
                        if let Some(handle) = running.resolving.remove(&ticket) {
                            running.resolved.insert(handle, item);
                            resolved = true;
                        }
                    }
                    Reply::Signature { ticket, help, .. } => {
                        if let Some(asked) =
                            running.signature.as_mut().filter(|s| s.ticket == ticket)
                        {
                            asked.help = help;
                            asked.answered = true;
                        }
                    }
                    Reply::State(_) => {}
                }
            }
        }
        if resolved {
            self.fill_in_the_resolved_rows();
        }
        if arrived {
            self.server_completions_arrived();
        }
    }

    /// The running server for the file that is showing, when there is one that answers.
    fn the_showing_server(&mut self) -> Option<(&mut Running, PathBuf)> {
        let path = self.files.active().path().map(Path::to_path_buf)?;
        let key = self.server_for(&path)?;
        let running = self.servers.running.get_mut(&key)?;
        running.server.state().answers().then_some((running, path))
    }

    /// What the language server has answered for the word at `offset`, as candidates, asking it when
    /// it has not been asked about this place at this revision. Empty while no server answers for the
    /// language, and for a hypothetical stem, which no server can be asked about.
    ///
    /// @param stem - what has been typed
    /// @param offset - the caret
    pub(crate) fn server_candidates(&mut self, stem: &str, offset: usize) -> Vec<Candidate> {
        if self.asking_hypothetically {
            return Vec::new();
        }
        let word_start = offset.saturating_sub(stem.len());
        let revision = self.document().text_revision();
        // The one byte before the word, when it starts a character: a trigger is `.`, `:` or `>`,
        // each one byte, and the byte before the word can be the middle of a letter such as `é`.
        let before_start = word_start.saturating_sub(1);
        let text = self.document().text();
        let before = match word_start > 0 && text.is_char_boundary(before_start) {
            true => text.byte_slice(before_start..word_start),
            false => String::new(),
        };
        let Some(path) = self.files.active().path().map(Path::to_path_buf) else {
            return Vec::new();
        };
        if let Some(key) = self.server_for(&path) {
            self.sync_the_showing_document(&key, &path);
        }
        let Some((running, path)) = self.the_showing_server() else { return Vec::new() };
        let asked_here =
            running.asked.get(&path).is_some_and(|(_, r, w)| *r == revision && *w == word_start);
        let previous = running.answers.get(&path).filter(|a| a.word_start == word_start).cloned();
        if !asked_here {
            let continued = previous.as_ref().is_some_and(|a| a.incomplete);
            let trigger = match before.chars().last() {
                Some(c @ ('.' | ':' | '>')) if stem.is_empty() => Trigger::Character(c),
                _ if continued => Trigger::Continued,
                _ => Trigger::Invoked,
            };
            let ticket = running.server.complete(&path, revision, offset, trigger);
            running.asked.insert(path.clone(), (ticket, revision, word_start));
        }
        let Some(answer) = previous else { return Vec::new() };
        answer
            .items
            .iter()
            .map(|item| {
                let mut c = candidate_of_item(item);
                if let Some(resolved) = running.resolved.get(&item.handle) {
                    c.info.doc = resolved.doc.clone().or(c.info.doc);
                    c.info.extra_edits = edits_of(&resolved.extra_edits);
                    c.info.needs_resolve = false;
                }
                c
            })
            .collect()
    }

    /// True while the server for the showing file has been asked about this revision and has not
    /// answered, which is what `editor complete --wait` waits for.
    pub(crate) fn server_is_being_asked(&mut self) -> bool {
        let revision = self.document().text_revision();
        let Some((running, path)) = self.the_showing_server() else { return false };
        let Some((_, asked_at, word_start)) = running.asked.get(&path).copied() else {
            return false;
        };
        let answered = running
            .answers
            .get(&path)
            .is_some_and(|a| a.word_start == word_start && a.revision == revision);
        asked_at == revision && !answered
    }

    /// A server's answer arrived: work the open popup out again with its rows in it, or open the popup
    /// with them when the person typed a trigger character and nothing else had anything to offer.
    fn server_completions_arrived(&mut self) {
        if let Some(state) = self.completion.as_mut() {
            // A revision no edit produces, so the next refresh works the rows out rather than deciding
            // nothing moved.
            state.revision = u64::MAX;
            self.keep_the_completion_fresh(false);
            return;
        }
        if std::mem::take(&mut self.awaiting_a_server_popup)
            && self.settings.suggestions.is_automatic()
        {
            let head = self.document().selection().head;
            self.open_the_completion_if_anything(head);
        }
    }

    /// Asks the server for the chosen row's documentation and import, once, when the row came from a
    /// server that resolves its rows.
    pub(crate) fn resolve_the_chosen_completion(&mut self) {
        let Some(row) = self.completion.as_ref().and_then(|s| s.chosen_row()).cloned() else {
            return;
        };
        if row.source != Source::Server || !row.info.needs_resolve {
            return;
        }
        let Some(handle) = row.info.handle.clone() else { return };
        let Some((running, path)) = self.the_showing_server() else { return };
        if running.resolved.contains_key(&handle)
            || running.resolving.values().any(|h| *h == handle)
        {
            return;
        }
        let item = running
            .answers
            .get(&path)
            .and_then(|a| a.items.iter().find(|i| i.handle == handle))
            .cloned();
        let Some(item) = item else { return };
        let ticket = running.server.resolve(&path, &item);
        running.resolving.insert(ticket, handle);
    }

    /// Puts what a server resolved into the rows the popup holds.
    fn fill_in_the_resolved_rows(&mut self) {
        let resolved: HashMap<String, Item> = self
            .servers
            .running
            .values()
            .flat_map(|r| r.resolved.iter().map(|(h, i)| (h.clone(), i.clone())))
            .collect();
        let Some(state) = self.completion.as_mut() else { return };
        for row in &mut state.rows {
            let Some(item) = row.info.handle.as_ref().and_then(|h| resolved.get(h)) else {
                continue;
            };
            row.info.doc = item.doc.clone().or(row.info.doc.take());
            row.info.extra_edits = edits_of(&item.extra_edits);
            row.info.needs_resolve = false;
        }
    }

    /// Asks for signature help at the caret, when the showing file's server answers.
    pub(crate) fn ask_for_signature_help(&mut self) {
        let offset = self.document().selection().head;
        let revision = self.document().text_revision();
        if let Some(path) = self.files.active().path().map(Path::to_path_buf) {
            if let Some(key) = self.server_for(&path) {
                self.sync_the_showing_document(&key, &path);
            }
        }
        let Some((running, path)) = self.the_showing_server() else { return };
        let same = running
            .signature
            .as_ref()
            .is_some_and(|s| s.path == path && s.revision == revision && s.offset == offset);
        if same {
            return;
        }
        let ticket = running.server.signature(&path, revision, offset);
        let asked = SignatureAsked { ticket, path, revision, offset, help: None, answered: false };
        running.signature = Some(asked);
    }

    /// The signature help the showing file's server answered for the caret: whether it has answered,
    /// and what. `None` when it was not asked about this place.
    pub(crate) fn server_signature(&mut self) -> Option<(bool, Option<SignatureHelp>)> {
        let offset = self.document().selection().head;
        let revision = self.document().text_revision();
        let (running, path) = self.the_showing_server()?;
        let asked = running.signature.as_ref()?;
        (asked.path == path && asked.revision == revision && asked.offset == offset)
            .then(|| (asked.answered, asked.help.clone()))
    }

    /// Every server and what it is doing, for the footer and the command line.
    pub fn server_states(&self) -> Vec<(String, ServerState, PathBuf)> {
        self.servers.states()
    }

    /// The state of the server that answers for the showing file, for the footer.
    pub(crate) fn showing_server_state(&self) -> Option<(String, ServerState)> {
        let path = self.files.active().path()?;
        let key = self.server_for(path)?;
        let running = self.servers.running.get(&key)?;
        Some((running.server.label().to_owned(), running.server.state()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_row_keeps_its_filter_order_and_insertion() {
        let item = Item {
            label: "caret_at".to_owned(),
            filter: "caret_at".to_owned(),
            order: 7,
            kind: Some(completion::Kind::Method),
            detail: Some("fn(&self, offset: usize) -> Caret".to_owned()),
            doc: None,
            deprecated: false,
            preselect: false,
            expected_type: true,
            insertion: Some(unluminous_lsp::Insertion {
                insert: 10..12,
                replace: 10..15,
                text: "caret_at()".to_owned(),
                caret: Some(9),
            }),
            callable: true,
            extra_edits: Vec::new(),
            needs_resolve: true,
            import: None,
            handle: "{}".to_owned(),
        };
        let c = candidate_of_item(&item);
        assert_eq!(c.source, Source::Server);
        assert_eq!(c.info.server_order, Some(7));
        assert!(c.info.expected_type);
        assert_eq!(c.info.filter, None, "a filter equal to the label is not kept twice");
        assert!(matches!(c.info.insert, Insert::Text { caret: Some(9), .. }));
    }

    #[test]
    fn a_files_language_id_is_read_off_its_extension() {
        assert_eq!(language_id(Path::new("a.rs")), "rust");
        assert_eq!(language_id(Path::new("a.tsx")), "typescriptreact");
        assert_eq!(language_id(Path::new("a.ts")), "typescript");
        assert_eq!(language_id(Path::new("a.js")), "javascript");
    }
}
