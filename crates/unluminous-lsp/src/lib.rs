//! A language server client, for completion and signature help. `task-2231` §5.3.
//!
//! `task-1675` refused a language server client for four reasons, and this crate is the answer to each
//! of them that `unluminous-dap` already gave for debugging: a server is a separate program, so it is
//! driven over a protocol on a worker thread with a scripted counterpart in the tests; it is found where
//! the toolchain puts it (`rustup`'s `rust-analyzer` next to `cargo`, the project's own
//! `node_modules/typescript/lib/tsserver.js`) and the window says what was found; it answers when it
//! pleases, so the structural tier answers first and nothing ever waits; and when it is missing or dies
//! the [`ServerState`] says so.
//!
//! **Two adapters behind one [`Server`]**, chosen by the plugin manifest's `language.server`:
//!
//! - [`Adapter::Lsp`] speaks the Language Server Protocol over stdio with `Content-Length` framing,
//!   written by hand on `serde_json` as `unluminous-dap` writes DAP. Positions are UTF-8: the client asks
//!   for `positionEncodings: ["utf-8"]` and rust-analyzer agrees, so a byte offset becomes a line and a
//!   byte column with no UTF-16 arithmetic.
//! - [`Adapter::TsServer`] speaks TypeScript's own protocol (one JSON request a line in, `Content-Length`
//!   framed messages out, `seq` and `request_seq`), which is what WebStorm speaks. Positions are a
//!   1 based line and a 1 based UTF-16 offset, converted at this crate's boundary and nowhere else.
//!
//! **Nothing here blocks the caller.** [`Server::start`] returns at once with the state `Starting` or
//! `Absent`; every request returns a ticket and its answer arrives through [`Server::poll`], after the
//! worker has called the waker it was given, which is how a frame is asked for only when there is
//! something to draw. A reply for an older ticket or an older revision than the newest asked is dropped
//! on arrival, which stands in for cancellation; `$/cancelRequest` is sent as well, cheaply.
//!
//! **The worker keeps its own copy of each open document.** [`Server::sync`] hands it the whole text at
//! a revision, and the worker works out the one change between its copy and the new text (the longest
//! common prefix and suffix) and sends that as an incremental change, so after `didOpen` the server never
//! receives the whole file again. Every position the server sends back is converted to bytes of the text
//! at the revision the request was asked at.
//!
//! **Process rules** (§5.3): one server per language per project root; the caller starts it on the second
//! frame after a file of the language opens, never before the first; a server that has not answered
//! `initialize` in [`INITIALIZE_TIMEOUT`] is `Failed`; a crash restarts it at most [`RESTARTS_AN_HOUR`]
//! times an hour, never while a request is in flight; [`Server::stop`] shuts it down and joins the
//! worker. Nothing is read from the network.
//!
//! No user interface dependency: its tests run with no window. `tests/scripted.rs` drives both adapters
//! against a scripted server; `tests/real.rs` drives the real rust-analyzer and tsserver when they are
//! installed, and fails rather than skips when `UNLUMINOUS_REQUIRE_SERVERS=1`.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub use unluminous_core::completion::Kind;

mod convert;
mod framing;
mod locate;
mod lsp;
mod tsserver;
mod worker;

pub use convert::{byte_of_utf16, byte_of_utf8_column, line_and_utf16, line_and_utf8_column};
pub use locate::find_program;

/// How long a server has to answer `initialize` before it is `Failed`.
pub const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);

/// How many times a crashed server is started again in an hour before it is left `Failed`.
pub const RESTARTS_AN_HOUR: usize = 3;

/// Which protocol a server speaks, named by a plugin manifest's `language.server`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Adapter {
    /// The Language Server Protocol: rust-analyzer.
    Lsp,
    /// TypeScript's own server protocol: tsserver.
    TsServer,
}

impl Adapter {
    /// The adapter a manifest names: `lsp` or `tsserver`.
    pub fn parse(name: &str) -> Option<Adapter> {
        match name {
            "lsp" => Some(Adapter::Lsp),
            "tsserver" => Some(Adapter::TsServer),
            _ => None,
        }
    }

    /// The name a manifest uses.
    pub fn name(self) -> &'static str {
        match self {
            Adapter::Lsp => "lsp",
            Adapter::TsServer => "tsserver",
        }
    }
}

