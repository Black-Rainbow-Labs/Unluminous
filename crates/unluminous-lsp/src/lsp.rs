//! The Language Server Protocol, for rust-analyzer.
//!
//! The client asks for UTF-8 positions, so a byte offset is a line and a byte column. A server that
//! answers with UTF-16 anyway (the capability says which) is handled by [`Enc`]. Messages are built and
//! read as `serde_json::Value`s, the way `unluminous-dap` does DAP.
//!
//! **What `expected_type` is.** rust-analyzer's `relevance` (its `type_match` flag) is not on the wire.
//! What the wire has is `preselect`, which rust-analyzer sets for a row whose type matches the expected
//! type (or a local of it), so `expected_type` is `preselect`. Its `sortText` is read as a number for
//! `order`, smaller first.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::convert::{byte_of_utf16, byte_of_utf8_column, line_and_utf16, line_and_utf8_column};
use crate::framing::encode_frame;
use crate::worker::{Incoming, Protocol, Request, What};
use crate::{Insertion, Item, Kind, Reply, ServerState, SignatureHelp, TextEdit, Trigger};

/// How columns are counted, as the server agreed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Enc {
    Utf8,
    Utf16,
}

impl Enc {
    /// A byte offset as an LSP position.
    fn position(self, text: &str, offset: usize) -> Value {
        let (line, character) = match self {
            Enc::Utf8 => line_and_utf8_column(text, offset),
            Enc::Utf16 => line_and_utf16(text, offset),
        };
        json!({"line": line, "character": character})
    }

    /// An LSP position as a byte offset, clamped.
    fn offset(self, text: &str, position: &Value) -> Option<usize> {
        let line = position.get("line")?.as_u64()? as u32;
        let character = position.get("character")?.as_u64()? as u32;
        Some(match self {
            Enc::Utf8 => byte_of_utf8_column(text, line, character),
            Enc::Utf16 => byte_of_utf16(text, line, character),
        })
    }

    /// An LSP range as bytes.
    fn range(self, text: &str, range: &Value) -> Option<Range<usize>> {
        let start = self.offset(text, range.get("start")?)?;
        let end = self.offset(text, range.get("end")?)?;
        Some(start..end.max(start))
    }
}

/// A request waiting for its response.
enum Pending {
    Initialize,
    Shutdown,
    Complete { ticket: u64, path: std::path::PathBuf, revision: u64, text: Arc<str> },
    Resolve { ticket: u64, item: Box<Item>, text: Arc<str> },
    Signature { ticket: u64, path: std::path::PathBuf, revision: u64 },
}

/// The client half of one LSP session.
pub(crate) struct Lsp {
    next_id: i64,
    pending: HashMap<i64, Pending>,
    tickets: HashMap<u64, i64>,
    progress: Vec<(String, String, Option<u32>)>,
    enc: Enc,
}

impl Lsp {
    pub(crate) fn new() -> Lsp {
        Lsp {
            next_id: 0,
            pending: HashMap::new(),
            tickets: HashMap::new(),
            progress: Vec::new(),
            enc: Enc::Utf8,
        }
    }

