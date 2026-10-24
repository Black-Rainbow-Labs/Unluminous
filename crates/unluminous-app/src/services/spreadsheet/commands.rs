//! The words an agent drives a workbook with, `unluminous-cli plugins run spreadsheet <verb> ...`, and the
//! menu entries' commands. Each verb chooses the cells it names, the way a person would click them, and
//! then makes the same [`Act`] the grid makes, so the two cannot do different things.

use serde_json::{json, Value as Json};
use unluminous_sheet::coord::{column_name, column_number, CellRef, Range};
use unluminous_sheet::edit::{clip_to_used, Clear, Direction, PasteMode};
use unluminous_sheet::model::{CfRule, Cfvo, ChartKind, FilterCriteria, Validation, Visibility};
use unluminous_sheet::styles::{AlignEdit, Color, FontEdit, StyleChange, Underline};
use unluminous_sheet::Value;

use super::actions::{apply, Act, BorderSides};
use super::{display, Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Every verb, with one line each, from the table the command line and the MCP tool are made from.
pub const LIST: &[(&str, &str)] = unluminous_cli::plugins::SPREADSHEET;

fn said(message: impl Into<String>) -> Answer {
    Answer::said(message)
}

/// The sheet and range a word names: `A1`, `B2:C9`, `A:C`, `3:5`, `Sheet2!A1:B2`, `'My sheet'!A1`, or a name.
pub fn parse_range(book: &Book, text: &str) -> Option<(usize, Range)> {
    let text = text.trim();
    let (sheet, rest) = match text.rsplit_once('!') {
        Some((name, rest)) => {
            let name = name.trim_matches('\'').replace("''", "'");
            (book.session.book.sheet_index(&name)?, rest)
        }
        None => (book.view.sheet, text),
    };
    if let Some(range) = Range::parse(&rest.replace('$', "")) {
        return Some((sheet, range));
    }
    // A defined name or a table.
    let defined = book.session.book.name(rest, book.view.sheet)?;
    let to = defined.formula.trim_start_matches('=');
    let (name, area) = to.rsplit_once('!')?;
    let index = book.session.book.sheet_index(name.trim_matches('\''))?;
    Some((index, Range::parse(&area.replace('$', ""))?))
}

/// Choose the range the first word names, if it names one, and answer the words after it.
pub fn take_range<'a>(book: &mut Book, args: &'a [String]) -> &'a [String] {
    if let Some(first) = args.first() {
        let ranges: Option<Vec<(usize, Range)>> =
            first.split(',').map(|part| parse_range(book, part)).collect();
        if let Some(ranges) = ranges.filter(|r| !r.is_empty()) {
            let sheet = ranges[0].0;
            if sheet != book.view.sheet {
                book.view.sheet = sheet;
                book.view.edit = None;
            }
            let chosen: Vec<Range> = ranges.into_iter().map(|(_, r)| r).collect();
            let last = *chosen.last().expect("not empty");
            book.view.held = None;
            book.view.extra = chosen[..chosen.len() - 1].to_vec();
            book.view.cursor = last.first;
            book.view.anchor = last.last;
            book.view.reveal = true;
            return &args[1..];
        }
    }
    args
}

pub fn act(sheet: &mut Spreadsheet, act: Act) -> Result<Answer, String> {
    let mut copied = sheet.clip.take();
    let result = match sheet.current_book() {
        Ok(book) => apply(book, &mut copied, act),
        Err(e) => Err(e),
    };
    sheet.clip = copied;
    result.map(said)
}

