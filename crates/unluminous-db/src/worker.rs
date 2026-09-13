//! The thread a query runs on.
//!
//! `unluminous_git::Worker` with a different payload, and the argument for it is the same one: the window
//! draws sixty times a second and a query takes as long as it takes, so nothing that talks to a
//! database may be called from inside a frame. A job goes down a channel, an answer comes back up
//! one, and `wake` brings a frame round when it does — the arrangement the terminal's reader, the
//! symbol index and `unluminous-dap` all already use.
//!
//! **One thread per connected data source**, holding one connection. Two panes reading the same source
//! therefore queue behind each other, which is what a single connection means and is what the reference editor's
//! own single-session console does. A second connection would be a second transaction and a second
//! `search_path`, which is a surprise nobody wants from a tree and a grid that look like one thing.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use crate::catalog::{Item, Kind, Table};
use crate::edit::Statement;
use crate::engine::{Database, Stopper};
use crate::rows::{Answer, Failure, Rows};
use crate::source::Source;
use crate::value::Value;

/// What a caller asked for.
#[derive(Debug, Clone)]
pub enum Job {
    /// A statement somebody typed.
    Query {
        sql: String,
        limit: usize,
    },
    /// A statement Unluminous composed, with values bound.
    Run {
        sql: String,
        values: Vec<Value>,
        limit: usize,
    },
    Databases,
    Schemas,
    Items {
        schema: String,
    },
    Describe {
        schema: String,
        table: String,
    },
    Ddl {
        schema: String,
        table: String,
        kind: Kind,
    },
    /// What a search table declared about itself. Only Inillucent has any; the others answer `None`.
    SearchIndex {
        name: String,
    },
    UseSchema {
        name: String,
    },
    /// Everything pending on a grid, written as one transaction or not at all.
    Write {
        statements: Vec<Statement>,
    },
    /// Stop reading and close the connection.
    Close,
}

/// What came back.
#[derive(Debug)]
pub enum Reply {
    Rows(Rows),
    Names(Vec<String>),
    Items(Vec<Item>),
    Table(Table),
    Text(String),
    /// A search table's declaration, or `None` when the named thing is not one.
    ///
    /// Boxed because it is much the largest variant and every other reply would otherwise be the size
    /// of this one.
    Search(Box<Option<crate::SearchIndex>>),
    /// How many rows each statement of a write changed.
    Written(Vec<u64>),
    Done,
}

/// One answered job.
#[derive(Debug)]
pub struct Answered {
    /// The number this job was given, so a caller with several outstanding can tell them apart.
    pub ticket: u64,
    pub answer: Answer<Reply>,
}

/// A connection, on a thread of its own.
pub struct Worker {
    jobs: Sender<(u64, Job)>,
    answers: Receiver<Answered>,
    /// What can stop a statement that is running, from this thread.
    ///
    /// `None` from the moment the connection is closed - and, for Inillucent, from the moment it is
    /// opened, because that engine has no way to stop a statement at all. The two are told apart by
    /// `can_stop` so that a refusal says which it is.
    stopper: Arc<Mutex<Option<Stopper>>>,
    /// Whether this engine can stop a statement that is already running.
    ///
    /// Read from the engine rather than assumed, and it is what the pane draws its Stop button from -
    /// or does not: Unluminous's rule is that a control which can never apply is absent.
    can_stop: bool,
    next: AtomicU64,
    /// Set by [`Worker::drop`], and read by the thread before it starts anything.
    ///
    /// **Stopping the statement that is running is not enough on its own**, because `Job::Close` goes
    /// to the *back* of the channel: a worker dropped with three statements queued behind the one
    /// running would stop that one and then run all three before it ever read the `Close`. Each of
    /// them writes to somebody's database and the join below waits for the lot, so the window freezes
    /// for as long as they take and the work is done after the pane that asked for it has gone.
    /// `task-1922`. This is the same shape as `services::text_search`'s generation counter: the thread
    /// asks whether the answer is still wanted before it does the work rather than after.
    closing: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// What the server called itself when the connection opened.
    pub version: String,
    pub encrypted: bool,
    pub engine: crate::source::Engine,
    /// How many jobs have been sent and not yet answered, which is what the pane draws a spinner from.
    outstanding: std::cell::Cell<usize>,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Worker")
            .field("version", &self.version)
            .field("encrypted", &self.encrypted)
            .field("outstanding", &self.outstanding.get())
            .finish()
    }
}

