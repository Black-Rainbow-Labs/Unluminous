//! Completion with a language server, and the structural tier that answers without one.
//! `task-2231`.
//!
//! The server here is this test binary started again: `scripted_server` below checks its arguments for
//! `serve=lsp` and, when it finds one, speaks the Language Server Protocol on its standard input and
//! output with fixed answers, then exits. That is `crates/unluminous-lsp/tests/scripted.rs`'s trick and
//! `unluminous-dap`'s before it, and it makes a picture of a server's rows the same on every run, which a
//! real rust-analyzer could never be. The real servers are tested in `unluminous-lsp`'s `tests/real.rs`.
//!
//! What is here is what only a window can show: the server's rows merged into the popup, a call
//! inserted with its brackets and the signature line opening over it, an import a server row brings
//! undone in one step with the name, the structural tier importing a name with no server at all, the
//! documentation panel, and the command line answering the rows the popup shows.

mod common;

use std::collections::HashMap;
use std::io::{BufRead, Write};

use common::*;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use unluminous_app::UnluminousApp;
use unluminous_core::Command;

// ---------------------------------------------------------------- the scripted server

/// Started as a server, this is it. Started as a test, it does nothing.
#[test]
fn scripted_server() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "serve=lsp") {
        serve_lsp();
        std::process::exit(0);
    }
}

/// One `Content-Length` frame from standard input.
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

/// One frame to standard output.
fn write_frame(value: &Value) {
    let body = value.to_string();
    let mut out = std::io::stdout().lock();
    write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    out.flush().unwrap();
}

/// The rows the scripted server offers, whatever is asked: three members of a `Layout`, one of which
/// the project's structure does not know, and a type that needs an import.
fn scripted_items() -> Value {
    json!([
        {"label": "width", "kind": 5, "detail": "f32", "sortText": "0001"},
        {"label": "caret_at(…)", "kind": 2, "detail": "fn(&self, offset: usize) -> f32", "sortText": "0002",
         "filterText": "caret_at", "insertText": "caret_at(${1:offset})$0", "insertTextFormat": 2},
        {"label": "len_bytes()", "kind": 2, "detail": "fn(&self) -> usize", "sortText": "0003",
         "filterText": "len_bytes", "insertText": "len_bytes()", "insertTextFormat": 1},
        {"label": "HashMap", "kind": 22, "detail": "std::collections::HashMap<K, V>", "sortText": "0004",
         "data": {"import": "std::collections::HashMap"}}
    ])
}

/// A document's text as the scripted server has been told it, with an incremental change applied.
fn apply_change(text: &mut String, change: &Value) {
    let Some(range) = change.get("range") else {
        *text = change["text"].as_str().unwrap_or("").to_owned();
        return;
    };
    let at = |text: &str, p: &Value| {
        unluminous_lsp::byte_of_utf8_column(
            text,
            p["line"].as_u64().unwrap() as u32,
            p["character"].as_u64().unwrap() as u32,
        )
    };
    let (start, end) = (at(text, &range["start"]), at(text, &range["end"]));
    text.replace_range(start..end, change["text"].as_str().unwrap_or(""));
}

/// The range of the identifier typed before a position, as rust-analyzer gives it in `itemDefaults`.
fn word_range(text: &str, position: &Value) -> Value {
    let line = position["line"].as_u64().unwrap() as u32;
    let column = position["character"].as_u64().unwrap() as u32;
    let at = unluminous_lsp::byte_of_utf8_column(text, line, column);
    let typed = text[..at]
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .map(char::len_utf8)
        .sum::<usize>();
    json!({"start": {"line": line, "character": column - typed as u32}, "end": position})
}