/// Run a verb.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Result<Answer, String> {
    match verb {
        "open" => {
            let path = args.first().ok_or("open needs a file")?;
            let name = sheet.open_path(std::path::Path::new(path), true)?;
            Ok(said(format!("Opened {name}")).with(book_value(sheet.current_book()?)))
        }
        "new" => {
            let path = args.first().ok_or("new needs a file name, such as budget.xlsx")?;
            let full = match (std::path::Path::new(path).is_absolute(), &sheet.project) {
                (false, Some(project)) => project.join(path),
                _ => std::path::PathBuf::from(path),
            };
            if full.exists() {
                return Err(format!("{} already exists; `open` opens it", full.display()));
            }
            let key = super::key_of_path(&full, sheet.project.as_deref());
            let book = sheet.book_for_key(&key)?;
            book.save()?;
            sheet.current = Some(key);
            sheet.ask(crate::services::plugin_ui::Request::OpenFile(full.clone()));
            Ok(said(format!("Made {}", full.display())))
        }
        "books" => {
            let current = sheet.current.clone();
            let list: Vec<Json> = sheet
                .books
                .iter()
                .map(|b| json!({ "file": b.path.display().to_string(), "modified": b.session.modified, "current": Some(&b.key) == current.as_ref() }))
                .collect();
            Ok(said(format!("{} open", list.len())).with(json!(list)))
        }
        "use" => {
            let path = args.first().ok_or("use needs a file")?;
            let name = sheet.open_path(std::path::Path::new(path), false)?;
            Ok(said(format!("Acting on {name}")))
        }
        "save" => {
            let book = sheet.current_book()?;
            Ok(said(book.save()?))
        }
        "save-as" => {
            let path = args.first().ok_or("save-as needs a file")?;
            let full = match (std::path::Path::new(path).is_absolute(), &sheet.project) {
                (false, Some(project)) => project.join(path),
                _ => std::path::PathBuf::from(path),
            };
            let kind = unluminous_sheet::FileKind::of(&full)?;
            let key = super::key_of_path(&full, sheet.project.as_deref());
            let project = sheet.project.clone();
            let book = sheet.current_book()?;
            book.path = full.clone();
            book.kind = kind;
            book.key = key.clone();
            let message = book.save()?;
            let _ = project;
            sheet.current = Some(key);
            sheet.ask(crate::services::plugin_ui::Request::OpenFile(full));
            Ok(said(message))
        }
        "sheets" => {
            let book = sheet.current_book()?;
            let list: Vec<Json> = book
                .session
                .book
                .sheets
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    json!({
                        "name": s.name,
                        "showing": i == book.view.sheet,
                        "hidden": s.state != Visibility::Visible,
                        "used": s.used_range().map(|r| r.a1()),
                    })
                })
                .collect();
            Ok(said(format!("{} sheets", list.len())).with(json!(list)))
        }
        "sheet" => sheet_verb(sheet, args),
        "select" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            if rest.len() == args.len() {
                return Err(format!(
                    "`{}` is not a range",
                    args.first().map_or("", String::as_str)
                ));
            }
            Ok(said(format!("Chose {}", book.view.range().a1())).with(selection_value(book)))
        }
        "selection" => {
            let book = sheet.current_book()?;
            Ok(said(book.view.range().a1()).with(selection_value(book)))
        }
        // `cell` and `read` answer questions, so they look at the cells they name without choosing them.
        "cell" => {
            let book = sheet.current_book()?;
            let (index, at) = match args.first().and_then(|a| parse_range(book, a)) {
                Some((index, range)) => (index, range.first),
                None => (book.view.sheet, book.view.cursor),
            };
            Ok(said(at.a1()).with(cell_value_on(book, index, at)))
        }
        "read" => {
            let book = sheet.current_book()?;
            let named = args.first().and_then(|a| parse_range(book, a));
            let rest = if named.is_some() { &args[1..] } else { args };
            let what = rest.first().map_or("values", String::as_str);
            let (index, range) = match named {
                Some((index, range)) => {
                    (index, clip_to_used(&book.session.book.sheets[index], range))
                }
                None => (
                    book.view.sheet,
                    book.session.book.sheets[book.view.sheet]
                        .used_range()
                        .unwrap_or(Range::cell(CellRef::new(0, 0))),
                ),
            };
            Ok(said(range.a1()).with(read_value(book, index, range, what)?))
        }
        "write" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            if rest.len() == args.len() {
                return Err("write needs a cell first, such as `write B2 42`".to_owned());
            }
            let text = rest.join(" ");
            let rows = rows_of(&text);
            let at = book.view.cursor;
            act(sheet, Act::TypeRows { at, rows })?;
            let book = sheet.current_book()?;
            Ok(said(format!("Wrote {}", book.view.range().a1())).with(selection_value(book)))
        }
        "type" => {
            let book = sheet.current_book()?;
            let at = book.view.cursor;
            let text = args.join(" ");
            act(sheet, Act::Type { at, text })?;
            let book = sheet.current_book()?;
            let below = book.view.cursor.offset(1, 0).unwrap_or(at);
            book.view.select_cell(below);
            Ok(said(format!("Typed into {}", at.a1())).with(cell_value(book, at)))
        }
        "formula" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            if rest.len() == args.len() || rest.is_empty() {
                return Err(
                    "formula needs a cell and a formula, such as `formula C2 =A2*B2`".to_owned()
                );
            }
            let mut text = rest.join(" ");
            if !text.starts_with('=') {
                text.insert(0, '=');
            }
            let at = book.view.cursor;
            act(sheet, Act::Type { at, text })?;
            let book = sheet.current_book()?;
            Ok(said(at.a1()).with(cell_value(book, at)))
        }
        "eval" => {
            let book = sheet.current_book()?;
            let text = args.join(" ");
            let value = book.session.evaluate_at(book.view.sheet, book.view.cursor, &text)?;
            Ok(said(value.to_text()).with(value_json(&value)))
        }
        "clear" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            let what = match rest.first().map(String::as_str).unwrap_or("contents") {
                "contents" => Clear::Contents,
                "formats" => Clear::Formats,
                "all" => Clear::All,
                "notes" => Clear::Notes,
                "links" => Clear::Links,
                other => {
                    return Err(format!(
                        "clear takes contents, formats, all, notes or links, not `{other}`"
                    ))
                }
            };
            act(sheet, Act::Clear(what))
        }
        "format" => {
            let book = sheet.current_book()?;
            let mut rest = take_range(book, args).to_vec();
            if rest.is_empty() {
                return Err(
                    "format needs what to change, such as `format A1:B2 bold fill #FFF2CC`"
                        .to_owned(),
                );
            }
            rest = relative_format_words(book, rest);
            for change in style_changes(&rest)? {
                act(sheet, Act::Style(change))?;
            }
            let book = sheet.current_book()?;
            Ok(said(format!("Formatted {}", book.view.range().a1()))
                .with(cell_value(book, book.view.cursor)))
        }
        "decimals" => {
            let more = match args.first().map(String::as_str) {
                Some("more") => true,
                Some("fewer") | Some("less") => false,
                _ => return Err("decimals takes more or fewer".to_owned()),
            };
            let book = sheet.current_book()?;
            take_range(book, &args[1..]);
            let s = &book.session.book;
            let at = book.view.cursor;
            let code = s.styles.format_code(display::style_at(&s.sheets[book.view.sheet], at));
            let value = s.sheets[book.view.sheet].value_at(at);
            act(sheet, Act::Style(StyleChange::NumberFormat(step_decimals(&code, &value, more))))
        }
        "indent" => {
            let more = args.first().map(String::as_str) != Some("less");
            let book = sheet.current_book()?;
            take_range(book, &args[1.min(args.len())..]);
            let s = &book.session.book;
            let indent = s
                .styles
                .xf(display::style_at(&s.sheets[book.view.sheet], book.view.cursor))
                .align
                .indent;
            let next = if more { indent + 1 } else { indent.saturating_sub(1) };
            act(
                sheet,
                Act::Style(StyleChange::Align(AlignEdit {
                    indent: Some(next),
                    horizontal: (more && next > 0).then(|| Some("left".to_owned())),
                    ..Default::default()
                })),
            )
        }
        "find-bar" => {
            let book = sheet.current_book()?;
            let mut find = book.view.find.clone().unwrap_or_default();
            find.focus = true;
            if args.first().map(String::as_str) == Some("replace") {
                find.replace.get_or_insert_with(String::new);
            }
            book.view.find = Some(find);
            Ok(said(String::new()))
        }
        "border" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args).to_vec();
            let sides = rest.first().and_then(|w| BorderSides::parse(w)).ok_or(
                "border takes all, outline, inside, top, bottom, left, right, thick or none",
            )?;
            let style = rest
                .get(1)
                .filter(|w| !w.starts_with('#'))
                .cloned()
                .unwrap_or_else(|| "thin".to_owned());
            let color = rest
                .iter()
                .find_map(|w| w.starts_with('#').then(|| Color::parse_hex(w)).flatten())
                .unwrap_or(Color::Auto);
            act(sheet, Act::Border { sides, style, color })
        }
        "merge" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            let center = rest.first().is_some_and(|w| w == "center");
            act(sheet, Act::Merge { center })
        }
        "unmerge" => {
            let book = sheet.current_book()?;
            take_range(book, args);
            act(sheet, Act::Unmerge)
        }
        "width" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            match rest.first().map(String::as_str) {
                Some("auto") => act(sheet, Act::AutoFitColumns),
                Some("default") => act(sheet, Act::Width(None)),
                Some(n) => {
                    let chars: f64 =
                        n.parse().map_err(|_| format!("`{n}` is not a width in characters"))?;
                    // A width a person types is in characters as Excel's dialog shows them; the file adds the
                    // padding, five pixels.
                    act(
                        sheet,
                        Act::Width(Some(super::axis::points_to_width(
                            (chars * 7.0 + 5.0).round() as f32
                        ))),
                    )
                }
                None => Err("width needs a number of characters, auto or default".to_owned()),
            }
        }
        "height" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            match rest.first().map(String::as_str) {
                Some("default") => act(sheet, Act::Height(None)),
                Some("auto") => act(sheet, Act::AutoFitRows),
                Some(n) => act(
                    sheet,
                    Act::Height(Some(
                        n.parse().map_err(|_| format!("`{n}` is not a height in points"))?,
                    )),
                ),
                None => Err("height needs points or default".to_owned()),
            }
        }
        "hide" | "unhide" => {
            let rows = match args.first().map(String::as_str) {
                Some("rows") | Some("row") => true,
                Some("columns") | Some("column") | Some("cols") => false,
                _ => return Err(format!("{verb} takes rows or columns first")),
            };
            let book = sheet.current_book()?;
            take_range(book, &args[1..]);
            act(sheet, if verb == "hide" { Act::Hide { rows } } else { Act::Unhide { rows } })
        }
        "insert" | "delete" => {
            if args.first().map(String::as_str) == Some("ask") {
                let book = sheet.current_book()?;
                return super::verbs::ask_which_way(book, verb == "insert");
            }
            let Some(shift) = super::verbs::shift_word(verb, args.first().map(String::as_str))
            else {
                return Err(format!(
                    "{verb} takes rows, columns, ask, or which way the cells beside move"
                ));
            };
            let book = sheet.current_book()?;
            take_range(book, &args[1..]);
            act(sheet, if verb == "insert" { Act::Insert(shift) } else { Act::Delete(shift) })
        }
        "fill" => {
            if args.first().map(String::as_str) == Some("series") {
                let book = sheet.current_book()?;
                let source = args
                    .get(1)
                    .and_then(|a| parse_range(book, a))
                    .ok_or("fill series needs the source cells and the target range")?;
                let target = args
                    .get(2)
                    .and_then(|a| parse_range(book, a))
                    .ok_or("fill series needs the target range")?;
                book.view.select_range(source.1);
                return act(sheet, Act::FillTo(target.1));
            }
            let direction = match args.first().map(String::as_str) {
                Some("down") => Direction::Down,
                Some("right") => Direction::Right,
                Some("up") => Direction::Up,
                Some("left") => Direction::Left,
                _ => return Err("fill takes down, right, up, left or series".to_owned()),
            };
            let book = sheet.current_book()?;
            take_range(book, &args[1..]);
            act(sheet, Act::Fill(direction))
        }
        "sort" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            let column = rest.iter().find_map(|w| column_number(w));
            let ascending = !rest.iter().any(|w| w == "desc" || w == "descending");
            act(sheet, Act::Sort { column, ascending })
        }
        "filter" => filter_verb(sheet, args),
        "remove-duplicates" => {
            let book = sheet.current_book()?;
            take_range(book, args);
            act(sheet, Act::RemoveDuplicates)
        }
        "split" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            let delimiter = rest.first().cloned().unwrap_or_else(|| ",".to_owned());
            let delimiter = if delimiter == "tab" {
                "\t".to_owned()
            } else if delimiter == "space" {
                " ".to_owned()
            } else {
                delimiter
            };
            act(sheet, Act::TextToColumns(delimiter))
        }
        "copy" | "cut" => {
            let book = sheet.current_book()?;
            take_range(book, args);
            let answer = act(sheet, Act::Copy { cut: verb == "cut" })?;
            if let Some(copied) = &sheet.clip {
                sheet.ask(crate::services::plugin_ui::Request::Copy(copied.text.clone()));
            }
            Ok(said(format!(
                "{} {}",
                if verb == "cut" { "Cut" } else { "Copied" },
                sheet.current_book()?.view.range().a1()
            ))
            .with(json!(answer.message)))
        }
        "paste" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            let mode = match rest.first().map(String::as_str).unwrap_or("all") {
                "all" => PasteMode::All,
                "values" => PasteMode::Values,
                "formulas" => PasteMode::Formulas,
                "formats" => PasteMode::Formats,
                "transpose" => PasteMode::Transpose,
                other => {
                    return Err(format!(
                        "paste takes all, values, formulas, formats or transpose, not `{other}`"
                    ))
                }
            };
            act(sheet, Act::Paste(mode))
        }
        "find" => {
            let text = args.first().ok_or("find needs text")?.clone();
            let flags: Vec<&str> = args[1..].iter().map(String::as_str).collect();
            let book = sheet.current_book()?;
            let found = book.session.find(
                if flags.contains(&"book") { None } else { Some(book.view.sheet) },
                &text,
                flags.contains(&"formulas"),
                flags.contains(&"whole"),
                flags.contains(&"case"),
            );
            if let Some((s, at)) = found.first() {
                book.view.sheet = *s;
                book.view.select_cell(*at);
            }
            let list: Vec<String> = found
                .iter()
                .map(|(s, at)| format!("{}!{}", book.session.book.sheets[*s].name, at.a1()))
                .collect();
            Ok(said(format!("{} found", list.len())).with(json!(list)))
        }
        "replace" => {
            let find =
                args.first().ok_or("replace needs what to find and what to put instead")?.clone();
            let with = args.get(1).cloned().unwrap_or_default();
            let flags: Vec<&str> = args[2.min(args.len())..].iter().map(String::as_str).collect();
            act(
                sheet,
                Act::Replace {
                    find,
                    with,
                    case: flags.contains(&"case"),
                    whole_book: flags.contains(&"book"),
                },
            )
        }
        "undo" => act(sheet, Act::Undo),
        "redo" => act(sheet, Act::Redo),
        "repeat" => act(sheet, Act::Repeat { redo_first: false }),
        "freeze" => match args.first().map(String::as_str) {
            Some("off") | Some("none") => act(sheet, Act::Unfreeze),
            Some("rows") => act(sheet, Act::FreezeAt { rows: number(args.get(1))?, cols: 0 }),
            Some("columns") => act(sheet, Act::FreezeAt { rows: 0, cols: number(args.get(1))? }),
            _ => {
                let book = sheet.current_book()?;
                take_range(book, args);
                act(sheet, Act::Freeze)
            }
        },
        "gridlines" => act(sheet, Act::Gridlines(args.first().map(String::as_str) != Some("off"))),
        "paper" => {
            let paper = args
                .first()
                .and_then(|w| super::config::Paper::parse(w))
                .ok_or("paper takes theme or light")?;
            sheet.config.paper = paper;
            for book in &mut sheet.books {
                book.view.paper = paper;
            }
            sheet.write_the_settings()?;
            Ok(said(format!("Sheets are drawn on {} paper", paper.name())))
        }
        "zoom" => {
            let percent: f32 = args
                .first()
                .and_then(|a| a.trim_end_matches('%').parse().ok())
                .ok_or("zoom needs a percentage")?;
            act(sheet, Act::Zoom(percent / 100.0))
        }
        "name" => match args.first().map(String::as_str) {
            Some("delete") => act(
                sheet,
                Act::Name {
                    name: args.get(1).ok_or("name delete needs a name")?.clone(),
                    refers_to: None,
                },
            ),
            Some(name) => {
                let book = sheet.current_book()?;
                let refers_to = match args.get(1) {
                    Some(to) => to.clone(),
                    None => {
                        let s = &book.session.book.sheets[book.view.sheet];
                        let r = book.view.range();
                        format!(
                            "{}!{}",
                            unluminous_sheet::formula::print::sheet_name(&s.name),
                            absolute(r)
                        )
                    }
                };
                act(sheet, Act::Name { name: name.to_owned(), refers_to: Some(refers_to) })
            }
            None => {
                let book = sheet.current_book()?;
                let list: Vec<Json> = book
                    .session
                    .book
                    .names
                    .iter()
                    .map(|n| json!({ "name": n.name, "refers_to": n.formula }))
                    .collect();
                Ok(said(format!("{} names", list.len())).with(json!(list)))
            }
        },
        "validate" => validate_verb(sheet, args),
        "conditional" => conditional_verb(sheet, args),
        "note" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            let text = rest.join(" ");
            act(sheet, Act::Note((text != "clear" && !text.is_empty()).then_some(text)))
        }
        "link" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            match rest.first().map(String::as_str) {
                Some("clear") | None => act(sheet, Act::Link { target: None, text: None }),
                Some(target) => act(
                    sheet,
                    Act::Link {
                        target: Some(target.to_owned()),
                        text: (rest.len() > 1).then(|| rest[1..].join(" ")),
                    },
                ),
            }
        }
        "table" => {
            let book = sheet.current_book()?;
            let rest = take_range(book, args);
            act(sheet, Act::Table(rest.first().cloned()))
        }
        "chart" if args.first().map(String::as_str) == Some("delete") => {
            let n: usize = args
                .get(1)
                .and_then(|n| n.parse().ok())
                .ok_or("chart delete needs the chart's number, from 1")?;
            let book = sheet.current_book()?;
            let index = book.view.sheet;
            book.session.delete_chart(index, n.saturating_sub(1))?;
            book.changed();
            Ok(said(format!("Deleted chart {n}")))
        }
        "chart" => {
            let kind = args
                .first()
                .and_then(|k| ChartKind::parse(k))
                .ok_or("chart takes column, bar, line, area, pie, doughnut or scatter")?;
            let book = sheet.current_book()?;
            let rest = take_range(book, &args[1..]);
            act(sheet, Act::Chart { kind, title: (!rest.is_empty()).then(|| rest.join(" ")) })
        }
        "autosum" => {
            let book = sheet.current_book()?;
            take_range(book, args);
            act(sheet, Act::AutoSum)
        }
        "recalc" => act(sheet, Act::Recalculate),
        "functions" => {
            let prefix = args.first().map(|p| p.to_ascii_uppercase()).unwrap_or_default();
            let list: Vec<Json> = unluminous_sheet::functions::all()
                .iter()
                .filter(|f| f.name.starts_with(&prefix))
                .map(|f| json!({ "name": f.name, "arguments": f.args, "about": f.about, "category": f.category }))
                .collect();
            Ok(said(format!("{} functions", list.len())).with(json!(list)))
        }
        "panel" => {
            let book = sheet.current_book()?;
            match args.first().map(String::as_str) {
                Some("close") => book.view.panel = None,
                _ => book.view.panel = Some(super::view::PanelPath(args.to_vec())),
            }
            Ok(said(String::new()).with(json!(book.view.panel.as_ref().map(|p| p.0.clone()))))
        }
        "menu" => {
            let book = sheet.current_book()?;
            match args.first().map(String::as_str) {
                Some("close") => book.view.menu = None,
                _ => {
                    let at = book
                        .view
                        .drawn
                        .map_or(egui::pos2(200.0, 200.0), |r| r.min + egui::vec2(160.0, 120.0));
                    book.view.menu = Some(at);
                }
            }
            Ok(said(String::new()))
        }
        "prompt" => {
            let words = args.to_vec();
            let book = sheet.current_book()?;
            let Some(prompt) = book.view.prompt.clone() else {
                return Err("Nothing is being asked.".to_owned());
            };
            match words.first().map(String::as_str) {
                Some("cancel") => {
                    book.view.prompt = None;
                    Ok(said("Cancelled"))
                }
                Some("ok") => {
                    book.view.prompt = None;
                    let verb = prompt.answer();
                    run(sheet, &verb[0], &verb[1..])
                }
                Some("field") => {
                    let name = words.get(1).ok_or("prompt field needs a field name and a value")?;
                    let value = words[2.min(words.len())..].join(" ");
                    let open = book.view.prompt.as_mut().expect("checked");
                    let field = open.field_mut(name).ok_or_else(|| {
                        let keys: Vec<String> =
                            prompt.fields.iter().map(|f| f.key.clone()).collect();
                        format!("the box has no field `{name}`; it has {}", keys.join(", "))
                    })?;
                    field.value = field_value(field, &value)?;
                    Ok(said(String::new()))
                }
                _ => {
                    book.view.prompt.as_mut().expect("checked").text = words.join(" ");
                    Ok(said(String::new()))
                }
            }
        }
        "edit" => {
            let book = sheet.current_book()?;
            let at = book.view.cursor;
            match args.first().map(String::as_str) {
                Some("cancel") => {
                    book.view.edit = None;
                    Ok(said("Cancelled"))
                }
                Some("enter") => {
                    let Some(edit) = book.view.edit.take() else {
                        return Err("No cell is being typed into.".to_owned());
                    };
                    act(sheet, Act::Type { at: edit.at, text: edit.text })
                }
                _ => {
                    let text = if args.is_empty() {
                        display::input_text(
                            &book.session.book,
                            &book.session.book.sheets[book.view.sheet],
                            at,
                        )
                    } else {
                        args.join(" ")
                    };
                    book.view.edit = Some(super::view::Edit {
                        sheet: book.view.sheet,
                        at,
                        text,
                        typed: !args.is_empty(),
                        in_bar: false,
                        pointing: None,
                        focus: true,
                        caret: None,
                        chosen: 0,
                        no_complete: false,
                    });
                    Ok(said(format!("Typing into {}", at.a1())))
                }
            }
        }
        // The window's own names for the menu entries that put something on the screen.
        "open-tab" | "open-pane" => Ok(said(String::new())),
        // The verbs round 2 added are in `verbs`, run once there.
        other => super::verbs::run(sheet, other, args).unwrap_or_else(|| {
            Err(format!(
                "there is no `{other}` command on `spreadsheet`: {}",
                LIST.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
            ))
        }),
    }
}

