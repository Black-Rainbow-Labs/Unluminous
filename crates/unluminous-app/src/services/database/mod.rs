//! The Database plugin's own state: the data sources, the connections, the tree and the pages.
//!
//! `crates/unluminous-db` is the half that talks to a database and has never heard of a window; this is the
//! half that has never heard of a wire protocol. Between them is `unluminous_db::Worker`, a connection on a
//! thread of its own, so nothing here is ever called from inside a frame while a query is running.
//!
//! `tasks/task-1777-database-plugin-tdd.md` is the design and
//! `_agent_output/task-1777-database-plugin/the reference editor-research.md` is what it was measured against.
//!
//! ## Three things worth knowing before changing anything here
//!
//! **A reply is routed by its ticket.** `Worker::take` drains the channel, so every answer has to be
//! looked up in [`Connection::waiting`] and given to the thing that asked for it. A caller that threw
//! away the answers it was not waiting for would lose the second of two outstanding queries, which is
//! a fault `unluminous-db`'s own tests found before this file existed.
//!
//! **Introspection is lazy and one level at a time.** Opening a data source asks for its schemas,
//! opening a schema asks for its items, opening a table asks for its columns. A database with four
//! thousand tables in it is the reason.
//!
//! **A row can only be changed if it can be addressed**, and this file never works around that. The
//! grid's Add, Delete and Submit are absent when `catalog::Table::can_be_changed` is false, and
//! `unluminous_db::Pending::statements` refuses to compose a statement for such a table even if something
//! here asked it to.

pub mod config;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use unluminous_db::catalog::{Item, Kind, Table};
use unluminous_db::edit::{Pending, Row};
use unluminous_db::rows::{Failure, Rows};
use unluminous_db::source::{Engine, Source};
use unluminous_db::value::Value;
use unluminous_db::worker::{Job, Reply, Worker};

use crate::services::plugin_ui::{Answer, Context, Look, Request, UiProvider};

pub use config::{password_for, Configuration};

/// One connected data source.
pub struct Connection {
    pub worker: Worker,
    /// What each outstanding ticket was asked for.
    waiting: BTreeMap<u64, Wanted>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Connection")
            .field("version", &self.worker.version)
            .field("waiting", &self.waiting.len())
            .finish()
    }
}

/// What an outstanding job was for, so its answer can be given to the right thing.
#[derive(Debug, Clone)]
enum Wanted {
    Schemas,
    Items {
        schema: String,
    },
    /// A table's columns, and what to do once they have arrived.
    Describe {
        schema: String,
        name: String,
        then: Then,
    },
    /// A `CREATE` statement. `show` is true when a person pressed the button, and false when an
    /// agent asked for the text — a command must not put a modal in front of somebody.
    Ddl {
        name: String,
        show: bool,
    },
    Rows {
        page: u64,
    },
    /// A table a console statement reads, described so the console can show it as that table's grid.
    ConsoleTable {
        page: u64,
    },
    Written {
        page: u64,
    },
    /// What a search table declared about itself, asked for as soon as one is seen in a schema.
    Search {
        name: String,
    },
    Nothing,
}

/// Why a table was described.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Then {
    /// Fill in the tree's column rows.
    ShowColumns,
    /// Open a grid on it, which needs the key before the first statement can be written.
    OpenGrid,
}

/// What has been introspected for one data source.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    pub schemas: Vec<String>,
    /// The items of each schema that has been opened.
    pub items: BTreeMap<String, Vec<Item>>,
    pub columns: BTreeMap<(String, String), Table>,
    pub open_schemas: BTreeSet<String>,
    /// `(schema, folder)` — `tables`, `views`, `routines` and the rest.
    pub open_folders: BTreeSet<(String, String)>,
    pub open_tables: BTreeSet<(String, String)>,
    /// What each search table declared about itself, by name.
    ///
    /// Only Inillucent puts anything here. It is what the tree draws under a search index -
    /// `768d - exact - cosine - porter` - so that the four things worth knowing before trusting a
    /// result are on the screen rather than in a `%_config` somebody would have to go and read.
    pub searches: BTreeMap<String, unluminous_db::SearchIndex>,
    /// What went wrong reading this source, if anything.
    pub problem: Option<String>,
}

/// A console: a SQL editor and whatever its last statement answered.
#[derive(Debug, Clone, Default)]
pub struct Console {
    /// Which query this is, counted from one across the session, so two consoles on one data source have
    /// two names: `Query 1` and `Query 2`.
    pub number: u64,
    pub source: String,
    pub schema: String,
    pub text: String,
    /// Where the caret is, in bytes, which is what decides the statement Execute runs.
    pub caret: usize,
    pub result: Option<Rows>,
    pub failure: Option<Failure>,
    /// What a statement that returned no rows said, newest last.
    pub output: Vec<String>,
    pub running: Option<u64>,
    /// What the last statement answered, as a grid, when it was a `SELECT` (`task-2253`). It is the grid a
    /// table opens in, so its rows sort, page and filter the same way, and when the statement read one
    /// table's own rows they are edited and saved the same way too. `result` is used only for an answer
    /// that cannot be read again, such as a `PRAGMA` or an `INSERT ... RETURNING`.
    pub grid: Option<Box<Grid>>,
    /// The table a statement reads while its columns are being asked for, with the statement itself.
    pub describing: Option<(unluminous_db::one_table::OneTable, String)>,
}

/// What a grid's rows are read with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Reads {
    /// Every column of the table, which is what a table opened from the tree reads.
    #[default]
    Table,
    /// The columns, the `LIMIT` and the `OFFSET` a console statement named on the one table it read.
    Columns { columns: Vec<String>, limit: Option<usize>, offset: usize },
    /// A console statement whose rows are not one table's own: read again inside
    /// `select * from (...)`, so its rows can still be filtered, sorted and paged, and never changed.
    Statement(String),
}

/// A grid on one table's rows.
#[derive(Debug, Clone)]
pub struct Grid {
    pub source: String,
    pub table: Table,
    pub kind: Kind,
    /// A fragment of SQL somebody typed, sent to the server rather than filtered here.
    pub where_clause: String,
    pub order_by: String,
    /// Which page, from zero.
    pub at: usize,
    pub rows: Rows,
    pub pending: Pending,
    pub failure: Option<Failure>,
    pub running: Option<u64>,
    /// The cell the keyboard is on, as `(row, column)`.
    pub chosen: Option<(usize, usize)>,
    /// The cell being typed into, and what has been typed so far.
    pub editing: Option<Editing>,
    pub reads: Reads,
}

/// A cell that is open for typing.
///
/// `task-1795`: *"I should be able to double click an entry in the table, type to update, and see a
/// save button that writes to the table for that value."* Committing records a **pending** change
/// rather than sending a statement, which is what lets Preview show what Save will do before
/// anything is written.
#[derive(Debug, Clone, PartialEq)]
pub struct Editing {
    /// The row, counted the way the grid draws them: the rows that were read, then the rows that
    /// have been added and not yet written.
    pub at: usize,
    pub column: usize,
    pub text: String,
    /// True on the frame it opened, which is when the box asks for the keyboard and selects itself.
    pub opened: bool,
}

impl Grid {
    /// A grid on a table with nothing read yet.
    ///
    /// @param source - the data source's name
    /// @param table - the table, described
    /// @param kind - what the tree says the table is
    /// @param reads - what the rows are read with
    pub fn new(source: &str, table: Table, kind: Kind, reads: Reads) -> Self {
        Grid {
            source: source.to_owned(),
            table,
            kind,
            where_clause: String::new(),
            order_by: String::new(),
            at: 0,
            rows: Rows::default(),
            pending: Pending::default(),
            failure: None,
            running: None,
            chosen: None,
            editing: None,
            reads,
        }
    }

    /// How many rows the grid draws: the ones that were read, then the ones that were added here.
    ///
    /// **Pressing `Add row` used to change a number and put nothing on the screen** — the grid drew
    /// `rows.rows` and a pending insert is not in it — which is the whole of `task-1795`’s report
    /// that "Add row doesn’t provide a new row in the ui for me to type in".
    pub fn row_count(&self) -> usize {
        self.rows.rows.len() + self.pending.added().len()
    }

    /// Which row of the database this row of the grid is, by the values of the table’s key.
    ///
    /// A row past the ones that were read is one that was added here, and it is named by when it was
    /// added rather than by a key it does not have yet.
    pub fn row_of(&self, at: usize) -> Option<Row> {
        if at >= self.rows.rows.len() {
            return self
                .pending
                .added()
                .get(at - self.rows.rows.len())
                .map(|added| Row::Added(*added));
        }
        let row = self.rows.rows.get(at)?;
        let key = self
            .table
            .key
            .iter()
            .map(|name| {
                let column = self.rows.column(name)?;
                Some(row.get(column)?.display())
            })
            .collect::<Option<Vec<String>>>()?;
        Some(Row::Keyed(key))
    }

    /// What a cell should show: whatever is pending on it, or what was read.
    ///
    /// A row that was added here has nothing read behind it, so a cell nobody has typed into is the
    /// empty string rather than `NULL` — an insert that named every column as NULL would refuse on
    /// the first `NOT NULL` column, and what a person has not filled in yet is not a value.
    pub fn cell(&self, at: usize, column: usize) -> (Value, bool) {
        let added = at >= self.rows.rows.len();
        let read = match added {
            true => Value::Text(String::new()),
            false => {
                self.rows.rows.get(at).and_then(|row| row.get(column)).cloned().unwrap_or_default()
            }
        };
        let Some(name) = self.rows.columns.get(column).map(|column| column.name.clone()) else {
            return (read, false);
        };
        match self.row_of(at).and_then(|row| self.pending.value_of(&row, &name).cloned()) {
            Some(pending) => (pending, true),
            None => (read, added),
        }
    }