/// The scripted Language Server Protocol server.
fn serve_lsp() {
    let mut input = std::io::stdin().lock();
    let mut texts: HashMap<String, String> = HashMap::new();
    while let Some(message) = read_frame(&mut input) {
        let method = message["method"].as_str().unwrap_or("");
        let params = &message["params"];
        let Some(id) = message.get("id").cloned() else {
            match method {
                "exit" => return,
                "textDocument/didOpen" => {
                    let document = &params["textDocument"];
                    texts.insert(
                        document["uri"].as_str().unwrap().to_owned(),
                        document["text"].as_str().unwrap().to_owned(),
                    );
                }
                "textDocument/didChange" => {
                    let text = texts
                        .entry(params["textDocument"]["uri"].as_str().unwrap().to_owned())
                        .or_default();
                    for change in params["contentChanges"].as_array().unwrap() {
                        apply_change(text, change);
                    }
                }
                _ => {}
            }
            continue;
        };
        let result = match method {
            "initialize" => json!({"capabilities": {"positionEncoding": "utf-8",
                "completionProvider": {"triggerCharacters": [".", ":"], "resolveProvider": true},
                "signatureHelpProvider": {"triggerCharacters": ["(", ","]}}}),
            "textDocument/completion" => {
                let text = texts
                    .get(params["textDocument"]["uri"].as_str().unwrap())
                    .cloned()
                    .unwrap_or_default();
                let range = word_range(&text, &params["position"]);
                json!({"isIncomplete": false, "itemDefaults": {"editRange": range}, "items": scripted_items()})
            }
            "completionItem/resolve" => {
                let mut item = message["params"].clone();
                item["documentation"] = json!({"kind": "markdown", "value": "A hash map."});
                if item["data"]["import"].is_string() {
                    item["additionalTextEdits"] = json!([{"range": {"start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 0}}, "newText": "use std::collections::HashMap;\n"}]);
                }
                item
            }
            "textDocument/signatureHelp" => json!({"signatures": [{
                "label": "caret_at(&self, offset: usize) -> f32",
                "parameters": [{"label": "&self"}, {"label": "offset: usize"}],
                "documentation": "Where the caret is drawn."}],
                "activeSignature": 0, "activeParameter": 1}),
            "shutdown" => Value::Null,
            _ => Value::Null,
        };
        write_frame(&json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }
}

// ---------------------------------------------------------------- the windows

/// A small crate: a `Layout` with a field and a method, a function taking one, and a second module
/// whose function is not imported where it is used.
fn semantic_folder() -> std::path::PathBuf {
    fixture("unluminous-screenshot-completion-semantic", &[
        ("Cargo.toml", "[package]\nname = \"semantic\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        ("src/lib.rs", "//! Laying a document out.\n\npub mod shapes;\n\n/// A laid out document.\npub struct Layout {\n    /// How wide it is.\n    pub width: f32,\n}\n\nimpl Layout {\n    /// Where the caret is drawn.\n    pub fn caret_at(&self, offset: usize) -> f32 {\n        offset as f32\n    }\n}\n\npub fn measure(layout: Layout) -> f32 {\n    \n}\n"),
        ("src/shapes.rs", "/// Makes a circle of a radius.\npub fn make_circle(radius: f32) -> f32 {\n    radius\n}\n"),
        ("src/main.rs", "fn main() {\n    \n}\n"),
    ])
}

/// A small TypeScript project: a card maker in one file and an empty file that would use it.
fn typescript_folder() -> std::path::PathBuf {
    fixture("unluminous-screenshot-completion-typescript", &[
        ("cards.ts", "/** Makes a card with a title. */\nexport function makeCard(title: string): string {\n  return title;\n}\n"),
        ("board.ts", "const first = 1;\n\nexport function build() {\n  \n}\n"),
    ])
}

/// Runs frames until a condition holds, with a short sleep between them because a server's answer
/// comes from another process. Panics with `what` when it never does.
fn until(
    harness: &mut Harness<'static, UnluminousApp>,
    what: &str,
    done: impl Fn(&UnluminousApp) -> bool,
) {
    for _ in 0..800 {
        pump(harness);
        if done(harness.state()) {
            steady(harness);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("{what} never happened");
}

/// A window on a folder with the named file open and its index built, and, when `server` is true, the
/// scripted server running and ready.
fn semantic_harness(
    folder: &std::path::Path,
    open: &str,
    server: bool,
) -> Harness<'static, UnluminousApp> {
    let mut harness = harness_in(folder);
    if server {
        let me = std::env::current_exe().expect("the test binary");
        let args = ["scripted_server", "--exact", "--nocapture", "--test-threads=1", "serve=lsp"];
        harness.state_mut().use_this_language_server(me, args.map(String::from).to_vec());
    }
    harness.state_mut().open_path_permanently(&folder.join(open)).expect("the file opens");
    until(&mut harness, "the project index", |app| {
        app.symbols_indexer().is_some_and(|i| !i.is_building() && !i.is_empty())
    });
    if server {
        until(&mut harness, "the scripted server to be ready", |app| {
            app.server_states()
                .iter()
                .any(|(_, state, _)| *state == unluminous_lsp::ServerState::Ready)
        });
    }
    harness
}

/// The signature at the caret once the server has answered for it, running frames until it has.
fn signature_from_the_server(
    harness: &mut Harness<'static, UnluminousApp>,
) -> unluminous_app::app::signature::Signature {
    for _ in 0..800 {
        pump(harness);
        if let Some(signature) =
            harness.state_mut().signature_at_the_caret().filter(|s| s.from_server)
        {
            steady(harness);
            return signature;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let text = harness.state().document().text().to_string();
    let head = harness.state().document().selection();
    panic!(
        "the server never answered the signature help: {:?} at {head:?} in {text}",
        harness.state_mut().signature_at_the_caret()
    );
}

/// Puts the caret just after the first place `needle` is found, plus `extra` bytes.
fn caret_after(harness: &mut Harness<'static, UnluminousApp>, needle: &str, extra: usize) {
    let text = harness.state().document().text().to_string();
    let at = text.find(needle).unwrap_or_else(|| panic!("{needle:?} is in the file"))
        + needle.len()
        + extra;
    harness.state_mut().command(Command::PlaceCaret { offset: at, extend: false });
    steady(harness);
}

/// Types text a character at a time, as real text events.
fn type_text(harness: &mut Harness<'static, UnluminousApp>, text: &str) {
    for letter in text.chars() {
        harness.input_mut().events.push(egui::Event::Text(letter.to_string()));
        steady(harness);
    }
}

/// The names on offer that came from the server.
fn server_rows(harness: &Harness<'static, UnluminousApp>) -> Vec<String> {
    harness
        .state()
        .completion()
        .map(|state| {
            state
                .rows
                .iter()
                .filter(|row| row.source == unluminous_core::completion::Source::Server)
                .map(|row| row.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------- with a server

#[test]
fn a_dot_offers_the_servers_members_beside_the_structures() {
    let folder = semantic_folder();
    let mut harness = semantic_harness(&folder, "src/lib.rs", true);
    caret_after(&mut harness, "measure(layout: Layout) -> f32 {\n    ", 0);
    type_text(&mut harness, "layout.");
    until(&mut harness, "the server's rows", |app| {
        app.completion().is_some_and(|s| s.rows.iter().any(|r| r.name == "len_bytes"))
    });
    let offered = completions(&harness);
    assert!(offered.contains(&"width".to_owned()), "{offered:?}");
    assert!(offered.contains(&"caret_at".to_owned()), "{offered:?}");
    assert!(
        server_rows(&harness).contains(&"len_bytes".to_owned()),
        "only the server knows len_bytes"
    );
    harness.get_by_label("Completion len_bytes");
    harness.snapshot(shot("completion_server_members"));
}

#[test]
fn accepting_a_method_inserts_its_brackets_and_opens_the_signature_line() {
    let folder = semantic_folder();
    let mut harness = semantic_harness(&folder, "src/lib.rs", true);
    caret_after(&mut harness, "measure(layout: Layout) -> f32 {\n    ", 0);
    type_text(&mut harness, "layout.car");
    until(&mut harness, "the server's caret_at row", |app| {
        app.completion().is_some_and(|s| s.rows.iter().any(|r| r.name == "caret_at"))
    });
    while harness
        .state()
        .completion()
        .and_then(|s| s.chosen_row())
        .is_some_and(|r| r.name != "caret_at")
    {
        harness.key_press(egui::Key::ArrowDown);
        steady(&mut harness);
    }
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    let text = harness.state().document().text().to_string();
    assert!(text.contains("layout.caret_at()"), "the call is inserted with its brackets: {text}");
    until(&mut harness, "the signature line", |app| app.signature_is_open());
    let signature = signature_from_the_server(&mut harness);
    assert_eq!(signature.label, "caret_at(&self, offset: usize) -> f32");
    assert_eq!(signature.active, Some(1));
    harness.get_by_label("Signature help: caret_at(&self, offset: usize) -> f32");
    harness.snapshot(shot("completion_signature_help"));
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert!(!harness.state().signature_is_open(), "Escape closes the line");
}

#[test]
fn a_server_row_that_needs_an_import_is_undone_in_one_step_with_its_name() {
    let folder = semantic_folder();
    let mut harness = semantic_harness(&folder, "src/lib.rs", true);
    caret_after(&mut harness, "measure(layout: Layout) -> f32 {\n    ", 0);
    let before = harness.state().document().text().to_string();
    type_text(&mut harness, "HashM");
    until(&mut harness, "the server's HashMap row", |app| {
        app.completion().is_some_and(|s| s.rows.iter().any(|r| r.name == "HashMap"))
    });
    while harness
        .state()
        .completion()
        .and_then(|s| s.chosen_row())
        .is_some_and(|r| r.name != "HashMap")
    {
        harness.key_press(egui::Key::ArrowDown);
        steady(&mut harness);
    }
    // The row is resolved once it has rested for a heartbeat, which is when its import arrives.
    until(&mut harness, "the HashMap row to be resolved", |app| {
        app.completion()
            .and_then(|s| s.chosen_row())
            .is_some_and(|r| !r.info.extra_edits.is_empty())
    });
    let typed = harness.state().document().text().to_string();
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    let text = harness.state().document().text().to_string();
    assert!(text.starts_with("use std::collections::HashMap;\n"), "{text}");
    assert!(text.contains("    HashMap\n"), "{text}");
    harness.state_mut().command(Command::Undo);
    steady(&mut harness);
    assert_eq!(
        harness.state().document().text().to_string(),
        typed,
        "one undo takes the name and the import"
    );
    assert_ne!(typed, before);
}

#[test]
fn the_command_line_answers_the_servers_rows_and_signature_and_its_state() {
    let folder = semantic_folder();
    let mut harness = semantic_harness(&folder, "src/lib.rs", true);
    caret_after(&mut harness, "measure(layout: Layout) -> f32 {\n    ", 0);
    let result =
        did_while_waiting(&mut harness, "editor complete --after layout. --limit 20 --wait 3000");
    let names: Vec<&str> = result["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .filter_map(|r| r["name"].as_str())
        .collect();
    assert!(names.contains(&"width"), "--after reads the members the structure knows: {names:?}");
    let servers = did(&mut harness, "status --section servers");
    let running = servers["servers"]["running"].as_array().expect("servers");
    assert_eq!(running.len(), 1, "{servers}");
    assert_eq!(running[0]["state"], "ready");
    assert_eq!(running[0]["answersTheShowingFile"], true);
    type_text(&mut harness, "layout.caret_at(");
    let signature = did_while_waiting(&mut harness, "editor signature --wait 3000");
    assert_eq!(signature["signature"], "caret_at(&self, offset: usize) -> f32");
    assert_eq!(signature["activeParameter"], "offset: usize");
    assert_eq!(signature["source"], "server");
    type_text(&mut harness, "1, ");
    harness.state_mut().command(Command::PlaceCaret { offset: 0, extend: false });
    steady(&mut harness);
    assert!(
        !harness.state().signature_is_open(),
        "the line closes once the caret leaves the brackets"
    );
}

#[test]
fn with_servers_off_no_server_is_started() {
    let folder = semantic_folder();
    let mut harness = harness_in(&folder);
    let me = std::env::current_exe().expect("the test binary");
    harness.state_mut().use_this_language_server(me, vec!["serve=lsp".to_owned()]);
    did(&mut harness, "settings set editor.servers off");
    harness.state_mut().open_path_permanently(&folder.join("src/lib.rs")).expect("the file opens");
    for _ in 0..20 {
        pump(&mut harness);
    }
    assert!(harness.state().server_states().is_empty(), "editor.servers off starts nothing");
    let servers = did(&mut harness, "status --section servers");
    assert_eq!(servers["servers"]["setting"], "off");
}

#[test]
fn a_dot_the_structure_cannot_answer_opens_the_list_when_the_server_does() {
    let folder = semantic_folder();
    let mut harness = semantic_harness(&folder, "src/lib.rs", true);
    caret_after(
        &mut harness,
        "measure(layout: Layout) -> f32 {
    ",
        0,
    );
    // Nothing in the project says what `mystery()` returns, so the structure offers no member.
    type_text(&mut harness, "mystery().");
    until(&mut harness, "the server's rows to open the list", |app| {
        app.completion().is_some_and(|s| s.rows.iter().any(|r| r.name == "len_bytes"))
    });
    assert!(server_rows(&harness).contains(&"width".to_owned()), "{:?}", completions(&harness));
}

#[test]
fn a_late_server_answer_does_not_reopen_a_list_that_was_closed() {
    let folder = semantic_folder();
    let mut harness = semantic_harness(&folder, "src/lib.rs", true);
    caret_after(
        &mut harness,
        "measure(layout: Layout) -> f32 {
    ",
        0,
    );
    type_text(&mut harness, "layout.");
    assert!(harness.state().completion().is_some(), "the structure's rows open the list at once");
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert!(harness.state().completion().is_none());
    // Whatever the server answers from here is about a list nobody has open.
    for _ in 0..50 {
        pump(&mut harness);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(harness.state().completion().is_none(), "the answer reopened the list");
}

#[test]
fn a_server_that_cannot_start_says_so_and_the_structure_still_answers() {
    let folder = semantic_folder();
    let mut harness = harness_in(&folder);
    let missing = folder.join("no-such-server.exe");
    harness.state_mut().use_this_language_server(missing, Vec::new());
    harness.state_mut().open_path_permanently(&folder.join("src/lib.rs")).expect("the file opens");
    until(&mut harness, "the project index", |app| {
        app.symbols_indexer().is_some_and(|i| !i.is_building() && !i.is_empty())
    });
    until(&mut harness, "the server to be reported as stopped", |app| {
        app.server_states()
            .iter()
            .any(|(_, state, _)| matches!(state, unluminous_lsp::ServerState::Failed(_)))
    });
    let servers = did(&mut harness, "status --section servers");
    assert_eq!(servers["servers"]["running"][0]["state"], "failed", "{servers}");
    caret_after(
        &mut harness,
        "measure(layout: Layout) -> f32 {
    ",
        0,
    );
    type_text(&mut harness, "layout.");
    assert!(completions(&harness).contains(&"width".to_owned()), "the structure still answers");
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    // The footer reads `Rust · rust-analyzer stopped`.
    harness.snapshot(shot("completion_server_stopped"));
}

// ---------------------------------------------------------------- with no server

#[test]
fn the_structure_imports_a_rust_function_from_another_module_with_no_server() {
    let folder = semantic_folder();
    let mut harness = semantic_harness(&folder, "src/lib.rs", false);
    caret_after(&mut harness, "measure(layout: Layout) -> f32 {\n    ", 0);
    type_text(&mut harness, "make_ci");
    let state = harness.state().completion().expect("the popup is open");
    let row = state.rows.iter().find(|r| r.name == "make_circle").expect("make_circle is offered");
    assert!(row.info.needs_import.is_some(), "a name from another module needs an import");
    while harness
        .state()
        .completion()
        .and_then(|s| s.chosen_row())
        .is_some_and(|r| r.name != "make_circle")
    {
        harness.key_press(egui::Key::ArrowDown);
        steady(&mut harness);
    }
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    let text = harness.state().document().text().to_string();
    assert!(text.contains("use crate::shapes::make_circle;\n"), "{text}");
    assert!(text.contains("make_circle("), "a function is inserted with its brackets: {text}");
    assert!(
        harness.state().signature_is_open(),
        "and the signature line opens over its parameters"
    );
    let signature = harness.state_mut().signature_at_the_caret().expect("the structure reads it");
    assert_eq!(signature.label, "make_circle(radius: f32) -> f32");
    assert!(!signature.from_server);
    harness.snapshot(shot("completion_structural_signature"));
}

#[test]
fn the_structure_imports_a_typescript_function_from_another_file_with_no_server() {
    let folder = typescript_folder();
    let mut harness = semantic_harness(&folder, "board.ts", false);
    caret_after(&mut harness, "build() {\n  ", 0);
    type_text(&mut harness, "makeC");
    while harness
        .state()
        .completion()
        .and_then(|s| s.chosen_row())
        .is_some_and(|r| r.name != "makeCard")
    {
        harness.key_press(egui::Key::ArrowDown);
        steady(&mut harness);
    }
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    let text = harness.state().document().text().to_string();
    assert!(text.starts_with("import { makeCard } from './cards';\n"), "{text}");
    assert!(text.contains("makeCard("), "{text}");
}

#[test]
fn the_chosen_row_shows_its_signature_and_documentation_after_resting() {
    let folder = semantic_folder();
    let mut harness = semantic_harness(&folder, "src/lib.rs", false);
    caret_after(&mut harness, "measure(layout: Layout) -> f32 {\n    ", 0);
    type_text(&mut harness, "layout.");
    assert!(completions(&harness).contains(&"caret_at".to_owned()), "{:?}", completions(&harness));
    while harness
        .state()
        .completion()
        .and_then(|s| s.chosen_row())
        .is_some_and(|r| r.name != "caret_at")
    {
        harness.key_press(egui::Key::ArrowDown);
        steady(&mut harness);
    }
    // The panel waits one heartbeat of rest; the harness's frames are a quarter of a second apart.
    for _ in 0..4 {
        harness.step();
    }
    steady(&mut harness);
    harness.get_by_label("Completion documentation");
    harness.snapshot(shot("completion_documentation"));
}