/// What a field of a box is set to by the `prompt field` words: a switch takes on or off, a choice one of
/// its words or labels, and a text field the text.
fn field_value(field: &super::view::Field, value: &str) -> Result<String, String> {
    use super::view::FieldKind;
    Ok(match &field.kind {
        FieldKind::Check => {
            if matches!(value, "on" | "yes" | "true" | "1" | "") {
                "on".to_owned()
            } else {
                String::new()
            }
        }
        FieldKind::Choice(options) => options
            .iter()
            .find(|(l, w)| w.eq_ignore_ascii_case(value) || l.eq_ignore_ascii_case(value))
            .map(|(_, w)| w.clone())
            .ok_or_else(|| {
                let words: Vec<String> = options.iter().map(|(_, w)| w.clone()).collect();
                format!("`{value}` is not one of {}", words.join(", "))
            })?,
        FieldKind::Text => value.to_owned(),
    })
}

fn number(arg: Option<&String>) -> Result<u32, String> {
    arg.and_then(|a| a.parse().ok()).ok_or_else(|| "that needs a number".to_owned())
}

/// A range written with dollar signs, for a name or a chart.
fn absolute(r: Range) -> String {
    let a = |at: CellRef| format!("${}${}", column_name(at.col), at.row + 1);
    if r.len() == 1 {
        a(r.first)
    } else {
        format!("{}:{}", a(r.first), a(r.last))
    }
}