    /// A request message, remembering what it is for.
    fn ask(&mut self, method: &str, params: Value, pending: Pending) -> Value {
        self.next_id += 1;
        self.pending.insert(self.next_id, pending);
        json!({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
    }

    /// The state the progress tokens add up to: indexing while any is running, else ready.
    fn progress_state(&self) -> ServerState {
        match self.progress.last() {
            Some((_, message, percent)) => {
                ServerState::Indexing { message: message.clone(), percent: *percent }
            }
            None => ServerState::Ready,
        }
    }

    /// `$/progress`: `begin` adds a token, `report` updates it, `end` removes it.
    fn on_progress(&mut self, params: &Value) -> Option<ServerState> {
        let token = match params.get("token")? {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let value = params.get("value")?;
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        let percent = value.get("percentage").and_then(Value::as_u64).map(|p| p.min(100) as u32);
        match value.get("kind")?.as_str()? {
            "begin" => self.progress.push((
                token,
                text("message").or_else(|| text("title")).unwrap_or_default(),
                percent,
            )),
            "report" => {
                if let Some(entry) = self.progress.iter_mut().find(|(t, _, _)| *t == token) {
                    if let Some(message) = text("message") {
                        entry.1 = message;
                    }
                    entry.2 = percent.or(entry.2);
                }
            }
            "end" => self.progress.retain(|(t, _, _)| *t != token),
            _ => return None,
        }
        Some(self.progress_state())
    }

    /// An answer to a request, as replies.
    fn on_response(&mut self, id: i64, message: &Value) -> Incoming {
        let Some(pending) = self.pending.remove(&id) else { return Incoming::default() };
        if let Some(error) = message.get("error") {
            let reason = error.get("message").and_then(Value::as_str).unwrap_or("error").to_owned();
            return match pending {
                Pending::Initialize => Incoming {
                    state: Some(ServerState::Failed(format!("initialize failed: {reason}"))),
                    ..Incoming::default()
                },
                _ => Incoming::default(),
            };
        }
        let result = message.get("result").cloned().unwrap_or(Value::Null);
        match pending {
            Pending::Initialize => self.initialized(&result),
            Pending::Shutdown => Incoming::default(),
            Pending::Complete { ticket, path, revision, text } => {
                let (incomplete, items) = map_completions(&result, &text, self.enc);
                reply(Reply::Completions { ticket, path, revision, incomplete, items })
            }
            Pending::Resolve { ticket, item, text } => reply(Reply::Resolved {
                ticket,
                item: resolved_item(*item, &result, &text, self.enc),
            }),
            Pending::Signature { ticket, path, revision } => {
                reply(Reply::Signature { ticket, path, revision, help: map_signature(&result) })
            }
        }
    }

    /// The server answered `initialize`: note its position encoding, send `initialized`, and be ready.
    fn initialized(&mut self, result: &Value) -> Incoming {
        let chosen = result.pointer("/capabilities/positionEncoding").and_then(Value::as_str);
        self.enc = if chosen == Some("utf-16") { Enc::Utf16 } else { Enc::Utf8 };
        let send = vec![json!({"jsonrpc": "2.0", "method": "initialized", "params": {}})];
        Incoming { send, state: Some(ServerState::Ready), initialized: true, ..Incoming::default() }
    }

    /// A request the server makes of the client, answered so the server never waits.
    fn on_server_request(&self, id: &Value, method: &str, message: &Value) -> Incoming {
        let result = match method {
            "workspace/configuration" => {
                let count =
                    message.pointer("/params/items").and_then(Value::as_array).map_or(0, Vec::len);
                Value::Array(vec![Value::Null; count])
            }
            "workspace/applyEdit" => json!({"applied": false}),
            _ => Value::Null,
        };
        Incoming {
            send: vec![json!({"jsonrpc": "2.0", "id": id, "result": result})],
            ..Incoming::default()
        }
    }
}

fn reply(reply: Reply) -> Incoming {
    Incoming { replies: vec![reply], ..Incoming::default() }
}

/// A `file:` address for a path, with the characters a URI cannot hold escaped.
///
/// @param path - an absolute path, with either kind of separator
pub(crate) fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

impl Protocol for Lsp {
    fn encode(&self, message: &Value) -> Vec<u8> {
        encode_frame(message)
    }

    fn initialize(&mut self, root: &Path) -> Vec<Value> {
        let uri = file_uri(root);
        let name = root
            .file_name()
            .map_or_else(|| "project".to_owned(), |n| n.to_string_lossy().into_owned());
        let params = json!({
            "processId": std::process::id(),
            "clientInfo": {"name": "Unluminous"},
            "rootUri": uri,
            "workspaceFolders": [{"uri": uri, "name": name}],
            "capabilities": client_capabilities(),
            "initializationOptions": {
                "completion": {"callable": {"snippets": "add_parentheses"}, "autoimport": {"enable": true}, "postfix": {"enable": true}, "limit": 200}
            },
        });
        vec![self.ask("initialize", params, Pending::Initialize)]
    }

    fn open(&mut self, path: &Path, language: &str, revision: u64, text: &str) -> Vec<Value> {
        let document = json!({"uri": file_uri(path), "languageId": language, "version": revision, "text": text});
        vec![
            json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": document}}),
        ]
    }

    fn change(
        &mut self,
        path: &Path,
        revision: u64,
        old: &str,
        range: &Range<usize>,
        replacement: &str,
    ) -> Vec<Value> {
        let span = json!({"start": self.enc.position(old, range.start), "end": self.enc.position(old, range.end)});
        let params = json!({
            "textDocument": {"uri": file_uri(path), "version": revision},
            "contentChanges": [{"range": span, "text": replacement}],
        });
        vec![json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": params})]
    }

    fn close(&mut self, path: &Path) -> Vec<Value> {
        let params = json!({"textDocument": {"uri": file_uri(path)}});
        vec![json!({"jsonrpc": "2.0", "method": "textDocument/didClose", "params": params})]
    }

    fn request(&mut self, request: Request) -> Vec<Value> {
        let Request { ticket, path, revision, text, what } = request;
        let document = json!({"uri": file_uri(&path)});
        let (message, id) = match what {
            What::Complete { offset, trigger } => {
                let params = json!({"textDocument": document, "position": self.enc.position(&text, offset), "context": trigger_context(trigger)});
                let pending = Pending::Complete { ticket, path, revision, text };
                (self.ask("textDocument/completion", params, pending), self.next_id)
            }
            What::Resolve { item, .. } => {
                let params: Value = serde_json::from_str(&item.handle).unwrap_or(Value::Null);
                let pending = Pending::Resolve { ticket, item, text };
                (self.ask("completionItem/resolve", params, pending), self.next_id)
            }
            What::Signature { offset } => {
                let params =
                    json!({"textDocument": document, "position": self.enc.position(&text, offset)});
                let pending = Pending::Signature { ticket, path, revision };
                (self.ask("textDocument/signatureHelp", params, pending), self.next_id)
            }
        };
        self.tickets.insert(ticket, id);
        vec![message]
    }

    fn cancel(&mut self, ticket: u64) -> Vec<Value> {
        match self.tickets.get(&ticket) {
            Some(id) if self.pending.contains_key(id) => {
                vec![json!({"jsonrpc": "2.0", "method": "$/cancelRequest", "params": {"id": id}})]
            }
            _ => Vec::new(),
        }
    }

    fn incoming(&mut self, message: Value) -> Incoming {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).map(str::to_owned);
        match (id, method) {
            (Some(id), Some(method)) => self.on_server_request(&id, &method, &message),
            (Some(id), None) => self.on_response(id.as_i64().unwrap_or(-1), &message),
            (None, Some(method)) if method == "$/progress" => {
                let state = message.get("params").and_then(|p| self.on_progress(p));
                Incoming { state, ..Incoming::default() }
            }
            _ => Incoming::default(),
        }
    }

    fn shutdown(&mut self) -> Vec<Value> {
        let ask = self.ask("shutdown", Value::Null, Pending::Shutdown);
        vec![ask, json!({"jsonrpc": "2.0", "method": "exit"})]
    }
}

/// What this client can do, which decides what rust-analyzer sends.
fn client_capabilities() -> Value {
    json!({
        "general": {"positionEncodings": ["utf-8"]},
        "window": {"workDoneProgress": true},
        "textDocument": {
            "completion": {
                "completionItem": {
                    "snippetSupport": true,
                    "resolveSupport": {"properties": ["documentation", "detail", "additionalTextEdits"]},
                    "insertReplaceSupport": true,
                    "labelDetailsSupport": true,
                    "deprecatedSupport": true,
                    "tagSupport": {"valueSet": [1]},
                },
                "completionList": {"itemDefaults": ["commitCharacters", "editRange", "insertTextFormat", "data"]},
            },
            "signatureHelp": {"signatureInformation": {"parameterInformation": {"labelOffsetSupport": true}}},
        },
    })
}

/// LSP's `context` for a trigger.
fn trigger_context(trigger: Trigger) -> Value {
    match trigger {
        Trigger::Invoked => json!({"triggerKind": 1}),
        Trigger::Character(c) => json!({"triggerKind": 2, "triggerCharacter": c.to_string()}),
        Trigger::Continued => json!({"triggerKind": 3}),
    }
}

/// A completion answer (a list, an object with `items`, or nothing) as rows, and whether it is incomplete.
fn map_completions(result: &Value, text: &str, enc: Enc) -> (bool, Vec<Item>) {
    let (list, incomplete, defaults) = match result {
        Value::Array(items) => (items.as_slice(), false, &Value::Null),
        Value::Object(_) => {
            let items =
                result.get("items").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
            (
                items,
                result.get("isIncomplete").and_then(Value::as_bool).unwrap_or(false),
                result.get("itemDefaults").unwrap_or(&Value::Null),
            )
        }
        _ => (&[][..], false, &Value::Null),
    };
    (incomplete, list.iter().map(|item| map_item(item, defaults, text, enc)).collect())
}

/// One LSP completion item as a row.
fn map_item(v: &Value, defaults: &Value, text: &str, enc: Enc) -> Item {
    let kind = v.get("kind").and_then(Value::as_u64).and_then(kind_of);
    let label = clean_label(v.get("label").and_then(Value::as_str).unwrap_or(""));
    let snippet = v
        .get("insertTextFormat")
        .or_else(|| defaults.get("insertTextFormat"))
        .and_then(Value::as_u64)
        == Some(2);
    let insertion = insertion_of(v, defaults, &label, text, enc, snippet);
    let callable = matches!(kind, Some(Kind::Function | Kind::Method))
        && insertion.as_ref().is_some_and(|i| i.text.contains('(') && i.text.ends_with(')'));
    let data = v.get("data").or_else(|| defaults.get("data")).filter(|d| !d.is_null());
    let has_doc = v.get("documentation").is_some_and(|d| !d.is_null());
    let has_edits = v.get("additionalTextEdits").is_some_and(|d| !d.is_null());
    let mut handle = v.clone();
    if let (Some(data), Some(object)) = (data, handle.as_object_mut()) {
        object.entry("data").or_insert_with(|| data.clone());
    }
    Item {
        filter: v
            .get("filterText")
            .and_then(Value::as_str)
            .map_or_else(|| label.clone(), str::to_owned),
        order: order_of(v.get("sortText").and_then(Value::as_str)),
        kind,
        detail: detail_of(v),
        doc: documentation_of(v),
        deprecated: v.get("deprecated").and_then(Value::as_bool).unwrap_or(false)
            || tagged_deprecated(v),
        preselect: v.get("preselect").and_then(Value::as_bool).unwrap_or(false),
        expected_type: v.get("preselect").and_then(Value::as_bool).unwrap_or(false),
        insertion,
        callable,
        extra_edits: edits_of(v, text, enc),
        needs_resolve: data.is_some() && !has_doc && !has_edits,
        import: import_of(v),
        handle: handle.to_string(),
        label,
    }
}

/// The path a row would import: the first of rust-analyzer's `data.imports`, or the path in a
/// `labelDetails.detail` of the form `(use std::borrow::Cow)`.
///
/// @param v - the server's row
fn import_of(v: &Value) -> Option<String> {
    let from_data = v.pointer("/data/imports/0/full_import_path").and_then(Value::as_str);
    let from_detail = v
        .pointer("/labelDetails/detail")
        .and_then(Value::as_str)
        .and_then(|d| d.trim().strip_prefix("(use ")?.strip_suffix(')'));
    from_data.or(from_detail).map(str::to_owned)
}

/// The name without the call part rust-analyzer appends: `caret_at()` and `Some(…)` are `caret_at` and `Some`.
///
/// @param label - the server's label
pub(crate) fn clean_label(label: &str) -> String {
    match label.find('(') {
        Some(open) if open > 0 && label.ends_with(')') => label[..open].to_owned(),
        _ => label.to_owned(),
    }
}

/// `sortText` as a number: its leading hex digits, smaller first. No `sortText` sorts last.
///
/// @param sort - the server's `sortText`
pub(crate) fn order_of(sort: Option<&str>) -> u64 {
    let digits: String =
        sort.unwrap_or("").chars().take_while(char::is_ascii_hexdigit).take(16).collect();
    u64::from_str_radix(&digits, 16).unwrap_or(u64::MAX)
}

fn kind_of(number: u64) -> Option<Kind> {
    Some(match number {
        2 => Kind::Method,
        3 | 4 => Kind::Function,
        5 | 10 => Kind::Field,
        6 => Kind::Variable,
        7 => Kind::Class,
        8 => Kind::Interface,
        9 | 17 | 19 => Kind::Module,
        12 | 21 => Kind::Constant,
        13 => Kind::Enum,
        14 => Kind::Keyword,
        15 => Kind::Snippet,
        20 => Kind::Variant,
        22 => Kind::Struct,
        25 => Kind::Type,
        _ => return None,
    })
}

fn detail_of(v: &Value) -> Option<String> {
    let details = v.get("labelDetails");
    let part = |key: &str| details.and_then(|d| d.get(key)).and_then(Value::as_str).unwrap_or("");
    let joined = format!(
        "{}{}",
        part("detail"),
        if part("description").is_empty() {
            String::new()
        } else {
            format!(" {}", part("description"))
        }
    );
    let joined = joined.trim().to_owned();
    if !joined.is_empty() {
        return Some(joined);
    }
    v.get("detail").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn documentation_of(v: &Value) -> Option<String> {
    match v.get("documentation")? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Object(o) => {
            o.get("value").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned)
        }
        _ => None,
    }
}

fn tagged_deprecated(v: &Value) -> bool {
    v.get("tags")
        .and_then(Value::as_array)
        .is_some_and(|tags| tags.iter().any(|t| t.as_u64() == Some(1)))
}

/// What accepting the row inserts: the item's `textEdit`, or its text over the list's default range.
fn insertion_of(
    v: &Value,
    defaults: &Value,
    label: &str,
    text: &str,
    enc: Enc,
    snippet: bool,
) -> Option<Insertion> {
    let (insert, replace, new_text) = match v.get("textEdit") {
        Some(edit) => {
            let (insert, replace) = edit_ranges(edit, text, enc)?;
            (insert, replace, edit.get("newText").and_then(Value::as_str)?.to_owned())
        }
        None => {
            let (insert, replace) = edit_ranges(defaults.get("editRange")?, text, enc)?;
            let new_text = v.get("insertText").and_then(Value::as_str).unwrap_or(label).to_owned();
            (insert, replace, new_text)
        }
    };
    let (text, caret) = if snippet { reduce_snippet(&new_text) } else { (new_text, None) };
    Some(Insertion { insert, replace, text, caret })
}

/// The insert and replace ranges of a `TextEdit` (one range), an `InsertReplaceEdit`, or an `editRange`.
fn edit_ranges(edit: &Value, text: &str, enc: Enc) -> Option<(Range<usize>, Range<usize>)> {
    if let (Some(insert), Some(replace)) = (edit.get("insert"), edit.get("replace")) {
        return Some((enc.range(text, insert)?, enc.range(text, replace)?));
    }
    let range = enc.range(text, edit.get("range").unwrap_or(edit))?;
    Some((range.clone(), range))
}

fn edits_of(v: &Value, text: &str, enc: Enc) -> Vec<TextEdit> {
    let Some(list) = v.get("additionalTextEdits").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|e| {
            Some(TextEdit {
                range: enc.range(text, e.get("range")?)?,
                text: e.get("newText")?.as_str()?.to_owned(),
            })
        })
        .collect()
}

/// The row after `completionItem/resolve`: the documentation and the imports it needed.
fn resolved_item(mut item: Item, result: &Value, text: &str, enc: Enc) -> Item {
    if result.is_object() {
        item.doc = documentation_of(result).or(item.doc);
        item.detail = item.detail.or_else(|| detail_of(result));
        let edits = edits_of(result, text, enc);
        if !edits.is_empty() {
            item.extra_edits = edits;
        }
    }
    item.needs_resolve = false;
    item
}

/// Reduces an LSP snippet to plain text and the byte where the caret goes.
///
/// Unluminous has no tab stops to walk, so the caret goes where typing should start: the lowest
/// numbered tab stop from `$1` up, else `$0`. A call whose brackets hold nothing but placeholders,
/// rust-analyzer's `caret_at(${1:offset})$0`, becomes `caret_at()` with the caret inside, which is what
/// the reference editor inserts: the parameter names are shown by the signature line instead of being typed into
/// the file, where they would have to be deleted. Other placeholders keep their default text, and a
/// choice keeps its first.
///
/// @param snippet - snippet syntax: `foo($0)`, `${1:name}`, `\$`
pub(crate) fn reduce_snippet(snippet: &str) -> (String, Option<usize>) {
    if let Some(call) = empty_call(snippet) {
        return call;
    }
    let chars: Vec<char> = snippet.chars().collect();
    let (mut out, mut stops, mut at) = (String::new(), Vec::new(), 0);
    snippet_into(&chars, &mut at, &mut out, &mut stops, false);
    let caret = stops
        .iter()
        .filter(|(n, _)| *n > 0)
        .min_by_key(|(n, _)| *n)
        .or_else(|| stops.iter().find(|(n, _)| *n == 0))
        .map(|(_, pos)| *pos);
    (out, caret)
}

/// `name(${1:a}, ${2:b})$0` as `name()` with the caret between the brackets, when the brackets hold
/// only placeholders and the commas between them. `None` for any other snippet.
///
/// @param snippet - the snippet
fn empty_call(snippet: &str) -> Option<(String, Option<usize>)> {
    let body = snippet.strip_suffix("$0").unwrap_or(snippet);
    let open = body.find('(')?;
    let name = &body[..open];
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ':') {
        return None;
    }
    let inside = body[open + 1..].strip_suffix(')')?;
    if !inside.contains('$') {
        return None;
    }
    let mut rest = inside;
    while !rest.is_empty() {
        rest = rest.trim_start_matches([',', ' ']);
        if rest.is_empty() {
            break;
        }
        let stop = rest.strip_prefix("${").and_then(|r| r.find('}').map(|end| &r[end + 1..]));
        let bare =
            rest.strip_prefix('$').map(|r| r.trim_start_matches(|c: char| c.is_ascii_digit()));
        rest = match (stop, bare) {
            (Some(after), _) => after,
            (None, Some(after)) if after.len() < rest.len() - 1 => after,
            _ => return None,
        };
    }
    Some((format!("{name}()"), Some(name.len() + 1)))
}

