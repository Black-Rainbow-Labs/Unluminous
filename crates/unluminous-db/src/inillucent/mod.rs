//! Speaking to Inillucent, through the driver that exists for exactly this.
//!
//! Inillucent is the first-party engine in `C:\jason\dev\inillucent`: a SQL database and a retrieval
//! engine in one file, and the reason this module exists is the second half of that sentence. It
//! stores **vectors**, and a database explorer that can list its tables but cannot show an embedding
//! has not finished the job — see [`crate::vector`].
//!
//! ## Why the driver rather than the engine
//!
//! `inillucent-driver` depends on `inillucent-engine` and on nothing else in that workspace, and that
//! single edge is the whole point of it: the rearchitecture is still deleting crates underneath, and a
//! consumer holding the driver is unaffected by all of it. Reaching past it — into the pool, the trees
//! or the catalog — would bind this window to the half of that workspace still being moved.
//!
//! The C ABI beside it (`inillucent-driver-capi`, 53 entry points) is for the other languages. Rust
//! does not go through it: it would cost a pointer round trip and a `catch_unwind` per call to reach
//! Rust from Rust, and the driver's own README says so.
//!
//! ## What is different from the other two engines, and why each one is
//!
//! - **A row count is exact.** The engine materialises, so `Rows::total` is the number of rows the
//!   statement really produced rather than an estimate, and the grid can say `1-200 of 4,317` and mean
//!   it. PostgreSQL and SQLite are asked for `limit + 1` and answer `200+`. The cost is that a query
//!   over a large table costs what the whole result costs, which is why the console's own `LIMIT` is
//!   still the thing to reach for.
//! - **There is no way to stop a statement**, so there is no `Stopper` — the capability table reports
//!   `cancel: no`, and its note is worth quoting: *"a cancel that set one would return success and do
//!   nothing. Do not draw a Stop button."* Unluminous's rule is that a control which can never apply is
//!   **absent**, not dimmed, so that is what happens.
//! - **Read-only is the driver's, not the file's.** The capability table says `partial` and says why:
//!   a statement that does not bind to a query is refused above the engine, but the file is still open
//!   for writing. That is weaker than `SQLITE_OPEN_READONLY` and the source page says so rather than
//!   implying the stronger thing.
//! - **A connection lasts one statement**, which is the one place this backend is thinner than the
//!   others. `Connection<'_>` borrows the `Database`, so a session holding both would be a
//!   self-referential struct; the engine has `connect_as`, which is exactly the answer for a caller
//!   handing out a connection per call, and the driver does not expose it yet. Filed rather than
//!   worked around. What it costs is written down in [`Session::connect`].

use std::path::{Path, PathBuf};

use inillucent_driver::{Database, OpenOptions, Status};

use crate::catalog::{Item, Table};
use crate::rows::{Answer, Failure, Rows};
use crate::value::{Column, Value};

pub mod search;

pub use search::SearchIndex;

/// How many buffer-pool frames a data source gets, at the engine's 32 KiB page.
///
/// **512 frames is 16 MiB, where the driver's own default is 4,096 and therefore 128 MiB.** That
/// default is right for the engine's benchmarks, which measure a resident working set on purpose, and
/// wrong for a pane in an editor: a person with four data sources open would be holding half a
/// gigabyte of page cache for three databases they are not looking at. The engine reads through the
/// pool either way, so this is a memory-for-time trade and not a limit on what can be opened.
pub const EXPLORER_FRAMES: usize = 512;

/// The bytes an Inillucent database begins with.
pub const MAGIC: &[u8] = b"RDB2\0\0\0\0";

/// The bytes a SQLite database begins with.
pub const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

/// One open database file.
pub struct Session {
    database: Database,
    file: PathBuf,
    read_only: bool,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("InillucentSession")
            .field("file", &self.file)
            .field("read_only", &self.read_only)
            .finish()
    }
}