    /// What a cell is edited **as**: the value, or nothing at all for a NULL.
    ///
    /// A box that opened with the word `NULL` in it would write the four letters back the moment
    /// anybody pressed Enter, so a NULL opens empty — and an empty box on a cell that was NULL is
    /// left as NULL. `unluminous_db::Value` keeps the two apart all the way from the wire and this is the
    /// one place a person could confuse them.
    pub fn text_of(&self, at: usize, column: usize) -> String {
        let (value, _) = self.cell(at, column);
        match value.is_null() {
            true => String::new(),
            false => value.display(),
        }
    }
}

/// One page in the workspace tab.
#[derive(Debug, Clone)]
pub enum Sheet {
    Console(Box<Console>),
    Grid(Box<Grid>),
}

/// A page and the number it is known by.
///
/// A number rather than an index, because a page is closed by pressing its own cross and the indexes
/// of every page after it then move — which is how an answer to a query gets applied to somebody
/// else's grid.
#[derive(Debug, Clone)]
pub struct Page {
    pub id: u64,
    pub sheet: Sheet,
}

impl Page {
    pub fn title(&self) -> String {
        match &self.sheet {
            // **The data source is not in a console's name** (`task-2250`), because a console can be pointed
            // at another one from the dropdown in its toolbar, and a page whose name changed under it would
            // be a different page to everything that had found it by name.
            Sheet::Console(console) => format!("Query {}", console.number.max(1)),
            Sheet::Grid(grid) => format!("{} \u{b7} {}", grid.table.name, grid.source),
        }
    }

    pub fn source(&self) -> &str {
        match &self.sheet {
            Sheet::Console(console) => &console.source,
            Sheet::Grid(grid) => &grid.source,
        }
    }

    /// Which job this page is waiting for, if any. What decides whether an answer is still wanted.
    pub fn running(&self) -> Option<u64> {
        match &self.sheet {
            Sheet::Console(console) => {
                console.running.or_else(|| console.grid.as_ref().and_then(|grid| grid.running))
            }
            Sheet::Grid(grid) => grid.running,
        }
    }

    /// The grid on this page: a table's own page, or the result of a console's last `SELECT`.
    pub fn grid(&self) -> Option<&Grid> {
        match &self.sheet {
            Sheet::Grid(grid) => Some(grid),
            Sheet::Console(console) => console.grid.as_deref(),
        }
    }

    /// The same, to change.
    pub fn grid_mut(&mut self) -> Option<&mut Grid> {
        match &mut self.sheet {
            Sheet::Grid(grid) => Some(grid),
            Sheet::Console(console) => console.grid.as_deref_mut(),
        }
    }
}

/// What is chosen in the tree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chosen {
    pub source: String,
    pub schema: String,
    pub name: String,
}

/// What a right click in the tree landed on, which decides the rows the menu offers.
///
/// A value rather than a list of entries, for the reason `components::context_menu`’s own state is
/// the window’s rather than egui’s: a menu that can only be opened by pressing the right mouse
/// button is a menu no test can look at, and `plugins run database menu` opens this one by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aimed {
    Source(String),
    Schema(String, String),
    /// A table or a view: the source, the schema the tree files it under, and its name.
    Item(String, String, String),
    Column(String),
}

/// A modal this plugin has open.
///
/// **There is no `Confirm`.** It used to ask before a console statement that changes rows was sent;
/// `task-1795` asks for no safety check at all, so the statement goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    /// Adding or editing a data source.
    Source(SourceForm),
    /// The statements Save is about to send.
    Preview { page: u64 },
    /// A `CREATE` statement.
    Ddl { title: String, text: String },
    /// Making a table: a name, its columns, and the statement they compose.
    NewTable(TableForm),
    /// One cell's vector, drawn and written out.
    ///
    /// The grid's cell says `768d - [0.0231, -0.0114, 0.0518, ...] - |v| 1.000`, which is what fits
    /// where it is. This is what that is a summary of: the figures, every component as a bar, and the
    /// numbers themselves.
    Vector { title: String, vector: unluminous_db::Vector },
}

/// The fields of the New Data Source dialog.
///
/// Name, engine, host, port, database, user, password and connection security. A password typed here
/// goes into this machine’s own credential store when Save is pressed and what is written down is
/// the name of the entry — see `components::database::modal::secret_of`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceForm {
    /// The name it had before it was edited, or empty when it is new.
    pub was: String,
    pub source: Source,
    /// A password typed here. Empty means "leave whatever this source already had".
    pub typed: String,
    /// What Test Connection said, if it has been pressed.
    pub tested: Option<Result<String, String>>,
}

/// One column on the New Table dialog.
///
/// `unluminous_db::sql::NewColumn` itself rather than a form of its own beside it: the dialog is typed
/// straight into the shape `create_table` composes from, so what is shown beside the form and what
/// Create sends cannot come apart. A type is a fragment of DDL — a dropdown offers the engine’s own
/// list and it can be typed into as well, because no closed list holds every type an engine has.
pub use unluminous_db::sql::NewColumn as ColumnForm;

/// The fields of the New Table dialog.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableForm {
    pub source: String,
    pub schema: String,
    pub name: String,
    pub columns: Vec<ColumnForm>,
    /// What the server said, if Create has been pressed and refused.
    pub problem: Option<String>,
}

/// The name of the credential-store entry a data source’s password is kept under.
///
/// One place, because the dialog writes it and `config::password_for` reads it, and a name composed
/// twice is a name that comes apart the first time either is edited. `keychain::is_a_safe_name`
/// allows letters, digits, a dash, a dot and an underscore, so everything else in the data source’s
/// name becomes a dash rather than being handed to a tool that would read it as an argument.
pub fn keychain_entry_for(source: &str) -> String {
    let kept: String = source
        .chars()
        .map(|character| {
            match character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | '_') {
                true => character,
                false => '-',
            }
        })
        .collect();
    format!("unluminous-database-{}", kept.trim_matches('-'))
}

/// The plugin.
pub struct DatabaseExplorer {
    open: bool,
    folder: Option<PathBuf>,
    project: Option<PathBuf>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    pub configuration: Configuration,
    connections: BTreeMap<String, Connection>,
    pub loaded: BTreeMap<String, Loaded>,
    pub open_sources: BTreeSet<String>,
    pub pages: Vec<Page>,
    pub current: usize,
    pub chosen: Option<Chosen>,
    pub filter: String,
    /// The last thing that went wrong, drawn under the tree.
    pub problem: Option<String>,
    pub modal: Option<Modal>,
    /// The tree’s own menu: where it is, and what it was opened on.
    pub menu: Option<(egui::Pos2, Aimed)>,
    next_page: u64,
    /// How many consoles have been opened, which is what the next one is numbered after.
    queries: u64,
    /// How far down the tree is scrolled, which the provider keeps so a zoom can correct it.
    pub scrolled: f32,
    pub scroll_to: Option<f32>,
    /// The last `CREATE` statement that was read, as `(name, text)`.
    pub last_ddl: Option<(String, String)>,
    /// What this provider wants the window to do, decided outside a draw.
    ///
    /// Drained once a frame through [`UiProvider::asking`]. Opening a page has to put the workspace
    /// tab in front whoever asked for it — the tree's own double click, the menu, or an agent running
    /// `plugins run database open` — and only the window can show a tab.
    asking: Vec<Request>,
    /// A Test Connection in flight. See [`ConnectionTest`].
    testing: Option<ConnectionTest>,
    /// Bumped whenever anything the tree draws changes.
    ///
    /// **What a cache over the tree's rows is keyed on** (`task-1984` S16). There are exactly two
    /// doors: `components::database::apply`, which is the one place an act becomes a change, and
    /// `take_the_replies`, which is the one place a worker's answer arrives.
    revision: u64,
    /// The rows the tree last drew, and what they were built from. See `tree::lines_cached`.
    #[allow(clippy::type_complexity)]
    pub drawn_lines: std::cell::RefCell<
        Option<((u64, String), std::rc::Rc<Vec<crate::components::database::tree::Line>>)>,
    >,
}

/// A Test Connection, on a thread.
///
/// **`task-1984` S2.** Test Connection called `unluminous_db::Database::connect` synchronously from
/// inside the modal's own draw function -- the one side effect on the world anywhere in
/// `components/`, and the only connection in the plugin that did not go through `Worker::open`. A
/// host that is down costs about twenty one seconds of TCP on Windows, during which the window drew
/// nothing and the control channel answered nothing, which is what `unluminous_git::Worker`'s comment
/// says never to do.
///
/// `update::Check`'s shape, because it is the same question: a thread, a channel, a waker, and a
/// `poll` the frame loop calls.
pub struct ConnectionTest {
    answers: std::sync::mpsc::Receiver<Result<String, String>>,
    asking: bool,
}

impl ConnectionTest {
    /// Open a connection, read what the server called itself, and close it again.
    ///
    /// **Read only whatever the data source is set to**: a Test Connection that could write is one
    /// nobody should press twice.
    pub fn start(
        source: &Source,
        password: Option<String>,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        let (sender, answers) = std::sync::mpsc::channel();
        let mut source = source.clone();
        source.read_only = true;
        std::thread::Builder::new()
            .name(format!("unluminous-db test {}", source.name))
            .spawn(move || {
                let answer = match unluminous_db::Database::connect(&source, password.as_deref()) {
                    Ok(mut database) => {
                        let said = database.version();
                        let encrypted = database.is_encrypted();
                        database.close();
                        Ok(match encrypted {
                            true => format!("{said}, encrypted"),
                            false => said,
                        })
                    }
                    Err(why) => Err(why.to_string()),
                };
                // The window may have gone; a send to a closed channel is the ordinary end of this
                // thread rather than something to report.
                let _ = sender.send(answer);
                if let Some(wake) = wake {
                    wake();
                }
            })
            .ok();
        Self { answers, asking: true }
    }

    /// The answer, once. `None` while the server has not replied.
    pub fn poll(&mut self) -> Option<Result<String, String>> {
        match self.answers.try_recv() {
            Ok(answer) => {
                self.asking = false;
                Some(answer)
            }
            Err(_) => None,
        }
    }

    pub fn is_asking(&self) -> bool {
        self.asking
    }
}