/// Reads snippet text into `out` until the end, or until the `}` that closes a placeholder.
fn snippet_into(
    chars: &[char],
    at: &mut usize,
    out: &mut String,
    stops: &mut Vec<(u32, usize)>,
    nested: bool,
) {
    while *at < chars.len() {
        match chars[*at] {
            '}' if nested => return,
            '\\' if chars.get(*at + 1).is_some_and(|c| matches!(c, '$' | '}' | '\\')) => {
                out.push(chars[*at + 1]);
                *at += 2;
            }
            '$' => dollar(chars, at, out, stops),
            c => {
                out.push(c);
                *at += 1;
            }
        }
    }
}

/// Reads one `$n`, `${n}`, `${n:default}`, `${n|a,b|}`, `$name` or `${name:default}`.
fn dollar(chars: &[char], at: &mut usize, out: &mut String, stops: &mut Vec<(u32, usize)>) {
    let braced = chars.get(*at + 1) == Some(&'{');
    let start = *at + if braced { 2 } else { 1 };
    let word: String = chars[start.min(chars.len())..]
        .iter()
        .take_while(|c| c.is_alphanumeric() || **c == '_')
        .collect();
    if word.is_empty() {
        out.push('$');
        *at += 1;
        return;
    }
    *at = start + word.chars().count();
    if let Ok(number) = word.parse::<u32>() {
        stops.push((number, out.len()));
    }
    if !braced {
        return;
    }
    match chars.get(*at) {
        Some(':') => {
            *at += 1;
            snippet_into(chars, at, out, stops, true);
        }
        Some('|') => choice(chars, at, out),
        _ => {}
    }
    if chars.get(*at) == Some(&'}') {
        *at += 1;
    }
}

