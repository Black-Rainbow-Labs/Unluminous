//! Passage search: questions written in plain English, answered from chunks
//! (`tasks/task-2138-unluminous-code-index-tdd.md` §6.2, §6.8, R2, R3, R8).
//!
//! Every chunk of every file a bundled language claims, and of Markdown and plain text, is a row of an
//! `inillucent_search` table: its header (the path and the enclosing signatures), its body, and the
//! split identifier words the engine does not make itself. Three facets narrow a search inside it:
//! `lang` (the extension), `kind` (code, test or docs) and `path_prefix` (the top folder). The
//! `chunk` table says where each row came from.
//!
//! Files the index holds for exact search only are left out: binary files, files over 1 MB, minified
//! and lock files, and files that look like secrets (`.env*`, `*.pem`, `id_*`), so none of them can
//! outrank source and none of their text reaches an answer about meaning (§6.3, §6.12).
//!
//! The table lives in the store's file and is only touched on the store thread, because the engine's
//! `Database` cannot leave the thread that opened it.

use inillucent_driver::{Connection, Value};

use crate::exact::{Exact, FileRecord};
use crate::outline::{self, CHUNK_BUDGET};
use crate::symbols::is_secondary;
use crate::words;

/// The version of how chunks are cut and what a row holds. A different value rebuilds the table.
pub const PASSAGE_VERSION: &str = "1";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS chunk (id INTEGER PRIMARY KEY, path TEXT, start_line INTEGER, end_line INTEGER,
    kind TEXT, symbol TEXT, header TEXT, chunk_hash BLOB);
CREATE INDEX IF NOT EXISTS chunk_path ON chunk(path);
";

/// One chunk a passage search found.
#[derive(Clone, Debug)]
pub struct PassageHit {
    /// The file.
    pub path: String,
    /// Its first line.
    pub start: u32,
    /// Its last line.
    pub end: u32,
    /// Its header.
    pub header: String,
    /// The engine's score.
    pub score: f64,
    /// The engine's confidence, 0 to 1.
    pub confidence: f64,
}

/// Whether a file's chunks belong in the passage table.
///
/// @param record - the file
pub fn is_passage_source(record: &FileRecord) -> bool {
    let lower = record.rel.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let generated = name.ends_with(".lock") || name.contains(".min.") || name == "package-lock.json" || name.ends_with(".map");
    let secret = name.starts_with(".env") || name.ends_with(".pem") || name.ends_with(".key") || name.starts_with("id_");
    let readable = crate::grammars::for_path(&record.rel).is_some() || name.ends_with(".md") || name.ends_with(".txt");
    !record.binary && record.size <= 1_048_576 && !generated && !secret && readable
}

/// What kind of file a path is, for the `kind` facet.
///
/// @param path - the path
pub fn kind_of(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".txt") {
        "docs"
    } else if is_secondary(path) {
        "test"
    } else {
        "code"
    }
}

/// The top folder of a path, for the `path_prefix` facet; empty for a file at the root.
///
/// @param path - the path
pub fn prefix_of(path: &str) -> &str {
    path.split_once('/').map_or("", |(top, _)| top)
}

