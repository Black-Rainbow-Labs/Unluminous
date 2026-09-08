//! The Inillucent backend, against a real database the engine itself wrote.
//!
//! There is no scripted server here and there could not be: the engine is a library in this process
//! rather than something on a socket, so the honest fixture is a database, and building one costs
//! milliseconds. What that buys is that every one of these tests exercises the real driver, the real
//! planner and the real storage — the `NULL` that stays different from the empty string is the
//! engine's own answer rather than a recording of one.

use std::path::PathBuf;

use super::*;
use crate::catalog::Kind;

/// A database with something in it, in a folder of this test's own.
///
/// Each test names its own file, which is `git_folder(name)`'s rule in the screenshot tests: a fixture
/// only one test uses may be written each time, and the name is what keeps them apart.
fn a_database(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("unluminous-db-inillucent-{}-{name}", std::process::id()));
    let _ = std::fs::create_dir_all(&folder);
    let file = folder.join("test.rdb");
    let _ = std::fs::remove_file(&file);
    let database = inillucent_driver::Database::open(&file).expect("a database");
    let connection = database.connect();
    connection
        .execute_batch(
            "create table member (id integer primary key, name text not null, note text);
             insert into member (id, name, note) values (1, 'Jason', null), (2, 'Ada', '');
             create table pairs (alpha text, beta text);
             insert into pairs values ('a', 'b');
             create index member_name on member (name);
             create view members as select id, name from member;",
        )
        .expect("a schema");
    connection
        .execute_batch(
            "create virtual table docs using inillucent_search(title, body, dims = 4, mode = 'exact', metric = 'cosine');",
        )
        .expect("a search table");
    let mut vector = Vec::new();
    for value in [0.5f32, -0.5, 0.5, 0.5] {
        vector.extend_from_slice(&value.to_le_bytes());
    }
    connection
        .execute(
            "insert into docs(title, body, vector) values(?1, ?2, ?3)",
            &[
                inillucent_driver::Value::Text("release process".to_owned()),
                inillucent_driver::Value::Text("how the release is cut and published".to_owned()),
                inillucent_driver::Value::Blob(vector),
            ],
        )
        .expect("a row with a vector");
    database.checkpoint().expect("checkpointed");
    drop(connection);
    drop(database);
    file
}

/// A SQLite database, for the two tests about telling the formats apart.
fn a_sqlite_database(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("unluminous-db-inillucent-{}-{name}", std::process::id()));
    let _ = std::fs::create_dir_all(&folder);
    let file = folder.join("sqlite.db");
    let _ = std::fs::remove_file(&file);
    let connection = rusqlite::Connection::open(&file).expect("a database");
    connection
        .execute_batch("create table note (id integer primary key, body text); insert into note values (1, 'one');")
        .expect("a schema");
    drop(connection);
    file
}

#[test]
fn a_file_that_is_not_there_is_said_rather_than_created() {
    // The driver creates by default, exactly as `rusqlite`'s flags do, so a mistyped path would
    // otherwise make an empty database and the tree would show a data source with nothing in it.
    let missing = std::env::temp_dir().join("unluminous-db-no-such-inillucent-file.rdb");
    let _ = std::fs::remove_file(&missing);
    let refused = Session::open(&missing, false).expect_err("refused");
    assert!(refused.message.contains("there is no file at"), "{refused}");
    assert!(!missing.exists(), "and nothing was created");
}

#[test]
fn a_sqlite_file_is_refused_with_somewhere_to_go() {
    // The driver would say that neither meta page is readable, which is true and useless. What a
    // person needs to hear is what the file *is* and what would read it.
    let file = a_sqlite_database("routing");
    let refused = Session::open(&file, false).expect_err("refused");
    assert!(refused.message.contains("is a SQLite database"), "{refused}");
    assert!(refused.message.contains("SQLite data source"), "{refused}");
    assert!(refused.message.contains("import"), "and the way across: {refused}");
}

#[test]
fn the_two_formats_are_told_apart_by_the_bytes_they_begin_with() {
    let inillucent = a_database("magic");
    let sqlite = a_sqlite_database("magic");
    assert!(is_an_inillucent_database(&inillucent));
    assert!(!is_an_inillucent_database(&sqlite));
    assert!(starts_with(&sqlite, SQLITE_MAGIC));
    // A file that is not there is not either of them, and asking does not create one.
    let missing = std::env::temp_dir().join("unluminous-db-nothing-here-at-all.rdb");
    let _ = std::fs::remove_file(&missing);
    assert!(!is_an_inillucent_database(&missing));
    assert!(!missing.exists());
}