/// Reads `|a,b,c|` keeping `a`.
fn choice(chars: &[char], at: &mut usize, out: &mut String) {
    *at += 1;
    while *at < chars.len() && !matches!(chars[*at], ',' | '|') {
        out.push(chars[*at]);
        *at += 1;
    }
    while *at < chars.len() && chars[*at] != '|' {
        *at += 1;
    }
    *at += 1;
}

/// The active signature as one line, with each parameter's bytes in it.
fn map_signature(result: &Value) -> Option<SignatureHelp> {
    let signatures = result.get("signatures")?.as_array()?;
    let chosen = result.get("activeSignature").and_then(Value::as_u64).unwrap_or(0) as usize;
    let signature = signatures.get(chosen).or_else(|| signatures.first())?;
    let label = signature.get("label")?.as_str()?.to_owned();
    let parameters: Vec<Range<usize>> =
        signature.get("parameters").and_then(Value::as_array).map_or_else(Vec::new, |list| {
            list.iter().filter_map(|p| parameter_range(p, &label)).collect()
        });
    let active = signature
        .get("activeParameter")
        .or_else(|| result.get("activeParameter"))
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .filter(|n| *n < parameters.len());
    Some(SignatureHelp { label, parameters, active, doc: documentation_of(signature) })
}

/// A parameter's bytes in the signature label: a substring of it, or offsets into it.
fn parameter_range(parameter: &Value, label: &str) -> Option<Range<usize>> {
    match parameter.get("label")? {
        Value::String(name) => label.find(name.as_str()).map(|start| start..start + name.len()),
        Value::Array(pair) => {
            let start = pair.first()?.as_u64()? as usize;
            let end = pair.get(1)?.as_u64()? as usize;
            let floor = |mut at: usize| {
                at = at.min(label.len());
                while !label.is_char_boundary(at) {
                    at -= 1;
                }
                at
            };
            Some(floor(start)..floor(end).max(floor(start)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_reduce_to_text_and_a_caret() {
        assert_eq!(reduce_snippet("foo($0)"), ("foo()".to_owned(), Some(4)));
        assert_eq!(reduce_snippet("foo()$0"), ("foo()".to_owned(), Some(5)));
        assert_eq!(reduce_snippet("foo(${1:a}, ${2:b})"), ("foo()".to_owned(), Some(4)));
        assert_eq!(reduce_snippet("caret_at(${1:offset})$0"), ("caret_at()".to_owned(), Some(9)));
        assert_eq!(reduce_snippet("foo(x, ${1:b})"), ("foo(x, b)".to_owned(), Some(7)));
        assert_eq!(reduce_snippet("plain"), ("plain".to_owned(), None));
        assert_eq!(
            reduce_snippet("price \\$5 \\} $1 ${2|x,y|}"),
            ("price $5 }  x".to_owned(), Some(11))
        );
        assert_eq!(reduce_snippet("a ${1:b ${2:c}} $0"), ("a b c ".to_owned(), Some(2)));
        assert_eq!(reduce_snippet("cost $"), ("cost $".to_owned(), None));
    }

    #[test]
    fn sort_text_is_read_as_hex_and_smaller_is_first() {
        assert_eq!(order_of(Some("00000001")), 1);
        assert_eq!(order_of(Some("7fffffff")), 0x7fff_ffff);
        assert!(order_of(Some("0000000a")) < order_of(Some("0000000b")));
        assert_eq!(order_of(Some("11zzz")), 0x11);
        assert_eq!(order_of(Some("zzz")), u64::MAX);
        assert_eq!(order_of(None), u64::MAX);
    }

    #[test]
    fn a_label_loses_its_call_part() {
        assert_eq!(clean_label("caret_at()"), "caret_at");
        assert_eq!(clean_label("Some(…)"), "Some");
        assert_eq!(clean_label("width"), "width");
        assert_eq!(clean_label("(a, b)"), "(a, b)");
    }

    #[test]
    fn item_defaults_fill_in_what_an_item_leaves_out() {
        let text = "ab\nlet x = fo";
        let defaults = json!({"editRange": {"start": {"line": 1, "character": 8}, "end": {"line": 1, "character": 10}}, "insertTextFormat": 2, "data": {"d": 1}});
        let item = json!({"label": "foo()", "kind": 3, "sortText": "00000002", "insertText": "foo($0)", "labelDetails": {"detail": "(a: u8)", "description": "u8"}});
        let (incomplete, items) = map_completions(
            &json!({"isIncomplete": true, "itemDefaults": defaults, "items": [item]}),
            text,
            Enc::Utf8,
        );
        assert!(incomplete);
        let item = &items[0];
        assert_eq!(item.label, "foo");
        assert_eq!(item.order, 2);
        assert_eq!(item.kind, Some(Kind::Function));
        assert_eq!(item.detail.as_deref(), Some("(a: u8) u8"));
        let insertion = item.insertion.as_ref().unwrap();
        assert_eq!(insertion.insert, 11..13);
        assert_eq!((insertion.text.as_str(), insertion.caret), ("foo()", Some(4)));
        assert!(item.callable);
        assert!(item.needs_resolve);
        assert!(item.handle.contains("\"d\":1"));
    }

    #[test]
    fn an_insert_replace_edit_gives_two_ranges() {
        let text = "let x = fo(1)";
        let edit = json!({"newText": "foo", "insert": {"start": {"line": 0, "character": 8}, "end": {"line": 0, "character": 10}}, "replace": {"start": {"line": 0, "character": 8}, "end": {"line": 0, "character": 13}}});
        let (_, items) = map_completions(
            &json!([{"label": "foo", "textEdit": edit, "preselect": true}]),
            text,
            Enc::Utf8,
        );
        let insertion = items[0].insertion.as_ref().unwrap();
        assert_eq!((insertion.insert.clone(), insertion.replace.clone()), (8..10, 8..13));
        assert!(items[0].preselect && items[0].expected_type);
    }

    #[test]
    fn signature_parameters_are_bytes_in_the_label() {
        let result = json!({"signatures": [{"label": "new(w: f32, h: f32) -> Self", "parameters": [{"label": "w: f32"}, {"label": [10, 16]}], "documentation": {"kind": "markdown", "value": "Makes one."}}], "activeSignature": 0, "activeParameter": 1});
        let help = map_signature(&result).unwrap();
        assert_eq!(help.parameters, vec![4..10, 10..16]);
        assert_eq!(help.active, Some(1));
        assert_eq!(help.doc.as_deref(), Some("Makes one."));
    }

    #[test]
    fn a_path_is_a_file_uri() {
        assert_eq!(
            file_uri(Path::new("C:\\jason\\my proj\\a.rs")),
            "file:///C:/jason/my%20proj/a.rs"
        );
        assert_eq!(file_uri(Path::new("/home/x/a.rs")), "file:///home/x/a.rs");
    }

    #[test]
    fn progress_tokens_make_indexing_until_all_end() {
        let mut lsp = Lsp::new();
        let begin = |token: &str| json!({"token": token, "value": {"kind": "begin", "title": "Indexing", "percentage": 10}});
        assert_eq!(
            lsp.on_progress(&begin("a")),
            Some(ServerState::Indexing { message: "Indexing".into(), percent: Some(10) })
        );
        lsp.on_progress(&begin("b"));
        assert!(matches!(
            lsp.on_progress(&json!({"token": "b", "value": {"kind": "end"}})),
            Some(ServerState::Indexing { .. })
        ));
        assert_eq!(
            lsp.on_progress(&json!({"token": "a", "value": {"kind": "end"}})),
            Some(ServerState::Ready)
        );
    }
}