/// What the worker's own thread reports once it has connected.
///
/// Everything about a connection that the caller needs and that can safely leave the thread: what the
/// server called itself, whether the link is encrypted, which engine answered, and what can stop a
/// statement - which is `None` for an engine that cannot stop one.
struct Opened {
    version: String,
    encrypted: bool,
    engine: crate::source::Engine,
    stopper: Option<Stopper>,
}

impl Worker {
    /// Open the connection **on the thread that will hold it**, and wait here for the answer.
    ///
    /// Connecting is what fails - a wrong password, a server that is not there, a certificate that
    /// will not verify - and it is the one thing the caller has to be told about straight away. That
    /// used to mean connecting on *this* thread and moving the connection across; it cannot any more,
    /// and the reason is worth writing down because it is a property of an engine rather than an
    /// inconvenience.
    ///
    /// **The Inillucent engine is single threaded and one file is one buffer pool, so its `Database`
    /// is neither `Send` nor `Sync` by construction.** A handle that could be moved between threads
    /// would be a second page cache over one set of bytes waiting to happen, and the driver refuses to
    /// let it compile rather than documenting that nobody should. So the connection is *made* where it
    /// will live, and this function waits for the first word back - which keeps the property that
    /// matters (a failure to connect is answered before `open` returns) without moving anything.
    ///
    /// `wake` is called whenever an answer is put on the channel. Without it a query that finished
    /// while nobody was moving the pointer would sit there unseen, which is `Context::wake`'s own
    /// reason.
    pub fn open(
        source: &Source,
        password: Option<&str>,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Answer<Worker> {
        let (jobs, take_a_job) = mpsc::channel::<(u64, Job)>();
        let (answer, answers) = mpsc::channel::<Answered>();
        // What the thread reports once it has tried to connect: everything the caller needs to know
        // about a connection it will never itself touch.
        let (opened, was_opened) = mpsc::channel::<Answer<Opened>>();
        let source_to_open = source.clone();
        let password = password.map(str::to_owned);
        let closing = Arc::new(AtomicBool::new(false));
        let asked_to_close = Arc::clone(&closing);
        let thread = std::thread::Builder::new()
            .name(format!("unluminous-db {}", source.name))
            .spawn(move || {
                let mut database = match Database::connect(&source_to_open, password.as_deref()) {
                    Ok(database) => database,
                    Err(why) => {
                        let _ = opened.send(Err(why));
                        return;
                    }
                };
                let report = Opened {
                    version: database.version(),
                    encrypted: database.is_encrypted(),
                    engine: database.engine(),
                    stopper: database.stopper(),
                };
                if opened.send(Ok(report)).is_err() {
                    // Nobody waited for the answer, so there is nobody to serve.
                    database.close();
                    return;
                }
                while let Ok((ticket, job)) = take_a_job.recv() {
                    // **Asked before the job is started, not only when the job is `Close`.** Anything
                    // still on the channel when the worker was dropped belongs to a pane that has
                    // gone, so it is thrown away rather than run — see `Worker::closing`.
                    if matches!(job, Job::Close) || asked_to_close.load(Ordering::Acquire) {
                        database.close();
                        return;
                    }
                    let reply = run(&mut database, job);
                    if answer.send(Answered { ticket, answer: reply }).is_err() {
                        // Nobody is listening any more: the pane has gone.
                        database.close();
                        return;
                    }
                    if let Some(wake) = &wake {
                        wake();
                    }
                }
                database.close();
            })
            .map_err(|why| {
                Failure::said(format!("a thread for this connection could not be started: {why}"))
            })?;
        let report = match was_opened.recv() {
            Ok(report) => report?,
            // The thread ended without saying anything, which it only does by panicking - and a panic
            // inside the engine is not something to answer with silence.
            Err(_) => {
                return Err(Failure::said(
                    "this connection's thread stopped before it said whether it had connected.",
                ))
            }
        };
        let can_stop = report.engine.can_stop_a_statement();
        Ok(Worker {
            jobs,
            answers,
            stopper: Arc::new(Mutex::new(report.stopper)),
            can_stop,
            next: AtomicU64::new(1),
            closing,
            thread: Some(thread),
            version: report.version,
            encrypted: report.encrypted,
            engine: report.engine,
            outstanding: std::cell::Cell::new(0),
        })
    }

    /// Ask for something, and answer with the ticket it will come back under.
    pub fn ask(&self, job: Job) -> Answer<u64> {
        let ticket = self.next.fetch_add(1, Ordering::Relaxed);
        self.jobs
            .send((ticket, job))
            .map_err(|_| Failure::said("this connection's thread has stopped."))?;
        self.outstanding.set(self.outstanding.get() + 1);
        Ok(ticket)
    }

    /// Whatever has been answered since this was last called. Never blocks.
    pub fn take(&self) -> Vec<Answered> {
        let mut out = Vec::new();
        while let Ok(answered) = self.answers.try_recv() {
            self.outstanding.set(self.outstanding.get().saturating_sub(1));
            out.push(answered);
        }
        out
    }

    /// True while something is running, which is what draws the spinner and lights the Stop button.
    pub fn is_busy(&self) -> bool {
        self.outstanding.get() > 0
    }

    /// Ask the engine to stop what it is doing.
    ///
    /// Called from the drawing thread while the worker is inside the engine, which is exactly why the
    /// stopper is a separate value: PostgreSQL opens a second connection and SQLite calls
    /// `sqlite3_interrupt`, and neither needs the connection this thread cannot borrow.
    pub fn stop(&self) -> Answer<()> {
        if !self.can_stop {
            // Said rather than silently doing nothing, because an agent asking for this deserves the
            // reason: the engine materialises a statement on its first step, so there is no loop in
            // which a flag would be read.
            return Err(Failure::said(format!(
                "a statement running on {} cannot be stopped once it has started, so there is nothing                  to ask.",
                self.engine.name()
            )));
        }
        match self.stopper.lock() {
            Ok(held) => match held.as_ref() {
                Some(stopper) => stopper.stop(),
                None => Err(Failure::said("this connection has already been closed.")),
            },
            Err(_) => Err(Failure::said("this connection's stopper cannot be reached.")),
        }
    }

    /// Whether a statement that is running can be stopped at all.
    ///
    /// The pane asks before it draws a Stop button, and does not draw one when the answer is no.
    pub fn can_stop(&self) -> bool {
        self.can_stop
    }

    /// What this engine says it does and does not do.
    ///
    /// Empty for every engine that reports no such thing, which today is both of the other two. It is
    /// answered without going near the connection's thread because the table is a property of the
    /// build rather than of the file - which is also what makes it safe to ask while a statement is
    /// running.
    pub fn capabilities(&self) -> &'static [inillucent_driver::Capability] {
        match self.engine {
            crate::source::Engine::Inillucent => inillucent_driver::CAPABILITIES,
            _ => &[],
        }
    }
}

