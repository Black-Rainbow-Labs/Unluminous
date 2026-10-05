//! The board's connection to Inillucent, in the shape every query in `store` is written in.
//!
//! `task-2193`: *"Agent tasks should use inillucent, not sqlite."* The store has about a hundred
//! queries, each written as `execute(sql, params![…])` or `query_row(sql, params![…], read)`. Rewriting
//! each one against `inillucent_driver` directly would change the wording of every query to change
//! nothing about what it asks. So this file answers in that shape instead, and the queries were left
//! as they were apart from the few the engine reads differently, which `store` says beside each.
//!
//! ## One session, kept by its number
//!
//! `inillucent_driver::Connection` borrows the `Database`, so a struct cannot hold both. What is kept
//! is the session's number, and every call continues that session through `Database::session_as`.
//! That matters for one thing: `last_insert_rowid` belongs to a session, and a fresh session for each
//! call would answer 0 after every insert.
//!
//! ## A transaction belongs to the database
//!
//! Two sessions on one database share one transaction in this engine, so [`Db::in_transaction`] opens
//! it on one connection and every statement the work runs joins it, whichever handle it goes through.
//! Dropping the driver's `Transaction` rolls it back, so an early `?` in the work leaves the board as
//! it was.

use std::path::Path;

use inillucent_driver::{Database, OpenOptions, Value};

/// What went wrong, as the sentence a person reads, and whether it was a query that found no row.
///
/// `NoRow` is kept apart from a real failure so [`Optional::optional`] can turn it into `None`, which
/// is what `rusqlite`'s own `optional` did and what every read of one row here relies on.
#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    NoRow,
    Said(String),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::NoRow => write!(out, "the query found no row"),
            Problem::Said(said) => write!(out, "{said}"),
        }
    }
}

impl From<inillucent_driver::Error> for Problem {
    fn from(error: inillucent_driver::Error) -> Self {
        Problem::Said(error.to_string())
    }
}

/// A result from the board's database.
pub type Outcome<T> = Result<T, Problem>;

/// Turn "no row" into `None`, and leave a real failure a failure.
pub trait Optional<T> {
    fn optional(self) -> Outcome<Option<T>>;
}

impl<T> Optional<T> for Outcome<T> {
    fn optional(self) -> Outcome<Option<T>> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(Problem::NoRow) => Ok(None),
            Err(problem) => Err(problem),
        }
    }
}

/// A value bound to `?1`, `?2`, …
pub trait ToCell {
    fn cell(&self) -> Value;
}

impl ToCell for i64 {
    fn cell(&self) -> Value {
        Value::Integer(*self)
    }
}

impl ToCell for bool {
    fn cell(&self) -> Value {
        Value::Integer(i64::from(*self))
    }
}

impl ToCell for str {
    fn cell(&self) -> Value {
        Value::Text(self.to_owned())
    }
}

impl ToCell for String {
    fn cell(&self) -> Value {
        Value::Text(self.clone())
    }
}

impl<T: ToCell + ?Sized> ToCell for &T {
    fn cell(&self) -> Value {
        (**self).cell()
    }
}

impl<T: ToCell> ToCell for Option<T> {
    fn cell(&self) -> Value {
        match self {
            Some(value) => value.cell(),
            None => Value::Null,
        }
    }
}

/// The values a statement is run with, written the way `rusqlite::params!` writes them.
macro_rules! params {
    () => { Vec::<inillucent_driver::Value>::new() };
    ($($value:expr),+ $(,)?) => {
        vec![$($crate::services::agent_tasks::db::ToCell::cell(&$value)),+]
    };
}
pub(crate) use params;

/// What a statement may be handed as its values: a `params!` list, or `[]` for none.
pub trait Params {
    fn values(self) -> Vec<Value>;
}

impl Params for Vec<Value> {
    fn values(self) -> Vec<Value> {
        self
    }
}

impl<const N: usize> Params for [Value; N] {
    fn values(self) -> Vec<Value> {
        self.into_iter().collect()
    }
}

/// A value read out of a row.
pub trait FromCell: Sized {
    fn from_cell(value: &Value) -> Outcome<Self>;
}

impl FromCell for i64 {
    fn from_cell(value: &Value) -> Outcome<Self> {
        match value {
            Value::Integer(number) => Ok(*number),
            // A whole number stored as a real is still that number, which is how an `AVG` or a `CAST`
            // can arrive; anything with a fraction is not one and is refused rather than rounded.
            Value::Real(number) if number.fract() == 0.0 => Ok(*number as i64),
            other => Err(Problem::Said(format!("expected a whole number and found {other:?}"))),
        }
    }
}