/// What an agent wrote as rows of text: a JSON array of arrays, a JSON array (one row), or one value.
pub fn rows_of(text: &str) -> Vec<Vec<String>> {
    let cell = |v: &Json| match v {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    };
    match serde_json::from_str::<Json>(text) {
        Ok(Json::Array(rows)) if rows.iter().all(Json::is_array) => rows
            .iter()
            .map(|row| row.as_array().map(|r| r.iter().map(cell).collect()).unwrap_or_default())
            .collect(),
        Ok(Json::Array(values)) => vec![values.iter().map(cell).collect()],
        _ => vec![vec![text.to_owned()]],
    }
}

/// A sheet's name and number, from a word that is either.
fn sheet_index(book: &Book, word: &str) -> Result<usize, String> {
    if let Some(i) = book.session.book.sheet_index(word) {
        return Ok(i);
    }
    match word.parse::<usize>() {
        Ok(n) if n >= 1 && n <= book.session.book.sheets.len() => Ok(n - 1),
        _ => Err(format!("there is no sheet `{word}`")),
    }
}

fn sheet_verb(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let what = args
        .first()
        .map(String::as_str)
        .ok_or("sheet takes show, add, rename, delete, move, duplicate, hide, unhide or color")?;
    let book = sheet.current_book()?;
    let named = |i: usize| -> Result<usize, String> {
        args.get(i).map_or(Ok(book.view.sheet), |w| sheet_index(book, w))
    };
    let a = match what {
        "show" => Act::ShowSheet(named(1)?),
        "add" => {
            let answer = act(sheet, Act::AddSheet)?;
            if let Some(name) = args.get(1) {
                let book = sheet.current_book()?;
                let index = book.view.sheet;
                act(sheet, Act::RenameSheet(index, name.clone()))?;
            }
            return Ok(answer);
        }
        "rename" => {
            if args.len() == 2 {
                Act::RenameSheet(book.view.sheet, args[1].clone())
            } else {
                Act::RenameSheet(named(1)?, args.get(2).ok_or("sheet rename needs the new name")?.clone())
            }
        }
        "delete" => Act::DeleteSheet(named(1)?),
        "move" => {
            let from = named(1)?;
            let to: usize = args.get(2).and_then(|w| w.parse().ok()).ok_or("sheet move needs a sheet and a position from 1")?;
            Act::MoveSheet(from, to.saturating_sub(1))
        }
        "duplicate" => Act::DuplicateSheet(named(1)?),
        "hide" => Act::HideSheet(named(1)?),
        "unhide" => Act::UnhideSheet(named(1)?),
        "color" | "colour" => {
            let index = if args.len() >= 3 { named(1)? } else { book.view.sheet };
            let word = args.last().map(String::as_str).unwrap_or("none");
            Act::TabColor(index, if word == "none" { None } else { Some(Color::parse_hex(word).ok_or("a colour is #RRGGBB or none")?) })
        }
        other => return Err(format!("sheet takes show, add, rename, delete, move, duplicate, hide, unhide or color, not `{other}`")),
    };
    act(sheet, a)
}

