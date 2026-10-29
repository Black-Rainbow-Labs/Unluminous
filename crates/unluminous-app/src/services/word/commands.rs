//! `plugins run word <command>`: the agent's half of every control, through the same session methods.
//!
//! The verbs are `unluminous_cli::plugins::WORD`, which is also what the MCP tool is generated from. Each
//! answers a sentence for the status bar and the same answer as data.

use std::path::{Path, PathBuf};

use serde_json::json;
use unluminous_word::builtin::ListKind;
use unluminous_word::edit::Unit;
use unluminous_word::model::{COLUMN_BREAK, LINE_BREAK, PAGE_BREAK};
use unluminous_word::position::{self, Pos, Selection, StoryId};
use unluminous_word::props;

use super::{actions, selection_value, Doc, Level, Word};
use crate::services::plugin_ui::{Answer, Request};

fn rest(arguments: &[String], from: usize) -> String {
    arguments.get(from..).map(|words| words.join(" ")).unwrap_or_default()
}

/// Text an agent typed, with `\n` and `\t` written as two characters turned into what they mean.
fn unescape(text: &str) -> String {
    text.replace("\\n", "\n").replace("\\t", "\t")
}

fn flag(arguments: &[String], name: &str) -> bool {
    arguments.iter().any(|a| a == name)
}

/// The arguments that are not flags. A bare `--` is kept, because it is what separates the two halves of
/// `replace <find> -- <with>`; it used to be dropped with the flags, so no replace ever found it.
fn words(arguments: &[String]) -> Vec<String> {
    arguments.iter().filter(|a| *a == "--" || !a.starts_with("--")).cloned().collect()
}

fn points(text: Option<&String>) -> Result<f32, String> {
    let text = text.ok_or("a number of points is needed")?;
    text.trim_end_matches("pt")
        .parse::<f32>()
        .map_err(|_| format!("{text} is not a number of points"))
}

fn on(text: Option<&String>) -> Option<bool> {
    match text.map(String::as_str) {
        Some("off" | "false" | "0" | "no") => Some(false),
        Some("on" | "true" | "1" | "yes") => Some(true),
        _ => None,
    }
}

/// The story `--story <kind> [n]` names, and the words after it: `body`, `header <n>`, `footer <n>`
/// (the nth header or footer part), `footnote <id>`, `endnote <id>`, `comment <id>` or `textbox <n>`.
fn story_from(
    document: &unluminous_word::model::Document,
    arguments: &[String],
) -> Result<(StoryId, Vec<String>), String> {
    let at = arguments.iter().position(|a| a == "--story").ok_or("--story names a story")?;
    let kind = arguments
        .get(at + 1)
        .ok_or("--story takes body, header, footer, footnote, endnote, comment or textbox")?;
    let number = arguments.get(at + 2).and_then(|n| n.parse::<i64>().ok());
    let used = if number.is_some() { 3 } else { 2 };
    let rest: Vec<String> = arguments
        .iter()
        .enumerate()
        .filter(|(i, a)| (*i < at || *i >= at + used) && a.as_str() != "--runs")
        .map(|(_, a)| a.clone())
        .collect();
    let n = number.unwrap_or(1);
    let story = match kind.as_str() {
        "body" => StoryId::Body,
        "header" | "footer" => {
            let footer = kind == "footer";
            let header = document.headers.iter().filter(|h| h.footer == footer).nth((n - 1).max(0) as usize).ok_or_else(|| format!("there is no {kind} {n}"))?;
            StoryId::Header(header.id.clone())
        }
        "footnote" => StoryId::Footnote(n),
        "endnote" => StoryId::Endnote(n),
        "comment" => StoryId::Comment(n),
        "textbox" => StoryId::TextBox(document.textboxes.get((n - 1).max(0) as usize).ok_or_else(|| format!("there is no text box {n}"))?.id),
        other => return Err(format!("--story takes body, header, footer, footnote, endnote, comment or textbox, not {other}")),
    };
    Ok((story, rest))
}

/// The body paragraph numbered `n` from 1, as `read` numbers them.
fn paragraph_path(doc: &Doc, n: usize) -> Result<Vec<u32>, String> {
    let all = position::paragraphs(&doc.session.document.body);
    let count = all.len();
    all.into_iter()
        .nth(n.saturating_sub(1))
        .map(|(path, _)| path)
        .ok_or_else(|| format!("there is no paragraph {n}: the document has {count}"))
}

/// A search from a verb's words: the text, `--case`, `--word`, `--wildcards`, `--all-stories` and
/// `--format=bold,italic,underline,highlight,color:RRGGBB,style:<id>,align:center`.
fn search_from(arguments: &[String]) -> Result<unluminous_word::search::Search, String> {
    Ok(unluminous_word::search::Search {
        query: unluminous_word::find::Query {
            text: rest(&words(arguments), 0),
            match_case: flag(arguments, "--case"),
            whole_word: flag(arguments, "--word"),
            wildcards: flag(arguments, "--wildcards"),
        },
        format: format_from(arguments, "--format=")?,
        every_story: flag(arguments, "--all-stories"),
    })
}

/// The formatting a `--format=` (or `--with-format=`) word names: comma separated, each `bold`,
/// `italic`, `underline`, `highlight`, `not-bold` and so on, `color:RRGGBB`, `style:<id>`, `align:<a>`.
fn format_from(
    arguments: &[String],
    prefix: &str,
) -> Result<unluminous_word::search::Format, String> {
    let mut format = unluminous_word::search::Format::default();
    let Some(list) = arguments.iter().find_map(|a| a.strip_prefix(prefix)) else {
        return Ok(format);
    };
    for item in list.split(',').map(str::trim).filter(|i| !i.is_empty()) {
        let (not, name) = match item.strip_prefix("not-") {
            Some(name) => (true, name),
            None => (false, item),
        };
        match name.split_once(':') {
            Some(("color" | "colour", hex)) => {
                format.colour =
                    Some(props::Rgb::parse(hex).ok_or_else(|| format!("{hex} is not a colour"))?)
            }
            Some(("style", id)) => format.style = Some(id.to_owned()),
            Some(("align", a)) => {
                format.align = Some(match a {
                    "center" | "centre" => props::Align::Center,
                    "right" => props::Align::Right,
                    "justify" => props::Align::Justify,
                    _ => props::Align::Left,
                })
            }
            _ => match name {
                "bold" => format.bold = Some(!not),
                "italic" => format.italic = Some(!not),
                "underline" => format.underline = Some(!not),
                "highlight" => format.highlight = Some(!not),
                other => return Err(format!("{other} is not a formatting the search knows")),
            },
        }
    }
    Ok(format)
}

/// `fields [update|unlink|lock|unlock|list]`: Update Fields, and the field commands on the selection.
fn fields(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    match arguments.get(1).map_or("update", String::as_str) {
        "update" => {
            doc.ensure_pages();
            let pages = doc.pages.clone();
            let count = doc.session.update_fields(&pages);
            Ok(Answer::said(format!("{count} fields updated")))
        }
        "unlink" => {
            let count = doc.session.unlink_fields();
            Ok(Answer::said(format!("{count} fields replaced with their results")))
        }
        "lock" | "unlock" => {
            let lock = arguments[1] == "lock";
            let count = doc.session.lock_fields(lock);
            Ok(Answer::said(format!("{count} fields {}", if lock { "locked" } else { "unlocked" })))
        }
        "list" => {
            let list = unluminous_word::references::field_list(&doc.session.document);
            let said = list
                .iter()
                .map(|(n, code, result)| format!("{n}: {code} = {result}"))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(Answer::said(said).with(json!(list
                .iter()
                .map(|(n, code, result)| json!({ "paragraph": n, "code": code, "result": result }))
                .collect::<Vec<_>>())))
        }
        other => Err(format!("fields takes update, unlink, lock, unlock or list, not {other}")),
    }
}

/// `toc insert [levels] [--figures <label>] [--no-pages] [--no-links] [--leader dot|hyphen|underscore|none]`,
/// `toc update` and `toc remove`.
fn toc(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    doc.ensure_pages();
    let pages = doc.pages.clone();
    // `arguments` starts after the verb: `toc insert 2` is `["insert", "2"]`.
    match arguments.first().map_or("insert", String::as_str) {
        "insert" => {
            let value = |name: &str| {
                arguments.iter().position(|a| a == name).and_then(|i| arguments.get(i + 1)).cloned()
            };
            let options = unluminous_word::references::TocOptions {
                levels: arguments.get(1).and_then(|n| n.parse().ok()).unwrap_or(3),
                page_numbers: !flag(arguments, "--no-pages"),
                hyperlinks: !flag(arguments, "--no-links"),
                leader: value("--leader").unwrap_or_else(|| "dot".into()),
                caption: value("--figures"),
                ..Default::default()
            };
            doc.session.insert_toc_with(&options, &pages);
            Ok(Answer::said(if options.caption.is_some() {
                "Table of figures added"
            } else {
                "Table of contents added"
            }))
        }
        "update" => {
            let count = doc.session.update_tocs(&pages);
            Ok(Answer::said(format!("{count} tables updated")))
        }
        "remove" => {
            if doc.session.remove_toc() {
                Ok(Answer::said("Table of contents removed"))
            } else {
                Err("there is no table of contents".into())
            }
        }
        other => Err(format!("toc takes insert, update or remove, not {other}")),
    }
}

/// `link show`, `link edit <address> [text]`, `link tip <tooltip>` and `link remove`, for the
/// hyperlink at the caret.
fn link(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    let at = doc.session.selection.head.clone();
    match arguments.get(1).map_or("show", String::as_str) {
        "show" => {
            let (address, tip) =
                doc.session.hyperlink_at(&at).ok_or("the caret is not in a link")?;
            Ok(Answer::said(address.clone()).with(json!({ "address": address, "tooltip": tip })))
        }
        "edit" => {
            let address = arguments.get(2).ok_or("link edit takes an address")?;
            let text = (arguments.len() > 3).then(|| rest(arguments, 3));
            let tip = doc.session.hyperlink_at(&at).and_then(|(_, t)| t);
            doc.session.edit_hyperlink(address, text.as_deref(), tip.as_deref())?;
            Ok(Answer::said("Link changed"))
        }
        "tip" | "tooltip" => {
            let (address, _) = doc.session.hyperlink_at(&at).ok_or("the caret is not in a link")?;
            doc.session.edit_hyperlink(&address, None, Some(&rest(arguments, 2)))?;
            Ok(Answer::said("Link tip changed"))
        }
        "remove" => {
            doc.session.remove_hyperlink()?;
            Ok(Answer::said("Link removed"))
        }
        other => Err(format!("link takes show, open, edit, tip or remove, not {other}")),
    }
}

