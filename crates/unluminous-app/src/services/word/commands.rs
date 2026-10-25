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

fn words(arguments: &[String]) -> Vec<String> {
    arguments.iter().filter(|a| !a.starts_with("--")).cloned().collect()
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

/// The body paragraph numbered `n` from 1, as `read` numbers them.
fn paragraph_path(doc: &Doc, n: usize) -> Result<Vec<u32>, String> {
    let all = position::paragraphs(&doc.session.document.body);
    let count = all.len();
    all.into_iter()
        .nth(n.saturating_sub(1))
        .map(|(path, _)| path)
        .ok_or_else(|| format!("there is no paragraph {n}: the document has {count}"))
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
            let path = arguments.first().map(PathBuf::from);
            let key = word.new_document(path.as_deref())?;
            let label = word.doc(&key)?.label();
            word.outbox.push(Request::OpenKeyedTab { key: key.clone(), label: label.clone() });
            return Ok(Answer::said(format!("New document {label}")).with(json!({ "key": key })));
        }
        "open" => {
            let path = PathBuf::from(rest(arguments, 0));
            let path = match (path.is_relative(), &word.project) {
                (true, Some(project)) => project.join(path),
                _ => path,
            };
            if !path.is_file() {
                return Err(format!("{} is not a file", path.display()));
            }
            word.outbox.push(Request::OpenFile(path.clone()));
            return Ok(Answer::said(format!("Opening {}", path.display()))
                .with(json!({ "key": super::tab_key(&path) })));
        }
        "docs" => {
            let docs: Vec<serde_json::Value> = word
                .docs
                .iter()
                .map(|d| json!({ "key": d.key, "path": d.path, "modified": d.modified(), "pages": d.pages.pages.len(), "current": word.current.as_deref() == Some(d.key.as_str()) }))
                .collect();
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
        _ => {}
    }
    let doc = word.current_doc()?;
    document_command(doc, command, arguments)
}

/// A command about the current document.
fn document_command(doc: &mut Doc, command: &str, arguments: &[String]) -> Result<Answer, String> {
    let session = &mut doc.session;
    match command {
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
            let wanted = rest(arguments, 0);
            let id = {
                let resolver = unluminous_word::styles::Resolver::new(&session.document);
                resolver.find(&wanted).map(str::to_owned)
            }
            .unwrap_or_else(|| wanted.replace(' ', ""));
            session.apply_style(&id)?;
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
                other => {
                    return Err(format!(
                        "list takes bullet, number, deeper, shallower or restart, not {other:?}"
                    ))
                }
            }
            Ok(Answer::said("Done"))
        }
        "table" => table(doc, arguments),
        "section" => section(doc, arguments),
        "comment" => comment(doc, arguments),
        "track" => {
            match arguments.first().map(String::as_str) {
                Some("on") => session.set_tracking(true),
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
                Some(other) => {
                    return Err(format!("track takes on, off, list, accept or reject, not {other}"))
                }
            }
            Ok(Answer::said(format!(
                "Track Changes is {}",
                if session.tracking { "on" } else { "off" }
            )))
        }
        "fields" => {
            doc.ensure_pages();
            let pages = doc.pages.clone();
            let count = doc.session.update_fields(&pages);
            Ok(Answer::said(format!("{count} fields updated")))
        }
        "undo" => Ok(Answer::said(if session.undo() { "Undone" } else { "Nothing to undo" })),
        "redo" => Ok(Answer::said(if session.redo() { "Redone" } else { "Nothing to redo" })),
        "export" => {
            let kind =
                arguments.first().map(String::as_str).ok_or("export takes pdf, html, md or txt")?;
            let to = arguments.get(1).map(PathBuf::from);
            let said = export_to(doc, kind, to.as_deref())?;
            Ok(Answer::said(said))
        }
        "properties" => {
            let names = [
                ("title", "dc:title"),
                ("subject", "dc:subject"),
                ("author", "dc:creator"),
                ("keywords", "cp:keywords"),
                ("description", "dc:description"),
            ];
            if let (Some(name), true) = (arguments.first(), arguments.len() > 1) {
                let el = names.iter().find(|(n, _)| n == name).map(|(_, el)| *el).ok_or_else(|| format!("{name} is not a property: title, subject, author, keywords or description"))?;
                let value = rest(arguments, 1);
                unluminous_word::new::set_property(&mut session.document, el, &value);
                session.revision += 1;
                session.modified = true;
                return Ok(Answer::said(format!("{name} is {value}")));
            }
            let values: serde_json::Map<String, serde_json::Value> = names
                .iter()
                .map(|(n, el)| {
                    (n.to_string(), json!(unluminous_word::new::property(&session.document, el)))
                })
                .collect();
            Ok(Answer::said("Properties").with(serde_json::Value::Object(values)))
        }
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
                _ => {}
            }
            Ok(Answer::said("View").with(json!({ "zoom": doc.view.zoom, "marks": doc.view.marks, "navigation": doc.view.navigation })))
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
                    doc.view.panel = if level == Level::Root {
                        vec![Level::Root]
                    } else {
                        vec![Level::Root, level]
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
            doc.session.insert_toc(3, &pages);
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
        other => return Err(format!("insert does not know `{other}`: paragraph, page-break, line-break, column-break, section-break, table, picture, footnote, endnote, toc, field, link, bookmark, date, page-number, symbol")),
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
                    "{name} is not one of Word's highlight colours: {}",
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
    let what = arguments.first().map(String::as_str).ok_or("paragraph takes a property")?;
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
        op => session.table_op(op)?,
    }
    Ok(Answer::said(format!("table {what}")))
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
        "delete" => {
            session.delete_comment(id()?)?;
            Ok(Answer::said("Deleted"))
        }
        "list" => {
            let comments = session.comments();
            Ok(Answer::said(format!("{} comments", comments.len())).with(json!(comments)))
        }
        other => {
            Err(format!("comment takes add, reply, resolve, reopen, delete or list, not {other}"))
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
pub fn export_to(doc: &mut Doc, kind: &str, to: Option<&Path>) -> Result<String, String> {
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
            let title = unluminous_word::new::property(&doc.session.document, "dc:title")
                .unwrap_or_else(|| doc.label());
            let document = &doc.session.document;
            unluminous_word::pdf::write(&doc.pages, &title, &|style| pdf_face(style), &|part| {
                pdf_picture(document, part)
            })
        }
        other => return Err(format!("{other} is not a format: pdf, html, md or txt")),
    };
    crate::services::store::write_atomically(&path, &bytes)
        .map_err(|e| format!("{} could not be written: {e}", path.display()))?;
    Ok(format!("Exported {}", path.display()))
}

/// The font file a style prints with, found the same way the window finds it.
pub(crate) fn pdf_face(style: &unluminous_core::CharStyle) -> Option<unluminous_word::pdf::FaceBytes> {
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
    }
}