impl std::fmt::Debug for DatabaseExplorer {
    /// Written by hand because the waker is a closure and no closure has a `Debug`. What is printed is
    /// what a test wants to see when an assertion about the plugin fails.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("DatabaseExplorer")
            .field("sources", &self.configuration.sources.len())
            .field("connected", &self.connections.keys().collect::<Vec<&String>>())
            .field("pages", &self.pages.len())
            .field("problem", &self.problem)
            .finish()
    }
}

impl Default for DatabaseExplorer {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseExplorer {
    pub fn new() -> Self {
        Self {
            open: false,
            folder: None,
            project: None,
            wake: None,
            configuration: Configuration::default(),
            connections: BTreeMap::new(),
            loaded: BTreeMap::new(),
            open_sources: BTreeSet::new(),
            pages: Vec::new(),
            current: 0,
            chosen: None,
            filter: String::new(),
            problem: None,
            modal: None,
            menu: None,
            next_page: 1,
            queries: 0,
            scrolled: 0.0,
            scroll_to: None,
            last_ddl: None,
            asking: Vec::new(),
            testing: None,
            revision: 0,
            drawn_lines: std::cell::RefCell::new(None),
        }
    }

    // ------------------------------------------------------------------ data sources

    pub fn sources(&self) -> &[Source] {
        &self.configuration.sources
    }

    pub fn is_connected(&self, name: &str) -> bool {
        self.connections.contains_key(name)
    }

    /// What the server called itself, for a connected source.
    pub fn version_of(&self, name: &str) -> Option<&str> {
        self.connections.get(name).map(|one| one.worker.version.as_str())
    }

    pub fn is_busy(&self, name: &str) -> bool {
        self.connections.get(name).is_some_and(|one| one.worker.is_busy())
    }

    /// True while anything at all is running, which is what keeps the window drawing.
    pub fn anything_running(&self) -> bool {
        self.connections.values().any(|one| one.worker.is_busy())
    }

    /// Open a connection, and ask it for its schemas.
    ///
    /// The password is fetched here and handed straight to the worker; nothing keeps it.
    pub fn connect(&mut self, name: &str) -> Result<(), String> {
        if self.connections.contains_key(name) {
            return Ok(());
        }
        let source = self
            .configuration
            .source(name)
            .cloned()
            .ok_or_else(|| format!("there is no data source called `{name}`."))?;
        let password = password_for(&source);
        let worker = Worker::open(&source, password.as_deref(), self.wake.clone())
            .map_err(|why| why.to_string())?;
        self.connections.insert(name.to_owned(), Connection { worker, waiting: BTreeMap::new() });
        self.loaded.entry(name.to_owned()).or_default().problem = None;
        self.ask(name, Job::Schemas, Wanted::Schemas);
        Ok(())
    }

    /// Close a connection and forget what was read through it.
    ///
    /// The tree's expansion is kept, because reopening a source somebody had open at `public.tables`
    /// and putting them back at the top would be a pane that forgets what they were doing.
    pub fn disconnect(&mut self, name: &str) {
        self.connections.remove(name);
        if let Some(loaded) = self.loaded.get_mut(name) {
            loaded.schemas.clear();
            loaded.items.clear();
            loaded.columns.clear();
        }
    }

    /// Start a Test Connection against what is on the screen, on a thread.
    ///
    /// `task-1984` S2. What is tested is what is **typed** rather than what is stored, so the
    /// password on the screen is the one that is tried -- and nothing is written to the credential
    /// store, because a test is not a save.
    pub fn test_the_source(&mut self, form: &SourceForm) {
        let mut source = form.source.clone();
        if !form.typed.is_empty() {
            source.secret = unluminous_db::source::Secret::Typed(form.typed.clone());
        }
        let password = password_for(&source);
        self.testing = Some(ConnectionTest::start(&source, password, self.wake.clone()));
    }

    /// How many times anything the tree draws has changed. See the field's own note.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Say that something the tree draws has changed.
    pub fn changed(&mut self) {
        self.revision += 1;
    }

    /// Whether a Test Connection is still waiting for an answer, which is what the modal draws.
    pub fn is_testing(&self) -> bool {
        self.testing.as_ref().is_some_and(ConnectionTest::is_asking)
    }

    /// Take the answer into the form, if one has arrived. Called once a frame beside the workers'.
    fn take_the_test_answer(&mut self) -> bool {
        let Some(test) = self.testing.as_mut() else {
            return false;
        };
        let Some(answer) = test.poll() else {
            return false;
        };
        self.testing = None;
        if let Some(Modal::Source(form)) = self.modal.as_mut() {
            form.tested = Some(answer);
        }
        true
    }

    /// Write a password into this machine's credential store, and say where it ended up.
    ///
    /// `task-1984` S9. It used to happen inside the modal's draw function, where on macOS and Linux
    /// it spawns `security` or `secret-tool` and can put an unlock prompt up behind a window that has
    /// stopped drawing. It is a change to the world, so it belongs where every other change does.
    ///
    /// **`Secret::Typed` when the store refuses**, which it has always been: refusing to connect at
    /// all because a keychain was locked would be a worse answer than a connection that lasts until
    /// the window closes.
    pub fn keep_the_password(
        &mut self,
        form: &SourceForm,
    ) -> (unluminous_db::source::Secret, Option<String>) {
        if form.typed.is_empty() {
            return (form.source.secret.clone(), None);
        }
        let entry = keychain_entry_for(&form.source.name);
        match crate::services::agent_tasks::keychain::write(&entry, &form.typed) {
            Ok(()) => (unluminous_db::source::Secret::Keychain(entry), None),
            Err(why) => (
                unluminous_db::source::Secret::Typed(form.typed.clone()),
                Some(format!(
                    "The password is held in this window only: {why}. It is not written to any file."
                )),
            ),
        }
    }

    /// Add or replace a data source, and write the file.
    pub fn save_source(&mut self, was: &str, source: Source) -> Result<(), String> {
        if source.name.trim().is_empty() {
            return Err("a data source needs a name.".to_owned());
        }
        if self
            .configuration
            .sources
            .iter()
            .any(|other| other.name == source.name && other.name != was)
        {
            return Err(format!("there is already a data source called `{}`.", source.name));
        }
        // A source that changed is reconnected rather than left on a connection to the old address.
        self.disconnect(was);
        match self.configuration.sources.iter().position(|other| other.name == was) {
            Some(at) => self.configuration.sources[at] = source.clone(),
            None => self.configuration.sources.push(source.clone()),
        }
        // **A rename takes everything that names the source with it.** The name is the key in five
        // places, and leaving four of them behind gives pages that report a data source which is not
        // connected and can never be, and a tree that has lost its expansion — which is exactly the
        // lifecycle fault the conventions warn about.
        if !was.is_empty() && was != source.name {
            self.rename(was, &source.name);
        }
        if self.configuration.chosen.is_empty() {
            self.configuration.chosen = source.name.clone();
        }
        self.write_the_configuration()
    }

    /// Move everything that names `was` onto `now`.
    fn rename(&mut self, was: &str, now: &str) {
        if let Some(loaded) = self.loaded.remove(was) {
            self.loaded.insert(now.to_owned(), loaded);
        }
        if self.open_sources.remove(was) {
            self.open_sources.insert(now.to_owned());
        }
        if self.configuration.chosen == was {
            self.configuration.chosen = now.to_owned();
        }
        if let Some(chosen) = self.chosen.as_mut().filter(|chosen| chosen.source == was) {
            chosen.source = now.to_owned();
        }
        for page in &mut self.pages {
            match &mut page.sheet {
                Sheet::Console(console) if console.source == was => console.source = now.to_owned(),
                Sheet::Grid(grid) if grid.source == was => grid.source = now.to_owned(),
                _ => {}
            }
        }
    }

    pub fn remove_source(&mut self, name: &str) -> Result<(), String> {
        if self.configuration.source(name).is_none() {
            return Err(format!("there is no data source called `{name}`."));
        }
        self.disconnect(name);
        self.loaded.remove(name);
        self.open_sources.remove(name);
        self.pages.retain(|page| page.source() != name);
        self.current = self.current.min(self.pages.len().saturating_sub(1));
        self.configuration.sources.retain(|source| source.name != name);
        if self.configuration.chosen == name {
            self.configuration.chosen = self
                .configuration
                .sources
                .first()
                .map(|source| source.name.clone())
                .unwrap_or_default();
        }
        self.write_the_configuration()
    }

    pub fn write_the_configuration(&self) -> Result<(), String> {
        match &self.folder {
            Some(folder) => self.configuration.write(folder),
            // A window with no store — which is every test — keeps its sources in memory, which is
            // the rule `UnluminousApp::load_settings` sets: a test must not read or write the settings of
            // the person running it.
            None => Ok(()),
        }
    }

    // ------------------------------------------------------------------ the tree

    /// Open or close a data source's row.
    pub fn toggle_source(&mut self, name: &str) {
        if self.open_sources.contains(name) {
            self.open_sources.remove(name);
            return;
        }
        self.open_sources.insert(name.to_owned());
        if !self.is_connected(name) {
            if let Err(why) = self.connect(name) {
                self.loaded.entry(name.to_owned()).or_default().problem = Some(why.clone());
                self.problem = Some(why);
            }
        }
    }

    /// Open or close a schema, asking for its items the first time.
    pub fn toggle_schema(&mut self, source: &str, schema: &str) {
        let loaded = self.loaded.entry(source.to_owned()).or_default();
        if loaded.open_schemas.contains(schema) {
            loaded.open_schemas.remove(schema);
            return;
        }
        loaded.open_schemas.insert(schema.to_owned());
        if !loaded.items.contains_key(schema) {
            self.ask(
                source,
                Job::Items { schema: schema.to_owned() },
                Wanted::Items { schema: schema.to_owned() },
            );
        }
    }

    pub fn toggle_folder(&mut self, source: &str, schema: &str, folder: &str) {
        let loaded = self.loaded.entry(source.to_owned()).or_default();
        let key = (schema.to_owned(), folder.to_owned());
        if !loaded.open_folders.insert(key.clone()) {
            loaded.open_folders.remove(&key);
        }
    }