/// Creates the passage and chunk tables when they are missing.
///
/// @param conn - a connection on the store thread
pub fn create(conn: &Connection<'_>) -> Result<(), String> {
    conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
    let exists = conn.query("SELECT name FROM sqlite_schema WHERE name = 'passage'", &[], 1).map_err(|e| e.to_string())?;
    if exists.rows.is_empty() {
        conn.execute("CREATE VIRTUAL TABLE passage USING inillucent_search(header, body, words, lang FACET, kind FACET, path_prefix FACET, tokenize = 'porter')", &[])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Writes every chunk of every passage source, replacing what the tables held, in one transaction.
///
/// @param conn - a connection on the store thread
/// @param exact - the exact index
pub fn rebuild(conn: &Connection<'_>, exact: &Exact) -> Result<usize, String> {
    create(conn)?;
    let tx = conn.begin().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM passage", &[]).map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM chunk", &[]).map_err(|e| e.to_string())?;
    let mut next = 1i64;
    for record in exact.files.iter().flatten().filter(|r| is_passage_source(r)) {
        next = write_file(&tx, record, next)?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    let _ = conn.execute("INSERT INTO passage(passage) VALUES('compact')", &[]);
    Ok((next - 1) as usize)
}

/// Replaces the chunks of files that changed and drops those of files that are gone.
///
/// @param conn - a connection on the store thread
/// @param removed - paths gone from the set
/// @param changed - files read again
pub fn update(conn: &Connection<'_>, removed: &[String], changed: &[&FileRecord]) -> Result<(), String> {
    let tx = conn.begin().map_err(|e| e.to_string())?;
    let next = match tx.query("SELECT max(id) FROM chunk", &[], 1).map_err(|e| e.to_string())?.value(0, 0) {
        Some(Value::Integer(n)) => n + 1,
        _ => 1,
    };
    for path in removed.iter().map(String::as_str).chain(changed.iter().map(|r| r.rel.as_str())) {
        let ids = tx.query("SELECT id FROM chunk WHERE path = ?1", &[Value::Text(path.to_owned())], usize::MAX).map_err(|e| e.to_string())?;
        for row in &ids.rows {
            if let Some(Value::Integer(id)) = row.first() {
                tx.execute("DELETE FROM passage WHERE rowid = ?1", &[Value::Integer(*id)]).map_err(|e| e.to_string())?;
            }
        }
        tx.execute("DELETE FROM chunk WHERE path = ?1", &[Value::Text(path.to_owned())]).map_err(|e| e.to_string())?;
    }
    let mut next = next;
    for record in changed.iter().filter(|r| is_passage_source(r)) {
        next = write_file(&tx, record, next)?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Writes one file's chunks from a given id, and returns the next free id.
///
/// @param tx - the open transaction
/// @param record - the file
/// @param first - the first id to use
fn write_file(tx: &inillucent_driver::Transaction<'_>, record: &FileRecord, first: i64) -> Result<i64, String> {
    let bytes = record.bytes();
    let text = String::from_utf8_lossy(&bytes);
    let read = outline::read(&record.rel, &text, CHUNK_BUDGET);
    let mut id = first;
    let (lang, kind, prefix) = (crate::store::extension(&record.rel), kind_of(&record.rel), prefix_of(&record.rel).to_owned());
    for chunk in &read.chunks {
        let hash = blake3::hash(format!("{}\n{}", chunk.header, chunk.body).as_bytes());
        tx.execute(
            "INSERT INTO chunk (id, path, start_line, end_line, kind, symbol, header, chunk_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &[Value::Integer(id), Value::Text(record.rel.clone()), Value::Integer(i64::from(chunk.start)), Value::Integer(i64::from(chunk.end)), Value::Text(chunk.kind.into()), chunk.symbol.clone().map_or(Value::Null, Value::Text), Value::Text(chunk.header.clone()), Value::Blob(hash.as_bytes().to_vec())],
        )
        .map_err(|e| e.to_string())?;
        let searched_words = format!("{} {}", words::search_words(&chunk.header), words::search_words(&chunk.body));
        tx.execute(
            "INSERT INTO passage (rowid, header, body, words, lang, kind, path_prefix) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[Value::Integer(id), Value::Text(chunk.header.clone()), Value::Text(chunk.body.clone()), Value::Text(searched_words), Value::Text(lang.clone()), Value::Text(kind.into()), Value::Text(prefix.clone())],
        )
        .map_err(|e| e.to_string())?;
        id += 1;
    }
    Ok(id)
}

/// The words of a question as an engine query: each word quoted, any of them may match, so a chunk
/// holding more of them ranks higher.
///
/// @param question - the question
pub fn match_query(question: &str) -> Option<String> {
    let terms = words::query_words(question);
    if terms.is_empty() {
        return None;
    }
    Some(terms.iter().map(|t| format!("\"{}\"", t.replace('"', ""))).collect::<Vec<_>>().join(" OR "))
}

/// The chunks that best answer a question, best first.
///
/// @param conn - a connection on the store thread
/// @param question - the question
/// @param k - how many to retrieve
/// @param kind - only chunks of this kind, if given
pub fn search(conn: &Connection<'_>, question: &str, k: usize, kind: Option<&str>) -> Result<Vec<PassageHit>, String> {
    let Some(query) = match_query(question) else { return Ok(Vec::new()) };
    let (filter, params) = match kind {
        Some(kind) => (" AND kind = ?3", vec![Value::Text(query), Value::Integer(k as i64), Value::Text(kind.to_owned())]),
        None => ("", vec![Value::Text(query), Value::Integer(k as i64)]),
    };
    let sql = format!(
        "SELECT p.rowid, score(passage), confidence(passage), c.path, c.start_line, c.end_line, c.header FROM passage p JOIN chunk c ON c.id = p.rowid WHERE passage MATCH ?1 AND k = ?2{filter} ORDER BY rank"
    );
    let rows = conn.query(&sql, &params, k).map_err(|e| e.to_string())?;
    let real = |v: Option<&Value>| match v {
        Some(Value::Real(r)) => *r,
        Some(Value::Integer(i)) => *i as f64,
        _ => 0.0,
    };
    let int = |v: Option<&Value>| match v {
        Some(Value::Integer(i)) => *i as u32,
        _ => 0,
    };
    Ok(rows
        .rows
        .iter()
        .map(|row| PassageHit {
            path: row.get(3).and_then(Value::text).unwrap_or_default().to_owned(),
            start: int(row.get(4)),
            end: int(row.get(5)),
            header: row.get(6).and_then(Value::text).unwrap_or_default().to_owned(),
            score: real(row.get(1)),
            confidence: real(row.get(2)),
        })
        .collect())
}