impl Session {
    /// Open a file.
    ///
    /// **A file that is not there is refused rather than created.** The driver's `create` is on by
    /// default because an application opening its own database means "make one if there is none"; an
    /// explorer means the opposite, and `task-1777` already paid for this once when `rusqlite`'s
    /// `SQLITE_OPEN_CREATE` turned a mistyped path into an empty database and a data source with
    /// nothing in it.
    ///
    /// **A SQLite file is refused with somewhere to go.** The driver would say that neither meta page
    /// is readable, which is true and useless; what a person needs to hear is that the file is a
    /// SQLite database, that Inillucent does not read one, and that there is an import.
    ///
    /// @param file - the database file
    /// @param read_only - refuse any statement that is not a query
    pub fn open(file: &Path, read_only: bool) -> Answer<Session> {
        if !file.exists() {
            return Err(Failure::said(format!("there is no file at {}.", file.display())));
        }
        if starts_with(file, SQLITE_MAGIC) {
            return Err(Failure::said(format!(
                "{} is a SQLite database, and Inillucent does not read one — the two file formats are \
                 unrelated. Add it as a SQLite data source, or import it with `plugins run database \
                 import`, which writes a new Inillucent database beside it and never touches the \
                 original.",
                file.display()
            )));
        }
        let options = OpenOptions {
            create: false,
            read_only,
            cache_frames: EXPLORER_FRAMES,
            // Off, and deliberately: the driver's diagnostic text may carry a file-system path or a
            // bound value, and this window puts a failure in front of a person and into a transcript.
            diagnostics: false,
        };
        let database = Database::open_with(file, options).map_err(said)?;
        Ok(Session { database, file: file.to_path_buf(), read_only })
    }

    /// Read a SQLite database and build an Inillucent one beside it.
    ///
    /// The engine's own route between the two formats, and the only one: the source is never written
    /// to, and the new file is the source path with `.rdb` on the end.
    ///
    /// @param file - the SQLite database to read
    pub fn import(file: &Path) -> Answer<PathBuf> {
        if !file.exists() {
            return Err(Failure::said(format!("there is no file at {}.", file.display())));
        }
        let database = Database::import_sqlite(file).map_err(said)?;
        Ok(database.path().to_path_buf())
    }

    /// What the driver and the engine call themselves, which is what Test Connection reports.
    pub fn version(&self) -> String {
        format!("Inillucent · {}", inillucent_driver::version())
    }