/// Run one command.
pub fn run(word: &mut Word, command: &str, arguments: &[String]) -> Result<Answer, String> {
    match command {
        "load" => {
            let key = arguments.first().ok_or("load takes a tab key and a path")?;
            let path = PathBuf::from(rest(arguments, 1));
            word.load(key, &path)?;
            return Ok(Answer::said(format!("Opened {}", path.display())));
        }
        "new" => {
            // new [path] [--template <file>]
            let template = arguments
                .iter()
                .position(|a| a == "--template")
                .and_then(|i| arguments.get(i + 1))
                .map(PathBuf::from);
            let path = arguments
                .iter()
                .enumerate()
                .find(|(i, a)| {
                    !a.starts_with("--") && (*i == 0 || arguments[i - 1] != "--template")
                })
                .map(|(_, a)| PathBuf::from(a));
            let key = word.new_from(path.as_deref(), template.as_deref())?;
            let label = word.doc(&key)?.label();
            word.outbox.push(Request::OpenKeyedTab { key: key.clone(), label: label.clone() });
            return Ok(Answer::said(format!("New document {label}")).with(json!({ "key": key })));
        }
        // The Word menu's Open as Document: a file dialog, then the file opened as a document.
        "open-as-document" if arguments.is_empty() => {
            let chosen = rfd::FileDialog::new()
                .set_title("Open as Document")
                .set_directory(word.project.clone().unwrap_or_default())
                .add_filter(
                    "Documents",
                    &[
                        "docx", "dotx", "docm", "dotm", "doc", "rtf", "odt", "wps", "pdf", "txt",
                        "md", "html", "htm",
                    ],
                )
                .pick_file()
                .ok_or("Nothing was opened")?;
            let words = vec!["--as-document".to_owned(), chosen.display().to_string()];
            return run(word, "open", &words);
        }
        "open-as-document" => {
            let mut words = vec!["--as-document".to_owned()];
            words.extend(arguments.iter().cloned());
            return run(word, "open", &words);
        }
        "open" => {
            // open [--as-document] <path>: a .txt, .md, .html or .pdf opens as a Word document with the flag.
            let as_document = flag(arguments, "--as-document");
            let words: Vec<String> =
                arguments.iter().filter(|a| *a != "--as-document").cloned().collect();
            let path = PathBuf::from(rest(&words, 0));
            let path = match (path.is_relative(), &word.project) {
                (true, Some(project)) => project.join(path),
                _ => path,
            };
            if !path.is_file() {
                return Err(format!("{} is not a file", path.display()));
            }
            if as_document {
                let key = super::tab_key(&path);
                word.load(&key, &path)?;
                let label = super::files::name_of(&path);
                word.outbox.push(Request::OpenKeyedTab { key: key.clone(), label });
                return Ok(Answer::said(format!("Opening {} as a document", path.display()))
                    .with(json!({ "key": key })));
            }
            word.outbox.push(Request::OpenFile(path.clone()));
            return Ok(Answer::said(format!("Opening {}", path.display()))
                .with(json!({ "key": super::tab_key(&path) })));
        }
        "docs" => {
            let mut docs: Vec<serde_json::Value> = word
                .docs
                .iter()
                .map(|d| json!({ "key": d.key, "path": d.path, "convertedFrom": d.converted_from, "encrypted": d.password.is_some(), "modified": d.modified(), "pages": d.pages.pages.len(), "current": word.current.as_deref() == Some(d.key.as_str()) }))
                .collect();
            // A tab still waiting for Word, or for a password, is listed with what it is waiting for.
            docs.extend(
                word.waiting
                    .iter()
                    .map(|(key, waiting)| json!({ "key": key, "waiting": waiting.sentence() })),
            );
            return Ok(Answer::said(format!("{} documents open", docs.len())).with(json!(docs)));
        }
        "use" => {
            let wanted = rest(arguments, 0);
            let found = word
                .docs
                .iter()
                .find(|d| {
                    d.key == wanted
                        || d.key == format!("word:{wanted}")
                        || d.path.as_ref().is_some_and(|p| p.ends_with(&wanted))
                })
                .map(|d| d.key.clone());
            let key = found.ok_or_else(|| format!("no open document is {wanted}"))?;
            word.current = Some(key.clone());
            return Ok(Answer::said(format!("Using {key}")));
        }
        "close" => {
            let key = word.current_doc()?.key.clone();
            word.outbox.push(Request::CloseKeyedTab { key: key.clone() });
            return Ok(Answer::said(format!("Closing {key}")));
        }
        "save" => {
            let to = arguments.first().map(PathBuf::from);
            let key = word.current_doc()?.key.clone();
            let saved = word.doc(&key)?.save(to.as_deref())?;
            if to.is_some() {
                let label = word.doc(&key)?.label();
                word.outbox.push(Request::RenameTab { key, label });
            }
            return Ok(
                Answer::said(format!("Saved {}", saved.display())).with(json!({ "path": saved }))
            );
        }
        "autocorrect" => return autocorrect(word, arguments),
        "password" => {
            let password = rest(arguments, 0);
            let key = word
                .current
                .clone()
                .filter(|k| word.waiting.iter().any(|(w, _)| w == k))
                .or_else(|| word.waiting.first().map(|(k, _)| k.clone()))
                .ok_or("no document is waiting for a password")?;
            word.give_password(&key, &password)?;
            return Ok(Answer::said(format!("Opened {}", word.doc(&key)?.label())));
        }
        "autosave" | "autorecover" => {
            let value = on(arguments.first());
            let slot = if command == "autosave" {
                &mut word.config.autosave
            } else {
                &mut word.config.autorecover
            };
            if let Some(value) = value {
                *slot = value;
                if let Some(folder) = &word.folder {
                    word.config.write(folder)?;
                }
            }
            let now =
                if command == "autosave" { word.config.autosave } else { word.config.autorecover };
            let name = if command == "autosave" { "AutoSave" } else { "AutoRecover" };
            return Ok(Answer::said(format!("{name} is {}", if now { "on" } else { "off" }))
                .with(json!({ "on": now })));
        }
        "link" if arguments.first().map(String::as_str) == Some("open") => {
            let doc = word.current_doc()?;
            let at = doc.session.selection.head.clone();
            let (address, _) = doc.session.hyperlink_at(&at).ok_or("the caret is not in a link")?;
            let mut requests = Vec::new();
            crate::components::word::page_view::follow_link(doc, &address, &mut requests);
            word.outbox.extend(requests);
            return Ok(
                Answer::said(format!("Following {address}")).with(json!({ "address": address }))
            );
        }
        _ => {}
    }
    let doc = word.current_doc()?;
    let before = doc.session.selection.clone();
    let answer = document_command(doc, command, arguments);
    // A command that moved the caret shows where it went, as a key press does, so the page follows an
    // agent typing the way it follows a person.
    if answer.is_ok() && doc.session.selection != before {
        doc.view.reveal = true;
    }
    answer
}

/// `autocorrect [<rule> on|off]`: the rules, or one switched, for every open document and the settings.
fn autocorrect(word: &mut Word, arguments: &[String]) -> Result<Answer, String> {
    use unluminous_word::autocorrect::RULES;
    if let (Some(name), Some(value)) = (arguments.first(), on(arguments.get(1))) {
        let slot = word.config.autocorrect.slot(name).ok_or_else(|| {
            format!(
                "{name} is not a rule: {}",
                RULES.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
            )
        })?;
        *slot = value;
        let rules = word.config.autocorrect.clone();
        for doc in &mut word.docs {
            doc.session.autocorrect_rules = rules.clone();
        }
        if let Some(folder) = &word.folder {
            word.config.write(folder)?;
        }
        return Ok(Answer::said(format!("{name} is {}", if value { "on" } else { "off" })));
    }
    let list: Vec<_> = RULES
        .iter()
        .map(|(name, label)| json!({ "rule": name, "label": label, "on": word.config.autocorrect.get(name) }))
        .collect();
    Ok(Answer::said(format!("{} rules", list.len())).with(json!(list)))
}