    /// Open or close a table's columns, asking for them the first time.
    pub fn toggle_table(&mut self, source: &str, schema: &str, name: &str) {
        let loaded = self.loaded.entry(source.to_owned()).or_default();
        let key = (schema.to_owned(), name.to_owned());
        if !loaded.open_tables.insert(key.clone()) {
            loaded.open_tables.remove(&key);
            return;
        }
        if !loaded.columns.contains_key(&key) {
            self.describe(source, schema, name, Then::ShowColumns);
        }
    }

    fn describe(&mut self, source: &str, schema: &str, name: &str, then: Then) {
        self.ask(
            source,
            Job::Describe { schema: schema.to_owned(), table: name.to_owned() },
            Wanted::Describe { schema: schema.to_owned(), name: name.to_owned(), then },
        );
    }

    /// Ask for a table's `CREATE` statement. `show` puts it in a modal; a command asks with `false`.
    pub fn ask_for_ddl(
        &mut self,
        source: &str,
        schema: &str,
        name: &str,
        kind: Kind,
        show: bool,
    ) -> Result<(), String> {
        self.last_ddl = None;
        let job = Job::Ddl { schema: schema.to_owned(), table: name.to_owned(), kind };
        match self.ask(source, job, Wanted::Ddl { name: name.to_owned(), show }) {
            Some(_) => Ok(()),
            None => Err(format!("`{source}` is not connected.")),
        }
    }

    /// Ask a source's worker for something, remembering what it was for.
    fn ask(&mut self, source: &str, job: Job, wanted: Wanted) -> Option<u64> {
        let connection = self.connections.get_mut(source)?;
        match connection.worker.ask(job) {
            Ok(ticket) => {
                connection.waiting.insert(ticket, wanted);
                Some(ticket)
            }
            Err(why) => {
                self.problem = Some(why.to_string());
                None
            }
        }
    }

    // ------------------------------------------------------------------ pages

    fn add_page(&mut self, sheet: Sheet) -> u64 {
        let id = self.next_page;
        self.next_page += 1;
        self.pages.push(Page { id, sheet });
        self.current = self.pages.len() - 1;
        // A page nobody can see is a page nobody asked for. Only the window can show a tab, so this is
        // asked for rather than done — the rule every `Request` keeps.
        self.asking.push(Request::ShowTab);
        id
    }

    pub fn page(&self, id: u64) -> Option<&Page> {
        self.pages.iter().find(|page| page.id == id)
    }

    /// The grid on a page, which is a table's page or a console showing what its `SELECT` answered.
    pub fn grid(&self, id: u64) -> Option<&Grid> {
        self.page(id).and_then(Page::grid)
    }

    /// The same, to change.
    pub fn grid_mut(&mut self, id: u64) -> Option<&mut Grid> {
        self.page_mut(id).and_then(Page::grid_mut)
    }

    fn page_mut(&mut self, id: u64) -> Option<&mut Page> {
        self.pages.iter_mut().find(|page| page.id == id)
    }

    pub fn close_page(&mut self, id: u64) {
        self.pages.retain(|page| page.id != id);
        self.current = self.current.min(self.pages.len().saturating_sub(1));
    }

    /// Open a console on a data source, which is what `Jump to query console` does in the reference editor.
    pub fn open_console(&mut self, source: &str) -> Result<u64, String> {
        if self.configuration.source(source).is_none() {
            return Err(format!("there is no data source called `{source}`."));
        }
        let _ = self.connect(source);
        let schema = self
            .loaded
            .get(source)
            .and_then(|loaded| loaded.schemas.first().cloned())
            .unwrap_or_default();
        self.queries += 1;
        Ok(self.add_page(Sheet::Console(Box::new(Console {
            number: self.queries,
            source: source.to_owned(),
            schema,
            ..Console::default()
        }))))
    }

    /// The data source a new console goes on: the one the page showing is on, else the one the tree is
    /// pointed at, else the first there is. `None` when there are no data sources at all.
    ///
    /// The page showing first, because `New query` pressed while reading one table's rows is a question
    /// about that table's database.
    pub fn source_for_a_new_query(&self) -> Option<String> {
        let known = |name: &str| self.configuration.source(name).is_some();
        let showing = self.pages.get(self.current).map(|page| page.source().to_owned());
        let chosen = self.chosen.as_ref().map(|chosen| chosen.source.clone());
        [showing, chosen, Some(self.configuration.chosen.clone())]
            .into_iter()
            .flatten()
            .find(|name| !name.is_empty() && known(name))
            .or_else(|| self.sources().first().map(|source| source.name.clone()))
    }

    /// Point a console at another data source, which is the dropdown at the right of its toolbar.
    ///
    /// The statement stays, because the reason to move a console is very often to run the same statement
    /// somewhere else. What the last statement answered goes, because it was the other database's answer and
    /// a result left under a different name would be read as this one's. A console that is still running is
    /// refused: its answer is on its way from the source it was sent to.
    pub fn point_the_console_at(&mut self, id: u64, source: &str) -> Result<(), String> {
        if self.configuration.source(source).is_none() {
            return Err(format!("there is no data source called `{source}`."));
        }
        let Some(Page { sheet: Sheet::Console(console), .. }) = self.page(id) else {
            return Err("that page is not a console.".to_owned());
        };
        if console.source == source {
            return Ok(());
        }
        if self.page(id).and_then(Page::running).is_some() {
            return Err("this console is still running a statement; stop it first.".to_owned());
        }
        let _ = self.connect(source);
        let schema = self
            .loaded
            .get(source)
            .and_then(|loaded| loaded.schemas.first().cloned())
            .unwrap_or_default();
        if let Some(Page { sheet: Sheet::Console(console), .. }) = self.page_mut(id) {
            console.source = source.to_owned();
            console.schema = schema;
            console.grid = None;
            console.describing = None;
            console.result = None;
            console.failure = None;
            console.output.clear();
        }
        self.changed();
        Ok(())
    }

    /// Show a page, by its number.
    pub fn show_page(&mut self, id: u64) -> Result<(), String> {
        match self.pages.iter().position(|page| page.id == id) {
            Some(at) => {
                self.current = at;
                Ok(())
            }
            None => Err(format!("there is no page {id}.")),
        }
    }

    /// Open a grid on a table. Its columns are asked for first, because the key decides everything.
    pub fn open_table(&mut self, source: &str, schema: &str, name: &str) -> Result<(), String> {
        if self.configuration.source(source).is_none() {
            return Err(format!("there is no data source called `{source}`."));
        }
        self.connect(source)?;
        match self
            .loaded
            .get(source)
            .and_then(|loaded| loaded.columns.get(&(schema.to_owned(), name.to_owned())))
        {
            Some(table) => {
                let table = table.clone();
                self.grid_for(source, schema, table);
            }
            None => self.describe(source, schema, name, Then::OpenGrid),
        }
        Ok(())
    }

    /// Open a grid on a table that has already been described.
    ///
    /// `schema` is the one the **tree** files this table under, which is not always `table.schema`:
    /// SQLite's own `Table` carries no schema at all, while the tree files its items under `main`.
    /// Looking the kind up by the wrong one made every SQLite view read as an editable table.
    fn grid_for(&mut self, source: &str, schema: &str, table: Table) {
        let kind = self.kind_of(source, schema, &table.name);
        let id = self.add_page(Sheet::Grid(Box::new(Grid::new(source, table, kind, Reads::Table))));
        self.reload(id);
    }

    /// Fill a grid, or run a console's statement again.
    pub fn reload(&mut self, id: u64) {
        let Some(grid) = self.grid(id) else { return };
        let source = grid.source.clone();
        let engine = self
            .configuration
            .source(&source)
            .map(|source| source.engine)
            .unwrap_or(Engine::Postgres);
        let limit = self.configuration.page_size;
        let statement = select_for(grid, engine, limit, grid.at);
        let ticket =
            self.ask(&source, Job::Query { sql: statement, limit }, Wanted::Rows { page: id });
        if let Some(grid) = self.grid_mut(id) {
            grid.running = ticket;
            grid.failure = None;
        }
    }

    /// What the tree says a table is, which is `Kind::Table` when the tree has not been read.
    fn kind_of(&self, source: &str, schema: &str, name: &str) -> Kind {
        self.loaded
            .get(source)
            .and_then(|loaded| loaded.items.get(schema))
            .and_then(|items| items.iter().find(|item| item.name == name))
            .map(|item| item.kind)
            .unwrap_or(Kind::Table)
    }

    /// Run the statement under the caret of a console.
    ///
    /// **Nothing is confirmed.** A dialog used to stand in front of a statement that changes rows;
    /// `task-1795` asks for no safety check at all, and it was never a guard an agent met anyway —
    /// `execute_now` went straight past it, because a command cannot press a button in a modal. The
    /// only thing that can still refuse is a data source somebody has deliberately set read only,
    /// and that refusal is the server’s.
    pub fn execute(&mut self, id: u64) -> Result<String, String> {
        self.execute_with(id)
    }

    /// The same, kept as its own name because the command line calls it. See [`Self::execute`].
    pub fn execute_now(&mut self, id: u64) -> Result<String, String> {
        self.execute_with(id)
    }

    fn execute_with(&mut self, id: u64) -> Result<String, String> {
        let Some(Page { sheet: Sheet::Console(console), .. }) = self.page(id) else {
            return Err("that page is not a console.".to_owned());
        };
        let statement = unluminous_db::sql::at(&console.text, console.caret)
            .ok_or_else(|| "there is no statement to run.".to_owned())?;
        let sql = statement.to_send().to_owned();
        if sql.is_empty() {
            return Err("there is no statement to run.".to_owned());
        }
        let source = console.source.clone();
        let read_only = self.configuration.source(&source).is_some_and(|source| source.read_only);
        if read_only && !unluminous_db::sql::only_reads(&statement) {
            return Err(format!(
                "`{}` is read only, so `{}` is not sent. `plugins run database read-only {} off` \
                 makes it writable.",
                source,
                statement.verb(),
                source
            ));
        }
        match unluminous_db::one_table::read(&sql) {
            Some(read) => self.read_one_table(id, read, &sql),
            None if unluminous_db::one_table::can_be_read_again(&sql) => {
                let reads = Reads::Statement(sql.clone());
                self.show_as_a_grid(id, reads, Table::default(), Kind::Table, "", "");
            }
            None => self.send_from_console(id, &sql),
        }
        Ok(sql)
    }

