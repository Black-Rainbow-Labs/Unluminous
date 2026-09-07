//! The two engines behind one value.
//!
//! An `enum` rather than a trait object, which is `dock::Panel`'s own argument made again: there are
//! two, and a third is a variant the compiler then names every place that has to answer for. Nothing
//! above this line asks which engine it is holding — `components::database::tree` draws
//! `catalog::Item`s and the grid draws `Rows`, and neither has heard of a `RowDescription`.

use std::path::Path;

use crate::catalog::{Item, Kind, Table};
use crate::inillucent;
use crate::postgres;
use crate::rows::{Answer, Failure, Rows};
use crate::source::{Engine, Source};
use crate::sqlite;
use crate::value::Value;

/// An open connection to one data source.
#[derive(Debug)]
pub enum Database {
    Postgres(Box<postgres::Session>),
    Sqlite(Box<sqlite::Session>),
    Inillucent(Box<inillucent::Session>),
}

/// What can stop a statement that is running, from another thread.
///
/// Held separately from the connection because that is the whole point: the thread running the
/// statement is inside the engine and cannot look at a flag, so the thing that stops it has to be
/// reachable from somewhere else. PostgreSQL opens a second connection; SQLite calls
/// `sqlite3_interrupt`.
pub enum Stopper {
    Postgres { host: String, port: u16, key: Option<(u32, u32)>, ssl: crate::source::SslMode },
    Sqlite(rusqlite::InterruptHandle),
}

impl std::fmt::Debug for Stopper {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stopper::Postgres { host, port, .. } => write!(out, "Stopper::Postgres({host}:{port})"),
            Stopper::Sqlite(_) => write!(out, "Stopper::Sqlite"),
        }
    }
}

impl Database {
    /// Open a connection.
    ///
    /// `password` is fetched by the caller at this moment and dropped as soon as this returns.
    pub fn connect(source: &Source, password: Option<&str>) -> Answer<Database> {
        match source.engine {
            Engine::Postgres => {
                Ok(Database::Postgres(Box::new(postgres::Session::connect(source, password)?)))
            }
            Engine::Sqlite => Ok(Database::Sqlite(Box::new(sqlite::Session::open(
                Path::new(&source.database),
                source.read_only,
            )?))),
            Engine::Inillucent => Ok(Database::Inillucent(Box::new(inillucent::Session::open(
                Path::new(&source.database),
                source.read_only,
            )?))),
        }
    }

    /// What the server calls itself — the string Test Connection reports, in the server's own words.
    pub fn version(&self) -> String {
        match self {
            Database::Postgres(session) => match session.version().is_empty() {
                true => "PostgreSQL".to_owned(),
                false => format!("PostgreSQL {}", session.version()),
            },
            Database::Sqlite(session) => session.version(),
            Database::Inillucent(session) => session.version(),
        }
    }

    pub fn engine(&self) -> Engine {
        match self {
            Database::Postgres(_) => Engine::Postgres,
            Database::Sqlite(_) => Engine::Sqlite,
            Database::Inillucent(_) => Engine::Inillucent,
        }
    }

    /// True when the connection itself is encrypted. Always false for SQLite, which is a file.
    pub fn is_encrypted(&self) -> bool {
        match self {
            Database::Postgres(session) => session.is_encrypted(),
            // Both are files on this machine, so there is no connection to encrypt.
            Database::Sqlite(_) | Database::Inillucent(_) => false,
        }
    }

