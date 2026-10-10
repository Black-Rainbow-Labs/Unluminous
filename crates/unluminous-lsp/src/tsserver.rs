//! TypeScript's own server protocol, for tsserver.
//!
//! Requests are one JSON object a line (`seq`, `type: "request"`, `command`, `arguments`); answers and
//! events come back `Content-Length` framed. Lines are 1 based and columns are 1 based UTF-16 code
//! units, converted here and nowhere else. `--disableAutomaticTypingAcquisition` is passed by
//! `find_program`, so the server never reaches for the network.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::convert::{byte_of_utf16, line_and_utf16};
use crate::framing::encode_line;
use crate::lsp::reduce_snippet;
use crate::worker::{Incoming, Protocol, Request, What};
use crate::{Insertion, Item, Kind, Reply, ServerState, SignatureHelp, TextEdit, Trigger};

/// A request waiting for its response.
enum Pending {
    Configure,
    Complete { ticket: u64, path: PathBuf, revision: u64, text: Arc<str>, offset: usize },
    Resolve { ticket: u64, path: PathBuf, item: Box<Item>, text: Arc<str> },
    Signature { ticket: u64, path: PathBuf, revision: u64 },
}

/// The client half of one tsserver session.
pub(crate) struct TsServer {
    seq: i64,
    root: PathBuf,
    pending: HashMap<i64, Pending>,
}

impl TsServer {
    pub(crate) fn new() -> TsServer {
        TsServer { seq: 0, root: PathBuf::new(), pending: HashMap::new() }
    }

    /// A request message. Its answer is read when `pending` is given.
    fn command(&mut self, command: &str, arguments: Value, pending: Option<Pending>) -> Value {
        self.seq += 1;
        if let Some(pending) = pending {
            self.pending.insert(self.seq, pending);
        }
        json!({"seq": self.seq, "type": "request", "command": command, "arguments": arguments})
    }

    /// An answer to a request, as replies.
    fn on_response(&mut self, message: &Value) -> Incoming {
        let Some(pending) = message
            .get("request_seq")
            .and_then(Value::as_i64)
            .and_then(|id| self.pending.remove(&id))
        else {
            return Incoming::default();
        };
        let success = message.get("success").and_then(Value::as_bool).unwrap_or(false);
        let body = message.get("body").unwrap_or(&Value::Null);
        match pending {
            Pending::Configure => Incoming {
                state: Some(ServerState::Ready),
                initialized: true,
                ..Incoming::default()
            },
            _ if !success => Incoming::default(),
            Pending::Complete { ticket, path, revision, text, offset } => {
                let (incomplete, items) = map_completions(body, &text, offset);
                Incoming {
                    replies: vec![Reply::Completions { ticket, path, revision, incomplete, items }],
                    state: Some(ServerState::Ready),
                    ..Incoming::default()
                }
            }
            Pending::Resolve { ticket, path, item, text } => Incoming {
                replies: vec![Reply::Resolved {
                    ticket,
                    item: resolved_item(*item, body, &path, &text),
                }],
                ..Incoming::default()
            },
            Pending::Signature { ticket, path, revision } => Incoming {
                replies: vec![Reply::Signature {
                    ticket,
                    path,
                    revision,
                    help: map_signature(body),
                }],
                ..Incoming::default()
            },
        }
    }

    /// The project loading events say whether the server is still reading the project.
    fn on_event(&self, message: &Value) -> Incoming {
        let state = match message.get("event").and_then(Value::as_str) {
            Some("projectLoadingStart") => {
                Some(ServerState::Indexing { message: "loading project".to_owned(), percent: None })
            }
            Some("projectLoadingFinish") => Some(ServerState::Ready),
            _ => None,
        };
        Incoming { state, ..Incoming::default() }
    }
}

/// A path as tsserver names a file: forward slashes.
fn file_name(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    text.strip_prefix("//?/").map_or(text.clone(), str::to_owned)
}