    /// Send a console's statement as it was written, and show what it answers under the editor.
    ///
    /// This is the way for a statement that is not a `SELECT`, and for a `SELECT` whose grid could not be read.
    pub fn send_from_console(&mut self, id: u64, sql: &str) {
        let Some(Page { sheet: Sheet::Console(console), .. }) = self.page(id) else { return };
        let source = console.source.clone();
        let limit = self.configuration.page_size;
        let ticket =
            self.ask(&source, Job::Query { sql: sql.to_owned(), limit }, Wanted::Rows { page: id });
        if let Some(Page { sheet: Sheet::Console(console), .. }) = self.page_mut(id) {
            console.running = ticket;
            console.failure = None;
            console.grid = None;
            console.describing = None;
        }
    }

    /// A console `SELECT` of one table's own rows: the table's columns are asked for first, because its key
    /// decides whether the rows can be changed, and then the rows are read as that table's grid.
    ///
    /// @param id - the console's page
    /// @param read - which table the statement reads, and how
    /// @param sql - the statement, for when the table cannot be described
    fn read_one_table(&mut self, id: u64, read: unluminous_db::one_table::OneTable, sql: &str) {
        let Some(Page { sheet: Sheet::Console(console), .. }) = self.page(id) else { return };
        let source = console.source.clone();
        let schema = self.schema_for(&source, &read, &console.schema);
        let name = self.name_in_the_tree(&source, &schema, &read.name);
        let described = self
            .loaded
            .get(&source)
            .and_then(|loaded| loaded.columns.get(&(schema.clone(), name.clone())))
            .cloned();
        if let Some(Page { sheet: Sheet::Console(console), .. }) = self.page_mut(id) {
            console.describing = Some((read, sql.to_owned()));
        }
        match described {
            Some(table) => self.console_table_described(id, table),
            None => {
                let ticket = self.ask(
                    &source,
                    Job::Describe { schema, table: name },
                    Wanted::ConsoleTable { page: id },
                );
                if let Some(Page { sheet: Sheet::Console(console), .. }) = self.page_mut(id) {
                    console.running = ticket;
                    console.failure = None;
                }
            }
        }
    }

    /// The schema a console statement's table is in: the one it was named in, else the console's own schema
    /// when the tree has that table there, else `public` when the tree has it there, which is where
    /// PostgreSQL's default search path finds an unqualified name. The console's schema otherwise.
    fn schema_for(
        &self,
        source: &str,
        read: &unluminous_db::one_table::OneTable,
        console_schema: &str,
    ) -> String {
        if let Some(schema) = &read.schema {
            return schema.clone();
        }
        let holds = |schema: &str| {
            self.loaded.get(source).and_then(|loaded| loaded.items.get(schema)).is_some_and(
                |items| items.iter().any(|item| item.name.eq_ignore_ascii_case(&read.name)),
            )
        };
        [console_schema, "public"]
            .into_iter()
            .find(|schema| holds(schema))
            .unwrap_or(console_schema)
            .to_owned()
    }