/// A command about the current document.
fn document_command(doc: &mut Doc, command: &str, arguments: &[String]) -> Result<Answer, String> {
    let session = &mut doc.session;
    match command {
        "read" if arguments.iter().any(|a| a == "--story") => {
            let (story, rest) = story_from(&session.document, arguments)?;
            let numbers: Vec<usize> = rest.iter().filter_map(|a| a.parse().ok()).collect();
            let from = numbers.first().copied().unwrap_or(1);
            let to = numbers.get(1).copied().unwrap_or(if numbers.is_empty() {
                usize::MAX
            } else {
                from
            });
            let blocks =
                position::story(&session.document, &story).ok_or("there is no such story")?;
            let paragraphs = unluminous_word::export::paragraphs_value_in(
                &session.document,
                blocks,
                from,
                to,
                flag(arguments, "--runs"),
            );
            Ok(Answer::said(format!(
                "{} paragraphs in {}",
                paragraphs.len(),
                super::story_name(&story)
            ))
            .with(json!(paragraphs)))
        }
        "locate" => {
            // locate <paragraph> [offset] [--story <kind> [n]]: where that place is drawn in the window, for
            // a press there.
            let (story, rest) = if arguments.iter().any(|a| a == "--story") {
                story_from(&session.document, arguments)?
            } else {
                (StoryId::Body, arguments.to_vec())
            };
            let numbers: Vec<usize> = rest.iter().filter_map(|a| a.parse().ok()).collect();
            let blocks =
                position::story(&session.document, &story).ok_or("there is no such story")?;
            let all = position::paragraphs(blocks);
            let (path, _) = all
                .get(numbers.first().copied().unwrap_or(1).saturating_sub(1))
                .ok_or("there is no such paragraph")?;
            let at =
                Pos { story, path: path.clone(), offset: numbers.get(1).copied().unwrap_or(0) };
            let area = doc.view.pages_rect;
            let (tops, _) = crate::components::word::page_view::page_tops(doc, area.width());
            let (index, line) = doc.pages.find(&at).ok_or("that place is not on a page yet")?;
            let zoom = doc.view.zoom;
            let origin = area.min + egui::vec2(tops[index].x, tops[index].y - doc.view.scroll);
            let x = origin.x + line.x_of(at.offset) * zoom;
            let y = origin.y + (line.top + line.height / 2.0) * zoom;
            let (x, y) = (x * doc.view.window_zoom, y * doc.view.window_zoom);
            Ok(Answer::said(format!("{x:.0}, {y:.0}"))
                .with(json!({ "x": x, "y": y, "page": index + 1 })))
        }
        "stories" => {
            // Every story an agent can read: the body, each header and footer, each note, comment and text box.
            let document = &session.document;
            let mut out = vec![json!({ "story": "body" })];
            for (n, header) in document.headers.iter().enumerate() {
                out.push(json!({ "story": if header.footer { "footer" } else { "header" }, "n": n + 1, "id": header.id }));
            }
            for (kind, part) in [
                ("footnote", &document.footnotes),
                ("endnote", &document.endnotes),
                ("comment", &document.comments),
            ] {
                if let Some(part) = part {
                    for note in &part.notes {
                        if let Some(id) = note.id().filter(|id| kind == "comment" || *id > 0) {
                            out.push(json!({ "story": kind, "n": id }));
                        }
                    }
                }
            }
            for (n, textbox) in document.textboxes.iter().enumerate() {
                out.push(json!({ "story": "textbox", "n": n + 1, "id": textbox.id }));
            }
            Ok(Answer::said(format!("{} stories", out.len())).with(json!(out)))
        }
        "read" => {
            let numbers: Vec<usize> = arguments.iter().filter_map(|a| a.parse().ok()).collect();
            let from = numbers.first().copied().unwrap_or(1);
            let to = numbers.get(1).copied().unwrap_or(if numbers.is_empty() {
                usize::MAX
            } else {
                from
            });
            let paragraphs = unluminous_word::export::paragraphs_value(
                &session.document,
                from,
                to,
                flag(arguments, "--runs"),
            );
            Ok(Answer::said(format!("{} paragraphs", paragraphs.len())).with(json!(paragraphs)))
        }
        "text" => {
            let text = unluminous_word::export::to_text(&session.document);
            Ok(Answer::said(format!("{} characters", text.chars().count())).with(json!(text)))
        }
        "outline" if !arguments.is_empty() => {
            // outline promote|demote|move-up|move-down|expand|collapse|heading-1|body|level <n>
            let id = match arguments[0].as_str() {
                "promote" => "promote-heading",
                "demote" => "demote-heading",
                "move-up" => "outline-move-up",
                "move-down" => "outline-move-down",
                "expand" => "outline-expand",
                "collapse" => "outline-collapse",
                "heading-1" => "outline-to-heading-1",
                "body" => "outline-to-body",
                "level" => {
                    doc.view.outline_level = arguments.get(1).and_then(|n| n.parse().ok()).unwrap_or(10).clamp(1, 10);
                    return Ok(Answer::said(format!("Showing level {}", doc.view.outline_level)));
                }
                other => return Err(format!("outline takes promote, demote, move-up, move-down, expand, collapse, heading-1, body or level <n>, not {other}")),
            };
            (super::actions::find(id).expect("an outline action").run)(doc).map(Answer::said)
        }
        "outline" => {
            doc.ensure_pages();
            let entries: Vec<serde_json::Value> = doc
                .session
                .toc_entries(9, &doc.pages)
                .into_iter()
                .map(|(level, text, page, path)| json!({ "level": level, "text": text, "page": page, "path": path }))
                .collect();
            Ok(Answer::said(format!("{} headings", entries.len())).with(json!(entries)))
        }
        "spelling" => spelling(doc, arguments),
        // The Word menu's Spelling entry, which goes to the next error as F7 does.
        "spelling-next" => spelling(doc, &["next".to_owned()]),
        "stats" => {
            doc.ensure_pages();
            let stats = doc.session.stats(Some(&doc.pages));
            Ok(Answer::said(format!("{} pages, {} words", stats.pages, stats.words)).with(json!({
                "pages": stats.pages, "words": stats.words, "characters": stats.characters,
                "charactersWithSpaces": stats.characters_with_spaces, "paragraphs": stats.paragraphs, "lines": stats.lines,
            })))
        }
        "pages" => {
            doc.ensure_pages();
            let all = position::paragraphs(&doc.session.document.body);
            let pages: Vec<serde_json::Value> = doc
                .pages
                .pages
                .iter()
                .enumerate()
                .map(|(index, page)| {
                    let first = page.first.as_ref();
                    let paragraph = first.and_then(|f| all.iter().position(|(p, _)| *p == f.path)).map(|i| i + 1);
                    let text = first.and_then(|f| all.iter().find(|(p, _)| *p == f.path)).map(|(_, p)| {
                        let t = p.flat_text();
                        let offset = first.map_or(0, |f| f.offset).min(t.len());
                        unluminous_word::session::clean_text(&t[offset..]).chars().take(60).collect::<String>()
                    });
                    json!({ "page": index + 1, "number": page.number, "width": page.width, "height": page.height, "paragraph": paragraph, "starts": text })
                })
                .collect();
            Ok(Answer::said(format!("{} pages", pages.len())).with(json!(pages)))
        }
        "select" => select(doc, arguments),
        "selection" => Ok(Answer::said(doc.session.selected_text()).with(json!({
            "selection": selection_value(&doc.session.selection),
            "text": doc.session.selected_text(),
        }))),
        "type" => {
            let text = unescape(&rest(arguments, 0));
            session.type_text(&text);
            Ok(Answer::said(format!("Typed {} characters", text.chars().count())))
        }
        "insert" => insert(doc, arguments),
        "delete" => {
            let unit = if flag(arguments, "word") { Unit::Word } else { Unit::Character };
            if flag(arguments, "forward") {
                session.delete(unit);
            } else {
                session.backspace(unit);
            }
            Ok(Answer::said("Deleted"))
        }
        "find"
            if flag(arguments, "--wildcards")
                || flag(arguments, "--all-stories")
                || arguments.iter().any(|a| a.starts_with("--format")) =>
        {
            let search = search_from(arguments)?;
            let found = session.search(&search)?;
            let hits: Vec<serde_json::Value> = found
                .iter()
                .map(|s| json!({ "story": super::story_name(&s.anchor.story), "path": s.anchor.path, "offset": s.anchor.offset, "length": s.head.offset - s.anchor.offset }))
                .collect();
            if let Some(first) = found.first() {
                session.selection = first.clone();
                doc.view.reveal = true;
            }
            Ok(Answer::said(format!("{} found", hits.len())).with(json!(hits)))
        }
        "replace"
            if flag(arguments, "--wildcards")
                || flag(arguments, "--all-stories")
                || arguments
                    .iter()
                    .any(|a| a.starts_with("--format") || a.starts_with("--with-format")) =>
        {
            let plain = words(arguments);
            let split = plain.iter().position(|a| a == "--").ok_or(
                "replace takes the text to find, then --, then the text to put in its place",
            )?;
            let mut search = search_from(arguments)?;
            search.query.text = plain[..split].join(" ");
            let with = unescape(&plain[split + 1..].join(" "));
            let format = format_from(arguments, "--with-format=")?;
            let count = session.replace_matches(&search, &with, &format)?;
            Ok(Answer::said(format!("{count} replaced")).with(json!({ "replaced": count })))
        }
        "goto" | "go-to" => {
            doc.ensure_pages();
            let what = arguments.first().ok_or("goto takes what to go to and which one")?;
            let which = rest(arguments, 1);
            let pages = doc.pages.clone();
            let at = doc.session.go_to(what, &which, &pages)?;
            doc.view.reveal = true;
            Ok(Answer::said(format!("At {what} {which}"))
                .with(json!({ "path": at.path, "offset": at.offset })))
        }
        "find" => {
            let needle = rest(&words(arguments), 0);
            let found = session.find(&needle, flag(arguments, "--case"), flag(arguments, "--word"));
            let all = position::paragraphs(&session.document.body);
            let hits: Vec<serde_json::Value> = found
                .iter()
                .map(|s| {
                    let index = all.iter().position(|(p, _)| *p == s.anchor.path).map(|i| i + 1);
                    json!({ "paragraph": index, "offset": s.anchor.offset, "length": s.head.offset - s.anchor.offset })
                })
                .collect();
            if let Some(first) = found.first() {
                session.selection = first.clone();
                doc.view.reveal = true;
            }
            Ok(Answer::said(format!("{} found", hits.len())).with(json!(hits)))
        }
        "replace" => {
            let plain = words(arguments);
            let split = plain.iter().position(|a| a == "--").ok_or(
                "replace takes the text to find, then --, then the text to put in its place",
            )?;
            let needle = plain[..split].join(" ");
            let with = unescape(&plain[split + 1..].join(" "));
            let (case, whole) = (flag(arguments, "--case"), flag(arguments, "--word"));
            let count = if flag(arguments, "--all") {
                session.replace_all(&needle, &with, case, whole)
            } else {
                match session.find(&needle, case, whole).into_iter().next() {
                    Some(first) => {
                        session.selection = first;
                        session.type_text(&with);
                        1
                    }
                    None => 0,
                }
            };
            Ok(Answer::said(format!("{count} replaced")).with(json!({ "replaced": count })))
        }
        "format" => format(doc, arguments),
        "paragraph" => paragraph(doc, arguments),
        "style" => {
            if arguments.first().map(String::as_str) == Some("list") {
                let kind = arguments.get(1).map_or("paragraph", String::as_str);
                let resolver = unluminous_word::styles::Resolver::new(&session.document);
                let styles: Vec<serde_json::Value> = resolver
                    .listed(kind)
                    .into_iter()
                    .map(|(id, name)| json!({ "id": id, "name": name }))
                    .collect();
                return Ok(Answer::said(format!("{} styles", styles.len())).with(json!(styles)));
            }
            let style_id = |session: &unluminous_word::session::Session, wanted: &str| {
                unluminous_word::styles::Resolver::new(&session.document)
                    .find(wanted)
                    .map(str::to_owned)
                    .unwrap_or_else(|| wanted.replace(' ', ""))
            };
            match arguments.first().map(String::as_str) {
                Some("modify") => {
                    let id = style_id(
                        session,
                        arguments
                            .get(1)
                            .ok_or("style modify takes a style, a property and a value")?,
                    );
                    let property = arguments
                        .get(2)
                        .ok_or("style modify takes a style, a property and a value")?;
                    session.modify_style(&id, property, &rest(arguments, 3))?;
                    return Ok(Answer::said(format!("{id} changed")));
                }
                Some("new") => {
                    let id = session.new_style_from_selection(&rest(arguments, 1))?;
                    return Ok(Answer::said(format!("New style {id}")).with(json!({ "id": id })));
                }
                Some("update") => {
                    let id = session.update_style_to_match()?;
                    return Ok(Answer::said(format!("{id} updated to match the selection")));
                }
                Some("delete") => {
                    let id = style_id(session, &rest(arguments, 1));
                    session.delete_style(&id)?;
                    return Ok(Answer::said(format!("{id} deleted")));
                }
                Some("revert") => {
                    let id = style_id(session, &rest(arguments, 1));
                    session.revert_style(&id)?;
                    return Ok(Answer::said(format!("{id} is the default again")));
                }
                _ => {}
            }
            let wanted = rest(arguments, 0);
            let id = {
                let resolver = unluminous_word::styles::Resolver::new(&session.document);
                resolver.find(&wanted).map(str::to_owned)
            }
            .unwrap_or_else(|| wanted.replace(' ', ""));
            session.apply_style_where_it_fits(&id)?;
            Ok(Answer::said(format!("Applied {wanted}")))
        }
        "list" => {
            match arguments.first().map(String::as_str) {
                Some("bullet" | "bullets") => session.toggle_list(ListKind::Bullet),
                Some("number" | "numbered" | "numbering") => session.toggle_list(ListKind::Number),
                Some("deeper" | "demote" | "indent") => {
                    session.change_list_level(true);
                }
                Some("shallower" | "promote" | "outdent") => {
                    session.change_list_level(false);
                }
                Some("restart") => {
                    session.restart_list(arguments.get(1).and_then(|n| n.parse().ok()).unwrap_or(1))
                }
                Some("continue") => session.continue_numbering()?,
                Some("value") => session.set_numbering_value(
                    arguments.get(1).and_then(|n| n.parse().ok()).ok_or("list value takes a number")?,
                )?,
                Some("bullet-symbol" | "symbol") => {
                    let symbol = arguments
                        .get(1)
                        .and_then(|s| s.chars().next())
                        .ok_or("list symbol takes the character to use")?;
                    session.define_bullet(symbol, arguments.get(2).map(String::as_str))?
                }
                Some("format") => session.define_number_format(
                    arguments.get(1).ok_or("list format takes a number format, such as upperRoman")?,
                    arguments.get(2).map_or("", String::as_str),
                    arguments.get(3).map_or(".", String::as_str),
                )?,
                Some("indents") => session.set_list_indents(
                    points(arguments.get(1))?,
                    arguments.get(2).and_then(|h| h.parse().ok()).unwrap_or(18.0),
                )?,
                Some("define") => {
                    // Each word after `define` is one level, `format:text`, such as decimal:%1.
                    let levels: Vec<unluminous_word::lists::LevelDefinition> = arguments[1..]
                        .iter()
                        .enumerate()
                        .map(|(i, spec)| {
                            let (format, text) = spec.split_once(':').unwrap_or((spec.as_str(), "%1."));
                            unluminous_word::lists::LevelDefinition {
                                format: format.to_owned(),
                                text: text.to_owned(),
                                start: 1,
                                indent: 36.0 * (i as f32 + 1.0),
                                hanging: 18.0,
                                font: None,
                            }
                        })
                        .collect();
                    session.define_multilevel(&levels)?
                }
                other => {
                    return Err(format!(
                        "list takes bullet, number, deeper, shallower, restart, continue, value, symbol, format, indents or define, not {other:?}"
                    ))
                }
            }
            Ok(Answer::said("Done"))
        }
        "table" => table(doc, arguments),
        "picture" | "object" => picture(doc, arguments),
        "section" => section(doc, arguments),
        "comment" => comment(doc, arguments),
        "track" => {
            match arguments.first().map(String::as_str) {
                Some("on") => session.set_tracking(true),
                Some("off") if session.tracking_locked() => {
                    return Err("Track Changes is locked on; track unlock <password> unlocks it".into())
                }
                Some("off") => session.set_tracking(false),
                Some("accept") | Some("reject") => {
                    let accept = arguments[0] == "accept";
                    let all = arguments.get(1).is_some_and(|a| a == "all");
                    let count = session.resolve_revisions(accept, all);
                    return Ok(Answer::said(format!(
                        "{count} changes {}",
                        if accept { "accepted" } else { "rejected" }
                    ))
                    .with(json!({ "count": count })));
                }
                Some("list") | None => {
                    let changes = revisions(&session.document);
                    return Ok(Answer::said(format!(
                        "{} tracked changes; Track Changes is {}",
                        changes.len(),
                        if session.tracking { "on" } else { "off" }
                    ))
                    .with(json!({ "tracking": session.tracking, "changes": changes })));
                }
                Some("view") => {
                    let mode = arguments
                        .get(1)
                        .and_then(|m| unluminous_word::review::Markup::parse(m))
                        .ok_or("track view takes simple, all, none or original")?;
                    doc.view.markup = mode;
                    doc.laid = 0;
                    return Ok(Answer::said(format!("Showing {} markup", mode.name())));
                }
                Some("next") | Some("previous") => {
                    let forward = arguments[0] == "next";
                    if !session.go_to_change(forward) {
                        return Err("there are no tracked changes".into());
                    }
                    doc.view.reveal = true;
                    return Ok(Answer::said("At the change"));
                }
                Some("lock") => {
                    let password = rest(arguments, 1);
                    if password.is_empty() {
                        return Err("track lock takes a password".into());
                    }
                    session.lock_tracking(&password);
                    return Ok(Answer::said("Track Changes is locked on"));
                }
                Some("unlock") => {
                    if !session.protection_password_matches(&rest(arguments, 1)) {
                        return Err("that is not the password Track Changes was locked with".into());
                    }
                    session.lock_tracking("");
                    return Ok(Answer::said("Track Changes is unlocked"));
                }
                Some(other) => {
                    return Err(format!("track takes on, off, list, accept, reject, view, next, previous, lock or unlock, not {other}"))
                }
            }
            Ok(Answer::said(format!(
                "Track Changes is {}",
                if session.tracking { "on" } else { "off" }
            )))
        }
        "fields" => fields(doc, arguments),
        "toc" => toc(doc, arguments),
        "caption" => {
            doc.ensure_pages();
            let pages = doc.pages.clone();
            // `arguments` starts after the verb: `caption Table Sales --above` is the label `Table`.
            let label = words(arguments).first().cloned().unwrap_or_else(|| "Figure".into());
            let below = !flag(arguments, "--above");
            let text = words(arguments).get(1..).map(|w| w.join(" ")).unwrap_or_default();
            doc.session.insert_caption(&label, &text, below, &pages);
            Ok(Answer::said(format!("{label} caption added")))
        }
        "xref" | "cross-reference" => {
            doc.ensure_pages();
            let pages = doc.pages.clone();
            let n: usize = arguments
                .get(1)
                .and_then(|n| n.parse().ok())
                .ok_or("xref takes the number of the paragraph it refers to")?;
            let what = arguments.get(2).map_or("text", String::as_str).to_owned();
            let target = paragraph_path(doc, n)?;
            doc.session.insert_cross_reference(&target, &what, &pages)?;
            Ok(Answer::said(format!("Cross reference to paragraph {n} added")))
        }
        "link" => link(doc, arguments),
        "thesaurus" | "synonyms" => {
            let word = match arguments.first() {
                Some(word) => word.clone(),
                None => doc.session.selected_text(),
            };
            let meanings: Vec<serde_json::Value> =
                unluminous_word::thesaurus::synonyms(word.trim())
                    .into_iter()
                    .map(|m| json!({ "partOfSpeech": m.part_of_speech, "synonyms": m.synonyms }))
                    .collect();
            Ok(Answer::said(format!("{} meanings of {}", meanings.len(), word.trim()))
                .with(json!(meanings)))
        }
        "language" => {
            let tag = arguments.first().ok_or("language takes a tag such as en-US or fr-FR")?;
            session.set_language(tag);
            Ok(Answer::said(format!("The selection is in {tag}")))
        }
        "accessibility" => {
            let issues: Vec<serde_json::Value> = session
                .accessibility()
                .into_iter()
                .map(|i| json!({ "kind": i.kind, "message": i.message, "path": i.at.path, "offset": i.at.offset }))
                .collect();
            Ok(Answer::said(format!("{} accessibility issues", issues.len())).with(json!(issues)))
        }
        "inspect" => match arguments.first().map(String::as_str) {
            Some("remove") => {
                let kind = arguments
                    .get(1)
                    .ok_or("inspect remove takes comments, revisions, hidden or personal")?;
                let count = session.remove_inspected(kind)?;
                Ok(Answer::said(format!("{count} removed")))
            }
            _ => {
                let found = session.inspect();
                Ok(Answer::said(format!(
                    "{} comments, {} tracked changes, {} hidden runs, {} personal names",
                    found.comments,
                    found.revisions,
                    found.hidden,
                    found.personal.len()
                ))
                .with(json!({
                    "comments": found.comments,
                    "revisions": found.revisions,
                    "hidden": found.hidden,
                    "personal": found.personal.iter().map(|(k, v)| json!({ "property": k, "value": v })).collect::<Vec<_>>(),
                })))
            }
        },
        "paste" => {
            // `paste --as source|merge|text` changes the last paste, as Paste Options does;
            // `paste --html <html>` and `paste --rtf <rtf>` paste formatted text an agent has in hand.
            let value = |name: &str| {
                arguments.iter().position(|a| a == name).map(|i| arguments[i + 1..].join(" "))
            };
            if let Some(html) = value("--html") {
                super::paste_from(
                    doc,
                    Some(&html),
                    None,
                    "",
                    unluminous_word::clipboard::PasteAs::Source,
                );
                return Ok(Answer::said("Pasted"));
            }
            if let Some(rtf) = value("--rtf") {
                super::paste_from(
                    doc,
                    None,
                    Some(&rtf),
                    "",
                    unluminous_word::clipboard::PasteAs::Source,
                );
                return Ok(Answer::said("Pasted"));
            }
            let how = arguments
                .iter()
                .position(|a| a == "--as")
                .and_then(|i| arguments.get(i + 1))
                .and_then(|n| unluminous_word::clipboard::PasteAs::parse(n))
                .ok_or("paste takes --as source|merge|text, --html <html> or --rtf <rtf>")?;
            super::paste_again(doc, how)?;
            Ok(Answer::said("Paste changed"))
        }
        "print" => {
            // print [pages] [--copies n] [--odd|--even] [--per-sheet 2|4] [--to <file>]
            let value = |name: &str| {
                arguments.iter().position(|a| a == name).and_then(|i| arguments.get(i + 1)).cloned()
            };
            doc.ensure_pages();
            let count = doc.pages.pages.len();
            let range =
                arguments.first().filter(|a| !a.starts_with("--")).cloned().unwrap_or_default();
            let options = super::print::PrintOptions {
                pages: super::print::parse_range(&range, count)?,
                parity: if flag(arguments, "--odd") {
                    super::print::Parity::Odd
                } else if flag(arguments, "--even") {
                    super::print::Parity::Even
                } else {
                    super::print::Parity::All
                },
                copies: value("--copies").and_then(|c| c.parse().ok()).unwrap_or(1),
                per_sheet: value("--per-sheet").and_then(|c| c.parse().ok()).unwrap_or(1),
            };
            match value("--to") {
                Some(to) => {
                    let sheets = super::print::sheets(&doc.pages, &options);
                    let bytes = pdf_bytes(doc, &sheets);
                    crate::services::store::write_atomically(Path::new(&to), &bytes)
                        .map_err(|e| format!("{to} could not be written: {e}"))?;
                    Ok(Answer::said(format!("{} sheets written to {to}", sheets.pages.len()))
                        .with(json!({ "sheets": sheets.pages.len(), "path": to })))
                }
                None => super::print::print(doc, &options).map(Answer::said),
            }
        }
        "undo" => Ok(Answer::said(if session.undo() { "Undone" } else { "Nothing to undo" })),
        "redo" => Ok(Answer::said(if session.redo() { "Redone" } else { "Nothing to redo" })),
        "export" => {
            let kind =
                arguments.first().map(String::as_str).ok_or("export takes pdf, html, md or txt")?;
            let to = arguments.iter().skip(1).find(|a| !a.starts_with("--")).map(PathBuf::from);
            let how = PdfHow {
                tagged: !flag(arguments, "--no-tags"),
                outline: !flag(arguments, "--no-outline"),
                pdfa: flag(arguments, "--pdfa"),
            };
            let said = export_with(doc, kind, to.as_deref(), how)?;
            Ok(Answer::said(said))
        }
        "properties" => properties(doc, arguments),
        "unlock" => {
            session.unlock_for_editing(&rest(arguments, 0))?;
            Ok(Answer::said("The document can be changed"))
        }
        "protect" => protect(doc, arguments),
        "recover" => match arguments.first().map(String::as_str) {
            Some("restore") => {
                doc.restore_recovery()?;
                Ok(Answer::said("Restored the recovered copy; save it to keep it"))
            }
            Some("discard") => {
                doc.discard_recovery();
                Ok(Answer::said("Kept the file and threw the recovered copy away"))
            }
            _ => Ok(Answer::said(match &doc.timing.offer {
                Some((_, when)) => format!(
                    "A recovered copy from {when} is offered: recover restore or recover discard"
                ),
                None => "There is no recovered copy".into(),
            })
            .with(json!({ "offered": doc.timing.offer.is_some() }))),
        },
        "view" => {
            match arguments.first().map(String::as_str) {
                Some("zoom") => {
                    doc.view.zoom = (points(arguments.get(1))? / 100.0).clamp(0.25, 5.0)
                }
                Some("marks") => {
                    doc.view.marks = on(arguments.get(1)).unwrap_or(!doc.view.marks);
                    doc.laid = 0;
                }
                Some("navigation") => {
                    doc.view.navigation = on(arguments.get(1)).unwrap_or(!doc.view.navigation)
                }
                Some("thumbnails") => {
                    doc.view.navigation = on(arguments.get(1)).unwrap_or(true);
                    doc.view.navigation_tab = 1;
                }
                Some("ruler") => doc.view.ruler = on(arguments.get(1)).unwrap_or(!doc.view.ruler),
                Some("field-shading") => {
                    let kind = arguments.get(1).map(String::as_str).unwrap_or("selected");
                    doc.view.field_shading = super::shading_kind(kind)
                        .ok_or_else(|| format!("field shading is always, selected or never, not {kind}"))?;
                }
                Some("gridlines") => doc.view.gridlines = on(arguments.get(1)).unwrap_or(!doc.view.gridlines),
                Some("multiple") => doc.view.multiple = on(arguments.get(1)).unwrap_or(!doc.view.multiple),
                Some(mode @ ("read" | "outline" | "print")) => {
                    doc.view.mode = match mode {
                        "read" => super::Mode::Read,
                        "outline" => super::Mode::Outline,
                        _ => super::Mode::Print,
                    };
                    doc.laid = u64::MAX;
                }
                Some(id @ ("fit-width" | "fit-page" | "new-window")) => {
                    let id = id.to_owned();
                    (super::actions::find(&id).expect("a view action").run)(doc)?;
                }
                Some("paper") => doc.view.dark = arguments.get(1).map(String::as_str) == Some("theme"),
                Some(other) => return Err(format!("view takes zoom, marks, navigation, thumbnails, ruler, gridlines, multiple, read, outline, print, fit-width, fit-page, new-window or paper, not {other}")),
                None => {}
            }
            let mode = match doc.view.mode {
                super::Mode::Print => "print",
                super::Mode::Read => "read",
                super::Mode::Outline => "outline",
            };
            Ok(Answer::said("View").with(json!({ "zoom": doc.view.zoom, "marks": doc.view.marks, "navigation": doc.view.navigation, "ruler": doc.view.ruler, "mode": mode, "multiple": doc.view.multiple, "gridlines": doc.view.gridlines, "fieldShading": doc.view.field_shading, "paper": if doc.view.dark { "theme" } else { "light" } })))
        }
        "panel" => {
            match arguments.first().map(String::as_str) {
                Some("close") | None => doc.view.panel.clear(),
                Some(name) => {
                    let level = Level::parse(name).ok_or_else(|| {
                        format!(
                            "{name} is not a level: {}",
                            Level::ALL
                                .iter()
                                .map(|l| l.name().to_lowercase())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?;
                    doc.view.panel = match (level, level.parent()) {
                        (Level::Root, _) => vec![Level::Root],
                        (_, Some(parent)) => vec![Level::Root, parent, level],
                        _ => vec![Level::Root, level],
                    };
                }
            }
            Ok(Answer::said("Panel").with(
                json!({ "panel": doc.view.panel.iter().map(|l| l.name()).collect::<Vec<_>>() }),
            ))
        }
        "action" => match arguments.first().map(String::as_str) {
            Some("list") | None => {
                let all: Vec<serde_json::Value> = actions::ACTIONS
                    .iter()
                    .map(|a| json!({ "id": a.id, "label": a.label, "level": a.level.name() }))
                    .collect();
                Ok(Answer::said(format!("{} actions", all.len())).with(json!(all)))
            }
            Some(id) => {
                let action = actions::find(id).ok_or_else(|| {
                    format!("there is no action `{id}`: `action list` names them all")
                })?;
                let said = (action.run)(doc)?;
                Ok(Answer::said(if said.is_empty() { action.label.to_owned() } else { said }))
            }
        },
        other => Err(format!(
            "there is no `{other}` command on `word`: {}",
            unluminous_cli::plugins::WORD
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn select(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    // select --story <kind> [n] <paragraph> [offset] [to-offset]: a place in a header, a note, a comment
    // or a text box, its paragraphs numbered from 1 as `read --story` numbers them.
    if arguments.iter().any(|a| a == "--story") {
        let (story, rest) = story_from(&doc.session.document, arguments)?;
        let numbers: Vec<usize> = rest.iter().filter_map(|a| a.parse().ok()).collect();
        let blocks =
            position::story(&doc.session.document, &story).ok_or("there is no such story")?;
        let all = position::paragraphs(blocks);
        let n = numbers.first().copied().unwrap_or(1);
        let (path, paragraph) = all
            .get(n.saturating_sub(1))
            .ok_or_else(|| format!("the story has {} paragraphs", all.len()))?;
        let len = paragraph.len();
        let from = numbers.get(1).copied().unwrap_or(0).min(len);
        let to = numbers.get(2).copied().unwrap_or(from).min(len);
        let anchor = Pos { story: story.clone(), path: path.clone(), offset: from };
        let head = Pos { story, path: path.clone(), offset: to };
        doc.session.select(anchor, head);
        doc.view.reveal = true;
        return Ok(Answer::said(doc.session.selected_text())
            .with(json!({ "selection": selection_value(&doc.session.selection) })));
    }
    match arguments.first().map(String::as_str) {
        Some("all") => doc.session.select_all(),
        Some("text") => {
            let needle = rest(arguments, 1);
            let first = doc
                .session
                .find(&needle, false, false)
                .into_iter()
                .next()
                .ok_or_else(|| format!("{needle} is not in the document"))?;
            doc.session.selection = first;
        }
        _ => {
            let numbers: Vec<usize> = arguments
                .iter()
                .map(|a| a.parse::<usize>().map_err(|_| format!("{a} is not a number")))
                .collect::<Result<_, _>>()?;
            let paragraph = numbers
                .first()
                .copied()
                .ok_or("select takes a paragraph number, `all`, or `text <words>`")?;
            let path = paragraph_path(doc, paragraph)?;
            let len = position::paragraph(&doc.session.document, &StoryId::Body, &path)
                .map_or(0, |p| p.len());
            let anchor = Pos::new(path.clone(), numbers.get(1).copied().unwrap_or(0).min(len));
            let head = match numbers.get(2) {
                Some(to) => {
                    let to_path = paragraph_path(doc, *to)?;
                    let to_len =
                        position::paragraph(&doc.session.document, &StoryId::Body, &to_path)
                            .map_or(0, |p| p.len());
                    Pos::new(to_path, numbers.get(3).copied().unwrap_or(to_len).min(to_len))
                }
                // One number and an offset puts the caret; one number alone selects the whole paragraph.
                None if numbers.len() == 1 => Pos::new(path, len),
                None => anchor.clone(),
            };
            doc.session.select(anchor, head);
        }
    }
    doc.view.reveal = true;
    Ok(Answer::said(doc.session.selected_text()).with(json!({ "selection": selection_value(&doc.session.selection), "text": doc.session.selected_text() })))
}

/// `picture [info]`, `picture size <w> [h]`, `picture wrap <kind>`, `picture position <relative> <x> <y>`,
/// `picture crop <left> <top> <right> <bottom>` (percent), `picture alt <text>`, `picture reset`,
/// `picture order forward|backward|front|back` and `picture select <paragraph> <offset>`, for the
/// picture or shape the selection is on.
fn picture(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    let chosen = match arguments.first().map(String::as_str) {
        Some("select") => {
            let paragraph: usize = arguments
                .get(1)
                .and_then(|n| n.parse().ok())
                .ok_or("picture select takes a paragraph number and an offset")?;
            Some(paragraph_path(doc, paragraph)?)
        }
        _ => None,
    };
    let session = &mut doc.session;
    let number = |i: usize| -> Result<f32, String> {
        arguments
            .get(i)
            .and_then(|n| n.trim_end_matches("pt").trim_end_matches('%').parse().ok())
            .ok_or_else(|| format!("picture {} takes numbers", arguments[0]))
    };
    let said = match arguments.first().map_or("info", String::as_str) {
        "info" => "",
        "select" => {
            let offset: usize = arguments.get(2).and_then(|n| n.parse().ok()).unwrap_or(0);
            let path = chosen.expect("worked out above");
            session.select_object(unluminous_word::position::Pos::new(path, offset));
            if session.selected_object().is_none() {
                return Err("there is no picture there".into());
            }
            "Selected"
        }
        "size" => {
            let height = arguments.get(2).map(|_| number(2)).transpose()?;
            session.set_object_size(number(1)?, height)?;
            "Resized"
        }
        "wrap" => {
            session.set_object_wrap(arguments.get(1).ok_or("picture wrap takes a kind")?)?;
            "Wrapped"
        }
        "position" => {
            session.set_object_position(arguments.get(1).ok_or("picture position takes what it is measured from")?, number(2)?, number(3)?)?;
            "Moved"
        }
        "crop" => {
            session.crop_object(number(1)?, number(2)?, number(3)?, number(4)?)?;
            "Cropped"
        }
        "alt" => {
            session.set_object_alt_text(&rest(arguments, 1))?;
            "Alt text set"
        }
        "reset" => {
            session.reset_object()?;
            "Reset"
        }
        "order" => {
            session.order_object(arguments.get(1).map_or("front", String::as_str))?;
            "Reordered"
        }
        other => return Err(format!("picture takes info, select, size, wrap, position, crop, alt, reset or order, not {other}")),
    };
    let info = session.object_info().ok_or("the selection is not on a picture or shape")?;
    let value = json!({
        "kind": info.kind, "name": info.name, "width": info.width, "height": info.height, "wrap": info.wrap,
        "horizontal": { "from": info.horizontal.0, "offset": info.horizontal.1 },
        "vertical": { "from": info.vertical.0, "offset": info.vertical.1 },
        "alt": info.alt, "crop": [info.crop.0, info.crop.1, info.crop.2, info.crop.3],
    });
    let said = if said.is_empty() {
        format!("{} {} by {} points, {}", info.kind, info.width, info.height, info.wrap)
    } else {
        said.to_owned()
    };
    Ok(Answer::said(said).with(value))
}

fn insert(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    let what = arguments.first().map(String::as_str).ok_or("insert takes what to insert first")?;
    let session = &mut doc.session;
    match what {
        "paragraph" => session.enter(),
        "page-break" => session.insert_break(PAGE_BREAK),
        "line-break" => session.insert_break(LINE_BREAK),
        "column-break" => session.insert_break(COLUMN_BREAK),
        "section-break" => session.insert_section_break(match arguments.get(1).map(String::as_str) {
            Some("continuous") => "continuous",
            Some("even") | Some("even-page") => "evenPage",
            Some("odd") | Some("odd-page") => "oddPage",
            _ => "nextPage",
        }),
        "table" => {
            let rows = arguments.get(1).and_then(|n| n.parse().ok()).unwrap_or(3);
            let columns = arguments.get(2).and_then(|n| n.parse().ok()).unwrap_or(3);
            session.insert_table(rows, columns);
        }
        "picture" => {
            let path = PathBuf::from(arguments.get(1).ok_or("insert picture takes a path")?);
            let width = arguments.get(2).and_then(|w| w.parse::<f32>().ok());
            let said = insert_picture_file(doc, &path, width)?;
            return Ok(Answer::said(said));
        }
        "footnote" => session.insert_note(false, &rest(arguments, 1)),
        "endnote" => session.insert_note(true, &rest(arguments, 1)),
        "toc" | "table-of-contents" => {
            doc.ensure_pages();
            let pages = doc.pages.clone();
            let levels = arguments.get(1).and_then(|n| n.parse().ok()).unwrap_or(3);
            doc.session.insert_toc(levels, &pages);
        }
        "caption" => {
            doc.ensure_pages();
            let pages = doc.pages.clone();
            let label = arguments.get(1).cloned().unwrap_or_else(|| "Figure".into());
            doc.session.insert_caption(&label, &rest(arguments, 2), true, &pages);
        }
        "field" => {
            let code = rest(arguments, 1);
            session.insert_field(&code, "");
        }
        "link" | "hyperlink" => {
            let target = arguments.get(1).ok_or("insert link takes an address")?.clone();
            let text = (arguments.len() > 2).then(|| rest(arguments, 2));
            session.insert_hyperlink(&target, text.as_deref());
        }
        "bookmark" => session.insert_bookmark(&rest(arguments, 1)),
        "date" => return (actions::find("date").expect("a date action").run)(doc).map(Answer::said),
        "page-number" => session.insert_field("PAGE", "1"),
        "symbol" => session.type_text(&rest(arguments, 1)),
        "textbox" | "text-box" => {
            let width = arguments.get(1).and_then(|n| n.parse().ok()).unwrap_or(144.0);
            let height = arguments.get(2).and_then(|n| n.parse().ok()).unwrap_or(72.0);
            session.insert_textbox(width, height)?;
        }
        "shape" => {
            let preset = arguments.get(1).ok_or("insert shape takes a preset, such as rect or ellipse")?;
            let width = arguments.get(2).and_then(|n| n.parse().ok()).unwrap_or(72.0);
            let height = arguments.get(3).and_then(|n| n.parse().ok()).unwrap_or(width);
            session.insert_shape(preset, width, height)?;
        }
        other => return Err(format!("insert does not know `{other}`: paragraph, page-break, line-break, column-break, section-break, table, picture, textbox, shape, footnote, endnote, toc, caption, field, link, bookmark, date, page-number, symbol")),
    }
    Ok(Answer::said(format!("Inserted {what}")))
}

fn format(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    let what = arguments.first().map(String::as_str).ok_or("format takes a property")?;
    let session = &mut doc.session;
    let toggles = [
        ("bold", "w:b"),
        ("italic", "w:i"),
        ("strike", "w:strike"),
        ("double-strike", "w:dstrike"),
        ("small-caps", "w:smallCaps"),
        ("caps", "w:caps"),
        ("hidden", "w:vanish"),
    ];
    if let Some((_, element)) = toggles.iter().find(|(name, _)| *name == what) {
        match on(arguments.get(1)) {
            Some(value) => {
                let element = *element;
                session.format(move |rpr| props::set_on_off(rpr, element, Some(value)));
            }
            None => session.toggle(element),
        }
        return Ok(Answer::said(format!("{what} set")));
    }
    let value = arguments.get(1).cloned().unwrap_or_default();
    match what {
        "underline" => {
            let style = match value.as_str() {
                "" | "on" => Some("single".to_owned()),
                "off" | "none" => None,
                other => Some(other.to_owned()),
            };
            session.format(move |rpr| props::set_val(rpr, "w:u", style.as_deref()));
        }
        "superscript" | "subscript" => {
            let off = on(arguments.get(1)) == Some(false);
            let what = what.to_owned();
            session.format(move |rpr| {
                props::set_val(rpr, "w:vertAlign", (!off).then_some(what.as_str()))
            });
        }
        "font" => {
            let name = rest(arguments, 1);
            session.format(move |rpr| {
                rpr.put(
                    unluminous_word::xml::El::new("w:rFonts")
                        .with("w:ascii", name.clone())
                        .with("w:hAnsi", name.clone())
                        .with("w:cs", name.clone())
                        .with("w:eastAsia", name.clone()),
                );
            });
        }
        "size" => {
            let size = points(arguments.get(1))?;
            let half = ((size * 2.0).round() as i64).to_string();
            session.format(move |rpr| {
                props::set_val(rpr, "w:sz", Some(&half));
                props::set_val(rpr, "w:szCs", Some(&half));
            });
        }
        "color" | "colour" => {
            let hex = value.trim_start_matches('#').to_uppercase();
            if hex != "AUTO" && props::Rgb::parse(&hex).is_none() {
                return Err(format!("{value} is not a colour: use RRGGBB or auto"));
            }
            let hex = if hex == "AUTO" { "auto".to_owned() } else { hex };
            session.format(move |rpr| props::set_val(rpr, "w:color", Some(&hex)));
        }
        "highlight" => {
            let name = value.clone();
            if name != "none" && props::highlight_colour(&name).is_none() {
                return Err(format!(
                    "{name} is not one of the highlight colours: {}",
                    props::HIGHLIGHTS.iter().map(|h| h.0).collect::<Vec<_>>().join(", ")
                ));
            }
            session.format(move |rpr| {
                props::set_val(rpr, "w:highlight", (name != "none").then_some(name.as_str()))
            });
        }
        "spacing" => {
            let pts = points(arguments.get(1))?;
            session.format(move |rpr| {
                props::set_val(rpr, "w:spacing", Some(&((pts * 20.0).round() as i64).to_string()))
            });
        }
        "clear" => session.clear_formatting(),
        other => return Err(format!("format does not know `{other}`")),
    }
    Ok(Answer::said(format!("{what} set")))
}

fn paragraph(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    let what = arguments.first().map(String::as_str).unwrap_or("info");
    // paragraph info: how the paragraph at the caret looks, its style's formatting and its own together.
    if what == "info" {
        let look = doc.session.paragraph_at_caret();
        let tabs: Vec<serde_json::Value> = look
            .tabs
            .iter()
            .map(|t| json!({ "position": t.position, "kind": format!("{:?}", t.kind).to_lowercase(), "leader": t.leader.map(|c| c.to_string()) }))
            .collect();
        return Ok(Answer::said(look.style.clone()).with(json!({
            "style": look.style, "align": look.align.name(), "indentLeft": look.indent_left, "indentRight": look.indent_right,
            "indentFirst": look.indent_first, "spaceBefore": look.space_before, "spaceAfter": look.space_after,
            "line": format!("{:?}", look.line), "keepNext": look.keep_next, "keepLines": look.keep_lines,
            "outlineLevel": look.outline_level.map(|l| l + 1), "tabs": tabs,
        })));
    }
    let session = &mut doc.session;
    let twips = |p: f32| ((p * 20.0).round() as i64).to_string();
    match what {
        "align" => {
            let value = match arguments.get(1).map(String::as_str) {
                Some("left") => "left",
                Some("center" | "centre") => "center",
                Some("right") => "right",
                Some("justify" | "both") => "both",
                other => {
                    return Err(format!(
                        "align takes left, center, right or justify, not {other:?}"
                    ))
                }
            };
            session.format_paragraphs(move |ppr| props::set_val(ppr, "w:jc", Some(value)));
        }
        "indent-left" | "indent-right" | "first-line" | "hanging" => {
            let value = twips(points(arguments.get(1))?);
            let attr = match what {
                "indent-left" => "w:left",
                "indent-right" => "w:right",
                "first-line" => "w:firstLine",
                _ => "w:hanging",
            };
            session.format_paragraphs(move |ppr| {
                let ind = ppr.child_or_insert("w:ind");
                if attr == "w:firstLine" {
                    ind.remove_attr("w:hanging");
                }
                if attr == "w:hanging" {
                    ind.remove_attr("w:firstLine");
                }
                ind.set_attr(attr, value.clone());
            });
        }
        "before" | "after" => {
            let value = twips(points(arguments.get(1))?);
            let attr = if what == "before" { "w:before" } else { "w:after" };
            session.format_paragraphs(move |ppr| {
                let spacing = ppr.child_or_insert("w:spacing");
                spacing.set_attr(attr, value.clone());
                spacing.remove_attr(if what_is_before(attr) {
                    "w:beforeAutospacing"
                } else {
                    "w:afterAutospacing"
                });
            });
        }
        "line" => {
            let (line, rule) = match arguments.get(1).map(String::as_str) {
                Some("exact" | "exactly") => (twips(points(arguments.get(2))?), "exact"),
                Some("at-least" | "atleast") => (twips(points(arguments.get(2))?), "atLeast"),
                Some(multiple) => (
                    ((multiple
                        .parse::<f32>()
                        .map_err(|_| format!("{multiple} is not a multiple"))?
                        * 240.0)
                        .round() as i64)
                        .to_string(),
                    "auto",
                ),
                None => {
                    return Err(
                        "line takes a multiple such as 1.5, or exact or at-least and points".into(),
                    )
                }
            };
            session.format_paragraphs(move |ppr| {
                let spacing = ppr.child_or_insert("w:spacing");
                spacing.set_attr("w:line", line.clone());
                spacing.set_attr("w:lineRule", rule);
            });
        }
        "keep-next" | "keep-lines" | "page-break-before" | "widow-control" => {
            let element = match what {
                "keep-next" => "w:keepNext",
                "keep-lines" => "w:keepLines",
                "page-break-before" => "w:pageBreakBefore",
                _ => "w:widowControl",
            };
            let value = on(arguments.get(1)).unwrap_or(true);
            session.format_paragraphs(move |ppr| props::set_on_off(ppr, element, Some(value)));
        }
        "shading" => {
            let fill = arguments.get(1).cloned().unwrap_or_default();
            session.format_paragraphs(move |ppr| match props::Rgb::parse(&fill) {
                Some(rgb) => ppr.put(
                    unluminous_word::xml::El::new("w:shd")
                        .with("w:val", "clear")
                        .with("w:color", "auto")
                        .with("w:fill", rgb.hex()),
                ),
                None => ppr.remove_children("w:shd"),
            });
        }
        other => return Err(format!("paragraph does not know `{other}`")),
    }
    Ok(Answer::said(format!("{what} set")))
}

fn what_is_before(attr: &str) -> bool {
    attr == "w:before"
}

fn table(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    let what = arguments.first().map(String::as_str).ok_or("table takes an operation")?;
    let session = &mut doc.session;
    match what {
        "width" if arguments.len() >= 3 => {
            // table width <column> <pt>: the column from 1, the next column giving or taking the difference.
            let column: usize =
                arguments[1].parse().map_err(|_| "table width takes a column number and points")?;
            let width = points(arguments.get(2))?;
            let (table, _, _) = session.cell_at_caret().ok_or("the caret is not in a table")?;
            let story = session.selection.head.story.clone();
            session.drag_column_edge(&story, &table, column.saturating_sub(1), width)?
        }
        "width" => session.set_column_width(points(arguments.get(1))?)?,
        "shade" => session.shade_cell(arguments.get(1).and_then(|c| props::Rgb::parse(c)))?,
        "cell" => {
            let (table_path, _, _) =
                session.cell_at_caret().ok_or("the caret is not in a table")?;
            let row: u32 = arguments
                .get(1)
                .and_then(|r| r.parse().ok())
                .ok_or("table cell takes a row and a column, from 1")?;
            let column: u32 = arguments
                .get(2)
                .and_then(|c| c.parse().ok())
                .ok_or("table cell takes a row and a column, from 1")?;
            let mut path = table_path;
            path.extend([row.saturating_sub(1), column.saturating_sub(1), 0]);
            if position::paragraph(&session.document, &StoryId::Body, &path).is_none() {
                return Err(format!("the table has no cell at row {row}, column {column}"));
            }
            session.selection = Selection::caret(Pos::new(path, 0));
        }
        "select" => session.select_in_table(arguments.get(1).map_or("cell", String::as_str))?,
        "merge" => session.merge_cells()?,
        "split-table" => session.split_table()?,
        "convert" => match arguments.get(1).map(String::as_str) {
            Some("to-text") => {
                session.table_to_text(arguments.get(2).map_or("tab", String::as_str))?
            }
            Some("to-table") => {
                session.text_to_table(arguments.get(2).map_or("tab", String::as_str))?
            }
            _ => {
                return Err(
                    "table convert takes to-text or to-table, then tab, comma or paragraph".into()
                )
            }
        },
        "sort" => {
            let column: usize = arguments
                .get(1)
                .and_then(|c| c.parse::<usize>().ok())
                .ok_or("table sort takes a column, from 1")?;
            let by = match arguments.get(2).map(String::as_str) {
                Some("number") => unluminous_word::tables::SortBy::Number,
                Some("date") => unluminous_word::tables::SortBy::Date,
                _ => unluminous_word::tables::SortBy::Text,
            };
            session.sort_table(
                column.saturating_sub(1),
                by,
                flag(arguments, "--descending") || flag(arguments, "desc"),
                flag(arguments, "--header"),
            )?
        }
        "distribute" => {
            doc.ensure_pages();
            let rows = arguments.get(1).map(String::as_str) != Some("columns");
            let heights = row_heights(doc);
            doc.session.distribute(rows, &heights)?
        }
        "properties" | "property" => {
            let name = arguments.get(1).ok_or("table properties takes a property and its value")?;
            session.table_property(name, arguments.get(2..).unwrap_or(&[]))?
        }
        "style" => {
            let id = arguments.get(1).ok_or("table style takes a style id, such as TableGrid")?;
            let look = arguments.get(2).map(|flags| {
                let on = |c: char| flags.contains(c);
                // h header row, t total row, f first column, l last column, r banded rows, c banded columns
                [on('h'), on('t'), on('f'), on('l'), on('r'), on('c')]
            });
            session.apply_table_style(id, look)?
        }
        "borders" => {
            let which = arguments.get(1).map_or("all", String::as_str);
            let width = arguments.get(2).and_then(|w| w.parse().ok()).unwrap_or(0.5);
            let colour = arguments.get(3).and_then(|c| props::Rgb::parse(c));
            session.set_table_borders(which, width, colour)?
        }
        op => session.table_op(op)?,
    }
    Ok(Answer::said(format!("table {what}")))
}

/// `properties [<name> <value>]`, `properties statistics` and `properties custom [<name> <kind> <value> | remove
/// <name>]`: Word's Properties dialog, its Summary, Statistics and Custom tabs.
fn properties(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    use unluminous_word::properties as extra;
    let core = [
        ("title", "dc:title"),
        ("subject", "dc:subject"),
        ("author", "dc:creator"),
        ("keywords", "cp:keywords"),
        ("comments", "dc:description"),
        ("description", "dc:description"),
        ("category", "cp:category"),
        ("status", "cp:contentStatus"),
    ];
    let app = [("manager", "Manager"), ("company", "Company")];
    match arguments.first().map(String::as_str) {
        Some("statistics") => {
            doc.ensure_pages();
            let document = &doc.session.document;
            let text = unluminous_word::export::to_text(document);
            let words = text.split_whitespace().count();
            let characters = text.chars().filter(|c| *c != '\n' && *c != '\r').count();
            let spaces = text.chars().filter(|c| *c == ' ').count();
            let paragraphs = unluminous_word::position::paragraphs(&document.body)
                .iter()
                .filter(|(_, p)| !p.flat_text().trim().is_empty())
                .count();
            let lines: usize = doc
                .pages
                .pages
                .iter()
                .map(|p| p.carets.iter().filter(|l| l.story == StoryId::Body).count())
                .sum();
            let property = |name: &str| unluminous_word::new::property(document, name);
            let value = json!({
                "created": property("dcterms:created"),
                "modified": property("dcterms:modified"),
                "printed": property("cp:lastPrinted"),
                "lastSavedBy": property("cp:lastModifiedBy"),
                "revision": property("cp:revision"),
                "editingMinutes": extra::extended(document, "TotalTime"),
                "pages": doc.pages.pages.len(),
                "words": words,
                "characters": characters - spaces,
                "charactersWithSpaces": characters,
                "lines": lines,
                "paragraphs": paragraphs,
            });
            Ok(Answer::said(format!("{} pages, {words} words", doc.pages.pages.len())).with(value))
        }
        Some("custom") => {
            let document = &mut doc.session.document;
            match (arguments.get(1).map(String::as_str), arguments.get(2)) {
                (Some("remove"), Some(name)) => {
                    extra::remove_custom(document, name)?;
                    doc.session.revision += 1;
                    doc.session.modified = true;
                    Ok(Answer::said(format!("Removed {name}")))
                }
                (Some(name), Some(kind)) => {
                    // A kind of two words, `yes or no`, is written yes-or-no or as the two words quoted.
                    let kind = kind.replace('-', " ");
                    let value = rest(arguments, 3);
                    extra::set_custom(document, name, &kind, &value)?;
                    doc.session.revision += 1;
                    doc.session.modified = true;
                    Ok(Answer::said(format!("{name} is {value}")))
                }
                _ => {
                    let list: Vec<serde_json::Value> = extra::custom(document)
                        .into_iter()
                        .map(|c| json!({ "name": c.name, "kind": c.kind, "value": c.value }))
                        .collect();
                    Ok(Answer::said(format!("{} custom properties", list.len())).with(json!(list)))
                }
            }
        }
        Some(name) if arguments.len() > 1 => {
            let value = rest(arguments, 1);
            let document = &mut doc.session.document;
            if let Some((_, el)) = core.iter().find(|(n, _)| *n == name) {
                unluminous_word::new::set_property(document, el, &value);
            } else if let Some((_, el)) = app.iter().find(|(n, _)| *n == name) {
                extra::set_extended(document, el, &value);
            } else {
                return Err(format!("{name} is not a property: title, subject, author, manager, company, category, keywords, comments or status"));
            }
            doc.session.revision += 1;
            doc.session.modified = true;
            Ok(Answer::said(format!("{name} is {value}")))
        }
        _ => {
            let document = &doc.session.document;
            let mut values: serde_json::Map<String, serde_json::Value> = core
                .iter()
                .filter(|(n, _)| *n != "description")
                .map(|(n, el)| (n.to_string(), json!(unluminous_word::new::property(document, el))))
                .collect();
            for (name, el) in app {
                values.insert(name.to_owned(), json!(extra::extended(document, el)));
            }
            values.insert("description".into(), values["comments"].clone());
            Ok(Answer::said("Properties").with(serde_json::Value::Object(values)))
        }
    }
}

/// `protect`: what protects the document; `protect password <password>|none` encrypts it when saved,
/// `protect modify <password>|none` sets Password to Modify, `protect restrict <readOnly|comments|
/// trackedChanges|forms> <password>` is Restrict Editing, and `protect stop <password>` ends it.
pub(crate) fn protect(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    let session = &mut doc.session;
    match arguments.first().map(String::as_str) {
        Some("password") => {
            let password = rest(arguments, 1);
            let off = password.is_empty() || password == "none";
            doc.password = (!off).then_some(password);
            session.revision += 1;
            session.modified = true;
            Ok(Answer::said(if off {
                "The document is not encrypted when it is saved"
            } else {
                "The document is encrypted with the password when it is saved"
            }))
        }
        Some("modify") => {
            let password = rest(arguments, 1);
            let password = if password == "none" { String::new() } else { password };
            session.set_password_to_modify(&password);
            Ok(Answer::said(if password.is_empty() {
                "Password to Modify is off"
            } else {
                "Opening the document to change it needs the password"
            }))
        }
        Some("restrict") => {
            let kind = arguments.get(1).ok_or("protect restrict takes readOnly, comments, trackedChanges or forms, and a password")?;
            session.restrict_editing(kind, &rest(arguments, 2))?;
            Ok(Answer::said(format!("Restrict Editing allows {kind}")))
        }
        Some("stop") => {
            session.stop_protection(&rest(arguments, 1))?;
            Ok(Answer::said("Restrict Editing is off"))
        }
        Some(other) => {
            Err(format!("protect takes password, modify, restrict or stop, not {other}"))
        }
        None => {
            let modify = session
                .document
                .settings
                .as_ref()
                .is_some_and(|s| s.root.child("w:writeProtection").is_some());
            let value = json!({
                "encrypted": doc.password.is_some(),
                "passwordToModify": modify,
                "readOnly": session.read_only,
                "restrict": session.protection,
            });
            Ok(Answer::said("Protection").with(value))
        }
    }
}

/// The height of each row of the caret's table as the layout drew it, for Distribute Rows.
pub(crate) fn row_heights(doc: &Doc) -> Vec<f32> {
    let Some(range) = doc.session.selected_cells() else { return Vec::new() };
    let mut tops: Vec<(u32, f32, f32)> = Vec::new();
    for page in &doc.pages.pages {
        for line in page.carets.iter().filter(|l| l.story == StoryId::Body) {
            if line.path.len() > range.table.len() && line.path.starts_with(&range.table) {
                let row = line.path[range.table.len()];
                let bottom = line.top + line.height;
                match tops.iter_mut().find(|(r, _, _)| *r == row) {
                    Some(entry) => {
                        entry.1 = entry.1.min(line.top);
                        entry.2 = entry.2.max(bottom);
                    }
                    None => tops.push((row, line.top, bottom)),
                }
            }
        }
    }
    tops.sort_by_key(|(r, _, _)| *r);
    tops.iter().map(|(_, top, bottom)| bottom - top).collect()
}

fn section(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    let what = arguments.first().map(String::as_str).ok_or("section takes a property")?;
    let session = &mut doc.session;
    match what {
        "size" => session.set_page_size(arguments.get(1).ok_or("section size takes a size")?)?,
        "orientation" => {
            session.set_orientation(arguments.get(1).is_some_and(|o| o == "landscape"))
        }
        "margins" => {
            let margins = match arguments.get(1).map(String::as_str) {
                Some(name) if unluminous_word::ops::MARGINS.iter().any(|(n, _)| *n == name) => {
                    unluminous_word::ops::MARGINS.iter().find(|(n, _)| *n == name).expect("found").1
                }
                _ => {
                    let values: Vec<f32> = arguments[1..]
                        .iter()
                        .filter_map(|v| v.trim_end_matches("pt").parse().ok())
                        .collect();
                    let [top, bottom, left, right] = values[..] else {
                        return Err("margins takes a preset (normal, narrow, moderate, wide) or four numbers of points: top bottom left right".into());
                    };
                    [top, bottom, left, right]
                }
            };
            session.set_margins(margins);
        }
        "columns" => session.set_columns(
            arguments
                .get(1)
                .and_then(|n| n.parse().ok())
                .ok_or("section columns takes a number")?,
        ),
        "header" | "footer" | "header-first" | "footer-first" | "header-even" | "footer-even" => {
            let footer = what.starts_with("footer");
            let which = what.split_once('-').map_or("default", |(_, w)| w);
            session.set_header_text(footer, which, &unescape(&rest(arguments, 1)));
        }
        "page-numbers" => session.set_header_text(true, "default", "Page {PAGE} of {NUMPAGES}"),
        other => return Err(format!("section does not know `{other}`")),
    }
    Ok(Answer::said(format!("section {what} set")))
}

fn comment(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    let what = arguments.first().map(String::as_str).unwrap_or("list");
    let session = &mut doc.session;
    let id = || {
        arguments
            .get(1)
            .and_then(|i| i.parse::<i64>().ok())
            .ok_or("a comment id is needed: comment list shows them")
    };
    match what {
        "add" => {
            let made = session.add_comment(&rest(arguments, 1));
            Ok(Answer::said(format!("Comment {made} added")).with(json!({ "id": made })))
        }
        "reply" => {
            let made = session.reply_to_comment(id()?, &rest(arguments, 2))?;
            Ok(Answer::said(format!("Reply {made} added")).with(json!({ "id": made })))
        }
        "resolve" => {
            session.resolve_comment(id()?, true)?;
            Ok(Answer::said("Resolved"))
        }
        "reopen" => {
            session.resolve_comment(id()?, false)?;
            Ok(Answer::said("Open again"))
        }
        "delete" if arguments.get(1).map(String::as_str) == Some("all") => {
            let count = session.delete_all_comments();
            Ok(Answer::said(format!("{count} comments deleted")))
        }
        "delete" => {
            session.delete_comment(id()?)?;
            Ok(Answer::said("Deleted"))
        }
        "edit" => {
            session.edit_comment(id()?, &rest(arguments, 2))?;
            Ok(Answer::said("Comment changed"))
        }
        "next" | "previous" => {
            let found = session.go_to_comment(what == "next").ok_or("there are no comments")?;
            doc.view.reveal = true;
            Ok(Answer::said(format!("Comment {found}")).with(json!({ "id": found })))
        }
        "list" => {
            let comments = session.comments();
            Ok(Answer::said(format!("{} comments", comments.len())).with(json!(comments)))
        }
        other => {
            Err(format!("comment takes add, reply, edit, resolve, reopen, delete [all], next, previous or list, not {other}"))
        }
    }
}

/// Every tracked change in the body: what it is, who made it, and its text.
fn revisions(document: &unluminous_word::Document) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for (index, (_, p)) in position::paragraphs(&document.body).into_iter().enumerate() {
        let mut current: Option<(u64, &str, String, String)> = None;
        for run in &p.runs {
            let wrap = run.wraps.iter().find(|w| {
                matches!(w.el.name.as_str(), "w:ins" | "w:del" | "w:moveFrom" | "w:moveTo")
            });
            let text = match &run.content {
                unluminous_word::model::Content::Text(t) => t.clone(),
                _ => String::new(),
            };
            match (wrap, current.as_mut()) {
                (Some(w), Some(c)) if c.0 == w.id => c.3.push_str(&text),
                (Some(w), _) => {
                    if let Some((_, kind, author, text)) = current.take() {
                        out.push(json!({ "paragraph": index + 1, "kind": kind, "author": author, "text": text }));
                    }
                    let kind = if matches!(w.el.name.as_str(), "w:ins" | "w:moveTo") {
                        "insertion"
                    } else {
                        "deletion"
                    };
                    current = Some((
                        w.id,
                        kind,
                        w.el.attr("w:author").unwrap_or_default().to_owned(),
                        text.clone(),
                    ));
                }
                (None, _) => {
                    if let Some((_, kind, author, text)) = current.take() {
                        out.push(json!({ "paragraph": index + 1, "kind": kind, "author": author, "text": text }));
                    }
                }
            }
            if run.props.child("w:rPrChange").is_some() {
                out.push(json!({ "paragraph": index + 1, "kind": "formatting", "author": run.props.child("w:rPrChange").and_then(|c| c.attr("w:author")).unwrap_or_default(), "text": text }));
            }
        }
        if let Some((_, kind, author, text)) = current.take() {
            out.push(
                json!({ "paragraph": index + 1, "kind": kind, "author": author, "text": text }),
            );
        }
    }
    out
}

/// Insert a picture from a file at the caret, at its own size or `width` points wide.
pub fn insert_picture_file(
    doc: &mut Doc,
    path: &Path,
    width: Option<f32>,
) -> Result<String, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("{} could not be read: {e}", path.display()))?;
    let (w, h) = crate::services::picture::dimensions_of(&bytes)
        .ok_or_else(|| format!("{} is not a picture Unluminous can read", path.display()))?;
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    // Pixels at 96 to the inch, which is the size Word gives a picture with no resolution of its own, and no
    // wider than the text.
    let text_width = doc
        .session
        .section_at_caret()
        .map_or(468.0, |s| unluminous_word::layout::section::Geometry::of(&s).text_width());
    let natural = (w * 0.75, h * 0.75);
    let width = width.unwrap_or(natural.0).min(text_width).max(1.0);
    let height = width * natural.1 / natural.0.max(1.0);
    unluminous_word::media::insert_picture(
        &mut doc.session,
        &bytes,
        &extension,
        width,
        height,
        path.file_name().and_then(|n| n.to_str()).unwrap_or("Picture"),
    )?;
    Ok(format!("Inserted {}", path.display()))
}

/// Export the document, beside itself or to `to`.
/// The bytes of a PDF of `pages`, with the document's title and its fonts and pictures.
pub(crate) fn pdf_bytes(doc: &Doc, pages: &unluminous_word::layout::Pages) -> Vec<u8> {
    pdf_bytes_with(doc, pages, PdfHow { tagged: false, outline: false, pdfa: false })
}

/// What an exported PDF holds besides its pages.
#[derive(Debug, Clone, Copy)]
pub struct PdfHow {
    pub tagged: bool,
    pub outline: bool,
    pub pdfa: bool,
}

/// The bytes of a PDF of `pages` with the document's information, and its outline, tags and PDF/A
/// metadata as `how` asks (task-2359, TDD 5.6).
pub(crate) fn pdf_bytes_with(
    doc: &Doc,
    pages: &unluminous_word::layout::Pages,
    how: PdfHow,
) -> Vec<u8> {
    let document = &doc.session.document;
    let property = |name: &str| unluminous_word::new::property(document, name).unwrap_or_default();
    let title = unluminous_word::new::property(document, "dc:title")
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| doc.label());
    let outline = if how.outline {
        doc.session
            .toc_entries(6, pages)
            .into_iter()
            .filter_map(|(level, text, _, path)| {
                let (page, line) = pages.find(&Pos::new(path, 0))?;
                Some((level, text, page, line.top))
            })
            .collect()
    } else {
        Vec::new()
    };
    let options = unluminous_word::pdf::PdfOptions {
        title,
        author: property("dc:creator"),
        subject: property("dc:subject"),
        keywords: property("cp:keywords"),
        lang: doc.session.language_tag(),
        outline,
        structure: (how.tagged || how.pdfa)
            .then(|| unluminous_word::pdf::structure(document, pages)),
        pdfa: how.pdfa,
    };
    unluminous_word::pdf::write_with(pages, &options, &|style| pdf_face(style), &|part| {
        pdf_picture(document, part)
    })
}

pub fn export_to(doc: &mut Doc, kind: &str, to: Option<&Path>) -> Result<String, String> {
    export_with(doc, kind, to, PdfHow { tagged: true, outline: true, pdfa: false })
}

pub fn export_with(
    doc: &mut Doc,
    kind: &str,
    to: Option<&Path>,
    how: PdfHow,
) -> Result<String, String> {
    let path = to
        .map(Path::to_path_buf)
        .or_else(|| Word::export_path(doc, kind))
        .ok_or("the document has not been saved, so give the export a path")?;
    let bytes: Vec<u8> = match kind {
        "txt" | "text" => unluminous_word::export::to_text(&doc.session.document).into_bytes(),
        "md" | "markdown" => {
            unluminous_word::export::to_markdown(&doc.session.document).into_bytes()
        }
        "html" => unluminous_word::export::to_html(&doc.session.document).into_bytes(),
        "pdf" => {
            doc.ensure_pages();
            let pages = doc.pages.clone();
            pdf_bytes_with(doc, &pages, how)
        }
        other => return Err(format!("{other} is not a format: pdf, html, md or txt")),
    };
    crate::services::store::write_atomically(&path, &bytes)
        .map_err(|e| format!("{} could not be written: {e}", path.display()))?;
    Ok(format!("Exported {}", path.display()))
}

/// The font file a style prints with, found the same way the window finds it.
pub(crate) fn pdf_face(
    style: &unluminous_core::CharStyle,
) -> Option<unluminous_word::pdf::FaceBytes> {
    thread_local! {
        static DATABASE: std::cell::RefCell<Option<fontdb::Database>> = const { std::cell::RefCell::new(None) };
    }
    DATABASE.with(|cell| {
        let mut slot = cell.borrow_mut();
        let database = slot.get_or_insert_with(|| {
            let mut database = fontdb::Database::new();
            database.load_system_fonts();
            for folder in super::fonts::office_font_folders() {
                database.load_fonts_dir(folder);
            }
            database
        });
        let families = [fontdb::Family::Name(&style.family), fontdb::Family::SansSerif];
        let query = fontdb::Query {
            families: &families,
            weight: if style.bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            style: if style.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
            stretch: fontdb::Stretch::Normal,
        };
        let id = database.query(&query)?;
        database.with_face_data(id, |data, index| (std::sync::Arc::new(data.to_vec()), index))
    })
}

/// A picture's pixels for a PDF: a JPEG as it is, anything else decoded to RGB.
fn pdf_picture(
    document: &unluminous_word::Document,
    part: &str,
) -> Option<unluminous_word::pdf::Picture> {
    let bytes = document.package.part(part)?;
    let image = crate::services::picture::decode_bytes(bytes).ok()?;
    let (width, height) = (image.size[0] as u32, image.size[1] as u32);
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return Some(unluminous_word::pdf::Picture::Jpeg { width, height, bytes: bytes.to_vec() });
    }
    // Transparency is laid over white, which is the page a picture sits on.
    let pixels: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|p| {
            let a = u16::from(p.a());
            let blend = |c: u8| ((u16::from(c) * a + 255 * (255 - a)) / 255) as u8;
            [blend(p.r()), blend(p.g()), blend(p.b())]
        })
        .collect();
    Some(unluminous_word::pdf::Picture::Rgb { width, height, pixels })
}

/// The provider as data, for `plugins view word`.
pub fn view(word: &Word) -> serde_json::Value {
    let current = word.current.as_ref().and_then(|key| word.docs.iter().find(|d| &d.key == key));
    json!({
        "docs": word.docs.iter().map(|d| json!({ "key": d.key, "path": d.path, "modified": d.modified() })).collect::<Vec<_>>(),
        "current": current.map(|d| json!({
            "key": d.key,
            "pages": d.pages.pages.len(),
            "laidOutWith": d.formatted_text,
            "drawn": d.drawn,
            "keyboard": d.keyboard.0,
            "fieldHasTheKeyboard": d.keyboard.1,
            "zoom": d.view.zoom,
            "scroll": d.view.scroll,
            "panel": d.view.panel.iter().map(|l| l.name()).collect::<Vec<_>>(),
            "slash": d.view.slash.as_ref().map(|(_, q, _)| q.clone()),
            "palette": d.view.palette.as_ref().map(|(q, _)| q.clone()),
            "find": d.view.find.as_ref().map(|f| f.needle.clone()),
            "selection": selection_value(&d.session.selection),
            "selected": d.session.selected_text(),
            "tracking": d.session.tracking,
            "marks": d.view.marks,
            "navigation": d.view.navigation,
            "canUndo": d.session.can_undo(),
            "canRedo": d.session.can_redo(),
            "mode": match d.view.mode { super::Mode::Print => "print", super::Mode::Read => "read", super::Mode::Outline => "outline" },
            "ruler": d.view.ruler,
            "rulerMarks": d.view.ruler_marks.map(|m| m.map(|v| v * d.view.window_zoom)),
            "multiple": d.view.multiple,
            "contextMenu": d.view.context_menu.as_ref().map(|m| m.entries.iter().flat_map(|e| std::iter::once(e).chain(e.children.iter())).map(|e| e.label.clone()).collect::<Vec<_>>()),
            "object": d.session.object_info().map(|info| {
                let screen = crate::components::word::objects::selected(d).and_then(|(page, rect)| {
                    crate::components::word::objects::to_screen(d, d.view.pages_rect, page, rect)
                });
                json!({
                    "kind": info.kind, "width": info.width, "height": info.height, "wrap": info.wrap, "alt": info.alt,
                    "screen": screen.map(|r| { let z = d.view.window_zoom; [r.left() * z, r.top() * z, r.width() * z, r.height() * z] }),
                })
            }),
            "cropping": d.view.cropping,
        })),
        "author": word.config.author,
    })
}

/// `spelling`: every underlined word in the document, with what the dictionary suggests for it.
/// `spelling add <word>` adds a word to the dictionary, `spelling ignore <word>` stops underlining it for
/// the session, and `spelling next` selects the next underlined word after the caret, as F7 does.
fn spelling(doc: &mut Doc, arguments: &[String]) -> Result<Answer, String> {
    use super::spelling as speller;
    match arguments.first().map(String::as_str) {
        Some("add") => {
            let word = rest(arguments, 1);
            speller::add_word(&word)?;
            Ok(Answer::said(format!("Added {word} to the dictionary")))
        }
        Some("ignore") => {
            let word = rest(arguments, 1);
            speller::ignore(&word);
            Ok(Answer::said(format!("Ignoring {word}")))
        }
        Some("next") => {
            let said = speller::next(doc)?;
            let said = if said.is_empty() { doc.session.selected_text() } else { said };
            Ok(Answer::said(said).with(json!({ "selection": selection_value(&doc.session.selection), "text": doc.session.selected_text() })))
        }
        Some(other) => Err(format!("spelling takes add, ignore or next, not {other}")),
        None => {
            let body = position::paragraphs(&doc.session.document.body);
            let found: Vec<serde_json::Value> = speller::misspellings(&doc.session.document)
                .into_iter()
                .map(|m| {
                    let paragraph = (m.story == StoryId::Body).then(|| body.iter().position(|(p, _)| *p == m.path).map(|i| i + 1)).flatten();
                    let (kind, suggestions) = match m.kind {
                        unluminous_word::spell::Kind::Spelling => ("spelling", speller::suggestions(&m.word, 5)),
                        unluminous_word::spell::Kind::Repeated => ("repeated", Vec::new()),
                    };
                    json!({ "story": story_name(&m.story), "paragraph": paragraph, "path": m.path, "offset": m.range.start, "word": m.word, "kind": kind, "suggestions": suggestions })
                })
                .collect();
            Ok(Answer::said(format!("{} spelling errors", found.len())).with(json!(found)))
        }
    }
}

/// A story as an agent names it: `body`, `header:<id>`, `footnote:<n>`, `endnote:<n>` or `comment:<n>`.
fn story_name(story: &StoryId) -> String {
    match story {
        StoryId::Body => "body".to_owned(),
        StoryId::Header(id) => format!("header:{id}"),
        StoryId::Footnote(n) => format!("footnote:{n}"),
        StoryId::Endnote(n) => format!("endnote:{n}"),
        StoryId::Comment(n) => format!("comment:{n}"),
        StoryId::TextBox(n) => format!("textbox:{n}"),
    }
}