/// What to start, for one language in one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSpec {
    pub adapter: Adapter,
    /// The project folder the server works in: its `rootUri`, or tsserver's working folder.
    pub root: PathBuf,
    /// The program, as [`find_program`] found it.
    pub program: PathBuf,
    /// Its arguments: tsserver's script path and flags, or a manifest's `language.server_args`.
    pub args: Vec<String>,
    /// The name the window and the footer use for the server: `rust-analyzer`, `tsserver`.
    pub label: String,
}

/// What a server is doing, which the footer draws and `status --section servers` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerState {
    /// The process is starting, or has not answered `initialize` yet.
    Starting,
    /// It is reading the project: rust-analyzer's `$/progress`, tsserver's project loading.
    Indexing { message: String, percent: Option<u32> },
    /// It answers.
    Ready,
    /// It died or never answered, with why. The structural tier answers alone.
    Failed(String),
    /// No program was found, with what would install one: `rustup component add rust-analyzer`.
    Absent(String),
}

impl ServerState {
    /// The words the footer and the command line show after the server's name.
    pub fn describe(&self) -> String {
        match self {
            ServerState::Starting => "starting".to_owned(),
            ServerState::Indexing { message, percent: Some(p) } if message.is_empty() => {
                format!("indexing {p}%")
            }
            ServerState::Indexing { message, percent: Some(p) } => {
                format!("indexing {p}% ({message})")
            }
            ServerState::Indexing { message, percent: None } if message.is_empty() => {
                "indexing".to_owned()
            }
            ServerState::Indexing { message, percent: None } => format!("indexing ({message})"),
            ServerState::Ready => "ready".to_owned(),
            ServerState::Failed(why) => format!("stopped: {why}"),
            ServerState::Absent(why) => format!("not found ({why})"),
        }
    }

    /// True when requests are worth sending.
    pub fn answers(&self) -> bool {
        matches!(self, ServerState::Ready | ServerState::Indexing { .. })
    }
}

/// One change to a document's text, in its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: Range<usize>,
    pub text: String,
}

/// How a completion is inserted, when the server says more than its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insertion {
    /// What `Enter` replaces, in bytes of the text at the request's revision.
    pub insert: Range<usize>,
    /// What `Tab` replaces. The same as `insert` when the server gave one range.
    pub replace: Range<usize>,
    /// The text, with any snippet reduced to plain text: `$0`, `$1`, `${1:x}` taken out.
    pub text: String,
    /// Where the caret goes, in bytes into `text`: at `$0`, or at the first `$1` when there is no `$0`.
    /// `None` puts it after the text.
    pub caret: Option<usize>,
}

/// One row a server answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// What the row shows: the name, without the signature rust-analyzer appends.
    pub label: String,
    /// What the stem is matched against: `filterText`, or the label.
    pub filter: String,
    /// The server's own order, smaller first: rust-analyzer's `sortText` read as a number, tsserver's
    /// sort group (`10` locals to `16` auto imports) as a number.
    pub order: u64,
    pub kind: Option<Kind>,
    /// The signature or type: rust-analyzer's `detail` or `labelDetails`, tsserver's `kindModifiers`
    /// and label details.
    pub detail: Option<String>,
    /// Documentation, once resolved.
    pub doc: Option<String>,
    pub deprecated: bool,
    pub preselect: bool,
    /// True when the server says the row has the type expected here: rust-analyzer's `type_match`
    /// relevance, tsserver's `isRecommended`.
    pub expected_type: bool,
    /// The edit, when the server gave one.
    pub insertion: Option<Insertion>,
    /// True when the row is a function whose insertion adds call brackets.
    pub callable: bool,
    /// Edits elsewhere: the import a name needs. Empty until resolved for a server that resolves them.
    pub extra_edits: Vec<TextEdit>,
    /// True when [`Server::resolve`] would add documentation or imports.
    pub needs_resolve: bool,
    /// What accepting the row would import, when it names something not yet in scope: rust-analyzer's
    /// `data.imports` path (`std::borrow::Cow`), tsserver's module `source` (`./cards`).
    pub import: Option<String>,
    /// The server's own data for resolving the row, as JSON text.
    pub handle: String,
}