fn filter_verb(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    // `filter B >10` and `filter B clear` name a column of the filter.
    if let (Some(first), Some(second)) = (args.first(), args.get(1)) {
        if let Some(col) = column_number(first)
            .filter(|_| Range::parse(first).is_none() || first.chars().all(char::is_alphabetic))
        {
            let criteria = if second == "clear" {
                None
            } else if let Some(op) =
                ["<>", ">=", "<=", ">", "<", "="].iter().find(|op| second.starts_with(**op))
            {
                let mut conditions = vec![(op_name(op).to_owned(), second[op.len()..].to_owned())];
                if let Some(more) = args.get(3) {
                    if let Some(op2) =
                        ["<>", ">=", "<=", ">", "<", "="].iter().find(|op| more.starts_with(**op))
                    {
                        conditions.push((op_name(op2).to_owned(), more[op2.len()..].to_owned()));
                    }
                }
                Some(FilterCriteria::Custom {
                    conditions,
                    and: args.get(2).map(String::as_str) != Some("or"),
                })
            } else {
                let values: Vec<String> =
                    args[1..].join(" ").split(',').map(|v| v.trim().to_owned()).collect();
                let blank = values.iter().any(|v| v == "(blank)");
                Some(FilterCriteria::Values {
                    values: values.into_iter().filter(|v| v != "(blank)").collect(),
                    blank,
                })
            };
            if book.session.book.sheets[book.view.sheet].filter.is_none() {
                act(sheet, Act::Filter)?;
            }
            return act(sheet, Act::FilterColumn { col, criteria });
        }
    }
    let has = book.session.book.sheets[book.view.sheet].filter.is_some();
    match args.first().map(String::as_str) {
        Some("off") => {
            if has {
                act(sheet, Act::Filter)
            } else {
                Ok(said("There is no filter"))
            }
        }
        _ => {
            take_range(book, args);
            if has {
                act(sheet, Act::Filter)?;
            }
            act(sheet, Act::Filter)
        }
    }
}

fn op_name(op: &str) -> &'static str {
    match op {
        ">" => "greaterThan",
        "<" => "lessThan",
        ">=" => "greaterThanOrEqual",
        "<=" => "lessThanOrEqual",
        "<>" => "notEqual",
        _ => "equal",
    }
}

fn comparison(word: &str) -> Option<&'static str> {
    Some(match word {
        "between" => "between",
        "not-between" => "notBetween",
        "=" | "equal" | "eq" => "equal",
        "<>" | "not-equal" | "ne" => "notEqual",
        ">" | "greater" | "gt" => "greaterThan",
        "<" | "less" | "lt" => "lessThan",
        ">=" | "ge" => "greaterThanOrEqual",
        "<=" | "le" => "lessThanOrEqual",
        _ => return None,
    })
}

