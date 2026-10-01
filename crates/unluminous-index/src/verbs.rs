//! The `search` area's verbs, answered from an `Index` (`tasks/task-2138-unluminous-code-index-tdd.md`
//! §6.10). The window and the headless host both call these, so a search typed into the command line, a
//! search an agent makes through MCP and a search the window answers are one function.
//!
//! Each verb takes the request's arguments as the catalogue names them and returns the reply's `result`:
//! structured fields for a program, and `text`, which is what the command line prints and what an agent
//! is shown.

use serde_json::{json, Map, Value};

use crate::exact::ExactRequest;
use crate::files::Scope;
use crate::index::Index;
use crate::shape::{self, DEFAULT_BUDGET};

/// Why a verb refused, with the protocol's code.
#[derive(Debug)]
pub struct Refusal {
    /// One of the protocol's codes: `usage`, `not-found`, `failed`.
    pub code: &'static str,
    /// What to tell the caller.
    pub message: String,
}

/// A refusal with the `usage` code.
///
/// @param message - what was wrong with the request
fn usage(message: impl Into<String>) -> Refusal {
    Refusal { code: "usage", message: message.into() }
}

/// A text argument.
///
/// @param args - the request's arguments
/// @param name - the argument's name
fn text<'a>(args: &'a Map<String, Value>, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// A whole number argument, given as a number or as text.
///
/// @param args - the request's arguments
/// @param name - the argument's name
fn whole(args: &Map<String, Value>, name: &str) -> Option<usize> {
    match args.get(name)? {
        Value::Number(n) => n.as_u64().map(|n| n as usize),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A switch argument: present and not `false`.
///
/// @param args - the request's arguments
/// @param name - the argument's name
fn switch(args: &Map<String, Value>, name: &str) -> bool {
    match args.get(name) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s != "false",
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

/// Splits a `glob` argument the way the Grep tool does: on white space, and on commas outside braces.
///
/// @param glob - the argument as given
pub fn split_globs(glob: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in glob.split_whitespace() {
        if word.contains('{') && word.contains('}') {
            out.push(word.to_owned());
        } else {
            out.extend(word.split(',').filter(|s| !s.is_empty()).map(str::to_owned));
        }
    }
    out
}

/// Whether a query reads as a regular expression rather than as plain text.
///
/// @param query - the query
fn looks_like_regex(query: &str) -> bool {
    query.contains(['\\', '^', '$', '|', '*', '+', '?', '(', ')', '[', ']', '{', '}']) || query.contains(".*")
}

/// Answers one verb.
///
/// @param index - the checkout's index
/// @param verb - the verb, such as `find`
/// @param args - the request's arguments
pub fn answer(index: &Index, verb: &str, args: &Map<String, Value>) -> Result<Value, Refusal> {
    match verb {
        "find" => find(index, args),
        "files" => files(index, args),
        "status" => Ok(status(index)),
        other => Err(Refusal { code: "unknown-command", message: format!("There is no search verb called `{other}`.") }),
    }
}

/// What `search find` was asked, read out of its arguments.
struct FindRequest {
    query: String,
    regex: bool,
    pattern: String,
    scope: Scope,
    budget: usize,
    case_insensitive: bool,
}

/// Reads `search find`'s arguments.
///
/// @param root - the root, which a path and globs are relative to
/// @param args - query, mode, path, glob, type, ignore-case, budget
fn read_find(root: &std::path::Path, args: &Map<String, Value>) -> Result<FindRequest, Refusal> {
    let query = text(args, "query").ok_or_else(|| usage("search find needs a query."))?.to_owned();
    let regex = match text(args, "mode").unwrap_or("auto") {
        "regex" => true,
        "literal" => false,
        "auto" => looks_like_regex(&query),
        other => return Err(usage(format!("`{other}` is not a mode this version has: auto, literal or regex."))),
    };
    let pattern = if regex { query.clone() } else { regex::escape(&query) };
    let globs = text(args, "glob").map(split_globs).unwrap_or_default();
    let types: Vec<String> = text(args, "type").map(|t| vec![t.to_owned()]).unwrap_or_default();
    let scope = Scope::new(root, text(args, "path").unwrap_or(""), &globs, &types).map_err(usage)?;
    let budget = whole(args, "budget").unwrap_or(DEFAULT_BUDGET);
    Ok(FindRequest { query, regex, pattern, scope, budget, case_insensitive: switch(args, "ignore-case") })
}

/// `search find`: an exact search, literal or regex, through the gate.
///
/// @param index - the index
/// @param args - query, mode, path, glob, type, ignore-case, budget
fn find(index: &Index, args: &Map<String, Value>) -> Result<Value, Refusal> {
    let asked = read_find(index.root(), args)?;
    let request = ExactRequest { pattern: &asked.pattern, case_insensitive: asked.case_insensitive, scope: &asked.scope };
    let started = std::time::Instant::now();
    let (found, engine) = index.exact(&request).map_err(|e| usage(format!("The pattern was refused: {e}")))?;
    Ok(found_value(&asked, &found, engine, started.elapsed()))
}

/// `search find` with no index at all, by scanning the files, for a caller that could reach no host.
///
/// @param root - the root
/// @param args - the same arguments `find` takes
pub fn scan_without_index(root: &std::path::Path, args: &Map<String, Value>) -> Result<Value, Refusal> {
    let asked = read_find(root, args)?;
    let request = ExactRequest { pattern: &asked.pattern, case_insensitive: asked.case_insensitive, scope: &asked.scope };
    let started = std::time::Instant::now();
    let found = crate::direct::scan(root, &request).map_err(|e| usage(format!("The pattern was refused: {e}")))?;
    Ok(found_value(&asked, &found, "none", started.elapsed()))
}

/// The reply of an exact search: counts, the hits shown, the work done, and the shaped text.
///
/// @param asked - the request
/// @param found - what the search found
/// @param engine - which engine answered
/// @param took - how long the search took
fn found_value(asked: &FindRequest, found: &crate::exact::ExactAnswer, engine: &str, took: std::time::Duration) -> Value {
    let needle = if asked.regex { "" } else { asked.query.as_str() };
    let shaped = shape::shape(&found.hits, needle, asked.budget);
    let files: std::collections::BTreeSet<&str> = found.hits.iter().map(|h| h.path.as_str()).collect();
    json!({
        "index": engine,
        "mode": if asked.regex { "regex" } else { "literal" },
        "total": found.hits.len(),
        "files": files.len(),
        "omitted": { "hits": shaped.omitted_hits, "files": shaped.omitted_files },
        "hits": shaped.shown.iter().map(|h| json!([h.path, h.line, h.text])).collect::<Vec<_>>(),
        "work": { "inScope": found.in_scope, "candidates": found.candidates, "verified": found.verified, "unbounded": found.unbounded, "micros": took.as_micros() as u64 },
        "text": shaped.text,
    })
}

/// `search files`: paths ranked by how well they match a name fragment or a glob.
///
/// @param index - the index
/// @param args - query, limit
fn files(index: &Index, args: &Map<String, Value>) -> Result<Value, Refusal> {
    let query = text(args, "query").ok_or_else(|| usage("search files needs a name or a glob."))?;
    let limit = whole(args, "limit").unwrap_or(20);
    let paths = index.with_exact(|exact| exact.by_path.keys().cloned().collect::<Vec<_>>()).unwrap_or_else(|| {
        crate::files::walk(index.root()).into_iter().map(|f| f.rel).collect()
    });
    let ranked = crate::paths::rank(&paths, query, limit);
    let text = if ranked.is_empty() { "no files match\n".to_owned() } else { ranked.iter().map(|p| format!("{p}\n")).collect() };
    Ok(json!({ "files": ranked, "total": ranked.len(), "text": text }))
}

/// `search status`: whether the index is ready, what it holds and what the gate has done.
///
/// @param index - the index
fn status(index: &Index) -> Value {
    let st = index.status();
    let (files, trigrams, bytes) = index.with_exact(|e| (e.live(), e.postings.len(), e.heap_bytes())).unwrap_or((0, 0, 0));
    let text = format!(
        "{} ({}), {} files, {} trigrams, {} MB in memory, {} gates, {} files read again\n{}\n",
        if st.ready { "ready" } else { "building" },
        st.origin,
        files,
        trigrams,
        bytes / (1 << 20),
        st.gates,
        st.reindexed,
        st.file.display()
    );
    json!({
        "root": index.root().to_string_lossy(),
        "ready": st.ready,
        "origin": st.origin,
        "loadMs": st.load_ms as u64,
        "files": files,
        "trigrams": trigrams,
        "memoryBytes": bytes,
        "gates": st.gates,
        "reindexed": st.reindexed,
        "storeError": st.store_error,
        "indexFile": st.file.to_string_lossy(),
        "versions": { "schema": crate::store::SCHEMA_VERSION, "trigram": crate::store::TRIGRAM_VERSION },
        "text": text,
    })
}