/// The active signature of a call, from `textDocument/signatureHelp` or tsserver's `signatureHelp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelp {
    /// The whole signature as one line: `draw(&self, ui: &mut Ui) -> Response`.
    pub label: String,
    /// Each parameter's bytes within `label`.
    pub parameters: Vec<Range<usize>>,
    /// Which parameter the caret is in.
    pub active: Option<usize>,
    pub doc: Option<String>,
}

/// What a server's worker hands back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Completions {
        ticket: u64,
        path: PathBuf,
        /// The revision the question was asked at; the items' ranges are bytes of that text.
        revision: u64,
        /// True when typing another letter should ask again rather than filter: rust-analyzer always
        /// says so.
        incomplete: bool,
        items: Vec<Item>,
    },
    Resolved {
        ticket: u64,
        item: Item,
    },
    Signature {
        ticket: u64,
        path: PathBuf,
        revision: u64,
        help: Option<SignatureHelp>,
    },
    State(ServerState),
}

/// How a request was triggered, which LSP's `triggerKind` and tsserver's `triggerKind` carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Asked by hand, or by typing a word.
    Invoked,
    /// A trigger character was typed: `.`, `:`, `?`.
    Character(char),
    /// Another letter of a word whose last answer was incomplete.
    Continued,
}

/// Wakes the window when a reply arrives.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// One running language server: the process, the worker thread that talks to it, and the replies
/// waiting to be taken.
pub struct Server {
    inner: worker::Handle,
}

impl Server {
    /// Starts the server's worker, which starts the program and runs `initialize`. Returns at once.
    ///
    /// @param spec - what to start
    /// @param wake - called whenever a reply is waiting, from the worker thread
    pub fn start(spec: ServerSpec, wake: Waker) -> Server {
        Server { inner: worker::Handle::start(spec, wake) }
    }

    /// A server that was never started, whose state says why: what `find_program` answered when it
    /// found nothing. Requests to it are tickets that are never answered.
    ///
    /// @param label - the server's name
    /// @param why - the state's reason
    pub fn absent(label: &str, why: &str) -> Server {
        Server { inner: worker::Handle::absent(label, why) }
    }

    /// Hands the worker a document's whole text at a revision: `didOpen` the first time, an incremental
    /// change after. The same revision twice sends nothing.
    ///
    /// @param path - the file
    /// @param language - the language id: `rust`, `typescript`, `typescriptreact`, `javascript`
    /// @param revision - the document's text revision
    /// @param text - its text, with `\n` line breaks
    pub fn sync(&mut self, path: &std::path::Path, language: &str, revision: u64, text: Arc<str>) {
        self.inner.sync(path, language, revision, text)
    }

    /// Says a document was closed.
    pub fn close(&mut self, path: &std::path::Path) {
        self.inner.close(path)
    }

    /// Asks for completions at a byte offset of the text last synced at `revision`.
    ///
    /// @param path - the file
    /// @param revision - the revision the offset is in
    /// @param offset - the caret
    /// @param trigger - what triggered it
    pub fn complete(
        &mut self,
        path: &std::path::Path,
        revision: u64,
        offset: usize,
        trigger: Trigger,
    ) -> u64 {
        self.inner.complete(path, revision, offset, trigger)
    }

    /// Asks for a row's documentation and the edits it needs (its import).
    pub fn resolve(&mut self, path: &std::path::Path, item: &Item) -> u64 {
        self.inner.resolve(path, item)
    }

    /// Asks for signature help at a byte offset of the text last synced at `revision`.
    pub fn signature(&mut self, path: &std::path::Path, revision: u64, offset: usize) -> u64 {
        self.inner.signature(path, revision, offset)
    }

    /// What the server is doing now.
    pub fn state(&self) -> ServerState {
        self.inner.state()
    }

    /// The server's name, as the spec gave it.
    pub fn label(&self) -> &str {
        self.inner.label()
    }

    /// Every reply waiting, oldest first. Never blocks.
    pub fn poll(&mut self) -> Vec<Reply> {
        self.inner.poll()
    }

    /// Shuts the server down (`shutdown` and `exit`, or killing it after a grace) and joins the worker.
    pub fn stop(self) {
        self.inner.stop()
    }
}