fn validate_verb(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let rest = take_range(book, args).to_vec();
    let kind = rest
        .first()
        .map(String::as_str)
        .ok_or("validate takes list, whole, decimal, date, length, custom or off")?;
    if kind == "off" || kind == "clear" {
        return act(sheet, Act::ClearValidation);
    }
    let mut validation = Validation {
        ranges: Vec::new(),
        kind: String::new(),
        operator: None,
        formula1: None,
        formula2: None,
        allow_blank: true,
        show_dropdown: false,
        prompt_title: None,
        prompt: None,
        error_title: None,
        error: None,
        error_style: None,
        attrs: Vec::new(),
    };
    match kind {
        "list" => {
            validation.kind = "list".to_owned();
            let source = rest[1..].join(" ");
            validation.formula1 = Some(if source.starts_with('=') {
                source.trim_start_matches('=').to_owned()
            } else {
                format!("\"{source}\"")
            });
        }
        "whole" | "decimal" | "date" | "time" | "length" => {
            validation.kind =
                if kind == "length" { "textLength".to_owned() } else { kind.to_owned() };
            let operator = rest
                .get(1)
                .and_then(|w| comparison(w))
                .ok_or("give a comparison: between, >, <, >=, <=, = or <>")?;
            validation.operator = Some(operator.to_owned());
            validation.formula1 = Some(
                rest.get(2)
                    .ok_or("give a value to compare with")?
                    .trim_start_matches('=')
                    .to_owned(),
            );
            if operator.contains("etween") {
                validation.formula2 = Some(
                    rest.get(3)
                        .ok_or("between needs two values")?
                        .trim_start_matches('=')
                        .to_owned(),
                );
            }
        }
        "custom" => {
            validation.kind = "custom".to_owned();
            validation.formula1 = Some(rest[1..].join(" ").trim_start_matches('=').to_owned());
        }
        other => {
            return Err(format!(
                "validate takes list, whole, decimal, date, length, custom or off, not `{other}`"
            ))
        }
    }
    act(sheet, Act::Validation(validation))
}

fn blank_rule(kind: &str) -> CfRule {
    CfRule {
        kind: kind.to_owned(),
        dxf: None,
        priority: 1,
        stop: false,
        operator: None,
        text: None,
        rank: None,
        percent: false,
        bottom: false,
        above_average: None,
        formulas: Vec::new(),
        color_scale: None,
        data_bar: None,
        attrs: Vec::new(),
        raw_children: String::new(),
    }
}

/// A conditional format rule from words, with the look it applies.
pub fn rule_from_words(
    words: &[String],
    dxfs: &mut Vec<unluminous_sheet::styles::Dxf>,
) -> Result<CfRule, String> {
    let kind = words.first().map(String::as_str).ok_or("conditional takes a rule first")?;
    let arg = |i: usize| words.get(i).cloned().ok_or_else(|| format!("`{kind}` needs a value"));
    let mut rule = match kind {
        "greater" | "less" | "equal" | "between" | "not-equal" | ">" | "<" | ">=" | "<=" | "="
        | "<>" | "not-between" => {
            let mut r = blank_rule("cellIs");
            let op = comparison(kind).ok_or("not a comparison")?;
            r.operator = Some(op.to_owned());
            r.formulas.push(quote_formula(&arg(1)?));
            if op.contains("etween") {
                r.formulas.push(quote_formula(&arg(2)?));
            }
            r
        }
        "contains" | "not-contains" | "begins" | "ends" => {
            let mut r = blank_rule(match kind {
                "contains" => "containsText",
                "not-contains" => "notContainsText",
                "begins" => "beginsWith",
                _ => "endsWith",
            });
            r.operator = Some(
                match kind {
                    "contains" => "containsText",
                    "not-contains" => "notContains",
                    "begins" => "beginsWith",
                    _ => "endsWith",
                }
                .to_owned(),
            );
            r.text = Some(arg(1)?);
            r
        }
        "duplicates" => blank_rule("duplicateValues"),
        "unique" => blank_rule("uniqueValues"),
        "top" | "bottom" => {
            let mut r = blank_rule("top10");
            let n = arg(1)?;
            r.percent = n.ends_with('%');
            r.rank =
                Some(n.trim_end_matches('%').parse().map_err(|_| format!("`{n}` is not a count"))?);
            r.bottom = kind == "bottom";
            r
        }
        "above" | "below" => {
            let mut r = blank_rule("aboveAverage");
            r.above_average = Some(kind == "above");
            r
        }
        "blanks" => blank_rule("containsBlanks"),
        "errors" => blank_rule("containsErrors"),
        "formula" => {
            let mut r = blank_rule("expression");
            r.formulas.push(arg(1)?.trim_start_matches('=').to_owned());
            r
        }
        "scale" => {
            let mut r = blank_rule("colorScale");
            let colors: Vec<Color> =
                words[1..].iter().filter_map(|w| Color::parse_hex(w)).collect();
            let cfvo =
                |k: &str, v: Option<&str>| Cfvo { kind: k.to_owned(), value: v.map(str::to_owned) };
            r.color_scale = Some(match colors.len() {
                0 => vec![
                    (cfvo("min", None), Color::rgb(0xF8, 0x69, 0x6B)),
                    (cfvo("percentile", Some("50")), Color::rgb(0xFF, 0xEB, 0x84)),
                    (cfvo("max", None), Color::rgb(0x63, 0xBE, 0x7B)),
                ],
                2 => vec![(cfvo("min", None), colors[0]), (cfvo("max", None), colors[1])],
                _ => vec![
                    (cfvo("min", None), colors[0]),
                    (cfvo("percentile", Some("50")), colors[1]),
                    (cfvo("max", None), colors[2]),
                ],
            });
            return Ok(r);
        }
        "bar" => {
            let mut r = blank_rule("dataBar");
            let color = words
                .get(1)
                .and_then(|w| Color::parse_hex(w))
                .unwrap_or(Color::rgb(0x63, 0x8E, 0xC6));
            let cfvo = |k: &str| Cfvo { kind: k.to_owned(), value: None };
            r.data_bar = Some((cfvo("min"), cfvo("max"), color));
            return Ok(r);
        }
        other => return Err(format!("`{other}` is not a conditional rule")),
    };
    // The look: `fill #hex`, `color #hex`, `bold`, `italic`, or Excel's light red fill with dark red text.
    let mut dxf = unluminous_sheet::styles::Dxf::default();
    let mut i = 1;
    while i < words.len() {
        match words[i].as_str() {
            "fill" => {
                dxf.fill = words.get(i + 1).and_then(|w| Color::parse_hex(w));
                i += 1;
            }
            "color" | "colour" | "text" => {
                dxf.font_color = words.get(i + 1).and_then(|w| Color::parse_hex(w));
                i += 1;
            }
            "bold" => dxf.bold = Some(true),
            "italic" => dxf.italic = Some(true),
            _ => {}
        }
        i += 1;
    }
    if dxf == unluminous_sheet::styles::Dxf::default() {
        dxf.fill = Some(Color::rgb(0xFF, 0xC7, 0xCE));
        dxf.font_color = Some(Color::rgb(0x9C, 0x00, 0x06));
    }
    dxfs.push(dxf);
    rule.dxf = Some((dxfs.len() - 1) as u32);
    Ok(rule)
}

/// A rule's value as a formula: a number or a reference as it is, text in quotes.
fn quote_formula(word: &str) -> String {
    let w = word.trim_start_matches('=');
    if w.parse::<f64>().is_ok()
        || Range::parse(&w.replace('$', "")).is_some()
        || word.starts_with('=')
        || w.starts_with('"')
    {
        w.to_owned()
    } else {
        format!("\"{w}\"")
    }
}

fn conditional_verb(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let rest = take_range(book, args).to_vec();
    if rest.first().map(String::as_str) == Some("clear") {
        return act(sheet, Act::ClearConditional);
    }
    let rule = rule_from_words(&rest, &mut book.session.book.styles.dxfs)?;
    act(sheet, Act::Conditional(rule))
}