/// **Throw away what is queued, stop what is running, close the connection and wait for the thread.**
///
/// Waited for rather than detached, because the thread holds a socket and a `Drop` that left it
/// running would leave a connection open on somebody's server after the pane that opened it had gone.
/// It is only ever waiting on a `recv`, or inside a statement.
///
/// **What this stops**, in the order it stops it:
///
/// 1. **Every job still on the channel**, through [`Worker::closing`], which the thread asks about
///    before it starts anything. `unluminous-git`'s `Drop` reaps a whole process tree for the
///    analogous reason — killing the `git` it could see was not enough, because `git fetch --all`
///    starts a `git` of its own — and the analogue here is not a second process but the queue: a
///    connection runs one job at a time, so stopping the statement in the engine leaves however many
///    were sent behind it, each of which would then run to completion against somebody's database
///    after the pane that asked for it had gone.
/// 2. **The statement that is running**, through the engine's own `Stopper`: a second PostgreSQL
///    connection carrying a cancel request, or `sqlite3_interrupt`. Stopped *before* the join and not
///    after it, because `Close` is read off the channel between jobs, so a worker in the middle of a
///    statement would not see it until that statement finished and the join would block the window
///    for as long as the query took. An earlier version took the stopper away and then waited, which
///    is the same fault with the one thing that could have helped thrown away first.
/// 3. **The connection**, which the thread closes on its way out — `Database::close` sends
///    PostgreSQL's `Terminate` and drops the SQLite handle — so the server is told rather than left
///    to notice a socket going.
///
/// **And what it cannot stop.** A statement running on an engine with no way to stop one: Inillucent
/// reports `cancel: no` in its capability table, so its `stopper` is `None` from the moment it is
/// opened and there is nothing to ask. The engine materialises a statement on its first step, so
/// there is no loop in which a flag would be read; a `Drop` arriving in the middle of a long query
/// there waits for that one query and then returns. It is one query rather than the whole queue,
/// which is what the first step above is worth. Nothing here kills a *process*, because there is no
/// process — a database connection is a socket or a file handle, and the thread that holds it is the
/// only thing that can let it go.
impl Drop for Worker {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        if let Ok(held) = self.stopper.lock() {
            if let Some(stopper) = held.as_ref() {
                let _ = stopper.stop();
            }
        }
        // Sent even though the flag is already set: a thread parked in `recv` with an empty channel
        // is woken by something arriving on it and by nothing else.
        let _ = self.jobs.send((0, Job::Close));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Ok(mut held) = self.stopper.lock() {
            held.take();
        }
    }
}