    /// A connection for one piece of work.
    ///
    /// **Each call is its own session**, because `Connection<'_>` borrows the `Database` and a struct
    /// holding both would be self-referential. `inillucent_engine::connect::Database::connect_as`
    /// exists for precisely this shape of caller and the driver does not expose it, so what is lost is
    /// stated rather than hidden: a `CREATE TEMP TABLE` or an `ATTACH` typed into the console belongs
    /// to the statement that made it. Everything the explorer itself does — introspection, a query
    /// with a limit, one transaction — is a single call and is unaffected.
    ///
    /// Foreign keys are turned on here rather than at open for the same reason. SQLite's default is
    /// off and so is this engine's; every other tool that edits a database turns them on, and a
    /// `DELETE` that should have been refused succeeding is the fault that makes it worth the pragma
    /// on every connection.
    fn connect(&self) -> inillucent_driver::Connection<'_> {
        let connection = self.database.connect();
        if !self.read_only {
            // A failure here is not worth refusing the caller's own work over: it would mean this
            // build has no such pragma, in which case foreign keys are off and the engine says so in
            // its own capability table.
            let _ = connection.execute("PRAGMA foreign_keys = ON", &[]);
        }
        connection
    }

    /// Run one statement, with its values bound as parameters.
    ///
    /// @param statement - the SQL
    /// @param values - what `?1`, `?2` … are bound to
    /// @param limit - how many rows to keep
    pub fn run(&mut self, statement: &str, values: &[Value], limit: usize) -> Answer<Rows> {
        let bound: Vec<inillucent_driver::Value> = values.iter().map(to_driver).collect();
        let connection = self.connect();
        let answered = connection.query(statement, &bound, limit).map_err(said)?;
        Ok(from_driver(answered))
    }

    /// Run several statements as one transaction, stopping and undoing everything at the first
    /// failure. This is what Save does.
    ///
    /// `check` is the driver's own argument and runs **before the commit**, which is the rule
    /// `engine::write` already keeps for the other two engines: a postcondition tested after the
    /// commit is a report about something that has already happened.
    pub fn in_one_transaction(
        &mut self,
        work: &[(String, Vec<Value>)],
        check: impl Fn(&[u64]) -> Answer<()>,
    ) -> Answer<Vec<u64>> {
        let bound: Vec<(String, Vec<inillucent_driver::Value>)> = work
            .iter()
            .map(|(statement, values)| {
                (statement.clone(), values.iter().map(to_driver).collect())
            })
            .collect();
        let connection = self.connect();
        connection
            .transaction(&bound, |affected| {
                // The driver's check answers in its own error type, so this crate's refusal — which
                // is the sentence a person reads — is carried across as the message rather than
                // being replaced by one of the driver's.
                check(affected).map_err(|why| {
                    inillucent_driver::Error::said(Status::InvalidState, why.message)
                })
            })
            .map_err(said)
    }

    /// The one schema an Inillucent file has, plus anything attached.
    ///
    /// Answered as a list so the tree has the same shape for every engine and
    /// `components::database::tree` never asks which one it is drawing.
    pub fn schemas(&mut self) -> Answer<Vec<String>> {
        let connection = self.connect();
        connection.schemas().map_err(said)
    }

    /// Everything in the file that the tree draws a row for.
    ///
    /// The driver answers tables, views, indexes and triggers; the two kinds it does not know about
    /// are read out of the schema's own text here — see [`search`].
    pub fn items(&mut self) -> Answer<Vec<Item>> {
        let connection = self.connect();
        let listed = connection.items().map_err(said)?;
        let declarations = search::declarations(&connection)?;
        Ok(listed
            .into_iter()
            .map(|item| Item { kind: search::kind_of(&item, &declarations), name: item.name })
            .collect())
    }

    /// One table's columns, its key, and which of its columns hold vectors.
    pub fn table(&mut self, name: &str) -> Answer<Table> {
        let connection = self.connect();
        let described = connection.table(name).map_err(said)?;
        let mut table = Table { schema: String::new(), name: name.to_owned(), ..Table::default() };
        for (at, column) in described.columns.iter().enumerate() {
            let mut into = Column::new(&column.name, &column.declared_type);
            into.not_null = described.column_not_null(at);
            into.in_key = described.column_in_key(&column.name);
            table.columns.push(into);
        }
        table.key = described.key.clone();
        let declarations = search::declarations(&connection)?;
        table.vector_columns = search::vector_columns(name, &declarations, &table);
        // What makes a shadow table read only despite its perfectly good key. Without this line
        // `docs_content` passes the addressability rule and a hand-written UPDATE leaves the search
        // index describing something the row no longer says.
        table.owned_by = declarations.owner_of(name).map(str::to_owned);
        Ok(table)
    }

    /// The `CREATE` statement, which the file keeps verbatim.
    pub fn ddl(&mut self, name: &str) -> Answer<String> {
        let connection = self.connect();
        connection.ddl(name).map_err(said)
    }

    /// What a search table declared about itself, or `None` when it is not one.
    pub fn search_index(&mut self, name: &str) -> Answer<Option<SearchIndex>> {
        let connection = self.connect();
        let declarations = search::declarations(&connection)?;
        match declarations.is_search(name) {
            false => Ok(None),
            true => search::read_config(&connection, name).map(Some),
        }
    }

    /// What this build of the engine does and does not do.
    ///
    /// Read from the driver at run time rather than copied into Unluminous, because a capability list
    /// nobody checks decays into claims that were true once — which is the failure the driver's own
    /// two-way probe exists to prevent, and copying it here would reintroduce it one layer up.
    pub fn capabilities(&self) -> &'static [inillucent_driver::Capability] {
        inillucent_driver::CAPABILITIES
    }

    /// Whether a statement can be stopped once it has started. It cannot, and see the module comment.
    pub fn can_be_stopped() -> bool {
        matches!(inillucent_driver::supports("cancel"), Some(inillucent_driver::Support::Yes))
    }

    /// Fold the log into the file, which is what makes the next open cheap.
    pub fn checkpoint(&mut self) -> Answer<()> {
        self.database.checkpoint().map_err(said)
    }

    /// Walk every tree, which is the engine's own `PRAGMA integrity_check`.
    pub fn integrity_check(&mut self) -> Answer<()> {
        self.database.integrity_check().map_err(said)
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
}