#[test]
fn a_source_reads_the_engine_off_the_file_rather_than_asking_anybody() {
    // A person who has a database has a path, not an opinion about which engine wrote it.
    use crate::source::{Engine, Source};
    let inillucent = a_database("parse");
    let sqlite = a_sqlite_database("parse");
    let read = Source::parse("db", &inillucent.to_string_lossy()).expect("a source");
    assert_eq!(read.engine, Engine::Inillucent);
    let read = Source::parse("db", &sqlite.to_string_lossy()).expect("a source");
    assert_eq!(read.engine, Engine::Sqlite);
    // And the scheme is still a way to say so outright.
    //
    // **The leading slash of an absolute path survives.** This asserted `tmp/whatever.rdb` — the bug it
    // was pinning: every leading `/` was trimmed, so `inillucent:///tmp/x.rdb`, which is the ordinary
    // spelling of an absolute path in a URL, became a path relative to whatever directory the process
    // happened to be in. A source added that way pointed at nothing, and the refusal named a rootless
    // path, which reads like a typo in the URL rather than like a fault in Unluminous.
    let named = Source::parse("db", "inillucent:///tmp/whatever.rdb").expect("a source");
    assert_eq!(named.engine, Engine::Inillucent);
    assert_eq!(named.database, "/tmp/whatever.rdb");
}

/// The three spellings of a path in a `sqlite://` or `inillucent://` URL, and what each one means.
///
/// Two slashes after the scheme is an empty authority and a relative path; three is an empty authority
/// and an absolute one. Both are ordinary, both appear in the wild, and before `path_from_url` they were
/// read as the same thing — which is what made an absolute path unusable.
#[test]
fn a_url_keeps_an_absolute_path_absolute_and_a_relative_one_relative() {
    use crate::source::{Engine, Source};
    for scheme in ["sqlite", "inillucent"] {
        let absolute = Source::parse("db", &format!("{scheme}:///Users/me/data.db")).expect("a source");
        assert_eq!(
            absolute.database, "/Users/me/data.db",
            "{scheme}:///… is an absolute path and keeps its root"
        );

        let relative = Source::parse("db", &format!("{scheme}://data/local.db")).expect("a source");
        assert_eq!(
            relative.database, "data/local.db",
            "{scheme}://… with no third slash is relative, and gains no root"
        );

        // A Windows path in a URL has no leading slash to lose, and must not gain one either.
        let windows = Source::parse("db", &format!("{scheme}://C:/data/local.db")).expect("a source");
        assert_eq!(windows.database, "C:/data/local.db");

        // The engine is what the scheme said whichever spelling was used.
        let wanted = match scheme {
            "sqlite" => Engine::Sqlite,
            _ => Engine::Inillucent,
        };
        assert_eq!(absolute.engine, wanted);
        assert_eq!(relative.engine, wanted);
    }
}

/// A path with no scheme at all is left exactly as it was typed.
///
/// The commonest way a source is added — `plugins run database add-source name /path/to/file` — goes
/// through the same parser, and it must not be touched by the URL handling beside it.
#[test]
fn a_bare_path_is_not_rewritten() {
    use crate::source::Source;
    for path in ["/Users/me/data.db", "data/local.db", "./beside.rdb"] {
        let read = Source::parse("db", path).expect("a source");
        assert_eq!(read.database, path, "a bare path is the path");
    }
}

#[test]
fn the_tree_reads_every_kind_including_the_two_the_search_engine_adds() {
    let mut session = Session::open(&a_database("items"), false).expect("opened");
    let items = session.items().expect("items");
    let named: Vec<(&str, Kind)> = items.iter().map(|item| (item.name.as_str(), item.kind)).collect();
    assert!(named.contains(&("member", Kind::Table)), "{named:?}");
    assert!(named.contains(&("members", Kind::View)), "{named:?}");
    assert!(named.contains(&("member_name", Kind::Index)), "{named:?}");
    // The two this engine introduces: the search table itself, and the five it keeps its state in.
    assert!(named.contains(&("docs", Kind::Search)), "{named:?}");
    for suffix in search::SUFFIXES {
        let shadow = format!("docs_{suffix}");
        let found = items.iter().find(|item| item.name == shadow);
        assert_eq!(found.map(|item| item.kind), Some(Kind::Shadow), "{shadow} in {named:?}");
    }
    assert_eq!(session.schemas().expect("schemas"), ["main"]);
}