    /// The databases on this server, for the top level of the tree.
    ///
    /// One for SQLite — the file — because the tree has one shape and asking which engine it is
    /// drawing is exactly what this enum exists to avoid.
    pub fn databases(&mut self) -> Answer<Vec<String>> {
        match self {
            Database::Postgres(session) => postgres::introspect::databases(session),
            Database::Sqlite(session) => Ok(vec![session
                .file()
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "database".to_owned())]),
            Database::Inillucent(session) => Ok(vec![session
                .file()
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "database".to_owned())]),
        }
    }

    pub fn schemas(&mut self) -> Answer<Vec<String>> {
        match self {
            Database::Postgres(session) => postgres::introspect::schemas(session),
            Database::Sqlite(session) => Ok(session.schemas()),
            Database::Inillucent(session) => session.schemas(),
        }
    }

    /// Everything in one schema, including its routines.
    pub fn items(&mut self, schema: &str) -> Answer<Vec<Item>> {
        match self {
            Database::Postgres(session) => {
                let mut items = postgres::introspect::items(session, schema)?;
                items.extend(postgres::introspect::routines(session, schema)?);
                Ok(items)
            }
            Database::Sqlite(session) => session.items(),
            Database::Inillucent(session) => session.items(),
        }
    }

    pub fn table(&mut self, schema: &str, name: &str) -> Answer<Table> {
        match self {
            Database::Postgres(session) => postgres::introspect::table(session, schema, name),
            Database::Sqlite(session) => session.table(name),
            Database::Inillucent(session) => session.table(name),
        }
    }

    pub fn ddl(&mut self, schema: &str, name: &str, kind: Kind) -> Answer<String> {
        match self {
            Database::Postgres(session) => postgres::introspect::ddl(session, schema, name, kind),
            Database::Sqlite(session) => session.ddl(name),
            Database::Inillucent(session) => session.ddl(name),
        }
    }

    /// Run a statement somebody typed, with nothing bound.
    pub fn query(&mut self, statement: &str, limit: usize) -> Answer<Rows> {
        match self {
            Database::Postgres(session) => session.simple(statement, limit),
            Database::Sqlite(session) => session.run(statement, &[], limit),
            Database::Inillucent(session) => session.run(statement, &[], limit),
        }
    }

    /// Run a statement Unluminous composed, with its values bound as parameters.
    pub fn run(&mut self, statement: &str, values: &[Value], limit: usize) -> Answer<Rows> {
        match self {
            Database::Postgres(session) => session.extended(statement, values, limit),
            Database::Sqlite(session) => session.run(statement, values, limit),
            Database::Inillucent(session) => session.run(statement, values, limit),
        }
    }

    /// Write a list of statements as one transaction, or write none of them.
    ///
    /// **Every statement must change exactly one row**, checked before the commit rather than after
    /// it. Zero means the row moved underneath — somebody else changed or deleted it since the grid
    /// was filled — and more than one means the key did not name a single row, which is the fault the
    /// whole editing rule exists to prevent. Either rolls the transaction back, and the message says
    /// which happened and to which statement.
    pub fn write(&mut self, work: &[(String, Vec<Value>)]) -> Answer<Vec<u64>> {
        match self {
            // **The check is handed in rather than run afterwards**, because SQLite's transaction
            // commits inside that call: a postcondition tested after the commit is a report about
            // something that has already happened rather than a guard against it.
            Database::Sqlite(session) => session.in_one_transaction(work, check),
            // The driver takes the same check for the same reason, so this is the one engine where
            // the rule is the client's own rather than something written out here.
            Database::Inillucent(session) => session.in_one_transaction(work, check),
            Database::Postgres(session) => {
                session.simple("BEGIN", usize::MAX)?;
                let mut affected = Vec::with_capacity(work.len());
                for (statement, values) in work {
                    match session.extended(statement, values, 0) {
                        Ok(rows) => affected.push(rows.affected.unwrap_or_default()),
                        Err(why) => {
                            let _ = session.simple("ROLLBACK", usize::MAX);
                            return Err(why);
                        }
                    }
                }
                if let Err(why) = check(&affected) {
                    let _ = session.simple("ROLLBACK", usize::MAX);
                    return Err(why);
                }
                session.simple("COMMIT", usize::MAX)?;
                Ok(affected)
            }
        }
    }

    /// Set the schema a console's unqualified names are resolved in.
    ///
    /// PostgreSQL's `search_path`, and nothing at all for SQLite, which has one schema. It is a
    /// statement rather than a connection parameter because the schema switcher changes it while the
    /// connection is open.
    pub fn use_schema(&mut self, schema: &str) -> Answer<()> {
        match self {
            Database::Postgres(session) => {
                // The schema name cannot be a parameter — nothing that is part of the statement's own
                // grammar can be — so it is quoted, which is what `catalog::quoted` is for.
                let statement =
                    format!("SET search_path TO {}", crate::catalog::quoted(schema, '"'));
                session.simple(&statement, usize::MAX).map(|_| ())
            }
            // One schema, so there is nothing to point at. Inillucent's `ATTACH` gives a connection
            // more than one, but a connection here lasts one statement — see `inillucent::Session`.
            Database::Sqlite(_) | Database::Inillucent(_) => Ok(()),
        }
    }

    /// What another thread can stop this connection with, when anything can.
    ///
    /// **`None` for Inillucent, and that is the capability table arriving in the window.** The engine
    /// materialises a statement on its first step, so there is no loop in which a cancel flag would be
    /// read and a Stop button would report success and do nothing. Unluminous draws no control that can
    /// never apply, so the caller gets nothing to draw one from.
    pub fn stopper(&self) -> Option<Stopper> {
        match self {
            Database::Postgres(session) => Some(session.stopper()),
            Database::Sqlite(session) => Some(Stopper::Sqlite(session.interrupt())),
            Database::Inillucent(_) => None,
        }
    }

    /// What a search table declared about itself, or `None` when it is not one.
    ///
    /// Only Inillucent has search tables; the other two answer `None` rather than being asked whether
    /// they might, which keeps the question out of the components that draw a tree.
    pub fn search_index(&mut self, name: &str) -> Answer<Option<crate::SearchIndex>> {
        match self {
            Database::Inillucent(session) => session.search_index(name),
            _ => Ok(None),
        }
    }

    /// What this engine does and does not do, as the engine itself reports it.
    pub fn capabilities(&self) -> &'static [inillucent_driver::Capability] {
        match self {
            Database::Inillucent(session) => session.capabilities(),
            _ => &[],
        }
    }

    pub fn close(&mut self) {
        match self {
            Database::Postgres(session) => session.close(),
            // Folding the log into the file is what makes the next open cheap. A database dropped
            // without it is not lost — the engine replays its log — so a failure here is not worth
            // refusing a close over.
            Database::Inillucent(session) => {
                let _ = session.checkpoint();
            }
            Database::Sqlite(_) => {}
        }
    }
}