/// Whether a file begins with these bytes.
///
/// Reads only as many bytes as the marker is long, so asking is cheap enough to do before every open
/// and on a file that turns out to be enormous.
///
/// @param file - the file to look at
/// @param marker - the bytes it should begin with
pub fn starts_with(file: &Path, marker: &[u8]) -> bool {
    use std::io::Read;
    let mut head = vec![0u8; marker.len()];
    match std::fs::File::open(file).and_then(|mut open| open.read_exact(&mut head)) {
        Ok(()) => head == marker,
        Err(_) => false,
    }
}

/// Whether this file is an Inillucent database, by its own first eight bytes.
///
/// Used by the **SQLite** session as well, so that a `.rdb` added as a SQLite data source is told what
/// it really is rather than being handed SQLite's `file is not a database`.
pub fn is_an_inillucent_database(file: &Path) -> bool {
    starts_with(file, MAGIC)
}

/// A cell going out.
fn to_driver(value: &Value) -> inillucent_driver::Value {
    match value {
        Value::Null => inillucent_driver::Value::Null,
        // Text, always, and never a guess at a number: the engine applies the column's own affinity on
        // the way in, which is a rule it has and this client would only ever get differently.
        Value::Text(text) => inillucent_driver::Value::Text(text.clone()),
        Value::Bytes(bytes) => inillucent_driver::Value::Blob(bytes.clone()),
    }
}

/// A cell coming back.
///
/// The driver's values are typed and this crate's are text, which is not a loss: `value.rs` says why
/// every cell arrives as text, and rendering a number here is the same three lines the SQLite session
/// already has for a `ValueRef`.
fn from_driver_value(value: &inillucent_driver::Value) -> Value {
    match value {
        inillucent_driver::Value::Null => Value::Null,
        inillucent_driver::Value::Integer(number) => Value::Text(number.to_string()),
        inillucent_driver::Value::Real(number) => Value::Text(number.to_string()),
        inillucent_driver::Value::Text(text) => Value::Text(text.clone()),
        inillucent_driver::Value::Blob(bytes) => Value::Bytes(bytes.clone()),
    }
}

/// A whole result coming back.
///
/// **`more` is the driver's, and `total` is exact.** The other two engines are asked for `limit + 1`
/// rows and answer `200+` because nobody counted the rest; this engine materialises, so the count is
/// the real one and the grid can say how many there are.
fn from_driver(answered: inillucent_driver::Rows) -> Rows {
    Rows {
        columns: answered
            .columns
            .iter()
            .map(|column| Column::new(&column.name, &column.declared_type))
            .collect(),
        rows: answered
            .rows
            .iter()
            .map(|row| row.iter().map(from_driver_value).collect())
            .collect(),
        affected: answered.affected,
        tag: answered.tag,
        elapsed: answered.elapsed,
        more: answered.more,
        notices: Vec::new(),
    }
}

/// The driver's own words, which is what a refusal quotes.
///
/// **`status` becomes the code**, so `unsupported` is visible in the console beside the sentence. That
/// is the distinction the driver was built to make and the one thing a caller must not lose: *this
/// engine cannot do that yet* is a different answer from *check your spelling*, and an explorer that
/// folded them together would have thrown the design away.
fn said(why: inillucent_driver::Error) -> Failure {
    let mut failure = Failure {
        message: why.message,
        code: why.status.name().to_owned(),
        position: why.offset.map(|offset| offset.saturating_add(1)),
        ..Failure::default()
    };
    if let Some(feature) = why.feature {
        failure.detail = format!("The engine has not built this yet: {feature}.");
    }
    failure
}

#[cfg(test)]
mod tests;