/// Format words that depend on the active cell turned into plain ones: `toggle bold` into `bold` or
/// `nobold`, and `bigger` and `smaller` into the next size on Excel's list.
fn relative_format_words(book: &Book, words: Vec<String>) -> Vec<String> {
    let s = &book.session.book;
    let font = s.styles.font_of(display::style_at(&s.sheets[book.view.sheet], book.view.cursor));
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        match words[i].as_str() {
            "toggle" => {
                let what = words.get(i + 1).map(String::as_str).unwrap_or("");
                let on = match what {
                    "bold" => font.bold,
                    "italic" => font.italic,
                    "underline" => font.underline != Underline::None,
                    "strike" => font.strike,
                    _ => false,
                };
                out.push(if on { format!("no{what}") } else { what.to_owned() });
                i += 2;
                continue;
            }
            "bigger" | "smaller" => {
                const SIZES: [f64; 16] = [
                    8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0,
                    36.0, 48.0, 72.0,
                ];
                let next = if words[i] == "bigger" {
                    SIZES.iter().copied().find(|s| *s > font.size).unwrap_or(font.size + 12.0)
                } else {
                    SIZES
                        .iter()
                        .rev()
                        .copied()
                        .find(|s| *s < font.size)
                        .unwrap_or(1.0f64.max(font.size - 1.0))
                };
                out.push("size".to_owned());
                out.push(unluminous_sheet::value::number_to_text(next));
            }
            other => out.push(other.to_owned()),
        }
        i += 1;
    }
    out
}

/// A number format with one more or one fewer decimal place, as the buttons in Excel's ribbon change it.
pub fn step_decimals(code: &str, value: &Value, more: bool) -> String {
    if code == "General" || code.is_empty() {
        // From General the count starts at what the value shows.
        let shown = match value {
            Value::Number(n) => unluminous_sheet::value::number_to_text(*n),
            _ => String::new(),
        };
        let places = shown.split_once('.').map_or(0, |(_, d)| d.len());
        let places = if more { places + 1 } else { places.saturating_sub(1) };
        return if places == 0 { "0".to_owned() } else { format!("0.{}", "0".repeat(places)) };
    }
    code.split(';')
        .map(|section| {
            let chars: Vec<char> = section.chars().collect();
            let mut quoted = false;
            let mut last_digit = None;
            let mut point = None;
            for (i, c) in chars.iter().enumerate() {
                match c {
                    '"' => quoted = !quoted,
                    '0' | '#' | '?' if !quoted => last_digit = Some(i),
                    '.' if !quoted && point.is_none() => point = Some(i),
                    _ => {}
                }
            }
            let Some(last) = last_digit else { return section.to_owned() };
            let mut out = chars.clone();
            match (more, point) {
                (true, Some(_)) => out.insert(last + 1, '0'),
                (true, None) => {
                    out.insert(last + 1, '0');
                    out.insert(last + 1, '.');
                }
                (false, Some(p)) if last > p => {
                    out.remove(last);
                    if last == p + 1 {
                        out.remove(p);
                    }
                }
                _ => {}
            }
            out.into_iter().collect()
        })
        .collect::<Vec<String>>()
        .join(";")
}

/// One style change, or several, from words: `bold`, `size 14`, `fill #FFF2CC`, `number 0.00`.
pub fn style_changes(words: &[String]) -> Result<Vec<StyleChange>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    let next =
        |i: usize| words.get(i + 1).cloned().ok_or_else(|| format!("`{}` needs a value", words[i]));
    while i < words.len() {
        let word = words[i].to_ascii_lowercase();
        let font = |edit: FontEdit| StyleChange::Font(edit);
        let align = |edit: AlignEdit| StyleChange::Align(edit);
        match word.as_str() {
            "bold" | "nobold" => out.push(font(FontEdit { bold: Some(word == "bold"), ..Default::default() })),
            "italic" | "noitalic" => out.push(font(FontEdit { italic: Some(word == "italic"), ..Default::default() })),
            "underline" | "nounderline" => out.push(font(FontEdit { underline: Some(if word == "underline" { Underline::Single } else { Underline::None }), ..Default::default() })),
            "double-underline" => out.push(font(FontEdit { underline: Some(Underline::Double), ..Default::default() })),
            "strike" | "nostrike" => out.push(font(FontEdit { strike: Some(word == "strike"), ..Default::default() })),
            "size" => {
                let v = next(i)?;
                out.push(font(FontEdit { size: Some(v.parse().map_err(|_| format!("`{v}` is not a size"))?), ..Default::default() }));
                i += 1;
            }
            "font" => {
                out.push(font(FontEdit { name: Some(next(i)?), ..Default::default() }));
                i += 1;
            }
            "color" | "colour" => {
                let v = next(i)?;
                let color = if v == "auto" { Color::Theme(1, 0.0) } else { Color::parse_hex(&v).ok_or_else(|| format!("`{v}` is not #RRGGBB"))? };
                out.push(font(FontEdit { color: Some(color), ..Default::default() }));
                i += 1;
            }
            "fill" => {
                let v = next(i)?;
                out.push(StyleChange::Fill(if v == "none" { None } else { Some(Color::parse_hex(&v).ok_or_else(|| format!("`{v}` is not #RRGGBB"))?) }));
                i += 1;
            }
            "number" | "format" => {
                out.push(StyleChange::NumberFormat(number_format(&next(i)?).to_owned()));
                i += 1;
            }
            "general" | "currency" | "accounting" | "percent" | "date" | "longdate" | "time" | "scientific" | "fraction" | "text" | "comma" | "integer" => {
                out.push(StyleChange::NumberFormat(number_format(&word).to_owned()));
            }
            "align" => {
                let v = next(i)?;
                let h = match v.as_str() {
                    "general" => None,
                    "left" | "center" | "right" | "fill" | "justify" => Some(v.clone()),
                    "center-across" => Some("centerContinuous".to_owned()),
                    other => return Err(format!("align takes left, center, right, general, fill, justify or center-across, not `{other}`")),
                };
                out.push(align(AlignEdit { horizontal: Some(h), ..Default::default() }));
                i += 1;
            }
            "valign" => {
                let v = next(i)?;
                let vertical = match v.as_str() {
                    "top" => Some("top".to_owned()),
                    "middle" | "center" => Some("center".to_owned()),
                    "bottom" => None,
                    other => return Err(format!("valign takes top, middle or bottom, not `{other}`")),
                };
                out.push(align(AlignEdit { vertical: Some(vertical), ..Default::default() }));
                i += 1;
            }
            "wrap" | "nowrap" => out.push(align(AlignEdit { wrap: Some(word == "wrap"), ..Default::default() })),
            "shrink" => out.push(align(AlignEdit { shrink: Some(true), ..Default::default() })),
            "indent" => {
                let v = next(i)?;
                out.push(align(AlignEdit { indent: Some(v.parse().map_err(|_| format!("`{v}` is not an indent"))?), ..Default::default() }));
                i += 1;
            }
            "rotate" => {
                let v = next(i)?;
                out.push(align(AlignEdit { rotation: Some(v.parse().map_err(|_| format!("`{v}` is not an angle"))?), ..Default::default() }));
                i += 1;
            }
            "locked" | "unlocked" => out.push(StyleChange::Locked(word == "locked")),
            other => return Err(format!("`{other}` is not a format; try bold, italic, size 12, font Arial, color #C00000, fill #FFF2CC, number 0.00, currency, percent, date, align center, wrap")),
        }
        i += 1;
    }
    Ok(out)
}