#[test]
fn a_search_table_says_what_it_declared_and_holds_no_key() {
    let mut session = Session::open(&a_database("search"), false).expect("opened");
    let index = session.search_index("docs").expect("asked").expect("a search table");
    assert_eq!(index.dimensions, 4);
    assert_eq!(index.mode, "exact");
    assert_eq!(index.metric, "cosine");
    assert_eq!(index.tokenizer, "porter");
    assert_eq!(index.columns, ["title", "body"]);
    assert_eq!(index.summary(), "4d · exact · cosine · porter");

    // An ordinary table is not one, and is not made to look like one.
    assert!(session.search_index("member").expect("asked").is_none());

    // And its rows are read but not changed: it has no key of its own, which is the ordinary rule
    // rather than a special case for search tables.
    let table = session.table("docs").expect("a table");
    assert!(table.key.is_empty(), "{:?}", table.key);
    assert!(!table.can_be_changed());
}

#[test]
fn a_shadow_table_is_read_only_although_it_has_a_key() {
    // The dangerous one. `docs_content` has an INTEGER PRIMARY KEY, so the addressability rule alone
    // would let a person edit the search index's own storage and leave it describing something the
    // row no longer says.
    let mut session = Session::open(&a_database("shadow"), false).expect("opened");
    let table = session.table("docs_content").expect("a table");
    assert!(!table.key.is_empty(), "it really does have a key: {:?}", table.key);
    assert!(!table.can_be_changed(), "and it is still not editable");
    let why = table.why_not_changeable().expect("a reason");
    assert!(why.contains("keeps its state"), "{why}");
    assert!(why.contains("`docs`"), "{why}");
}

#[test]
fn a_vector_round_trips_and_is_read_from_the_column_the_schema_names() {
    let mut session = Session::open(&a_database("vectors"), false).expect("opened");
    // The schema says which column holds a vector; nothing here guesses from the bytes.
    let table = session.table("docs").expect("a table");
    assert_eq!(table.vector_columns, [search::VECTOR_COLUMN]);
    let content = session.table("docs_content").expect("a table");
    assert_eq!(content.vector_columns, [search::CONTENT_VECTOR_COLUMN]);
    // And an ordinary table's columns are left alone whatever they hold.
    assert!(session.table("member").expect("a table").vector_columns.is_empty());

    let rows = session.run("select vector from docs", &[], usize::MAX).expect("rows");
    let cell = rows.rows.first().and_then(|row| row.first()).expect("a cell");
    let bytes = match cell {
        Value::Bytes(bytes) => bytes.clone(),
        other => panic!("a vector arrives as bytes, not {other:?}"),
    };
    let vector = crate::vector::Vector::decode(&bytes).expect("a vector");
    assert_eq!(vector.values, [0.5, -0.5, 0.5, 0.5], "byte for byte what was written");
    assert_eq!(vector.summary(), "4d · |v| 1.000 · [0.5000, -0.5000, 0.5000, …]");
}

#[test]
fn a_search_finds_the_row_and_ranks_it() {
    // The engine's own `MATCH`, which is the whole point of the table: a person can ask a question in
    // words and get the rows back in an order somebody can argue with.
    let mut session = Session::open(&a_database("match"), false).expect("opened");
    let rows = session
        .run("select title, rank from docs where docs match 'release' order by rank limit 3", &[], 10)
        .expect("rows");
    assert_eq!(rows.rows.len(), 1, "{:?}", rows.rows);
    assert_eq!(rows.rows[0][0], Value::Text("release process".to_owned()));
    // `rank` is negated so that ascending is best first, which is FTS5's convention and this
    // engine's; a positive score here would mean the ordering had been inverted underneath us.
    let rank: f32 = rows.rows[0][1].text().expect("a rank").parse().expect("a number");
    assert!(rank < 0.0, "rank is negated so ascending is best first: {rank}");
}

#[test]
fn null_and_the_empty_string_survive_the_round_trip() {
    let mut session = Session::open(&a_database("nulls"), false).expect("opened");
    let rows = session.run("select note from member order by id", &[], usize::MAX).expect("rows");
    assert_eq!(rows.rows[0][0], Value::Null);
    assert_eq!(rows.rows[1][0], Value::Text(String::new()));
}