/// A byte offset as tsserver's 1 based line and 1 based UTF-16 offset.
fn location(text: &str, offset: usize) -> (u32, u32) {
    let (line, column) = line_and_utf16(text, offset);
    (line + 1, column + 1)
}

/// tsserver's `{line, offset}` as a byte offset of the text.
fn byte_of(text: &str, place: &Value) -> Option<usize> {
    let line = place.get("line")?.as_u64()?.saturating_sub(1) as u32;
    let offset = place.get("offset")?.as_u64()?.saturating_sub(1) as u32;
    Some(byte_of_utf16(text, line, offset))
}

/// A `{start, end}` span as bytes.
fn span_of(text: &str, span: &Value) -> Option<Range<usize>> {
    let start = byte_of(text, span.get("start")?)?;
    let end = byte_of(text, span.get("end")?)?;
    Some(start..end.max(start))
}

fn script_kind(language: &str) -> &'static str {
    match language {
        "typescriptreact" => "TSX",
        "javascript" => "JS",
        "javascriptreact" => "JSX",
        _ => "TS",
    }
}

impl Protocol for TsServer {
    fn encode(&self, message: &Value) -> Vec<u8> {
        encode_line(message)
    }

    fn initialize(&mut self, root: &Path) -> Vec<Value> {
        self.root = root.to_path_buf();
        let preferences = json!({
            "includeCompletionsForModuleExports": true,
            "includeCompletionsWithInsertText": true,
            "includeCompletionsWithSnippetText": true,
            "includeCompletionsForImportStatements": true,
            "allowIncompleteCompletions": true,
            "useLabelDetailsInCompletionEntries": true,
        });
        vec![self.command(
            "configure",
            json!({"hostInfo": "Unluminous", "preferences": preferences}),
            Some(Pending::Configure),
        )]
    }

    fn open(&mut self, path: &Path, language: &str, _revision: u64, text: &str) -> Vec<Value> {
        let arguments = json!({"file": file_name(path), "fileContent": text, "projectRootPath": file_name(&self.root), "scriptKindName": script_kind(language)});
        vec![self.command("open", arguments, None)]
    }

    fn change(
        &mut self,
        path: &Path,
        _revision: u64,
        old: &str,
        range: &Range<usize>,
        replacement: &str,
    ) -> Vec<Value> {
        let (line, offset) = location(old, range.start);
        let (end_line, end_offset) = location(old, range.end);
        let arguments = json!({"file": file_name(path), "line": line, "offset": offset, "endLine": end_line, "endOffset": end_offset, "insertString": replacement});
        vec![self.command("change", arguments, None)]
    }

    fn close(&mut self, path: &Path) -> Vec<Value> {
        vec![self.command("close", json!({"file": file_name(path)}), None)]
    }

    fn request(&mut self, request: Request) -> Vec<Value> {
        let Request { ticket, path, revision, text, what } = request;
        let file = file_name(&path);
        let message = match what {
            What::Complete { offset, trigger } => {
                let (line, column) = location(&text, offset);
                let mut arguments = json!({"file": file, "line": line, "offset": column, "includeExternalModuleExports": true, "includeInsertTextCompletions": true});
                add_trigger(&mut arguments, trigger);
                self.command(
                    "completionInfo",
                    arguments,
                    Some(Pending::Complete { ticket, path, revision, text, offset }),
                )
            }
            What::Resolve { item, offset } => {
                let (line, column) = location(&text, offset);
                let arguments = json!({"file": file, "line": line, "offset": column, "entryNames": [entry_name(&item)]});
                self.command(
                    "completionEntryDetails",
                    arguments,
                    Some(Pending::Resolve { ticket, path, item, text }),
                )
            }
            What::Signature { offset } => {
                let (line, column) = location(&text, offset);
                self.command(
                    "signatureHelp",
                    json!({"file": file, "line": line, "offset": column}),
                    Some(Pending::Signature { ticket, path, revision }),
                )
            }
        };
        vec![message]
    }