/// A number format from a preset's name, or the code itself.
pub fn number_format(word: &str) -> &str {
    match word {
        "general" => "General",
        "integer" => "0",
        "number" => "#,##0.00",
        "comma" => "#,##0",
        "currency" => "\"$\"#,##0.00",
        "accounting" => "_(\"$\"* #,##0.00_);_(\"$\"* \\(#,##0.00\\);_(\"$\"* \"-\"??_);_(@_)",
        "percent" => "0%",
        "date" => "m/d/yyyy",
        "longdate" => "dddd, mmmm d, yyyy",
        "time" => "h:mm:ss AM/PM",
        "scientific" => "0.00E+00",
        "fraction" => "# ?/?",
        "text" => "@",
        code => code,
    }
}

pub fn value_json(v: &Value) -> Json {
    match v {
        Value::Empty => Json::Null,
        Value::Number(n) => json!(n),
        Value::Text(t) => json!(t.to_string()),
        Value::Bool(b) => json!(b),
        Value::Error(e) => json!(e.text()),
    }
}

/// One cell of the sheet showing, for `cell` and the view.
pub fn cell_value(book: &Book, at: CellRef) -> Json {
    cell_value_on(book, book.view.sheet, at)
}

/// One cell of a sheet.
pub fn cell_value_on(book: &Book, sheet: usize, at: CellRef) -> Json {
    let workbook = &book.session.book;
    let s = &workbook.sheets[sheet];
    let shown = display::shown(workbook, s, at, 255);
    let style = display::style_at(s, at);
    let font = workbook.styles.font_of(style);
    json!({
        "cell": at.a1(),
        "value": value_json(&s.value_at(at)),
        "formula": s.cell(at).and_then(|c| c.formula.as_ref()).map(|f| f.display()),
        "input": display::input_text(workbook, s, at),
        "shown": shown.text,
        "number_format": workbook.styles.format_code(style),
        "font": { "name": font.name, "size": font.size, "bold": font.bold, "italic": font.italic, "color": font.color.to_hex(&workbook.styles.theme) },
        "fill": shown.fill.map(|(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}")),
        "align": workbook.styles.xf(style).align.horizontal,
        "merged": s.merge_at(at).map(|m| m.a1()),
        "note": s.notes.get(&(at.row, at.col)).map(|n| n.text.clone()),
        "link": s.links.iter().find(|l| l.range.contains(at)).map(|l| match &l.target {
            unluminous_sheet::model::LinkTarget::External(t) | unluminous_sheet::model::LinkTarget::Location(t) => t.clone(),
        }),
        "spill": s.spills.range_of((at.row, at.col)).map(|r| r.a1()),
    })
}

fn selection_value(book: &Book) -> Json {
    let summary = book.session.summary(book.view.sheet, &book.view.ranges());
    json!({
        "sheet": book.session.book.sheets[book.view.sheet].name,
        "ranges": book.view.ranges().iter().map(|r| r.a1()).collect::<Vec<_>>(),
        "active": book.view.cursor.a1(),
        "cell": cell_value(book, book.view.cursor),
        "count": summary.count,
        "numbers": summary.numbers,
        "sum": summary.sum,
        "average": summary.average(),
        "min": summary.min,
        "max": summary.max,
    })
}

fn read_value(book: &Book, sheet: usize, range: Range, what: &str) -> Result<Json, String> {
    if range.len() > 20_000 {
        return Err(format!(
            "{} is {} cells; read at most 20,000 at a time",
            range.a1(),
            range.len()
        ));
    }
    let workbook = &book.session.book;
    let s = &workbook.sheets[sheet];
    let rows: Vec<Json> = (range.first.row..=range.last.row)
        .map(|r| {
            Json::Array(
                (range.first.col..=range.last.col)
                    .map(|c| {
                        let at = CellRef::new(r, c);
                        match what {
                            "formulas" => match s.cell(at).and_then(|cell| cell.formula.as_ref()) {
                                Some(f) => json!(f.display()),
                                None => value_json(&s.value_at(at)),
                            },
                            "text" => json!(display::shown(workbook, s, at, 255).text),
                            "formats" => {
                                json!(workbook.styles.format_code(display::style_at(s, at)))
                            }
                            _ => value_json(&s.value_at(at)),
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    Ok(json!({ "range": range.a1(), "rows": rows }))
}

fn book_value(book: &Book) -> Json {
    json!({
        "file": book.path.display().to_string(),
        "modified": book.session.modified,
        "sheets": book.session.book.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "sheet": book.session.book.sheets[book.view.sheet].name,
    })
}

/// The whole plugin as data, for `plugins view spreadsheet`.
pub fn view_value(sheet: &Spreadsheet) -> Json {
    let current = sheet.current.clone().or_else(|| sheet.books.last().map(|b| b.key.clone()));
    let showing = sheet.books.iter().find(|b| Some(&b.key) == current.as_ref());
    json!({
        "books": sheet.books.iter().map(|b| json!({ "file": b.path.display().to_string(), "modified": b.session.modified })).collect::<Vec<_>>(),
        "current": showing.map(|book| {
            let v = &book.view;
            json!({
                "file": book.path.display().to_string(),
                "modified": book.session.modified,
                "sheet": book.session.book.sheets[v.sheet].name,
                "sheets": book.session.book.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
                "selection": selection_value(book),
                "editing": v.edit.as_ref().map(|e| json!({ "cell": e.at.a1(), "text": e.text, "in_bar": e.in_bar })),
                "panel": v.panel.as_ref().map(|p| p.0.clone()),
                "menu": v.menu.is_some(),
                "book_menu": v.book_menu.clone(),
                "find": v.find.as_ref().map(|f| json!({ "text": f.text, "replace": f.replace })),
                "prompt": v.prompt.as_ref().map(|p| json!({
                    "title": p.title,
                    "text": p.text,
                    "fields": p.fields.iter().map(|f| json!({ "key": f.key, "label": f.label, "value": f.value })).collect::<Vec<_>>(),
                })),
                "frozen": book.session.book.sheets[v.sheet].frozen.map(|(r, c)| json!({ "rows": r, "columns": c })),
                "zoom": v.zoom,
                "can_undo": book.session.can_undo(),
                "undo": book.session.undo_label(),
                "scroll": [v.scroll.x, v.scroll.y],
                "charts": book.session.book.sheets[v.sheet].charts.iter().map(|c| json!({ "kind": c.kind.name(), "title": c.title, "series": c.series.len() })).collect::<Vec<_>>(),
            })
        }),
    })
}