#[test]
fn a_value_with_a_quote_a_newline_and_a_backslash_is_a_non_event() {
    // The case that is a fault in every implementation that builds SQL by concatenation.
    let mut session = Session::open(&a_database("awkward"), false).expect("opened");
    let awkward = "it's \"quoted\";\nand C:\\dev\\ -- not a comment";
    session
        .run(
            "insert into member (id, name) values (?1, ?2)",
            &[Value::typed("9"), Value::typed(awkward)],
            0,
        )
        .expect("inserted");
    let rows = session
        .run("select name from member where id = ?1", &[Value::typed("9")], usize::MAX)
        .expect("rows");
    assert_eq!(rows.rows[0][0], Value::Text(awkward.to_owned()));
}

#[test]
fn a_count_is_exact_rather_than_a_plus_sign() {
    // The one place this engine answers better than the other two: it materialises, so the row count
    // is the real one and a grid can say how many there are instead of `200+`.
    let mut session = Session::open(&a_database("counting"), false).expect("opened");
    let one = session.run("select id from member order by id", &[], 1).expect("rows");
    assert_eq!(one.rows.len(), 1);
    assert!(one.more, "there is another row");
    let all = session.run("select id from member order by id", &[], usize::MAX).expect("rows");
    assert_eq!(all.rows.len(), 2);
    assert!(!all.more);
}

#[test]
fn a_transaction_that_fails_its_check_leaves_nothing_behind() {
    let mut session = Session::open(&a_database("transaction"), false).expect("opened");
    let work = vec![
        (
            "update member set note = ?1 where id = ?2".to_owned(),
            vec![Value::typed("edited"), Value::typed("1")],
        ),
        // The second names a row that is not there, so the check refuses and the first must not
        // survive either.
        (
            "update member set note = ?1 where id = ?2".to_owned(),
            vec![Value::typed("nope"), Value::typed("99")],
        ),
    ];
    let refused = session
        .in_one_transaction(&work, |affected| match affected.iter().all(|count| *count == 1) {
            true => Ok(()),
            false => Err(Failure::said("a statement changed something other than one row")),
        })
        .expect_err("refused");
    assert!(refused.message.contains("something other than one row"), "{refused}");
    let rows = session.run("select note from member where id = 1", &[], usize::MAX).expect("rows");
    assert_eq!(rows.rows[0][0], Value::Null, "the first update did not survive");
}

#[test]
fn a_transaction_that_passes_its_check_is_committed() {
    let mut session = Session::open(&a_database("committed"), false).expect("opened");
    let work = vec![(
        "update member set note = ?1 where id = ?2".to_owned(),
        vec![Value::typed("edited"), Value::typed("1")],
    )];
    let affected = session.in_one_transaction(&work, |_| Ok(())).expect("committed");
    assert_eq!(affected, [1]);
    let rows = session.run("select note from member where id = 1", &[], usize::MAX).expect("rows");
    assert_eq!(rows.rows[0][0], Value::Text("edited".to_owned()));
}

#[test]
fn a_read_only_source_is_refused_by_the_driver_rather_than_by_unluminous() {
    let file = a_database("readonly");
    let mut session = Session::open(&file, true).expect("opened");
    assert!(session.run("select count(*) from member", &[], usize::MAX).is_ok());
    let refused = session
        .run("insert into member (id, name) values (99, 'x')", &[], 0)
        .expect_err("refused");
    assert_eq!(refused.code, "readonly", "{refused}");
}

#[test]
fn what_the_engine_has_not_built_keeps_its_own_words_and_its_own_code() {
    // The distinction the driver exists to make: *this engine cannot do that yet* is a different
    // answer from *check your spelling*, and an explorer that folded them together would have thrown
    // the design away. If a later phase builds VACUUM, this test says so by failing.
    let mut session = Session::open(&a_database("gaps"), false).expect("opened");
    let refused = session.run("VACUUM", &[], 0).expect_err("refused");
    assert_eq!(refused.code, "unsupported", "{refused}");
    assert!(refused.detail.contains("has not built this yet"), "{refused}");
    assert!(refused.detail.contains("VACUUM"), "the engine's own word for it: {refused}");

    // And a typo is still a typo, with a different code.
    let missing = session.run("select * from nope", &[], 0).expect_err("refused");
    assert_eq!(missing.code, "not_found", "{missing}");
    assert!(missing.message.contains("no such table"), "{missing}");
}