impl Stopper {
    /// Ask the engine to stop what it is doing. Never waits for an answer: PostgreSQL sends none, and
    /// SQLite's interrupt returns at once.
    pub fn stop(&self) -> Answer<()> {
        match self {
            Stopper::Postgres { host, port, key, ssl } => {
                postgres::session::cancel(host, *port, *key, *ssl)
            }
            Stopper::Sqlite(handle) => {
                handle.interrupt();
                Ok(())
            }
        }
    }
}

/// Every statement the row editor writes names exactly one row, so exactly one is what it must change.
///
/// **Not "at least one".** Zero means the row is not there any more — somebody else changed or
/// deleted it since the grid was filled — and *more* than one means the key did not name what it
/// claimed to, which is the fault the whole editing rule exists to prevent and the more dangerous of
/// the two. Both roll the transaction back, and the message says which happened.
fn check(affected: &[u64]) -> Answer<()> {
    for (at, count) in affected.iter().enumerate() {
        if *count == 1 {
            continue;
        }
        return Err(Failure::said(match count {
            0 => format!(
                "statement {} changed no rows, which means that row is not there any more — somebody \
                 else has changed or deleted it since this grid was filled. Nothing was written.",
                at + 1
            ),
            many => format!(
                "statement {} would have changed {many} rows, and every statement this writes names \
                 exactly one. The key it matched on does not name a single row. Nothing was written.",
                at + 1
            ),
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_statement_that_changed_anything_but_one_row_stops_the_whole_write() {
        // Two faults, not one. Zero is a row somebody else deleted, and a grid that reported the edit
        // as saved would be lying. More than one is worse: it means the key did not name a single
        // row, which is the thing the whole editing rule exists to prevent.
        assert!(check(&[1, 1, 1]).is_ok());
        let none = check(&[1, 0, 1]).expect_err("refused");
        assert!(none.message.contains("statement 2"), "{none}");
        assert!(none.message.contains("changed no rows"), "{none}");
        assert!(none.message.contains("Nothing was written"), "{none}");
        let many = check(&[1, 4]).expect_err("refused");
        assert!(many.message.contains("would have changed 4 rows"), "{many}");
        assert!(many.message.contains("does not name a single row"), "{many}");
    }
}