    /// The name the tree files a table under when it matches the one written apart from its case, which is
    /// how SQLite and an unquoted PostgreSQL name are both compared. The written name otherwise.
    fn name_in_the_tree(&self, source: &str, schema: &str, written: &str) -> String {
        self.loaded
            .get(source)
            .and_then(|loaded| loaded.items.get(schema))
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.name == written)
                    .or_else(|| items.iter().find(|item| item.name.eq_ignore_ascii_case(written)))
            })
            .map_or_else(|| written.to_owned(), |item| item.name.clone())
    }

    /// The table a console statement reads has been described, so its rows are read as its grid.
    ///
    /// A table with no columns is one the server does not know, such as a name a `WITH` made up, and then the
    /// statement is sent as it was written so the server's own answer is what is shown.
    ///
    /// @param id - the console's page
    /// @param table - the table, described
    fn console_table_described(&mut self, id: u64, table: Table) {
        let Some(Page { sheet: Sheet::Console(console), .. }) = self.page(id) else { return };
        let Some((read, sql)) = console.describing.clone() else { return };
        let source = console.source.clone();
        let schema = self.schema_for(&source, &read, &console.schema);
        if table.columns.is_empty() {
            self.send_from_console(id, &sql);
            return;
        }
        let name = self.name_in_the_tree(&source, &schema, &read.name);
        self.loaded
            .entry(source.clone())
            .or_default()
            .columns
            .insert((schema.clone(), name.clone()), table.clone());
        let kind = self.kind_of(&source, &schema, &name);
        let reads =
            Reads::Columns { columns: read.columns, limit: read.limit, offset: read.offset };
        self.show_as_a_grid(id, reads, table, kind, &read.where_clause, &read.order_by);
    }

    /// Put a grid under a console's editor and read its first page.
    ///
    /// @param id - the console's page
    /// @param reads - what the grid's rows are read with
    /// @param table - the table the rows are, or an empty table for a statement's rows
    /// @param kind - what the tree says the table is
    /// @param where_clause - the statement's `WHERE`, put in the grid's own field
    /// @param order_by - the statement's `ORDER BY`, put in the grid's own field
    fn show_as_a_grid(
        &mut self,
        id: u64,
        reads: Reads,
        table: Table,
        kind: Kind,
        where_clause: &str,
        order_by: &str,
    ) {
        let Some(Page { sheet: Sheet::Console(console), .. }) = self.page_mut(id) else { return };
        let mut grid = Grid::new(&console.source, table, kind, reads);
        grid.where_clause = where_clause.to_owned();
        grid.order_by = order_by.to_owned();
        console.grid = Some(Box::new(grid));
        console.result = None;
        console.failure = None;
        console.describing = None;
        console.running = None;
        self.reload(id);
    }

    /// Send a statement that changes the shape of the database, and read the schema again.
    ///
    /// `CREATE TABLE` and `DROP TABLE` go down the same path a console statement does, so a server
    /// that refuses says so in its own words rather than in a sentence of Unluminous’s about it. It waits
    /// briefly for the answer, which introspection already does and for the same reason: a dialog
    /// that closed before the server had agreed would leave a tree claiming a table that is not
    /// there. The wait is bounded by [`commands::PATIENCE`], because this runs inside a frame.
    ///
    /// The schema it went into is then read again, so the new table is in the tree without anything
    /// being pressed — which is what `Act::Refresh` does the long way round.
    pub fn run_the_ddl(&mut self, source: &str, schema: &str, sql: &str) -> Result<String, String> {
        self.connect(source)?;
        let limit = self.configuration.page_size;
        let ticket = self
            .ask(source, Job::Query { sql: sql.to_owned(), limit }, Wanted::Nothing)
            .ok_or_else(|| format!("`{source}` is not connected."))?;
        let _ = ticket;
        let until = std::time::Instant::now() + commands::PATIENCE;
        loop {
            self.take_the_replies();
            if let Some(problem) = self.loaded.get(source).and_then(|loaded| loaded.problem.clone())
            {
                self.loaded.entry(source.to_owned()).or_default().problem = None;
                return Err(problem);
            }
            if !self.is_busy(source) {
                break;
            }
            if std::time::Instant::now() > until {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        // The schema is read again whether or not the answer arrived inside the wait: a statement
        // that is still running will have finished by the time anybody looks, and asking twice for a
        // list of tables costs one round trip.
        if let Some(loaded) = self.loaded.get_mut(source) {
            loaded.items.remove(schema);
        }
        if !schema.is_empty() {
            self.ask(
                source,
                Job::Items { schema: schema.to_owned() },
                Wanted::Items { schema: schema.to_owned() },
            );
        }
        Ok(format!("sent to `{source}`"))
    }

    /// What a source's engine says it does and does not do.
    ///
    /// Empty for every engine but Inillucent, which is the only one that reports such a thing — and
    /// it is read from the driver rather than kept here, because a capability list nobody checks
    /// decays into claims that were true once. That is the failure the driver's own two-way probe
    /// exists to prevent, and copying the table into Unluminous would reintroduce it one layer up.
    ///
    /// @param source - the data source's name
    pub fn capabilities(&self, source: &str) -> &'static [inillucent_driver::Capability] {
        match self.connections.get(source) {
            Some(connection) => connection.worker.capabilities(),
            None => &[],
        }
    }

    /// Whether a statement running on this source could be stopped at all.
    ///
    /// **What the Stop button is drawn from, and Inillucent answers no**: that engine materialises a
    /// statement on its first step, so there is no loop in which a cancel flag would be read and a
    /// button would report success while doing nothing. Unluminous's rule is that a control which can
    /// never apply is absent rather than dimmed, so the button is not drawn at all. The answer comes
    /// from the driver's own capability table rather than from a list here, so the day the engine
    /// grows a cancel the button appears without anybody editing this.
    ///
    /// @param source - the data source's name
    pub fn can_stop(&self, source: &str) -> bool {
        self.configuration
            .source(source)
            .map(|source| source.engine.can_stop_a_statement())
            .unwrap_or(true)
    }

    /// Stop whatever the page's data source is doing.
    pub fn stop(&mut self, id: u64) -> Result<(), String> {
        let Some(page) = self.page(id) else { return Err("no such page.".to_owned()) };
        let source = page.source().to_owned();
        match self.connections.get(&source) {
            Some(connection) => connection.worker.stop().map_err(|why| why.to_string()),
            None => Err(format!("`{source}` is not connected.")),
        }
    }

    /// Write a grid's pending changes as one transaction.
    pub fn submit(&mut self, id: u64) -> Result<usize, String> {
        let Some(grid) = self.grid(id) else {
            return Err("that page has nothing to submit.".to_owned());
        };
        if grid.pending.is_empty() {
            return Err("there is nothing to submit.".to_owned());
        }
        let source = grid.source.clone();
        let engine = self
            .configuration
            .source(&source)
            .map(|source| source.engine)
            .unwrap_or(Engine::Postgres);
        let statements =
            grid.pending.statements(&grid.table, engine).map_err(|why| why.to_string())?;
        let count = statements.len();
        let ticket = self.ask(&source, Job::Write { statements }, Wanted::Written { page: id });
        if let Some(grid) = self.grid_mut(id) {
            grid.running = ticket;
            grid.failure = None;
        }
        Ok(count)
    }

    /// The statements a grid's pending changes would become, for the preview modal.
    pub fn preview(&self, id: u64) -> Result<Vec<unluminous_db::Statement>, String> {
        let Some(grid) = self.grid(id) else {
            return Err("that page has nothing pending.".to_owned());
        };
        let engine = self
            .configuration
            .source(&grid.source)
            .map(|source| source.engine)
            .unwrap_or(Engine::Postgres);
        grid.pending.statements(&grid.table, engine).map_err(|why| why.to_string())
    }

    // ------------------------------------------------------------------ replies

    /// Take whatever the workers have answered, and give each answer to whatever asked for it.
    ///
    /// Called once a frame from `UiProvider::catch_up`. It must be cheap when nothing has arrived,
    /// which it is: `try_recv` on each open connection and nothing else.
    pub fn take_the_replies(&mut self) -> bool {
        let names: Vec<String> = self.connections.keys().cloned().collect();
        let mut anything = self.take_the_test_answer();
        // One of the two doors the tree's rows change through -- see `DatabaseExplorer::revision`.
        self.changed();
        for name in names {
            let answered = match self.connections.get(&name) {
                Some(connection) => connection.worker.take(),
                None => continue,
            };
            for one in answered {
                anything = true;
                let wanted = self
                    .connections
                    .get_mut(&name)
                    .and_then(|connection| connection.waiting.remove(&one.ticket))
                    .unwrap_or(Wanted::Nothing);
                self.apply(&name, one.ticket, wanted, one.answer);
            }
        }
        anything || self.anything_running()
    }

    fn apply(
        &mut self,
        source: &str,
        ticket: u64,
        wanted: Wanted,
        answer: unluminous_db::Answer<Reply>,
    ) {
        let failure = match &answer {
            Ok(_) => None,
            Err(why) => Some(why.clone()),
        };
        match (wanted, answer) {
            (Wanted::Schemas, Ok(Reply::Names(names))) => {
                let loaded = self.loaded.entry(source.to_owned()).or_default();
                loaded.schemas = names;
                loaded.problem = None;
            }
            (Wanted::Items { schema }, Ok(Reply::Items(items))) => {
                // A search index's own declaration is asked for as soon as one is seen, because the
                // tree draws it under the row and a folder that filled in a beat later would flicker.
                // There are rarely more than one or two, and an engine with none asks nothing.
                let searches: Vec<String> = items
                    .iter()
                    .filter(|item| item.kind == Kind::Search)
                    .map(|item| item.name.clone())
                    .collect();
                self.loaded.entry(source.to_owned()).or_default().items.insert(schema, items);
                for name in searches {
                    self.ask(
                        source,
                        Job::SearchIndex { name: name.clone() },
                        Wanted::Search { name },
                    );
                }
            }
            (Wanted::Search { name }, Ok(Reply::Search(index))) => {
                if let Some(index) = *index {
                    self.loaded.entry(source.to_owned()).or_default().searches.insert(name, index);
                }
            }
            (Wanted::Describe { schema, name, then }, Ok(Reply::Table(table))) => {
                let asked = schema.clone();
                self.loaded
                    .entry(source.to_owned())
                    .or_default()
                    .columns
                    .insert((schema, name), table.clone());
                if then == Then::OpenGrid {
                    self.grid_for(source, &asked, table);
                }
            }
            (Wanted::ConsoleTable { page }, Ok(Reply::Table(table))) => {
                if self.page(page).and_then(Page::running) == Some(ticket) {
                    self.console_table_described(page, table);
                }
            }
            (Wanted::Ddl { name, show }, Ok(Reply::Text(text))) => {
                self.last_ddl = Some((name.clone(), text.clone()));
                if show {
                    self.modal = Some(Modal::Ddl { title: name, text });
                }
            }
            (Wanted::Rows { page }, Ok(Reply::Rows(rows))) => self.rows_arrived(page, ticket, rows),
            (Wanted::Written { page }, Ok(Reply::Written(affected))) => {
                if self.page(page).and_then(Page::running) != Some(ticket) {
                    return;
                }
                if let Some(grid) = self.grid_mut(page) {
                    grid.pending.clear();
                    grid.running = None;
                }
                self.problem = Some(format!(
                    "{} statement(s) written, {} row(s) changed.",
                    affected.len(),
                    affected.iter().sum::<u64>()
                ));
                // Read the rows back, because what is on the screen after a write should be what is
                // in the database rather than what was typed.
                self.reload(page);
            }
            (wanted, _) => {
                let Some(why) = failure else { return };
                self.failed(source, ticket, wanted, why);
            }
        }
    }

    /// A result arriving for a console or a grid.
    ///
    /// **A reply that is not the one this page is waiting for is thrown away.** A second query or a
    /// reload can be asked for while the first is still outstanding — the command line makes that
    /// easy — and the older answer arriving afterwards would otherwise overwrite the newer one, so
    /// the grid would end up showing the result of a statement nobody is looking at. The ticket is
    /// what says which, and it is the same reason `unluminous_db::Worker` hands one out at all.
    fn rows_arrived(&mut self, page: u64, ticket: u64, rows: Rows) {
        let Some(page) = self.page_mut(page) else { return };
        if page.running() != Some(ticket) {
            return;
        }
        match &mut page.sheet {
            Sheet::Console(console)
                if console.grid.as_ref().is_some_and(|grid| grid.running == Some(ticket)) =>
            {
                if let Some(grid) = console.grid.as_deref_mut() {
                    console.output.push(rows.summary());
                    grid.running = None;
                    grid.rows = rows;
                    grid.chosen = None;
                    grid.editing = None;
                }
            }
            Sheet::Console(console) => {
                {
                    console.running = None;
                }
                for notice in &rows.notices {
                    console.output.push(notice.clone());
                }
                match rows.columns.is_empty() {
                    // A statement that returned no rows fills `Output`, which is the reference editor's own
                    // arrangement and is what tells an `UPDATE` from a `SELECT` of nothing.
                    true => console.output.push(rows.summary()),
                    false => {
                        console.output.push(rows.summary());
                        console.result = Some(rows);
                    }
                }
            }
            Sheet::Grid(grid) => {
                grid.running = None;
                grid.rows = rows;
                grid.chosen = None;
                grid.editing = None;
            }
        }
    }

    /// Something went wrong, and it is shown where the thing that asked for it is.
    ///
    /// A failure is dropped for the same reason a result is when it belongs to a superseded ticket:
    /// an old refusal landing on a page that is running something else would clear the spinner and
    /// report a statement nobody is waiting for.
    fn failed(&mut self, source: &str, ticket: u64, wanted: Wanted, why: Failure) {
        let said = why.to_string();
        if self.a_console_grid_gives_way(&wanted, ticket) {
            return;
        }
        match wanted {
            // The table a console statement named could not be described, so the statement goes to the
            // server as it was written and the server's own answer is what the console shows.
            Wanted::ConsoleTable { page } => {
                let sql = match self.page(page) {
                    Some(Page { sheet: Sheet::Console(console), .. })
                        if console.running == Some(ticket) =>
                    {
                        console.describing.as_ref().map(|(_, sql)| sql.clone())
                    }
                    _ => None,
                };
                if let Some(sql) = sql {
                    self.send_from_console(page, &sql);
                }
                return;
            }
            Wanted::Rows { page } | Wanted::Written { page } => {
                if let Some(page) =
                    self.page_mut(page).filter(|page| page.running() == Some(ticket))
                {
                    match &mut page.sheet {
                        Sheet::Console(console) if console.running != Some(ticket) => {
                            if let Some(grid) = console.grid.as_deref_mut() {
                                grid.running = None;
                                grid.failure = Some(why);
                            }
                        }
                        Sheet::Console(console) => {
                            console.running = None;
                            console.output.push(said.clone());
                            console.failure = Some(why);
                        }
                        Sheet::Grid(grid) => {
                            grid.running = None;
                            grid.failure = Some(why);
                        }
                    }
                }
            }
            _ => {
                self.loaded.entry(source.to_owned()).or_default().problem = Some(said.clone());
            }
        }
        self.problem = Some(said);
    }

    /// A console grid whose **first** read failed gives way to the statement it was made from.
    ///
    /// The grid reads the statement through a statement of its own, so a server's refusal of it would quote
    /// text nobody typed, and an engine that refused a subquery would make a `SELECT` that runs anywhere else
    /// fail here. So the statement is sent as it was written, and what the server says about that is what is
    /// shown. A grid that has read rows before keeps its failure, because that one is about a filter or a sort
    /// somebody has typed into it since. True when it gave way.
    fn a_console_grid_gives_way(&mut self, wanted: &Wanted, ticket: u64) -> bool {
        let Wanted::Rows { page } = wanted else { return false };
        let sql = match self.page(*page) {
            Some(Page { sheet: Sheet::Console(console), .. }) => match console.grid.as_deref() {
                Some(grid)
                    if grid.running == Some(ticket)
                        && grid.rows.columns.is_empty()
                        && grid.at == 0 =>
                {
                    unluminous_db::sql::at(&console.text, console.caret)
                        .map(|statement| statement.to_send().to_owned())
                }
                _ => None,
            },
            _ => None,
        };
        match sql {
            Some(sql) if !sql.is_empty() => {
                self.send_from_console(*page, &sql);
                true
            }
            _ => false,
        }
    }

    /// Hand a page a result as though a worker had answered it. Tests only.
    ///
    /// `UiProvider::as_any_mut` exists for exactly two callers and a test is one of them; these two
    /// are the same idea for a path that otherwise needs a real server to be slow at the right moment.
    #[cfg(test)]
    pub fn take_a_result_for_tests(&mut self, page: u64, ticket: u64, rows: Rows) {
        self.rows_arrived(page, ticket, rows);
    }

    /// Say which job a page is waiting for. Tests only.
    #[cfg(test)]
    pub fn mark_running_for_tests(&mut self, page: u64, ticket: u64) -> Option<u64> {
        match self.page_mut(page)? {
            Page { sheet: Sheet::Console(console), .. } => match console.grid.as_deref_mut() {
                Some(grid) => grid.running = Some(ticket),
                None => console.running = Some(ticket),
            },
            Page { sheet: Sheet::Grid(grid), .. } => grid.running = Some(ticket),
        }
        Some(ticket)
    }

    // ------------------------------------------------------------------ what it is showing

    fn view_value(&self) -> serde_json::Value {
        serde_json::json!({
            "sources": self.configuration.sources.iter().map(|source| serde_json::json!({
                "name": source.name,
                "engine": source.engine.name(),
                "url": source.url(),
                "read_only": source.read_only,
                "password": source.secret.describe(),
                "connected": self.is_connected(&source.name),
                "version": self.version_of(&source.name),
                "busy": self.is_busy(&source.name),
                "problem": self.loaded.get(&source.name).and_then(|loaded| loaded.problem.clone()),
            })).collect::<Vec<serde_json::Value>>(),
            "chosen": self.configuration.chosen,
            "page_size": self.configuration.page_size,
            "tree": self.loaded.keys().map(|name| serde_json::json!({
                "source": name,
                "open": self.open_sources.contains(name),
                "schemas": self.loaded.get(name).map(|loaded| loaded.schemas.clone()).unwrap_or_default(),
                "open": self.loaded.get(name).map(|loaded| loaded.open_schemas.iter().cloned().collect::<Vec<String>>()).unwrap_or_default(),
                "items": self.loaded.get(name).map(|loaded| loaded.items.iter().map(|(schema, items)| serde_json::json!({
                    "schema": schema,
                    "items": items.iter().map(|item| serde_json::json!({ "name": item.name, "kind": item.kind.name() })).collect::<Vec<serde_json::Value>>(),
                })).collect::<Vec<serde_json::Value>>()).unwrap_or_default(),
            })).collect::<Vec<serde_json::Value>>(),
            "chosen_row": self.chosen.as_ref().map(|chosen| serde_json::json!({
                "source": chosen.source, "schema": chosen.schema, "name": chosen.name,
            })),
            "pages": self.pages.iter().map(|page| self.page_value(page)).collect::<Vec<serde_json::Value>>(),
            "current": self.pages.get(self.current).map(|page| page.id),
            // **Whether a Test Connection is still out** (`task-1984` S2). It is on a thread now, so
            // the answer arrives on a later frame and there has to be a way to ask whether it has --
            // which is the same sentence `busy` above makes about a query, and what the dialog itself
            // draws as `Testing…`.
            "testing": self.is_testing(),
            // Which dialog is in front of somebody, by name. An agent that has just asked for one
            // has no other way to know it arrived, and one that is about to press a button in the
            // pane behind it needs to know there is something over it — `task-1795`.
            "modal": self.modal.as_ref().map(|modal| match modal {
                Modal::Source(_) => "source",
                Modal::Preview { .. } => "preview",
                Modal::Ddl { .. } => "ddl",
                Modal::Vector { .. } => "vector",
                Modal::NewTable(_) => "new-table",
            }),
            // What the New Table dialog is about to send, from `unluminous_db::sql::create_table` — the
            // same call `Create` makes and the same one the dialog draws beside the form, so what is
            // read here, what is on the screen and what happens cannot come apart. It is the only way
            // in from outside the window: see `components::modal::monospaced`.
            "new_table_sql": match &self.modal {
                Some(Modal::NewTable(form)) => {
                    let engine = self
                        .configuration
                        .source(&form.source)
                        .map_or(unluminous_db::source::Engine::Postgres, |source| source.engine);
                    match unluminous_db::sql::create_table(&form.schema, &form.name, &form.columns, engine) {
                        Ok(sql) => serde_json::json!(sql),
                        Err(why) => serde_json::json!({ "problem": why }),
                    }
                }
                _ => serde_json::Value::Null,
            },
            "problem": self.problem,
        })
    }

    fn page_value(&self, page: &Page) -> serde_json::Value {
        match &page.sheet {
            Sheet::Console(console) => serde_json::json!({
                "id": page.id,
                "kind": "console",
                "title": page.title(),
                "source": console.source,
                "schema": console.schema,
                "text": console.text,
                "running": page.running().is_some(),
                "output": console.output,
                // A result read once, such as a `PRAGMA`'s, is never a table's rows, so nothing there is a
                // declared vector.
                "result": console.result.as_ref().map(|rows| rows_value(rows, &[])),
                // What a `SELECT` answered, as the grid it is shown in, with the same fields a table's page has.
                "grid": console.grid.as_deref().map(|grid| grid_value(page, grid)),
                "failure": console.failure.as_ref().map(std::string::ToString::to_string),
            }),
            Sheet::Grid(grid) => grid_value(page, grid),
        }
    }
}

