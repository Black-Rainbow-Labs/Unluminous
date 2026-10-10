//! The client against scripted servers.
//!
//! The server is this test binary started again: `scripted_entry` below checks its arguments for
//! `script=<name>` and, when it finds one, serves that script on its own standard input and output and
//! exits. That is `unluminous-dap`'s scripted adapters with a process in place of a pipe pair, and it
//! needs no second program built. The script name travels in the arguments and not in the environment
//! because the tests share one environment and run in parallel.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use unluminous_lsp::{
    byte_of_utf16, byte_of_utf8_column, find_program, Adapter, Item, Reply, Server, ServerSpec,
    ServerState, TextEdit, Trigger, RESTARTS_AN_HOUR,
};

// ---------------------------------------------------------------- the scripted servers

/// Started as a server, this is it. Started as a test, it does nothing.
#[test]
fn scripted_entry() {
    let args: Vec<String> = std::env::args().collect();
    let Some(script) = args.iter().find_map(|a| a.strip_prefix("script=")) else { return };
    let log = args.iter().find_map(|a| a.strip_prefix("log=")).map(PathBuf::from);
    if let Some(log) = &log {
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(log).unwrap();
        writeln!(file, "start").unwrap();
    }
    match script {
        "ts" => serve_ts(),
        other => serve_lsp(other),
    }
    std::process::exit(0);
}