/// One job, on the worker's thread.
fn run(database: &mut Database, job: Job) -> Answer<Reply> {
    match job {
        Job::Query { sql, limit } => database.query(&sql, limit).map(Reply::Rows),
        Job::Run { sql, values, limit } => database.run(&sql, &values, limit).map(Reply::Rows),
        Job::Databases => database.databases().map(Reply::Names),
        Job::Schemas => database.schemas().map(Reply::Names),
        Job::Items { schema } => database.items(&schema).map(Reply::Items),
        Job::Describe { schema, table } => database.table(&schema, &table).map(Reply::Table),
        Job::Ddl { schema, table, kind } => database.ddl(&schema, &table, kind).map(Reply::Text),
        Job::SearchIndex { name } => {
            database.search_index(&name).map(|index| Reply::Search(Box::new(index)))
        }
        Job::UseSchema { name } => database.use_schema(&name).map(|_| Reply::Done),
        Job::Write { statements } => {
            let work: Vec<(String, Vec<Value>)> =
                statements.into_iter().map(|statement| (statement.sql, statement.values)).collect();
            database.write(&work).map(Reply::Written)
        }
        Job::Close => Ok(Reply::Done),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    fn a_database(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-db-worker-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&folder);
        let file = folder.join("test.db");
        let _ = std::fs::remove_file(&file);
        let connection = rusqlite::Connection::open(&file).expect("a database");
        connection
            .execute_batch(
                "create table member (id integer primary key, name text not null);
                 insert into member (id, name) values (1, 'Jason'), (2, 'Ada');",
            )
            .expect("a schema");
        file
    }

    /// Wait for one answer, with a deadline rather than for ever.
    ///
    /// **Everything else that arrives is kept**, because `Worker::take` drains the channel: a helper
    /// that threw away the answers it was not waiting for would lose the second of two outstanding
    /// jobs, which is exactly the fault the ticket numbers exist to prevent. A real caller keeps them
    /// the same way.
    fn wait_for(worker: &Worker, ticket: u64, kept: &mut Vec<Answered>) -> Answer<Reply> {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(at) = kept.iter().position(|answered| answered.ticket == ticket) {
                return kept.remove(at).answer;
            }
            if Instant::now() > until {
                panic!("no answer to ticket {ticket} in ten seconds");
            }
            kept.extend(worker.take());
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_query_runs_on_the_thread_and_comes_back_with_its_rows() {
        let source = Source::sqlite("test", a_database("query").to_string_lossy());
        let worker = Worker::open(&source, None, None).expect("opened");
        assert!(worker.version.starts_with("SQLite"));
        let ticket = worker
            .ask(Job::Query { sql: "select name from member order by id".to_owned(), limit: 100 })
            .expect("asked");
        let mut kept = Vec::new();
        let Reply::Rows(rows) = wait_for(&worker, ticket, &mut kept).expect("rows") else {
            panic!()
        };
        assert_eq!(rows.rows.len(), 2);
        assert_eq!(rows.rows[0][0], Value::typed("Jason"));
        assert!(!worker.is_busy(), "nothing outstanding once it has been taken");
    }

    #[test]
    fn the_window_is_woken_when_an_answer_arrives() {
        // Without this a query that finished while nobody was moving the pointer would sit there
        // unseen until the next frame happened for some other reason.
        let woken = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&woken);
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        let source = Source::sqlite("test", a_database("wake").to_string_lossy());
        let worker = Worker::open(&source, None, Some(wake)).expect("opened");
        let ticket = worker.ask(Job::Schemas).expect("asked");
        let _ = wait_for(&worker, ticket, &mut Vec::new());
        assert!(woken.load(Ordering::Relaxed) >= 1, "the window was asked to draw again");
    }

    #[test]
    fn a_failing_statement_comes_back_as_a_refusal_rather_than_stopping_the_thread() {
        let source = Source::sqlite("test", a_database("failing").to_string_lossy());
        let worker = Worker::open(&source, None, None).expect("opened");
        let bad = worker
            .ask(Job::Query { sql: "select * from nothing_like_this".to_owned(), limit: 10 })
            .expect("asked");
        let mut kept = Vec::new();
        assert!(wait_for(&worker, bad, &mut kept).is_err());
        // And the connection is still usable, which is the point: one bad statement in a console is
        // not a reason to lose the session.
        let good = worker.ask(Job::Query { sql: "select 1".to_owned(), limit: 10 }).expect("asked");
        assert!(wait_for(&worker, good, &mut kept).is_ok());
    }

    #[test]
    fn every_job_keeps_its_own_ticket_so_two_outstanding_do_not_get_confused() {
        let source = Source::sqlite("test", a_database("tickets").to_string_lossy());
        let worker = Worker::open(&source, None, None).expect("opened");
        let first = worker.ask(Job::Query { sql: "select 1".to_owned(), limit: 1 }).expect("asked");
        let second =
            worker.ask(Job::Query { sql: "select 2".to_owned(), limit: 1 }).expect("asked");
        assert_ne!(first, second);
        let mut kept = Vec::new();
        let Reply::Rows(one) = wait_for(&worker, first, &mut kept).expect("rows") else { panic!() };
        let Reply::Rows(two) = wait_for(&worker, second, &mut kept).expect("rows") else {
            panic!()
        };
        assert_eq!(one.rows[0][0], Value::typed("1"));
        assert_eq!(two.rows[0][0], Value::typed("2"));
    }

    #[test]
    fn a_write_is_one_transaction_and_the_file_really_changes() {
        let file = a_database("write");
        let source = Source::sqlite("test", file.to_string_lossy());
        let worker = Worker::open(&source, None, None).expect("opened");
        let statements = vec![Statement {
            sql: "UPDATE \"member\" SET \"name\" = ?1 WHERE \"id\" = ?2".to_owned(),
            values: vec![Value::typed("Grace"), Value::typed("1")],
            what: String::new(),
        }];
        let ticket = worker.ask(Job::Write { statements }).expect("asked");
        let Reply::Written(affected) = wait_for(&worker, ticket, &mut Vec::new()).expect("written")
        else {
            panic!()
        };
        assert_eq!(affected, [1]);
        // Read it back through a connection of its own, so this is the file rather than a cache.
        let connection = rusqlite::Connection::open(&file).expect("opened");
        let name: String = connection
            .query_row("select name from member where id = 1", [], |row| row.get(0))
            .expect("a row");
        assert_eq!(name, "Grace");
    }

    /// A statement long enough to still be running a quarter of a second after it was sent.
    ///
    /// It counts to fifty million in SQLite's own loop, which is seconds rather than milliseconds,
    /// and `sqlite3_interrupt` ends it in the middle of that loop. `count(*)` is what makes the whole
    /// count happen: a row limit trims what comes back and not what the engine did.
    const A_LONG_STATEMENT: &str = "with recursive counting(n) as \
         (select 1 union all select n + 1 from counting where n < 50000000) \
         select count(*) from counting";

    #[test]
    fn dropping_a_worker_stops_the_statement_it_is_running_and_never_starts_the_ones_queued_behind_it(
    ) {
        let file = a_database("dropped");
        let source = Source::sqlite("test", file.to_string_lossy());
        let worker = Worker::open(&source, None, None).expect("opened");
        worker.ask(Job::Query { sql: A_LONG_STATEMENT.to_owned(), limit: 1 }).expect("asked");
        // Three writes queued behind it, so what ran and what did not is a question the file answers
        // rather than a question about a clock.
        for name in ["Hopper", "Lovelace", "Liskov"] {
            worker
                .ask(Job::Query {
                    sql: format!("insert into member (name) values ('{name}')"),
                    limit: 1,
                })
                .expect("asked");
        }
        // Long enough that the engine really is inside the first statement, which is what makes this
        // a test of the drop rather than of an empty queue.
        std::thread::sleep(Duration::from_millis(250));
        let began = Instant::now();
        drop(worker);
        let took = began.elapsed();
        let connection = rusqlite::Connection::open(&file).expect("opened");
        let rows: i64 = connection
            .query_row("select count(*) from member", [], |row| row.get(0))
            .expect("counted");
        assert_eq!(
            rows, 2,
            "the three statements queued behind the one that was running never ran, so the file \
             still holds the two rows it was made with"
        );
        assert!(
            took < Duration::from_secs(5),
            "the drop waited {took:?} for a statement it could have interrupted"
        );
    }
}