/// A grid as data, for a table's page and for a console showing what its `SELECT` answered.
fn grid_value(page: &Page, grid: &Grid) -> serde_json::Value {
    let mut value = serde_json::json!({
                "id": page.id,
                "kind": "grid",
                "title": page.title(),
                "source": grid.source,
                "schema": grid.table.schema,
                "table": grid.table.name,
                "key": grid.table.key,
                "editable": grid.table.can_be_changed() && grid.kind.can_be_changed(),
                "why_not_editable": why_not(grid),
                "where": grid.where_clause,
                "order_by": grid.order_by,
                "page": grid.at,
                "running": grid.running.is_some(),
                "pending": grid.pending.len(),
                "added": grid.pending.added().len(),
                "editing": grid.editing.as_ref().map(|editing| serde_json::json!({
                    "row": editing.at + 1,
                    "column": grid.rows.columns.get(editing.column).map(|column| column.name.clone()),
                    "text": editing.text,
                })),
                "rows": rows_value(&grid.rows, &grid.table.vector_columns),
                "failure": grid.failure.as_ref().map(std::string::ToString::to_string),
    });
    value["reads"] = serde_json::Value::String(
        match &grid.reads {
            Reads::Table => "table",
            Reads::Columns { .. } => "columns",
            Reads::Statement(_) => "statement",
        }
        .to_owned(),
    );
    value
}

/// Why a grid cannot be edited, in one sentence, or nothing when it can.
///
/// **Three reasons rather than one**, because they are different situations and a person acting on
/// the sentence needs to know which. A view's rows really do belong to something else. A search
/// index's rows belong to it and simply cannot be addressed - it has no key of its own. And a shadow
/// table *can* be addressed perfectly well and must not be, which is the one that would otherwise
/// read as an arbitrary refusal.
pub fn why_not(grid: &Grid) -> Option<String> {
    if let Reads::Statement(_) = grid.reads {
        return Some(
            "These rows come from a statement rather than from one table, so they cannot be changed here. \
             A `SELECT` of one table's own columns, or the table opened from the tree, can be edited."
                .to_owned(),
        );
    }
    // The table knows about ownership, so a shadow table's own sentence - which names the object to
    // write to instead - wins over anything composed from the kind.
    if grid.table.owned_by.is_some() {
        return grid.table.why_not_changeable();
    }
    // **A search index is recognised by its schema rather than by the tree**, because a grid can be
    // opened without the tree ever having been read: `plugins run database open docs` on a window
    // nobody has clicked in leaves `kind` at its default, and the released build then explained a
    // search table with "has no primary key" - true, and the weaker of the two answers. The schema
    // says what this is whichever way the grid was opened.
    let is_a_search = grid.kind == Kind::Search || !grid.table.vector_columns.is_empty();
    if is_a_search && grid.table.key.is_empty() {
        return Some(format!(
            "`{}` is a search index, and it has no key of its own: a row here is found by searching rather than addressed, so there is no way to name one and change only it.",
            grid.table.name
        ));
    }
    if !grid.kind.can_be_changed() {
        return Some(match grid.kind {
            Kind::Search => format!(
                "`{}` is a search index, and it has no key of its own: a row here is found by searching rather than addressed, so there is no way to name one and change only it.",
                grid.table.name
            ),
            kind => format!(
                "`{}` is a {}, and its rows belong to the tables underneath it.",
                grid.table.name,
                kind.name()
            ),
        });
    }
    grid.table.why_not_changeable()
}

/// A result as data, bounded, for `plugins view` and for a test.
fn rows_value(rows: &Rows, vectors: &[String]) -> serde_json::Value {
    // Which result columns the schema says hold a vector, worked out once rather than per cell.
    let is_a_vector: Vec<bool> =
        rows.columns.iter().map(|column| vectors.contains(&column.name)).collect();
    serde_json::json!({
        "columns": rows.columns.iter().map(|column| serde_json::json!({
            "name": column.name,
            "type": column.type_name,
            "key": column.in_key,
        })).collect::<Vec<serde_json::Value>>(),
        // Bounded, because an agent handed three thousand rows to learn how many there are stops
        // asking — `task-1704`'s rule about proportionate replies.
        //
        // **And a vector reads as a vector here too.** Driving the released window found this: the
        // cell drew `8d · |v| 1.000 · […]` while `result` answered
        // `32 bytes: 00 00 00 00 bf 69 34 3e…`, so a person and an agent looking at the same row saw
        // different things — and the agent saw the one that is no use. `plugins run database vector`
        // gives the components; what belongs here is what the grid is showing.
        "rows": rows.rows.iter().take(50).map(|row| row.iter().enumerate().map(|(at, value)| match value {
            Value::Null => serde_json::Value::Null,
            other => serde_json::Value::String(
                match is_a_vector.get(at).copied().unwrap_or(false) {
                    true => other
                        .bytes()
                        .and_then(unluminous_db::Vector::decode)
                        .map_or_else(|| other.display(), |vector| vector.summary()),
                    false => other.display(),
                },
            ),
        }).collect::<Vec<serde_json::Value>>()).collect::<Vec<Vec<serde_json::Value>>>(),
        "count": rows.rows.len(),
        "more": rows.more,
        "affected": rows.affected,
        "tag": rows.tag,
        "elapsed_ms": rows.elapsed.as_millis() as u64,
        "notices": rows.notices,
    })
}