fn read_frame(input: &mut impl BufRead) -> Option<Value> {
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length:") {
            length = n.trim().parse().ok()?;
        }
    }
    let mut body = vec![0u8; length];
    std::io::Read::read_exact(input, &mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn write_frame(value: &Value, newline: bool) {
    let mut body = value.to_string();
    if newline {
        body.push('\n');
    }
    let mut out = std::io::stdout().lock();
    write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    out.flush().unwrap();
}

fn apply_lsp_change(text: &mut String, change: &Value) {
    let at = |p: &Value| {
        byte_of_utf8_column(
            text,
            p["line"].as_u64().unwrap() as u32,
            p["character"].as_u64().unwrap() as u32,
        )
    };
    let (start, end) = (at(&change["range"]["start"]), at(&change["range"]["end"]));
    text.replace_range(start..end, change["text"].as_str().unwrap());
}

fn serve_lsp(script: &str) {
    let mut input = std::io::stdin().lock();
    let mut texts: HashMap<String, String> = HashMap::new();
    let mut held: Vec<Value> = Vec::new();
    while let Some(message) = read_frame(&mut input) {
        let method = message["method"].as_str().unwrap_or("");
        let id = message.get("id").cloned();
        match (method, id) {
            ("initialize", Some(id)) => {
                let caps = &message["params"]["capabilities"];
                assert_eq!(caps["general"]["positionEncodings"], json!(["utf-8"]));
                assert_eq!(
                    caps["textDocument"]["completion"]["completionItem"]["snippetSupport"],
                    json!(true)
                );
                assert_eq!(
                    message["params"]["initializationOptions"]["completion"]["limit"],
                    json!(200)
                );
                write_frame(
                    &json!({"jsonrpc": "2.0", "id": id, "result": {"capabilities": {"positionEncoding": "utf-8"}}}),
                    false,
                );
                if script == "crash" {
                    eprintln!("boom");
                    std::process::exit(1);
                }
            }
            ("initialized", _) => {
                write_frame(
                    &json!({"jsonrpc": "2.0", "id": 100, "method": "window/workDoneProgress/create", "params": {"token": "t"}}),
                    false,
                );
                write_frame(
                    &json!({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": "t", "value": {"kind": "begin", "title": "Indexing", "percentage": 50}}}),
                    false,
                );
            }
            ("", Some(id)) if id == json!(100) => {
                write_frame(
                    &json!({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": "t", "value": {"kind": "end"}}}),
                    false,
                );
            }
            ("textDocument/didOpen", _) => {
                let document = &message["params"]["textDocument"];
                texts.insert(
                    document["uri"].as_str().unwrap().to_owned(),
                    document["text"].as_str().unwrap().to_owned(),
                );
            }
            ("textDocument/didChange", _) => {
                let uri = message["params"]["textDocument"]["uri"].as_str().unwrap();
                for change in message["params"]["contentChanges"].as_array().unwrap() {
                    apply_lsp_change(texts.get_mut(uri).unwrap(), change);
                }
            }
            ("textDocument/completion", Some(id)) => {
                let uri = message["params"]["textDocument"]["uri"].as_str().unwrap();
                let position = message["params"]["position"].clone();
                if script == "lsp_slow" {
                    held.push(id);
                    if held.len() == 2 {
                        for (n, id) in held.drain(..).enumerate() {
                            write_frame(
                                &json!({"jsonrpc": "2.0", "id": id, "result": [{"label": format!("answer{n}")}]}),
                                false,
                            );
                        }
                    }
                    continue;
                }
                let echo = format!("{}:{}", position["line"], position["character"]);
                let items = json!([
                    {"label": "foo()", "kind": 3, "sortText": "00000002", "insertText": "foo($0)", "filterText": echo, "detail": texts[uri], "data": {"k": 1}},
                    {"label": "zeta", "kind": 6, "sortText": "00000001"}
                ]);
                let defaults = json!({"editRange": {"start": position, "end": position}, "insertTextFormat": 2});
                write_frame(
                    &json!({"jsonrpc": "2.0", "id": id, "result": {"isIncomplete": true, "itemDefaults": defaults, "items": items}}),
                    false,
                );
            }
            ("completionItem/resolve", Some(id)) => {
                let mut item = message["params"].clone();
                item["documentation"] = json!({"kind": "markdown", "value": "Docs."});
                item["additionalTextEdits"] = json!([{"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}, "newText": "use x;\n"}]);
                write_frame(&json!({"jsonrpc": "2.0", "id": id, "result": item}), false);
            }
            ("shutdown", Some(id)) => {
                write_frame(&json!({"jsonrpc": "2.0", "id": id, "result": null}), false)
            }
            ("exit", _) => return,
            _ => {}
        }
    }
}

fn ts_response(request: &Value, body: Value) {
    write_frame(
        &json!({"seq": 0, "type": "response", "command": request["command"], "request_seq": request["seq"], "success": true, "body": body}),
        true,
    );
}

fn serve_ts() {
    let mut input = std::io::stdin().lock();
    let mut texts: HashMap<String, String> = HashMap::new();
    loop {
        let mut line = String::new();
        if input.read_line(&mut line).unwrap() == 0 {
            return;
        }
        let request: Value = serde_json::from_str(line.trim()).unwrap();
        let arguments = &request["arguments"];
        let file = arguments["file"].as_str().unwrap_or("").to_owned();
        match request["command"].as_str().unwrap() {
            "configure" => {
                assert_eq!(
                    arguments["preferences"]["includeCompletionsForModuleExports"],
                    json!(true)
                );
                write_frame(
                    &json!({"seq": 0, "type": "event", "event": "projectLoadingStart", "body": {}}),
                    true,
                );
                ts_response(&request, json!(true));
                write_frame(
                    &json!({"seq": 0, "type": "event", "event": "projectLoadingFinish", "body": {}}),
                    true,
                );
            }
            "open" => {
                texts.insert(file, arguments["fileContent"].as_str().unwrap().to_owned());
            }
            "change" => {
                let text = texts.get_mut(&file).unwrap();
                let at = |line: &str, offset: &str| {
                    byte_of_utf16(
                        text,
                        arguments[line].as_u64().unwrap() as u32 - 1,
                        arguments[offset].as_u64().unwrap() as u32 - 1,
                    )
                };
                let (start, end) = (at("line", "offset"), at("endLine", "endOffset"));
                text.replace_range(start..end, arguments["insertString"].as_str().unwrap());
            }
            "completionInfo" => {
                let echo = format!("{}:{}", arguments["line"], arguments["offset"]);
                let entries = json!([
                    {"name": echo, "kind": "var", "sortText": "11", "labelDetails": {"detail": texts[&file]}},
                    {"name": "makeCard", "kind": "function", "sortText": "16", "hasAction": true, "source": "./a", "data": {"exportName": "makeCard"}}
                ]);
                ts_response(&request, json!({"isMemberCompletion": false, "entries": entries}));
            }
            "completionEntryDetails" => {
                assert_eq!(arguments["entryNames"][0]["name"], json!("makeCard"));
                assert_eq!(arguments["entryNames"][0]["data"]["exportName"], json!("makeCard"));
                let change = json!({"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}, "newText": "import { makeCard } from \"./a\";\n"});
                let action = json!({"description": "Add import", "changes": [{"fileName": file, "textChanges": [change]}]});
                ts_response(
                    &request,
                    json!([{"displayParts": [{"text": "function makeCard"}], "documentation": [{"text": "Makes."}], "codeActions": [action]}]),
                );
            }
            "exit" => return,
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- the client side

fn spec(script: &str, adapter: Adapter, extra: &[String]) -> ServerSpec {
    let mut args: Vec<String> =
        ["scripted_entry", "--exact", "--nocapture", "--test-threads=1"].map(String::from).to_vec();
    args.push(format!("script={script}"));
    args.extend(extra.iter().cloned());
    ServerSpec {
        adapter,
        root: std::env::temp_dir(),
        program: std::env::current_exe().unwrap(),
        args,
        label: script.to_owned(),
    }
}

/// A server and everything it has said so far.
struct Probe {
    server: Server,
    replies: Vec<Reply>,
}

impl Probe {
    fn start(script: &str, adapter: Adapter, extra: &[String]) -> Probe {
        Probe {
            server: Server::start(spec(script, adapter, extra), Arc::new(|| {})),
            replies: Vec::new(),
        }
    }

    /// Polls until `done` is true of what has arrived, or fails after twenty seconds.
    fn wait(&mut self, what: &str, done: impl Fn(&Probe) -> bool) {
        let started = Instant::now();
        loop {
            self.replies.extend(self.server.poll());
            if done(self) {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "timed out waiting for {what}; state {:?}",
                self.server.state()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_ready(&mut self) {
        self.wait("ready", |p| p.server.state() == ServerState::Ready);
    }

    fn completions(&mut self, ticket: u64) -> (bool, Vec<Item>) {
        self.wait("completions", |p| p.find_completions(ticket).is_some());
        self.find_completions(ticket).unwrap()
    }

    fn find_completions(&self, ticket: u64) -> Option<(bool, Vec<Item>)> {
        self.replies.iter().find_map(|r| match r {
            Reply::Completions { ticket: t, incomplete, items, .. } if *t == ticket => {
                Some((*incomplete, items.clone()))
            }
            _ => None,
        })
    }

    fn resolved(&mut self, ticket: u64) -> Item {
        let find = |p: &Probe| {
            p.replies.iter().find_map(|r| {
                if let Reply::Resolved { ticket: t, item } = r {
                    (*t == ticket).then(|| item.clone())
                } else {
                    None
                }
            })
        };
        self.wait("resolved", |p| find(p).is_some());
        find(self).unwrap()
    }
}

fn file(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

fn after_emoji(text: &str) -> usize {
    text.find('\u{1F600}').unwrap() + 4
}

const TEXT: &str = "fn a() {}\nlet s = \"\u{1F600}\";\n";

// ---------------------------------------------------------------- the tests

#[test]
fn lsp_session_from_initialize_to_resolve() {
    let mut probe = Probe::start("lsp", Adapter::Lsp, &[]);
    assert_eq!(probe.server.state(), ServerState::Starting);
    probe.wait("indexing then ready", |p| {
        p.replies
            .iter()
            .any(|r| matches!(r, Reply::State(ServerState::Indexing { percent: Some(50), .. })))
            && p.server.state() == ServerState::Ready
    });
    let path = file("scripted-lsp-a.rs");
    probe.server.sync(&path, "rust", 1, Arc::from(TEXT));
    let ticket = probe.server.complete(&path, 1, after_emoji(TEXT), Trigger::Character('.'));
    let (incomplete, items) = probe.completions(ticket);
    assert!(incomplete);
    let foo = &items[0];
    assert_eq!(foo.label, "foo");
    assert_eq!(foo.filter, "1:13", "a byte column after the emoji, 9 + 4");
    assert_eq!(foo.detail.as_deref(), Some(TEXT), "the server's copy is the editor's text");
    let insertion = foo.insertion.as_ref().unwrap();
    assert_eq!(
        (insertion.insert.clone(), insertion.text.as_str(), insertion.caret),
        (after_emoji(TEXT)..after_emoji(TEXT), "foo()", Some(4))
    );
    assert!(foo.callable && foo.needs_resolve);
    assert_eq!(items[1].label, "zeta");

    let edited = TEXT.replace("\"\u{1F600}", "\"X\u{1F600}");
    probe.server.sync(&path, "rust", 2, Arc::from(edited.as_str()));
    let ticket = probe.server.complete(&path, 2, after_emoji(&edited), Trigger::Invoked);
    let (_, items) = probe.completions(ticket);
    assert_eq!(
        items[0].detail.as_deref(),
        Some(edited.as_str()),
        "one incremental change in multibyte text"
    );
    assert_eq!(items[0].filter, "1:14");

    let third = edited.replace("fn a()", "fn bb()");
    probe.server.sync(&path, "rust", 3, Arc::from(third.as_str()));
    probe.server.sync(&path, "rust", 3, Arc::from("a different text at the same revision"));
    let ticket = probe.server.complete(&path, 3, 0, Trigger::Invoked);
    let (_, items) = probe.completions(ticket);
    assert_eq!(items[0].detail.as_deref(), Some(third.as_str()), "the same revision sends nothing");

    let ticket = probe.server.resolve(&path, &items[0]);
    let resolved = probe.resolved(ticket);
    assert_eq!(resolved.doc.as_deref(), Some("Docs."));
    assert_eq!(resolved.extra_edits, vec![TextEdit { range: 0..0, text: "use x;\n".to_owned() }]);
    assert!(!resolved.needs_resolve);
    assert_eq!(probe.server.state(), ServerState::Ready);
    probe.server.stop();
}

#[test]
fn a_reply_for_a_superseded_ticket_is_dropped() {
    let mut probe = Probe::start("lsp_slow", Adapter::Lsp, &[]);
    probe.wait_ready();
    let path = file("scripted-lsp-slow.rs");
    probe.server.sync(&path, "rust", 1, Arc::from(TEXT));
    let first = probe.server.complete(&path, 1, 0, Trigger::Invoked);
    let second = probe.server.complete(&path, 1, 3, Trigger::Invoked);
    assert!(second > first);
    let (_, items) = probe.completions(second);
    assert_eq!(items[0].label, "answer1");
    std::thread::sleep(Duration::from_millis(200));
    probe.replies.extend(probe.server.poll());
    assert!(probe.find_completions(first).is_none(), "the stale reply must not be delivered");
    probe.server.stop();
}

#[test]
fn tsserver_session_uses_one_based_utf16_positions() {
    let mut probe = Probe::start("ts", Adapter::TsServer, &[]);
    probe.wait("indexing then ready", |p| {
        p.replies.iter().any(|r| matches!(r, Reply::State(ServerState::Indexing { .. })))
            && p.server.state() == ServerState::Ready
    });
    let path = file("scripted-ts-b.ts");
    probe.server.sync(&path, "typescript", 1, Arc::from(TEXT));
    let ticket = probe.server.complete(&path, 1, after_emoji(TEXT), Trigger::Character('.'));
    let (_, items) = probe.completions(ticket);
    assert_eq!(items[0].label, "2:12", "line 2, offset 9 + 2 for the emoji + 1");
    assert_eq!(items[0].order, 11);
    assert_eq!(items[0].detail.as_deref(), Some(TEXT.trim_end()), "label details are trimmed");

    let edited = TEXT.replace("\"\u{1F600}", "\"X\u{1F600}");
    probe.server.sync(&path, "typescript", 2, Arc::from(edited.as_str()));
    let ticket = probe.server.complete(&path, 2, after_emoji(&edited), Trigger::Invoked);
    let (_, items) = probe.completions(ticket);
    assert_eq!(
        items[0].detail.as_deref(),
        Some(edited.trim_end()),
        "the change was sent in UTF-16 positions"
    );
    assert_eq!(items[0].label, "2:13");

    let make_card = items.iter().find(|i| i.label == "makeCard").unwrap();
    assert!(make_card.needs_resolve);
    let ticket = probe.server.resolve(&path, make_card);
    let resolved = probe.resolved(ticket);
    assert_eq!(resolved.doc.as_deref(), Some("Makes."));
    assert_eq!(resolved.detail.as_deref(), Some("function makeCard"));
    assert_eq!(resolved.extra_edits.len(), 1);
    assert_eq!(resolved.extra_edits[0].range, 0..0);
    probe.server.stop();
}

#[test]
fn a_crashing_server_is_restarted_a_limited_number_of_times() {
    let log = file(&format!("scripted-crash-{}.log", std::process::id()));
    let _ = std::fs::write(&log, "");
    let mut probe = Probe::start("crash", Adapter::Lsp, &[format!("log={}", log.display())]);
    probe.wait("failed", |p| matches!(p.server.state(), ServerState::Failed(_)));
    let ServerState::Failed(why) = probe.server.state() else { unreachable!() };
    assert!(why.contains("exited") && why.contains("boom"), "{why}");
    let starts = std::fs::read_to_string(&log).unwrap().lines().count();
    assert_eq!(starts, 1 + RESTARTS_AN_HOUR, "the first start and each restart");
    probe.server.stop();
    let _ = std::fs::remove_file(&log);
}

#[test]
fn an_absent_server_says_why() {
    let mut server = Server::absent("rust-analyzer", "rustup component add rust-analyzer");
    assert_eq!(
        server.state(),
        ServerState::Absent("rustup component add rust-analyzer".to_owned())
    );
    assert!(!server.state().answers());
    assert_eq!(server.label(), "rust-analyzer");
    server.sync(Path::new("a.rs"), "rust", 1, Arc::from(""));
    server.complete(Path::new("a.rs"), 1, 0, Trigger::Invoked);
    assert!(server.poll().is_empty());
    server.stop();
}

#[test]
fn find_program_reads_the_manifest_and_the_project() {
    let missing = find_program(
        Adapter::Lsp,
        &std::env::temp_dir(),
        &file("a.rs"),
        Some("/no/such/folder/rust-analyzer"),
        &[],
    );
    assert!(missing.unwrap_err().contains("does not exist"));

    let root = file(&format!("scripted-project-{}", std::process::id()));
    let lib = root.join("node_modules/typescript/lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(lib.join("tsserver.js"), "").unwrap();
    match find_program(Adapter::TsServer, &root, &root.join("src/a.ts"), None, &["--x".to_owned()])
    {
        Ok(spec) => {
            assert!(spec.args[0]
                .replace('\\', "/")
                .ends_with("node_modules/typescript/lib/tsserver.js"));
            assert!(
                spec.args.contains(&"--disableAutomaticTypingAcquisition".to_owned())
                    && spec.args.contains(&"--x".to_owned())
            );
            assert_eq!(spec.label, "tsserver");
        }
        Err(why) => assert_eq!(why, "node is not on PATH"),
    }
    let _ = std::fs::remove_file(lib.join("tsserver.js"));
}