    fn cancel(&mut self, _ticket: u64) -> Vec<Value> {
        Vec::new()
    }

    fn incoming(&mut self, message: Value) -> Incoming {
        match message.get("type").and_then(Value::as_str) {
            Some("response") => self.on_response(&message),
            Some("event") => self.on_event(&message),
            _ => Incoming::default(),
        }
    }

    fn shutdown(&mut self) -> Vec<Value> {
        vec![self.command("exit", json!({}), None)]
    }
}

/// tsserver's `triggerKind` and `triggerCharacter`; it only knows seven characters.
fn add_trigger(arguments: &mut Value, trigger: Trigger) {
    let (kind, character) = match trigger {
        Trigger::Invoked => (1, None),
        Trigger::Character(c)
            if matches!(c, '.' | '"' | '\'' | '`' | '/' | '@' | '<' | '#' | ' ') =>
        {
            (2, Some(c))
        }
        Trigger::Character(_) => (1, None),
        Trigger::Continued => (3, None),
    };
    arguments["triggerKind"] = json!(kind);
    if let Some(c) = character {
        arguments["triggerCharacter"] = json!(c.to_string());
    }
}

/// The `entryNames` element for a row: its name, and its source and data when it has them.
fn entry_name(item: &Item) -> Value {
    let entry: Value = serde_json::from_str(&item.handle).unwrap_or(Value::Null);
    let mut name = Map::new();
    name.insert("name".to_owned(), entry.get("name").cloned().unwrap_or_else(|| json!(item.label)));
    for key in ["source", "data"] {
        if let Some(value) = entry.get(key).filter(|v| !v.is_null()) {
            name.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(name)
}

/// `completionInfo`'s body (an object, or in old servers a bare list) as rows and whether it is incomplete.
fn map_completions(body: &Value, text: &str, offset: usize) -> (bool, Vec<Item>) {
    let entries = match body {
        Value::Array(entries) => entries.as_slice(),
        _ => body.get("entries").and_then(Value::as_array).map_or(&[][..], Vec::as_slice),
    };
    let default_span = body.get("optionalReplacementSpan").and_then(|s| span_of(text, s));
    let incomplete = body.get("isIncomplete").and_then(Value::as_bool).unwrap_or(false);
    (incomplete, entries.iter().map(|e| map_entry(e, default_span.clone(), text, offset)).collect())
}

/// One tsserver completion entry as a row.
fn map_entry(e: &Value, default_span: Option<Range<usize>>, text: &str, offset: usize) -> Item {
    let name = e.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
    let kind = e.get("kind").and_then(Value::as_str).and_then(kind_of);
    let insertion = insertion_of(e, &name, default_span, text, offset);
    let recommended = e.get("isRecommended").and_then(Value::as_bool).unwrap_or(false);
    let modifiers = e.get("kindModifiers").and_then(Value::as_str).unwrap_or("");
    Item {
        filter: e
            .get("filterText")
            .and_then(Value::as_str)
            .map_or_else(|| name.clone(), str::to_owned),
        order: order_of(e.get("sortText").and_then(Value::as_str)),
        callable: matches!(kind, Some(Kind::Function | Kind::Method))
            && insertion.as_ref().is_some_and(|i| i.text.contains('(') && i.text.ends_with(')')),
        kind,
        detail: detail_of(e),
        doc: None,
        deprecated: modifiers.split(',').any(|m| m.trim() == "deprecated"),
        preselect: recommended,
        expected_type: recommended,
        insertion,
        extra_edits: Vec::new(),
        needs_resolve: true,
        // An entry from a module the file does not import yet carries the module and an action.
        import: e
            .get("source")
            .and_then(Value::as_str)
            .filter(|_| e.get("hasAction").and_then(Value::as_bool).unwrap_or(false))
            .map(str::to_owned),
        handle: e.to_string(),
        label: name,
    }
}

/// The edit for an entry: its `insertText` (reduced when it is a snippet) over its `replacementSpan`, the
/// list's span, or the word before the caret.
fn insertion_of(
    e: &Value,
    name: &str,
    default_span: Option<Range<usize>>,
    text: &str,
    offset: usize,
) -> Option<Insertion> {
    let given = e.get("insertText").and_then(Value::as_str);
    let span = e.get("replacementSpan").and_then(|s| span_of(text, s));
    if given.is_none() && span.is_none() {
        return None;
    }
    let replace = span.or(default_span).unwrap_or_else(|| word_around(text, offset));
    let end = offset.clamp(replace.start, replace.end);
    let new_text = given.unwrap_or(name);
    let snippet = e.get("isSnippet").and_then(Value::as_bool).unwrap_or(false);
    let (text, caret) =
        if snippet { reduce_snippet(new_text) } else { (new_text.to_owned(), None) };
    Some(Insertion { insert: replace.start..end, replace, text, caret })
}

/// The identifier the caret is in: from its start to its end.
fn word_around(text: &str, offset: usize) -> Range<usize> {
    let at = offset.min(text.len());
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let start = text[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(at, |(i, _)| i);
    let end =
        text[at..].char_indices().find(|(_, c)| !is_word(*c)).map_or(text.len(), |(i, _)| at + i);
    start..end.max(at)
}

fn detail_of(e: &Value) -> Option<String> {
    let details = e.get("labelDetails");
    let part = |key: &str| details.and_then(|d| d.get(key)).and_then(Value::as_str).unwrap_or("");
    let joined = format!("{} {}", part("detail"), part("description")).trim().to_owned();
    if !joined.is_empty() {
        return Some(joined);
    }
    let source = e
        .get("sourceDisplay")
        .map(parts_text)
        .filter(|s| !s.is_empty())
        .or_else(|| e.get("source").and_then(Value::as_str).map(str::to_owned));
    source.filter(|s| !s.is_empty())
}

/// `sortText` as a number: its leading decimal digits. `11` is the locals group, `16` auto imports.
///
/// @param sort - tsserver's `sortText`
pub(crate) fn order_of(sort: Option<&str>) -> u64 {
    let digits: String =
        sort.unwrap_or("").chars().take_while(char::is_ascii_digit).take(18).collect();
    digits.parse().unwrap_or(u64::MAX)
}

/// A tsserver `kind` string as a row kind.
fn kind_of(kind: &str) -> Option<Kind> {
    Some(match kind {
        "method" | "constructor" => Kind::Method,
        "property" | "getter" | "setter" | "accessor" => Kind::Field,
        "function" | "local function" | "call" | "construct" | "index" => Kind::Function,
        "class" | "local class" => Kind::Class,
        "interface" => Kind::Interface,
        "enum" => Kind::Enum,
        "enum member" => Kind::Variant,
        "var" | "let" | "local var" | "using" | "alias" => Kind::Variable,
        "const" | "local const" | "string" => Kind::Constant,
        "parameter" => Kind::Parameter,
        "module" | "script" | "external module name" | "directory" | "file" => Kind::Module,
        "type" => Kind::TypeAlias,
        "type parameter" | "primitive type" => Kind::Type,
        "keyword" => Kind::Keyword,
        _ => return None,
    })
}

/// The text of tsserver's display parts, or of a plain string.
fn parts_text(parts: &Value) -> String {
    match parts {
        Value::String(s) => s.clone(),
        Value::Array(list) => {
            list.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect()
        }
        _ => String::new(),
    }
}

/// The row after `completionEntryDetails`: its signature, documentation and the import it needs.
fn resolved_item(mut item: Item, body: &Value, path: &Path, text: &str) -> Item {
    let Some(details) = body.as_array().and_then(|list| list.first()) else {
        item.needs_resolve = false;
        return item;
    };
    let signature = details.get("displayParts").map(parts_text).filter(|s| !s.is_empty());
    item.detail = signature.or(item.detail);
    item.doc = details.get("documentation").map(parts_text).filter(|s| !s.is_empty()).or(item.doc);
    item.extra_edits = import_edits(details, path, text);
    item.needs_resolve = false;
    item
}

/// The text changes of a row's code actions that are in this file.
fn import_edits(details: &Value, path: &Path, text: &str) -> Vec<TextEdit> {
    let wanted = file_name(path).to_lowercase();
    let actions =
        details.get("codeActions").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    let changes =
        actions.iter().filter_map(|a| a.get("changes").and_then(Value::as_array)).flatten();
    changes
        .filter(|c| {
            c.get("fileName")
                .and_then(Value::as_str)
                .is_some_and(|f| f.replace('\\', "/").to_lowercase() == wanted)
        })
        .filter_map(|c| c.get("textChanges").and_then(Value::as_array))
        .flatten()
        .filter_map(|t| {
            Some(TextEdit {
                range: span_of(text, t)?,
                text: t.get("newText")?.as_str()?.to_owned(),
            })
        })
        .collect()
}

/// `signatureHelp`'s body as the active signature on one line, with each parameter's bytes in it.
fn map_signature(body: &Value) -> Option<SignatureHelp> {
    let items = body.get("items")?.as_array()?;
    let chosen = body.get("selectedItemIndex").and_then(Value::as_u64).unwrap_or(0) as usize;
    let item = items.get(chosen).or_else(|| items.first())?;
    let separator =
        item.get("separatorDisplayParts").map(parts_text).unwrap_or_else(|| ", ".to_owned());
    let mut label = item.get("prefixDisplayParts").map(parts_text).unwrap_or_default();
    let mut parameters = Vec::new();
    for (index, parameter) in item
        .get("parameters")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .enumerate()
    {
        if index > 0 {
            label.push_str(&separator);
        }
        let start = label.len();
        label.push_str(&parameter.get("displayParts").map(parts_text).unwrap_or_default());
        parameters.push(start..label.len());
    }
    label.push_str(&item.get("suffixDisplayParts").map(parts_text).unwrap_or_default());
    let active = body
        .get("argumentIndex")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .filter(|n| *n < parameters.len());
    let doc = item.get("documentation").map(parts_text).filter(|s| !s.is_empty());
    Some(SignatureHelp { label, parameters, active, doc })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locations_count_utf16_from_one() {
        let text = "\u{1F600} x.";
        assert_eq!(location(text, text.len()), (1, 6));
        assert_eq!(byte_of(text, &json!({"line": 1, "offset": 6})), Some(text.len()));
        assert_eq!(location("a\nb", 2), (2, 1));
    }

    #[test]
    fn a_change_is_sent_in_utf16_of_the_old_text() {
        let mut ts = TsServer::new();
        let old = "\u{1F600}ab\nc";
        let sent = ts.change(Path::new("C:\\p\\a.ts"), 2, old, &(5..6), "XY");
        let arguments = &sent[0]["arguments"];
        assert_eq!(sent[0]["command"], "change");
        assert_eq!(arguments["file"], "C:/p/a.ts");
        assert_eq!(
            (&arguments["line"], &arguments["offset"], &arguments["endOffset"]),
            (&json!(1), &json!(4), &json!(5))
        );
        assert_eq!(arguments["insertString"], "XY");
    }

    #[test]
    fn sort_text_groups_are_numbers() {
        assert_eq!(order_of(Some("11")), 11);
        assert_eq!(order_of(Some("16")), 16);
        assert_eq!(order_of(Some("15z")), 15);
        assert_eq!(order_of(None), u64::MAX);
    }

    #[test]
    fn an_entry_becomes_a_row() {
        let text = "const c = ma";
        let entry = json!({"name": "makeCard", "kind": "function", "kindModifiers": "export,deprecated", "sortText": "16", "hasAction": true, "source": "./a",
            "isRecommended": true, "replacementSpan": {"start": {"line": 1, "offset": 11}, "end": {"line": 1, "offset": 13}}});
        let (incomplete, items) =
            map_completions(&json!({"entries": [entry], "isIncomplete": true}), text, 12);
        assert!(incomplete);
        let item = &items[0];
        assert_eq!(
            (item.label.as_str(), item.order, item.kind),
            ("makeCard", 16, Some(Kind::Function))
        );
        assert!(item.deprecated && item.expected_type && item.needs_resolve);
        assert_eq!(item.detail.as_deref(), Some("./a"));
        let insertion = item.insertion.as_ref().unwrap();
        assert_eq!(
            (insertion.insert.clone(), insertion.replace.clone(), insertion.text.as_str()),
            (10..12, 10..12, "makeCard")
        );
    }

    #[test]
    fn an_insert_text_without_a_span_replaces_the_word_at_the_caret() {
        let text = "x.fo bar";
        let entry = json!({"name": "foo", "kind": "method", "sortText": "11", "insertText": "foo()", "isSnippet": false});
        let (_, items) = map_completions(&json!({"entries": [entry]}), text, 4);
        let insertion = items[0].insertion.as_ref().unwrap();
        assert_eq!((insertion.insert.clone(), insertion.replace.clone()), (2..4, 2..4));
        assert!(items[0].callable);
    }

    #[test]
    fn details_give_the_import_edit_for_this_file_only() {
        let text = "makeCard(";
        let item = map_entry(&json!({"name": "makeCard", "kind": "function"}), None, text, 0);
        let body = json!([{"displayParts": [{"text": "function"}, {"text": " "}, {"text": "makeCard"}], "documentation": [{"text": "Makes."}],
            "codeActions": [{"description": "Add import", "changes": [
                {"fileName": "c:/p/b.ts", "textChanges": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}, "newText": "import { makeCard } from \"./a\";\n"}]},
                {"fileName": "c:/p/other.ts", "textChanges": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}, "newText": "no"}]}]}]}]);
        let resolved = resolved_item(item, &body, Path::new("C:\\p\\b.ts"), text);
        assert_eq!(resolved.doc.as_deref(), Some("Makes."));
        assert_eq!(resolved.detail.as_deref(), Some("function makeCard"));
        assert_eq!(
            resolved.extra_edits,
            vec![TextEdit { range: 0..0, text: "import { makeCard } from \"./a\";\n".to_owned() }]
        );
        assert!(!resolved.needs_resolve);
    }

    #[test]
    fn signature_parameters_are_bytes_in_the_label() {
        let body = json!({"selectedItemIndex": 0, "argumentIndex": 1, "items": [{
            "prefixDisplayParts": [{"text": "makeCard("}], "suffixDisplayParts": [{"text": "): Card"}], "separatorDisplayParts": [{"text": ", "}],
            "parameters": [{"displayParts": [{"text": "title: string"}]}, {"displayParts": [{"text": "count: number"}]}]}]});
        let help = map_signature(&body).unwrap();
        assert_eq!(help.label, "makeCard(title: string, count: number): Card");
        assert_eq!(help.parameters, vec![9..22, 24..37]);
        assert_eq!(help.active, Some(1));
    }

    #[test]
    fn progress_follows_project_loading() {
        let ts = TsServer::new();
        assert!(matches!(
            ts.on_event(&json!({"type": "event", "event": "projectLoadingStart"})).state,
            Some(ServerState::Indexing { .. })
        ));
        assert_eq!(
            ts.on_event(&json!({"type": "event", "event": "projectLoadingFinish"})).state,
            Some(ServerState::Ready)
        );
        assert_eq!(ts.on_event(&json!({"type": "event", "event": "other"})).state, None);
    }
}