/// The statement that fills a grid.
///
/// `WHERE` and `ORDER BY` are fragments somebody typed, pasted in as they are: they are part of the
/// statement's own grammar and cannot be parameters in any engine, and pretending otherwise would
/// mean building a filter language the reference editor's own users go round anyway. Every **value** the grid
/// writes back is bound — see `unluminous_db::edit`.
///
/// ## With no `ORDER BY` typed, the key is the order, and that is a correctness fix rather than a
/// tidiness one
///
/// `LIMIT … OFFSET` over an unordered result is not a page sequence: no engine promises an order it
/// was not asked for, so page two can repeat a row from page one and skip another entirely. It shows
/// up sooner than that, though, and it showed up on the first real PostgreSQL run of this code: an
/// `UPDATE` moves a row to the end of the heap, so reloading after a submit put the row that had just
/// been edited at the bottom and the grid appeared to shuffle itself. A row number then meant a
/// different row than it had a moment earlier, which is what `set <n>` and `delete-row <n>` are
/// addressed by.
///
/// So a table with a key is read **in key order** unless somebody has typed an order of their own.
/// A table with no key has nothing to order by, and its rows are read-only anyway.
pub fn select_for(grid: &Grid, engine: Engine, limit: usize, at: usize) -> String {
    // A console statement whose rows are not one table's own is read inside a subquery, so the grid's own
    // `WHERE`, `ORDER BY` and paging apply to its rows as they apply to a table's (`task-2253`). Its comments
    // are taken out first, because a `--` comment at the end would swallow the closing bracket.
    let (columns, from) = match &grid.reads {
        Reads::Statement(sql) => (
            "*".to_owned(),
            format!("({}) as \"result\"", unluminous_db::sql::strip_comments(sql).trim()),
        ),
        Reads::Table | Reads::Columns { .. } => {
            (the_column_list(grid, engine), grid.table.qualified('"'))
        }
    };
    let mut statement = format!("select {columns} from {from}");
    if !grid.where_clause.trim().is_empty() {
        statement.push_str(&format!(" where {}", grid.where_clause.trim()));
    }
    match grid.order_by.trim() {
        typed if !typed.is_empty() => statement.push_str(&format!(" order by {typed}")),
        _ if !grid.table.key.is_empty() => {
            let by = grid
                .table
                .key
                .iter()
                .map(|name| unluminous_db::catalog::quoted(name, '"'))
                .collect::<Vec<String>>()
                .join(", ");
            statement.push_str(&format!(" order by {by}"));
        }
        _ => {}
    }
    let (skip, ask) = the_window(grid, limit, at);
    statement.push_str(&format!(" limit {ask} offset {skip}"));
    statement
}

/// How many rows a page asks for and how many it skips.
///
/// One more than is kept, which is what makes `1-200 of 200+` honest. A console statement's own `LIMIT` is a
/// cap across every page, and its `OFFSET` moves where the first page starts, so the grid shows exactly the
/// rows the statement would have answered, a page at a time.
fn the_window(grid: &Grid, limit: usize, at: usize) -> (usize, usize) {
    let (cap, offset) = match &grid.reads {
        Reads::Columns { limit: cap, offset, .. } => (*cap, *offset),
        Reads::Table | Reads::Statement(_) => (None, 0),
    };
    let start = at * limit;
    let ask = match cap {
        Some(cap) => cap.saturating_sub(start).min(limit + 1),
        None => limit + 1,
    };
    (offset + start, ask)
}

/// Whether a column name as a statement wrote it names the key column `key`: by its exact text when it was
/// quoted, and without regard to case when it was not, which is how both engines compare an unquoted name.
fn names_the_column(written: &str, key: &str) -> bool {
    match unluminous_db::one_table::is_quoted(written) {
        true => unluminous_db::one_table::unquote(written) == key,
        false => written.eq_ignore_ascii_case(key),
    }
}

/// The column list a table's grid asks for.
///
/// A console statement that named its columns gets those columns, with any key column it left out put in
/// front of them: a row is changed by its key, so a grid without the key could show the rows and never save one.
fn the_column_list(grid: &Grid, engine: Engine) -> String {
    // SQLite's implicit key is not in `select *`, so a table addressed by it has to ask for it by
    // name — and by **whichever** of its three names this table has not shadowed, which is what
    // `unluminous_db::sqlite::ROWID_ALIASES` decides. Asking for the literal `rowid` on a table that
    // declares a column of that name would select the declared column instead.
    let by_rowid = engine == Engine::Sqlite
        && grid
            .table
            .key
            .first()
            .is_some_and(|name| unluminous_db::sqlite::ROWID_ALIASES.contains(&name.as_str()));
    if let Reads::Columns { columns: named, .. } = &grid.reads {
        if !named.is_empty() {
            let mut list: Vec<String> = grid
                .table
                .key
                .iter()
                .filter(|key| !named.iter().any(|written| names_the_column(written, key)))
                .map(|key| match by_rowid {
                    true => key.clone(),
                    false => unluminous_db::catalog::quoted(key, '"'),
                })
                .collect();
            list.extend(named.iter().cloned());
            return list.join(", ");
        }
    }
    let mut columns = match by_rowid {
        true => format!("{}, *", grid.table.key[0]),
        false => "*".to_owned(),
    };
    // **A vector is not in `select *` either**, and for the same reason the rowid is not: on a search
    // table the vector arrives through a *hidden* column, so a grid that asked for `*` would show the
    // title and the body of a row whose whole point is the embedding beside them. Measured against
    // the engine: `select * from docs` answers `["title", "body"]`, and naming the column is what puts
    // it back. This is the one line that decides whether the ticket's "ways to see our vectors" is
    // true of the grid at all. From Inillucent 2.0.2 `table_info` leaves hidden columns out, so the
    // hidden `vector` is not in the column list and is named here; a vector column that is an ordinary
    // column, such as `v` of a `%_content` shadow, is already in `*` and is not named twice.
    for column in &grid.table.vector_columns {
        if !grid.table.columns.iter().any(|had| had.name == *column) {
            columns.push_str(&format!(", {}", unluminous_db::catalog::quoted(column, '"')));
        }
    }
    columns
}

impl UiProvider for DatabaseExplorer {
    fn id(&self) -> &'static str {
        "database"
    }

    fn open(&mut self, context: &Context) -> Result<(), String> {
        self.folder = context.folder.clone();
        self.project = context.project.clone();
        self.wake = context.wake.clone();
        if let Some(folder) = &self.folder {
            let (configuration, refused) = Configuration::read(folder);
            self.configuration = configuration;
            if !refused.is_empty() {
                self.problem = Some(refused.join(" "));
            }
            // The Agent-Tasks board, offered once as a data source (`task-2293`). Its folder is the board
            // plugin's own, beside this one, and its file is whatever that plugin's settings name.
            if let Some(board_folder) = folder.parent().map(|plugins| plugins.join("agent-tasks")) {
                let board = crate::services::agent_tasks::Configuration::read(&board_folder)
                    .database_path(Some(&board_folder));
                // Only a board that is there: a window that has never opened the board has nothing to offer.
                if let Some(board) = board.filter(|board| board.is_file()) {
                    if self.configuration.offer_the_board(&board) {
                        if let Err(problem) = self.configuration.write(folder) {
                            self.problem = Some(problem);
                        }
                    }
                }
            }
        }
        // **Nothing is connected at startup**, which is the laziness `UiProvider::open`'s own comment
        // describes: opening a pane must not open a socket to somebody's server. A source is
        // connected when its row in the tree is opened.
        self.open = true;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }

    /// The wells, the raised cards and the gradient on Execute — the decoration `egui` cannot draw.
    fn draws_chrome(&self) -> bool {
        true
    }

    fn pane(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::database::pane(self, ui, look)
    }

    fn tab(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::database::tab(self, ui, look)
    }

    fn settings(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::database::settings_page::show(self, ui, look)
    }

    fn modal(&mut self, ctx: &egui::Context, look: &Look<'_>) -> (Vec<Request>, bool) {
        crate::components::database::modal::show(self, ctx, look)
    }

    fn command(&mut self, command: &str, arguments: &[String]) -> Result<Answer, String> {
        crate::services::database::commands::run(self, command, arguments)
    }

    fn commands(&self) -> Vec<(&'static str, &'static str)> {
        commands::LIST.to_vec()
    }

    fn view(&self) -> serde_json::Value {
        self.view_value()
    }

    fn catch_up(&mut self) -> bool {
        self.take_the_replies()
    }

    fn asking(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.asking)
    }

    fn zoomed(&mut self, ratio: f32, above: f32) {
        if !ratio.is_finite() || ratio <= 0.0 {
            return;
        }
        self.scroll_to = Some(((self.scrolled + above) * ratio - above).max(0.0));
    }

    fn showing(&mut self, project: Option<&std::path::Path>, _file: Option<&std::path::Path>) {
        self.project = project.map(std::path::Path::to_path_buf);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn close(&mut self) {
        // Every connection goes, which is what `Worker`'s own `Drop` is for: a socket left open on
        // somebody's server after the pane that opened it has gone is a connection nobody can see.
        self.connections.clear();
        self.open = false;
    }
}

pub mod commands;

#[cfg(test)]
mod tests;