impl FromCell for String {
    fn from_cell(value: &Value) -> Outcome<Self> {
        match value {
            Value::Text(text) => Ok(text.clone()),
            Value::Integer(number) => Ok(number.to_string()),
            other => Err(Problem::Said(format!("expected text and found {other:?}"))),
        }
    }
}

impl<T: FromCell> FromCell for Option<T> {
    fn from_cell(value: &Value) -> Outcome<Self> {
        match value {
            Value::Null => Ok(None),
            other => T::from_cell(other).map(Some),
        }
    }
}

/// One row of an answer, read a column at a time.
pub struct Row<'a> {
    cells: &'a [Value],
}

impl Row<'_> {
    /// The value in column `index`, from zero.
    pub fn get<T: FromCell>(&self, index: usize) -> Outcome<T> {
        let cell = self
            .cells
            .get(index)
            .ok_or_else(|| Problem::Said(format!("the row has no column {index}")))?;
        T::from_cell(cell)
    }
}

/// The board's database, and the one session every query runs in.
pub struct Db {
    database: Database,
    session: u64,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Db").field("path", &self.database.path()).finish()
    }
}

/// How many pages the board keeps in memory.
///
/// The driver's default is 4,096 frames, which is 128 MiB at the engine's 32 KiB page — the right size
/// for a database somebody queries and far more than a board of a few thousand tickets will ever read.
/// Every window has one board open, and every test opens several, so this is the difference between a
/// few megabytes and a few hundred.
const CACHE_FRAMES: usize = 256;

impl Db {
    /// Open the database at `path`, making it when there is nothing there.
    pub fn open(path: &Path) -> Outcome<Self> {
        let options = OpenOptions { cache_frames: CACHE_FRAMES, ..OpenOptions::default() };
        let database = Database::open_with(path, options)?;
        let session = database.session().session();
        Ok(Self { database, session })
    }

    /// Build the database at `to` out of the SQLite file at `from`, and open it.
    ///
    /// `from` is read and never written. The engine writes the rows into `to` with their ids, their
    /// constraints and the autoincrement counters, which `store`'s migration test checks.
    pub fn import(from: &Path, to: &Path) -> Outcome<Self> {
        let database = Database::import_sqlite_into(from, to)?;
        // Everything into the main file before it is closed, so the file is the whole board and the
        // caller can move it without having to carry the log beside it.
        database.checkpoint()?;
        drop(database);
        Self::open(to)
    }

    pub fn path(&self) -> &Path {
        self.database.path()
    }

    fn connection(&self) -> inillucent_driver::Connection<'_> {
        self.database.session_as(self.session)
    }

    /// Run a statement for its effect, and answer how many rows it changed.
    pub fn execute(&self, sql: &str, params: impl Params) -> Outcome<usize> {
        Ok(self.connection().execute(sql, &params.values())? as usize)
    }

    /// Run several statements separated by semicolons.
    pub fn execute_batch(&self, sql: &str) -> Outcome<()> {
        Ok(self.connection().execute_batch(sql)?)
    }

    /// Read the first row of an answer. No row is [`Problem::NoRow`].
    pub fn query_row<T>(
        &self,
        sql: &str,
        params: impl Params,
        read: impl FnOnce(&Row<'_>) -> Outcome<T>,
    ) -> Outcome<T> {
        let rows = self.connection().query(sql, &params.values(), 1)?;
        let first = rows.rows.first().ok_or(Problem::NoRow)?;
        read(&Row { cells: first })
    }

    /// Read every row of an answer.
    pub fn query_map<T>(
        &self,
        sql: &str,
        params: impl Params,
        mut read: impl FnMut(&Row<'_>) -> Outcome<T>,
    ) -> Outcome<Vec<T>> {
        let rows = self.connection().query_all(sql, &params.values())?;
        rows.rows.iter().map(|cells| read(&Row { cells })).collect()
    }

    /// The rowid the last `INSERT` in this session gave.
    pub fn last_insert_rowid(&self) -> i64 {
        self.connection().last_insert_rowid().unwrap_or_default()
    }

    /// Write everything in the log into the main file.
    pub fn checkpoint(&self) -> Outcome<()> {
        Ok(self.database.checkpoint()?)
    }

    /// Write a complete copy of the database to `to`.
    pub fn backup_to(&self, to: &Path) -> Outcome<()> {
        Ok(self.database.backup_to(to)?)
    }

    /// Run `work` as one transaction: all of its statements, or none of them.
    pub fn in_transaction<T>(
        &self,
        work: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let connection = self.connection();
        let transaction = connection
            .begin()
            .map_err(|problem| format!("the board could not begin a transaction: {problem}"))?;
        let answer = work()?;
        transaction
            .commit()
            .map_err(|problem| format!("the board could not be written: {problem}"))?;
        Ok(answer)
    }
}
