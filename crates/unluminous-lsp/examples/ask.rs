//! A warm query loop for the completion evaluation harness.
//!
//! `ask --adapter lsp|tsserver --root <folder>` starts the server for the project once, waits until it
//! is ready, then answers one JSON line on standard input with one JSON line on standard output:
//!
//! ```text
//! in:  {"id":"q1","prefix":2,"path":"C:/p/src/lib.rs","text":"<whole edited text>","offset":123}
//! out: {"id":"q1","prefix":2,"labels":["caret_at","width"],"ms":4.2}
//! ```
//!
//! The rows are sorted by the server's own order and filtered by the identifier typed before the offset
//! the way a client would: rows whose filter text starts with it, or when there are none, rows that
//! contain it as a subsequence. Progress goes to standard error.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use unluminous_lsp::{
    find_program, Adapter, Item, Reply, Server, ServerSpec, ServerState, Trigger,
};

/// How long one question may take before it is answered with nothing.
const ANSWER_WITHIN: Duration = Duration::from_secs(5);
/// How long the server may take to become ready the first time.
const READY_WITHIN: Duration = Duration::from_secs(300);

fn main() {
    let (adapter, root) = read_arguments();
    let mut server = Server::start(spec_for(adapter, &root), Arc::new(|| {}));
    wait_until_ready(&mut server);
    let mut revision = 0u64;
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let Ok(query) = serde_json::from_str::<Value>(&line) else { continue };
        revision += 1;
        let answer = answer(&mut server, &query, revision);
        println!("{answer}");
        let _ = std::io::stdout().flush();
    }
    server.stop();
}

/// `--adapter` and `--root`, or a usage line and exit status 2.
fn read_arguments() -> (Adapter, PathBuf) {
    let args: Vec<String> = std::env::args().collect();
    let value =
        |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let adapter = value("--adapter").and_then(|name| Adapter::parse(&name));
    match (adapter, value("--root")) {
        (Some(adapter), Some(root)) => (adapter, PathBuf::from(root)),
        _ => {
            eprintln!("usage: ask --adapter lsp|tsserver --root <folder>");
            std::process::exit(2);
        }
    }
}

/// The program for the project, with a TypeScript this machine is known to have as the last resort.
fn spec_for(adapter: Adapter, root: &Path) -> ServerSpec {
    match find_program(adapter, root, root, None, &[]) {
        Ok(spec) => spec,
        Err(why) => {
            let known = std::env::var("UNLUMINOUS_TEST_TSSERVER").unwrap_or_else(|_| {
                "C:/jason/dev/ai-service/ui/node_modules/typescript/lib/tsserver.js".to_owned()
            });
            if adapter == Adapter::TsServer && Path::new(&known).is_file() {
                let args = [
                    known.as_str(),
                    "--disableAutomaticTypingAcquisition",
                    "--suppressDiagnosticEvents",
                ]
                .map(String::from)
                .to_vec();
                return ServerSpec {
                    adapter,
                    root: root.to_path_buf(),
                    program: PathBuf::from("node"),
                    args,
                    label: "tsserver".to_owned(),
                };
            }
            eprintln!("no server: {why}");
            std::process::exit(1);
        }
    }
}

/// Waits for a state that has held for a moment: `Ready`, not changing, and after indexing began.
fn wait_until_ready(server: &mut Server) {
    let started = Instant::now();
    let mut changed = Instant::now();
    let mut last = server.state();
    while started.elapsed() < READY_WITHIN {
        std::thread::sleep(Duration::from_millis(50));
        server.poll();
        let now = server.state();
        if now != last {
            eprintln!("[{:.1}s] {}", started.elapsed().as_secs_f32(), now.describe());
            last = now;
            changed = Instant::now();
        }
        match last {
            ServerState::Ready
                if changed.elapsed() > Duration::from_millis(1500)
                    && started.elapsed() > Duration::from_secs(3) =>
            {
                return
            }
            ServerState::Failed(_) | ServerState::Absent(_) => break,
            _ => {}
        }
    }
    eprintln!("not ready: {}", last.describe());
}

fn language_of(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("tsx") => "typescriptreact",
        Some("js") => "javascript",
        Some("jsx") => "javascriptreact",
        _ => "typescript",
    }
}

/// Syncs the text, asks at the offset, and writes the answer as a JSON value.
fn answer(server: &mut Server, query: &Value, revision: u64) -> Value {
    let path = PathBuf::from(query["path"].as_str().unwrap_or(""));
    let text = query["text"].as_str().unwrap_or("");
    let offset = (query["offset"].as_u64().unwrap_or(0) as usize).min(text.len());
    let started = Instant::now();
    server.sync(&path, language_of(&path), revision, Arc::from(text));
    let trigger =
        if text[..offset].ends_with('.') { Trigger::Character('.') } else { Trigger::Invoked };
    let ticket = server.complete(&path, revision, offset, trigger);
    let items = wait_for(server, ticket).unwrap_or_default();
    // `"raw": n` adds the server's own JSON for the first n rows, which is how a ranking question about
    // a server's rows is looked into.
    let raw: Vec<Value> = match query["raw"].as_u64() {
        Some(n) => items
            .iter()
            .take(n as usize)
            .filter_map(|i| serde_json::from_str(&i.handle).ok())
            .collect(),
        None => Vec::new(),
    };
    let labels = labels_for(items, &stem_before(text, offset));
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut answer =
        json!({"id": query["id"], "prefix": query["prefix"], "labels": labels, "ms": ms});
    if !raw.is_empty() {
        answer["raw"] = Value::Array(raw);
    }
    answer
}

/// The reply to one ticket, or nothing after [`ANSWER_WITHIN`].
fn wait_for(server: &mut Server, ticket: u64) -> Option<Vec<Item>> {
    let started = Instant::now();
    while started.elapsed() < ANSWER_WITHIN {
        for reply in server.poll() {
            if let Reply::Completions { ticket: t, items, .. } = reply {
                if t == ticket {
                    return Some(items);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    None
}

/// The identifier characters just before the offset.
fn stem_before(text: &str, offset: usize) -> String {
    let word: Vec<char> = text[..offset]
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    word.into_iter().rev().collect()
}

/// Sorted by the server's order, then filtered by the stem, at most fifty labels.
fn labels_for(mut items: Vec<Item>, stem: &str) -> Vec<String> {
    items.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.label.cmp(&b.label)));
    let stem = stem.to_lowercase();
    let starts: Vec<&Item> =
        items.iter().filter(|i| i.filter.to_lowercase().starts_with(&stem)).collect();
    let kept = if starts.is_empty() {
        items.iter().filter(|i| is_subsequence(&stem, &i.filter.to_lowercase())).collect()
    } else {
        starts
    };
    kept.into_iter().take(50).map(|i| i.label.clone()).collect()
}

/// True when the letters of `stem` appear in `word` in order.
fn is_subsequence(stem: &str, word: &str) -> bool {
    let mut letters = word.chars();
    stem.chars().all(|c| letters.any(|w| w == c))
}