#[test]
fn the_ddl_is_the_text_that_was_typed() {
    let mut session = Session::open(&a_database("ddl"), false).expect("opened");
    let ddl = session.ddl("member").expect("ddl");
    assert!(ddl.contains("create table member") || ddl.contains("CREATE TABLE member"), "{ddl}");
    assert!(ddl.contains("name text not null"), "verbatim, down to the case: {ddl}");
    let search = session.ddl("docs").expect("ddl");
    assert!(search.to_ascii_lowercase().contains("using inillucent_search"), "{search}");
}

#[test]
fn a_table_reports_its_key_and_which_columns_refuse_null() {
    let mut session = Session::open(&a_database("keys"), false).expect("opened");
    let member = session.table("member").expect("a table");
    assert_eq!(member.key, ["id"]);
    assert!(member.can_be_changed());
    assert!(member.columns.iter().any(|column| column.name == "name" && column.not_null));
    assert!(member.columns.iter().any(|column| column.name == "id" && column.in_key));

    // A table nobody gave a key to cannot be addressed, and says so rather than being edited by a
    // statement that would change every row that looks like it.
    let pairs = session.table("pairs").expect("a table");
    assert!(!pairs.can_be_changed());
    assert!(pairs.why_not_changeable().expect("a reason").contains("no primary key"));
}

#[test]
fn a_statement_cannot_be_stopped_and_the_engine_is_what_says_so() {
    // Read from the driver's capability table rather than written down here, because a capability
    // list nobody checks decays into claims that were true once. The day the engine grows a cancel,
    // this flips on its own and the button appears.
    use crate::source::Engine;
    assert!(!Session::can_be_stopped());
    assert!(!Engine::Inillucent.can_stop_a_statement());
    assert!(Engine::Sqlite.can_stop_a_statement());
    assert!(Engine::Postgres.can_stop_a_statement());
}

#[test]
fn the_capability_table_is_the_engines_own_and_names_the_two_that_matter() {
    let session = Session::open(&a_database("capabilities"), false).expect("opened");
    let capabilities = session.capabilities();
    assert!(capabilities.len() > 20, "the whole table, not a summary: {}", capabilities.len());
    let named = |name: &str| capabilities.iter().find(|entry| entry.name == name);
    assert!(named("cancel").is_some(), "the one that decides whether a Stop button exists");
    assert!(named("readonly_open").is_some(), "the one that is only partial");
    // Every row carries a sentence, because a table of yes and no with nothing to read is a table
    // that gets copied into a comment and goes stale.
    for entry in capabilities {
        assert!(!entry.note.is_empty(), "{} has no note", entry.name);
    }
}

#[test]
fn an_import_writes_a_new_database_and_never_touches_the_original() {
    let sqlite = a_sqlite_database("import");
    let before = std::fs::read(&sqlite).expect("the original");
    let made = Session::import(&sqlite).expect("imported");
    assert!(made.exists(), "the imported database is at {}", made.display());
    assert!(is_an_inillucent_database(&made), "and it is one this engine wrote");
    assert_eq!(std::fs::read(&sqlite).expect("the original"), before, "the source is untouched");

    // And the rows came across.
    let mut session = Session::open(&made, false).expect("opened");
    let rows = session.run("select body from note where id = 1", &[], 10).expect("rows");
    assert_eq!(rows.rows[0][0], Value::Text("one".to_owned()));
}

#[test]
fn a_file_that_is_not_a_database_at_all_refuses_rather_than_panicking() {
    // A hostile or corrupt file must produce a refusal. The pages are parsed beneath the driver by a
    // crate that forbids unsafe and bounds-checks every read against the page's own header, and what
    // this asserts is that the refusal reaches the caller as words rather than as a panic.
    let folder = std::env::temp_dir().join(format!("unluminous-db-inillucent-{}-junk", std::process::id()));
    let _ = std::fs::create_dir_all(&folder);
    let file = folder.join("junk.rdb");
    // The right magic and nothing else that is right, which is the case a length check alone misses.
    let mut bytes = MAGIC.to_vec();
    bytes.extend(std::iter::repeat(0xA5).take(8192));
    std::fs::write(&file, &bytes).expect("written");
    let refused = Session::open(&file, false).expect_err("refused");
    assert!(!refused.message.is_empty(), "and it says something");
}
