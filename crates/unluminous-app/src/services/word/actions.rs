//! Every command a person can run on a document by name: one list for the `Ctrl+/` search, the `/` menu,
//! the panel's buttons and `plugins run word action <id>`.
//!
//! Each row is a name an agent uses, the words a person reads, the panel level it belongs to, whether the
//! `/` menu offers it, and the function it runs. The function takes the document, so the four ways in are
//! one path.

use unluminous_word::builtin::ListKind;
use unluminous_word::model::{COLUMN_BREAK, LINE_BREAK, PAGE_BREAK};
use unluminous_word::props;

use super::{Doc, Level};

/// One command.
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub level: Level,
    /// Offered by the `/` menu, which is for what can be inserted at the start of a paragraph.
    pub slash: bool,
    pub run: fn(&mut Doc) -> Result<String, String>,
}

fn done(said: &str) -> Result<String, String> {
    Ok(said.to_owned())
}

fn toggle(doc: &mut Doc, name: &'static str) -> Result<String, String> {
    doc.session.toggle(name);
    Ok(String::new())
}

fn vert(doc: &mut Doc, value: &'static str) -> Result<String, String> {
    let on = match value {
        "superscript" => doc.session.look_at_caret().vert != props::VertAlign::Superscript,
        _ => doc.session.look_at_caret().vert != props::VertAlign::Subscript,
    };
    doc.session.format(move |rpr| props::set_val(rpr, "w:vertAlign", on.then_some(value)));
    Ok(String::new())
}

fn align(doc: &mut Doc, value: &'static str) -> Result<String, String> {
    doc.session.format_paragraphs(move |ppr| props::set_val(ppr, "w:jc", Some(value)));
    Ok(String::new())
}

fn line_spacing(doc: &mut Doc, line: i64) -> Result<String, String> {
    doc.session.format_paragraphs(move |ppr| {
        let spacing = ppr.child_or_insert("w:spacing");
        spacing.set_attr("w:line", line.to_string());
        spacing.set_attr("w:lineRule", "auto");
    });
    Ok(String::new())
}

fn space(doc: &mut Doc, attr: &'static str, twips: i64) -> Result<String, String> {
    doc.session.format_paragraphs(move |ppr| {
        ppr.child_or_insert("w:spacing").set_attr(attr, twips.to_string());
    });
    Ok(String::new())
}

fn paragraph_toggle(doc: &mut Doc, name: &'static str) -> Result<String, String> {
    let look = doc.session.paragraph_at_caret();
    let on = match name {
        "w:keepNext" => !look.keep_next,
        "w:keepLines" => !look.keep_lines,
        _ => !look.page_break_before,
    };
    doc.session.format_paragraphs(move |ppr| props::set_on_off(ppr, name, Some(on)));
    Ok(String::new())
}

/// Fit Width and One Page: the zoom that makes the page as wide as the pane, or the whole page fit.
fn fit(doc: &mut Doc, whole: bool) {
    let Some(page) = doc.pages.pages.first() else { return };
    let across = (doc.view.pane_width - 48.0) / page.width;
    let down = (doc.view.viewport_height - 32.0) / page.height;
    doc.view.zoom = if whole { across.min(down) } else { across }.clamp(0.1, 5.0);
    doc.view.multiple = false;
}

/// Outline view's Move Up and Move Down: a folded heading takes its whole section, anything else moves alone.
fn outline_move(doc: &mut Doc, up: bool) -> Result<String, String> {
    let head = doc.session.selection.head.clone();
    if doc.view.outline_collapsed.contains(&head.path) {
        let old = head.path.clone();
        doc.session.move_section(up)?;
        doc.view.outline_collapsed.remove(&old);
        doc.view.outline_collapsed.insert(doc.session.selection.head.path.clone());
    } else {
        doc.session.move_paragraph(up)?;
    }
    Ok(String::new())
}

fn underline_kind(doc: &mut Doc, kind: &'static str) -> Result<String, String> {
    let on = doc.session.look_at_caret().underline != props::Underline::parse(kind);
    doc.session.format(move |rpr| props::set_val(rpr, "w:u", on.then_some(kind)));
    Ok(String::new())
}

/// The selection's size, a point at a time, as Ctrl+] and Ctrl+[ step it.
fn font_step(doc: &mut Doc, by: f32) -> Result<String, String> {
    let size = (doc.session.look_at_caret().size + by).clamp(1.0, 1638.0);
    let half_points = ((size * 2.0).round() as i64).to_string();
    doc.session.format(move |rpr| {
        props::set_val(rpr, "w:sz", Some(half_points.as_str()));
        props::set_val(rpr, "w:szCs", Some(half_points.as_str()));
    });
    Ok(String::new())
}

fn style(doc: &mut Doc, id: &'static str) -> Result<String, String> {
    doc.session.apply_style_where_it_fits(id)?;
    Ok(String::new())
}

fn indent(doc: &mut Doc, more: bool) -> Result<String, String> {
    if doc.session.change_list_level(more) {
        return Ok(String::new());
    }
    let left = doc.session.paragraph_at_caret().indent_left;
    let next = if more { left + 36.0 } else { (left - 36.0).max(0.0) };
    doc.session.format_paragraphs(move |ppr| {
        ppr.child_or_insert("w:ind").set_attr("w:left", ((next * 20.0).round() as i64).to_string());
    });
    Ok(String::new())
}

fn grow(doc: &mut Doc, larger: bool) -> Result<String, String> {
    // Word's steps: 8 to 12 by one, then 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72.
    const STEPS: [f32; 17] = [
        8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0,
        48.0, 72.0,
    ];
    let size = doc.session.look_at_caret().size;
    let next = if larger {
        STEPS.iter().copied().find(|s| *s > size + 0.01).unwrap_or(size + 10.0)
    } else {
        STEPS.iter().rev().copied().find(|s| *s < size - 0.01).unwrap_or((size - 1.0).max(1.0))
    };
    let half = ((next * 2.0).round() as i64).to_string();
    doc.session.format(move |rpr| props::set_val(rpr, "w:sz", Some(&half)));
    Ok(format!("{next} pt"))
}

fn page_numbers(doc: &mut Doc) -> Result<String, String> {
    doc.session.set_header_text(true, "default", "Page {PAGE} of {NUMPAGES}");
    done("Page numbers are in the footer")
}

fn insert_picture(doc: &mut Doc) -> Result<String, String> {
    let chosen = rfd::FileDialog::new()
        .set_title("Insert Picture")
        .add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "bmp"])
        .pick_file()
        .ok_or("no picture was chosen")?;
    super::commands::insert_picture_file(doc, &chosen, None)
}

fn export(doc: &mut Doc, extension: &'static str) -> Result<String, String> {
    super::commands::export_to(doc, extension, None)
}

/// The commands, in the order the `Ctrl+/` search lists them when nothing is typed.
pub const ACTIONS: &[Action] = &[
    // Text
    Action {
        id: "bold",
        label: "Bold",
        level: Level::Text,
        slash: false,
        run: |d| toggle(d, "w:b"),
    },
    Action {
        id: "italic",
        label: "Italic",
        level: Level::Text,
        slash: false,
        run: |d| toggle(d, "w:i"),
    },
    Action {
        id: "underline",
        label: "Underline",
        level: Level::Text,
        slash: false,
        run: |d| toggle(d, "w:u"),
    },
    Action {
        id: "strikethrough",
        label: "Strikethrough",
        level: Level::Text,
        slash: false,
        run: |d| toggle(d, "w:strike"),
    },
    Action {
        id: "double-strikethrough",
        label: "Double strikethrough",
        level: Level::Text,
        slash: false,
        run: |d| toggle(d, "w:dstrike"),
    },
    Action {
        id: "superscript",
        label: "Superscript",
        level: Level::Text,
        slash: false,
        run: |d| vert(d, "superscript"),
    },
    Action {
        id: "subscript",
        label: "Subscript",
        level: Level::Text,
        slash: false,
        run: |d| vert(d, "subscript"),
    },
    Action {
        id: "small-caps",
        label: "Small caps",
        level: Level::Text,
        slash: false,
        run: |d| toggle(d, "w:smallCaps"),
    },
    Action {
        id: "all-caps",
        label: "All caps",
        level: Level::Text,
        slash: false,
        run: |d| toggle(d, "w:caps"),
    },
    Action {
        id: "hidden",
        label: "Hidden text",
        level: Level::Text,
        slash: false,
        run: |d| toggle(d, "w:vanish"),
    },
    Action {
        id: "grow-font",
        label: "Larger text",
        level: Level::Text,
        slash: false,
        run: |d| grow(d, true),
    },
    Action {
        id: "shrink-font",
        label: "Smaller text",
        level: Level::Text,
        slash: false,
        run: |d| grow(d, false),
    },
    Action {
        id: "clear-formatting",
        label: "Clear all formatting",
        level: Level::Text,
        slash: false,
        run: |d| {
            d.session.clear_formatting();
            Ok(String::new())
        },
    },
    Action {
        id: "uppercase",
        label: "UPPERCASE",
        level: Level::Text,
        slash: false,
        run: |d| {
            d.session.change_case("upper");
            Ok(String::new())
        },
    },
    Action {
        id: "lowercase",
        label: "lowercase",
        level: Level::Text,
        slash: false,
        run: |d| {
            d.session.change_case("lower");
            Ok(String::new())
        },
    },
    Action {
        id: "sentence-case",
        label: "Sentence case",
        level: Level::Text,
        slash: false,
        run: |d| {
            d.session.change_case("sentence");
            Ok(String::new())
        },
    },
    Action {
        id: "title-case",
        label: "Capitalize Each Word",
        level: Level::Text,
        slash: false,
        run: |d| {
            d.session.change_case("title");
            Ok(String::new())
        },
    },
    Action {
        id: "toggle-case",
        label: "tOGGLE cASE",
        level: Level::Text,
        slash: false,
        run: |d| {
            d.session.change_case("toggle");
            Ok(String::new())
        },
    },
    Action {
        id: "copy-formatting",
        label: "Copy formatting",
        level: Level::Text,
        slash: false,
        run: |d| {
            let head = d.session.selection.ordered().0.clone();
            let props =
                unluminous_word::position::paragraph(&d.session.document, &head.story, &head.path)
                    .map(|p| {
                        unluminous_word::edit::props_at(
                            p,
                            head.offset + usize::from(!d.session.selection.is_empty()),
                        )
                        .0
                        .as_ref()
                        .clone()
                    });
            d.view.copied_format = props;
            done("Formatting copied")
        },
    },
    Action {
        id: "paste-formatting",
        label: "Paste formatting",
        level: Level::Text,
        slash: false,
        run: |d| {
            let props = d.view.copied_format.clone().ok_or("no formatting has been copied")?;
            d.session.format(move |rpr| *rpr = props.clone());
            Ok(String::new())
        },
    },
    // Paragraph
    Action {
        id: "align-left",
        label: "Align left",
        level: Level::Paragraph,
        slash: false,
        run: |d| align(d, "left"),
    },
    Action {
        id: "align-center",
        label: "Center",
        level: Level::Paragraph,
        slash: false,
        run: |d| align(d, "center"),
    },
    Action {
        id: "align-right",
        label: "Align right",
        level: Level::Paragraph,
        slash: false,
        run: |d| align(d, "right"),
    },
    Action {
        id: "justify",
        label: "Justify",
        level: Level::Paragraph,
        slash: false,
        run: |d| align(d, "both"),
    },
    Action {
        id: "indent-more",
        label: "Increase indent",
        level: Level::Paragraph,
        slash: false,
        run: |d| indent(d, true),
    },
    Action {
        id: "indent-less",
        label: "Decrease indent",
        level: Level::Paragraph,
        slash: false,
        run: |d| indent(d, false),
    },
    Action {
        id: "line-single",
        label: "Line spacing 1.0",
        level: Level::Paragraph,
        slash: false,
        run: |d| line_spacing(d, 240),
    },
    Action {
        id: "line-115",
        label: "Line spacing 1.15",
        level: Level::Paragraph,
        slash: false,
        run: |d| line_spacing(d, 276),
    },
    Action {
        id: "line-15",
        label: "Line spacing 1.5",
        level: Level::Paragraph,
        slash: false,
        run: |d| line_spacing(d, 360),
    },
    Action {
        id: "line-double",
        label: "Line spacing 2.0",
        level: Level::Paragraph,
        slash: false,
        run: |d| line_spacing(d, 480),
    },
    Action {
        id: "space-before",
        label: "Add space before paragraph",
        level: Level::Paragraph,
        slash: false,
        run: |d| space(d, "w:before", 240),
    },
    Action {
        id: "no-space-before",
        label: "Remove space before paragraph",
        level: Level::Paragraph,
        slash: false,
        run: |d| space(d, "w:before", 0),
    },
    Action {
        id: "space-after",
        label: "Add space after paragraph",
        level: Level::Paragraph,
        slash: false,
        run: |d| space(d, "w:after", 240),
    },
    Action {
        id: "no-space-after",
        label: "Remove space after paragraph",
        level: Level::Paragraph,
        slash: false,
        run: |d| space(d, "w:after", 0),
    },
    Action {
        id: "keep-with-next",
        label: "Keep with next",
        level: Level::Paragraph,
        slash: false,
        run: |d| paragraph_toggle(d, "w:keepNext"),
    },
    Action {
        id: "keep-lines",
        label: "Keep lines together",
        level: Level::Paragraph,
        slash: false,
        run: |d| paragraph_toggle(d, "w:keepLines"),
    },
    Action {
        id: "page-break-before",
        label: "Page break before",
        level: Level::Paragraph,
        slash: false,
        run: |d| paragraph_toggle(d, "w:pageBreakBefore"),
    },
    // Styles
    Action {
        id: "normal",
        label: "Normal text",
        level: Level::Styles,
        slash: true,
        run: |d| style(d, "Normal"),
    },
    Action {
        id: "title",
        label: "Title",
        level: Level::Styles,
        slash: true,
        run: |d| style(d, "Title"),
    },
    Action {
        id: "subtitle",
        label: "Subtitle",
        level: Level::Styles,
        slash: true,
        run: |d| style(d, "Subtitle"),
    },
    Action {
        id: "heading-1",
        label: "Heading 1",
        level: Level::Styles,
        slash: true,
        run: |d| style(d, "Heading1"),
    },
    Action {
        id: "heading-2",
        label: "Heading 2",
        level: Level::Styles,
        slash: true,
        run: |d| style(d, "Heading2"),
    },
    Action {
        id: "heading-3",
        label: "Heading 3",
        level: Level::Styles,
        slash: true,
        run: |d| style(d, "Heading3"),
    },
    Action {
        id: "heading-4",
        label: "Heading 4",
        level: Level::Styles,
        slash: true,
        run: |d| style(d, "Heading4"),
    },
    Action {
        id: "quote",
        label: "Quote",
        level: Level::Styles,
        slash: true,
        run: |d| style(d, "Quote"),
    },
    Action {
        id: "intense-quote",
        label: "Intense quote",
        level: Level::Styles,
        slash: true,
        run: |d| style(d, "IntenseQuote"),
    },
    Action {
        id: "no-spacing",
        label: "No spacing",
        level: Level::Styles,
        slash: false,
        run: |d| style(d, "NoSpacing"),
    },
    // Lists
    Action {
        id: "bullets",
        label: "Bulleted list",
        level: Level::Lists,
        slash: true,
        run: |d| {
            d.session.toggle_list(ListKind::Bullet);
            Ok(String::new())
        },
    },
    Action {
        id: "numbering",
        label: "Numbered list",
        level: Level::Lists,
        slash: true,
        run: |d| {
            d.session.toggle_list(ListKind::Number);
            Ok(String::new())
        },
    },
    Action {
        id: "list-deeper",
        label: "Demote list item",
        level: Level::Lists,
        slash: false,
        run: |d| {
            d.session.change_list_level(true);
            Ok(String::new())
        },
    },
    Action {
        id: "list-shallower",
        label: "Promote list item",
        level: Level::Lists,
        slash: false,
        run: |d| {
            d.session.change_list_level(false);
            Ok(String::new())
        },
    },
    Action {
        id: "restart-numbering",
        label: "Restart numbering at 1",
        level: Level::Lists,
        slash: false,
        run: |d| {
            d.session.restart_list(1);
            Ok(String::new())
        },
    },
    // Insert
    Action {
        id: "table",
        label: "Table",
        level: Level::Insert,
        slash: true,
        run: |d| {
            d.session.insert_table(3, 3);
            Ok(String::new())
        },
    },
    Action {
        id: "merge-cells",
        label: "Merge cells",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.merge_cells().map(|()| String::new()),
    },
    Action {
        id: "split-table",
        label: "Split table",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.split_table().map(|()| String::new()),
    },
    Action {
        id: "distribute-rows",
        label: "Distribute rows",
        level: Level::Insert,
        slash: false,
        run: |d| {
            let heights = super::commands::row_heights(d);
            d.session.distribute(true, &heights).map(|()| String::new())
        },
    },
    Action {
        id: "distribute-columns",
        label: "Distribute columns",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.distribute(false, &[]).map(|()| String::new()),
    },
    Action {
        id: "table-to-text",
        label: "Convert table to text",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.table_to_text("tab").map(|()| String::new()),
    },
    Action {
        id: "text-to-table",
        label: "Convert text to table",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.text_to_table("tab").map(|()| String::new()),
    },
    Action {
        id: "sort-table",
        label: "Sort table A to Z",
        level: Level::Insert,
        slash: false,
        run: |d| {
            let column = d.session.selected_cells().map_or(0, |r| r.cells.0);
            d.session
                .sort_table(column, unluminous_word::tables::SortBy::Text, false, false)
                .map(|()| String::new())
        },
    },
    Action {
        id: "select-row",
        label: "Select row",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.select_in_table("row").map(|()| String::new()),
    },
    Action {
        id: "select-column",
        label: "Select column",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.select_in_table("column").map(|()| String::new()),
    },
    Action {
        id: "select-table",
        label: "Select table",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.select_in_table("table").map(|()| String::new()),
    },
    Action {
        id: "picture",
        label: "Picture...",
        level: Level::Insert,
        slash: true,
        run: insert_picture,
    },
    Action {
        id: "page-break",
        label: "Page break",
        level: Level::Insert,
        slash: true,
        run: |d| {
            d.session.insert_break(PAGE_BREAK);
            Ok(String::new())
        },
    },
    Action {
        id: "column-break",
        label: "Column break",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.insert_break(COLUMN_BREAK);
            Ok(String::new())
        },
    },
    Action {
        id: "line-break",
        label: "Line break",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.insert_break(LINE_BREAK);
            Ok(String::new())
        },
    },
    Action {
        id: "section-next-page",
        label: "Section break (next page)",
        level: Level::Insert,
        slash: true,
        run: |d| {
            d.session.insert_section_break("nextPage");
            Ok(String::new())
        },
    },
    Action {
        id: "section-continuous",
        label: "Section break (continuous)",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.insert_section_break("continuous");
            Ok(String::new())
        },
    },
    Action {
        id: "horizontal-line",
        label: "Horizontal line",
        level: Level::Insert,
        slash: true,
        run: |d| {
            d.session.format_paragraphs(|ppr| {
                ppr.put(
                    unluminous_word::xml::El::new("w:pBdr").with_child(
                        unluminous_word::xml::El::new("w:bottom")
                            .with("w:val", "single")
                            .with("w:sz", "6")
                            .with("w:space", "1")
                            .with("w:color", "auto"),
                    ),
                );
            });
            Ok(String::new())
        },
    },
    Action {
        id: "date",
        label: "Today's date",
        level: Level::Insert,
        slash: true,
        run: |d| {
            let now = unluminous_word::session::now_iso();
            let shown = unluminous_word::fields::format_date(&now, Some("d MMMM yyyy"), false);
            d.session.insert_field("DATE \\@ \"d MMMM yyyy\"", &shown);
            Ok(String::new())
        },
    },
    Action {
        id: "page-number",
        label: "Page number",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.insert_field("PAGE", "1");
            Ok(String::new())
        },
    },
    // Page
    Action {
        id: "letter",
        label: "Letter paper",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_page_size("letter")?;
            Ok(String::new())
        },
    },
    Action {
        id: "a4",
        label: "A4 paper",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_page_size("a4")?;
            Ok(String::new())
        },
    },
    Action {
        id: "portrait",
        label: "Portrait",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_orientation(false);
            Ok(String::new())
        },
    },
    Action {
        id: "landscape",
        label: "Landscape",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_orientation(true);
            Ok(String::new())
        },
    },
    Action {
        id: "margins-normal",
        label: "Normal margins",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_margins([72.0; 4]);
            Ok(String::new())
        },
    },
    Action {
        id: "margins-narrow",
        label: "Narrow margins",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_margins([36.0; 4]);
            Ok(String::new())
        },
    },
    Action {
        id: "margins-wide",
        label: "Wide margins",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_margins([72.0, 72.0, 144.0, 144.0]);
            Ok(String::new())
        },
    },
    Action {
        id: "one-column",
        label: "One column",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_columns(1);
            Ok(String::new())
        },
    },
    Action {
        id: "two-columns",
        label: "Two columns",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_columns(2);
            Ok(String::new())
        },
    },
    Action {
        id: "three-columns",
        label: "Three columns",
        level: Level::Page,
        slash: false,
        run: |d| {
            d.session.set_columns(3);
            Ok(String::new())
        },
    },
    Action {
        id: "page-numbers",
        label: "Page numbers in the footer",
        level: Level::Page,
        slash: false,
        run: page_numbers,
    },
    // References
    Action {
        id: "table-of-contents",
        label: "Table of contents",
        level: Level::References,
        slash: true,
        run: |d| {
            let pages = d.pages.clone();
            d.session.insert_toc(3, &pages);
            Ok(String::new())
        },
    },
    Action {
        id: "footnote",
        label: "Footnote",
        level: Level::References,
        slash: true,
        run: |d| {
            d.session.insert_note(false, "");
            Ok(String::new())
        },
    },
    Action {
        id: "endnote",
        label: "Endnote",
        level: Level::References,
        slash: false,
        run: |d| {
            d.session.insert_note(true, "");
            Ok(String::new())
        },
    },
    Action {
        id: "update-fields",
        label: "Update fields",
        level: Level::References,
        slash: false,
        run: |d| {
            let pages = d.pages.clone();
            let count = d.session.update_fields(&pages);
            Ok(format!("{count} fields updated"))
        },
    },
    Action {
        id: "caption",
        label: "Caption",
        level: Level::References,
        slash: true,
        run: |d| {
            let pages = d.pages.clone();
            d.session.insert_caption("Figure", "", true, &pages);
            Ok(String::new())
        },
    },
    Action {
        id: "table-of-figures",
        label: "Table of figures",
        level: Level::References,
        slash: false,
        run: |d| {
            let pages = d.pages.clone();
            let options = unluminous_word::references::TocOptions {
                caption: Some("Figure".into()),
                ..Default::default()
            };
            d.session.insert_toc_with(&options, &pages);
            Ok(String::new())
        },
    },
    Action {
        id: "update-table",
        label: "Update table of contents",
        level: Level::References,
        slash: false,
        run: |d| {
            let pages = d.pages.clone();
            let count = d.session.update_tocs(&pages);
            Ok(format!("{count} tables updated"))
        },
    },
    Action {
        id: "remove-table-of-contents",
        label: "Remove table of contents",
        level: Level::References,
        slash: false,
        run: |d| {
            if d.session.remove_toc() {
                Ok("Table of contents removed".into())
            } else {
                Err("there is no table of contents".into())
            }
        },
    },
    Action {
        id: "unlink-fields",
        label: "Unlink fields",
        level: Level::References,
        slash: false,
        run: |d| Ok(format!("{} fields replaced with their results", d.session.unlink_fields())),
    },
    Action {
        id: "lock-fields",
        label: "Lock fields",
        level: Level::References,
        slash: false,
        run: |d| Ok(format!("{} fields locked", d.session.lock_fields(true))),
    },
    Action {
        id: "unlock-fields",
        label: "Unlock fields",
        level: Level::References,
        slash: false,
        run: |d| Ok(format!("{} fields unlocked", d.session.lock_fields(false))),
    },
    Action {
        id: "remove-link",
        label: "Remove link",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.remove_hyperlink().map(|()| "Link removed".to_owned()),
    },
    // Review
    Action {
        id: "track-changes",
        label: "Track changes",
        level: Level::Review,
        slash: false,
        run: |d| {
            let on = !d.session.tracking;
            d.session.set_tracking(on);
            Ok(if on { "Track Changes is on".into() } else { "Track Changes is off".into() })
        },
    },
    Action {
        id: "accept-change",
        label: "Accept change",
        level: Level::Review,
        slash: false,
        run: |d| {
            let n = d.session.resolve_revisions(true, false);
            Ok(format!("{n} accepted"))
        },
    },
    Action {
        id: "reject-change",
        label: "Reject change",
        level: Level::Review,
        slash: false,
        run: |d| {
            let n = d.session.resolve_revisions(false, false);
            Ok(format!("{n} rejected"))
        },
    },
    Action {
        id: "accept-all",
        label: "Accept all changes",
        level: Level::Review,
        slash: false,
        run: |d| {
            let n = d.session.resolve_revisions(true, true);
            Ok(format!("{n} accepted"))
        },
    },
    Action {
        id: "reject-all",
        label: "Reject all changes",
        level: Level::Review,
        slash: false,
        run: |d| {
            let n = d.session.resolve_revisions(false, true);
            Ok(format!("{n} rejected"))
        },
    },
    Action {
        id: "comment",
        label: "Comment",
        level: Level::Review,
        slash: true,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Review];
            d.view.fields.insert("comment", String::new());
            done("Type the comment in the panel")
        },
    },
    Action {
        id: "spelling",
        label: "Spelling (F7)",
        level: Level::Review,
        slash: false,
        run: super::spelling::next,
    },
    Action {
        id: "check-spelling",
        label: "Check spelling as you type",
        level: Level::Review,
        slash: false,
        run: |d| {
            d.view.spelling = !d.view.spelling;
            d.view.spell_menu = None;
            Ok(if d.view.spelling {
                "Spell check is on".into()
            } else {
                "Spell check is off".into()
            })
        },
    },
    // Document
    Action {
        id: "field-codes",
        label: "Show field codes",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.codes = !d.view.codes;
            d.laid = 0;
            Ok(String::new())
        },
    },
    Action {
        id: "formatting-marks",
        label: "Show formatting marks",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.marks = !d.view.marks;
            d.laid = 0;
            Ok(String::new())
        },
    },
    Action {
        id: "navigation",
        label: "Navigation pane",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.navigation = !d.view.navigation;
            Ok(String::new())
        },
    },
    Action {
        id: "zoom-in",
        label: "Zoom in",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.zoom = (d.view.zoom * 1.1).min(5.0);
            Ok(String::new())
        },
    },
    Action {
        id: "zoom-out",
        label: "Zoom out",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.zoom = (d.view.zoom / 1.1).max(0.25);
            Ok(String::new())
        },
    },
    Action {
        id: "zoom-100",
        label: "Zoom 100%",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.zoom = 1.0;
            Ok(String::new())
        },
    },
    Action {
        id: "find",
        label: "Find",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.find = Some(Default::default());
            Ok(String::new())
        },
    },
    Action {
        id: "replace",
        label: "Replace",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.find = Some(super::Find { replace: true, ..Default::default() });
            Ok(String::new())
        },
    },
    Action {
        id: "select-all",
        label: "Select all",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.session.select_all();
            Ok(String::new())
        },
    },
    Action {
        id: "undo",
        label: "Undo",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.session.undo();
            Ok(String::new())
        },
    },
    Action {
        id: "redo",
        label: "Redo",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.session.redo();
            Ok(String::new())
        },
    },
    Action {
        id: "export-pdf",
        label: "Export as PDF",
        level: Level::Document,
        slash: false,
        run: |d| export(d, "pdf"),
    },
    Action {
        id: "export-html",
        label: "Export as HTML",
        level: Level::Document,
        slash: false,
        run: |d| export(d, "html"),
    },
    Action {
        id: "export-md",
        label: "Export as Markdown",
        level: Level::Document,
        slash: false,
        run: |d| export(d, "md"),
    },
    Action {
        id: "export-txt",
        label: "Export as plain text",
        level: Level::Document,
        slash: false,
        run: |d| export(d, "txt"),
    },
    Action {
        id: "text-box",
        label: "Text box",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_textbox(144.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "shape-rect",
        label: "Shape: Rectangle",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("rect", 72.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "shape-roundrect",
        label: "Shape: Rounded rectangle",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("roundRect", 72.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "shape-ellipse",
        label: "Shape: Oval",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("ellipse", 72.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "shape-line",
        label: "Shape: Line",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("line", 144.0, 0.0).map(|()| String::new()),
    },
    Action {
        id: "shape-straightconnector1",
        label: "Shape: Straight connector",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("straightConnector1", 144.0, 0.0).map(|()| String::new()),
    },
    Action {
        id: "shape-rightarrow",
        label: "Shape: Right arrow",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("rightArrow", 72.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "shape-leftarrow",
        label: "Shape: Left arrow",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("leftArrow", 72.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "shape-uparrow",
        label: "Shape: Up arrow",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("upArrow", 72.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "shape-downarrow",
        label: "Shape: Down arrow",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("downArrow", 72.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "shape-triangle",
        label: "Shape: Triangle",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("triangle", 72.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "shape-diamond",
        label: "Shape: Diamond",
        level: Level::Insert,
        slash: true,
        run: |d| d.session.insert_shape("diamond", 72.0, 72.0).map(|()| String::new()),
    },
    Action {
        id: "wrap-inline",
        label: "Wrap text: In line with text",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.set_object_wrap("inline").map(|()| String::new()),
    },
    Action {
        id: "wrap-square",
        label: "Wrap text: Square",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.set_object_wrap("square").map(|()| String::new()),
    },
    Action {
        id: "wrap-tight",
        label: "Wrap text: Tight",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.set_object_wrap("tight").map(|()| String::new()),
    },
    Action {
        id: "wrap-through",
        label: "Wrap text: Through",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.set_object_wrap("through").map(|()| String::new()),
    },
    Action {
        id: "wrap-top-and-bottom",
        label: "Wrap text: Top and bottom",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.set_object_wrap("top-and-bottom").map(|()| String::new()),
    },
    Action {
        id: "wrap-behind",
        label: "Wrap text: Behind text",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.set_object_wrap("behind").map(|()| String::new()),
    },
    Action {
        id: "wrap-front",
        label: "Wrap text: In front of text",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.set_object_wrap("front").map(|()| String::new()),
    },
    Action {
        id: "bring-forward",
        label: "Bring forward",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.order_object("forward").map(|()| String::new()),
    },
    Action {
        id: "send-backward",
        label: "Send backward",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.order_object("backward").map(|()| String::new()),
    },
    Action {
        id: "bring-to-front",
        label: "Bring to front",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.order_object("front").map(|()| String::new()),
    },
    Action {
        id: "send-to-back",
        label: "Send to back",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.order_object("back").map(|()| String::new()),
    },
    Action {
        id: "crop",
        label: "Crop",
        level: Level::Picture,
        slash: false,
        run: |d| {
            d.view.cropping = !d.view.cropping && d.session.object_info().is_some_and(|i| i.kind == "Picture");
            done(if d.view.cropping { "Drag a handle to crop" } else { "" })
        },
    },
    Action {
        id: "reset-picture",
        label: "Reset picture",
        level: Level::Picture,
        slash: false,
        run: |d| d.session.reset_object().map(|()| String::new()),
    },
    Action {
        id: "picture-properties",
        label: "Size and position",
        level: Level::Picture,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Insert, Level::Picture];
            done("")
        },
    },
    Action {
        id: "alt-text",
        label: "Edit alt text",
        level: Level::Picture,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Insert, Level::Picture];
            done("")
        },
    },
    Action {
        id: "font-dialog",
        label: "Font",
        level: Level::Text,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Text];
            done("")
        },
    },
    Action {
        id: "paragraph-dialog",
        label: "Paragraph",
        level: Level::Paragraph,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Paragraph];
            done("")
        },
    },
    Action {
        id: "styles-level",
        label: "Styles",
        level: Level::Styles,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Styles];
            done("")
        },
    },
    Action {
        id: "insert-link",
        label: "Link",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Insert];
            d.view.fields.insert("link", String::new());
            done("Type the address in the panel")
        },
    },
    Action {
        id: "edit-link",
        label: "Edit hyperlink",
        level: Level::Insert,
        slash: false,
        run: |d| {
            let at = d.session.selection.head.clone();
            let (address, _) = d.session.hyperlink_at(&at).ok_or("the caret is not on a link")?;
            d.view.panel = vec![Level::Root, Level::Insert];
            d.view.fields.insert("link", address);
            done("Change the address in the panel")
        },
    },
    Action {
        id: "insert-row-above",
        label: "Insert row above",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.table_op("row-above").map(|()| String::new()),
    },
    Action {
        id: "insert-row-below",
        label: "Insert row below",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.table_op("row-below").map(|()| String::new()),
    },
    Action {
        id: "insert-column-left",
        label: "Insert column to the left",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.table_op("column-left").map(|()| String::new()),
    },
    Action {
        id: "insert-column-right",
        label: "Insert column to the right",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.table_op("column-right").map(|()| String::new()),
    },
    Action {
        id: "delete-row",
        label: "Delete row",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.table_op("delete-row").map(|()| String::new()),
    },
    Action {
        id: "delete-column",
        label: "Delete column",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.table_op("delete-column").map(|()| String::new()),
    },
    Action {
        id: "delete-table",
        label: "Delete table",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.table_op("delete-table").map(|()| String::new()),
    },
    Action {
        id: "split-cells",
        label: "Split cells",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.table_op("split").map(|()| String::new()),
    },
    Action {
        id: "table-properties",
        label: "Table properties",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Insert, Level::TableProperties];
            done("")
        },
    },
    Action {
        id: "borders-and-shading",
        label: "Borders and shading",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Insert, Level::TableProperties];
            done("")
        },
    },
    Action {
        id: "update-field",
        label: "Update field",
        level: Level::References,
        slash: false,
        run: |d| {
            let pages = d.pages.clone();
            let count = d.session.update_selected_fields(&pages);
            Ok(format!("{count} fields updated"))
        },
    },
    Action {
        id: "edit-field",
        label: "Edit field",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Insert];
            d.view.fields.insert("field", String::new());
            done("Type the field code in the panel")
        },
    },
    Action {
        id: "go-to-note",
        label: "Go to note",
        level: Level::References,
        slash: false,
        run: |d| {
            let at = d.session.selection.head.clone();
            let story = d.session.note_at(&at).ok_or("the caret is not at a note reference")?;
            let pos = unluminous_word::position::Pos { story, path: vec![0], offset: 0 };
            d.session.select(pos.clone(), pos);
            done("")
        },
    },
    Action {
        id: "convert-note",
        label: "Convert note",
        level: Level::References,
        slash: false,
        run: |d| d.session.convert_note().map(|()| String::new()),
    },
    Action {
        id: "remove-header",
        label: "Remove header or footer",
        level: Level::Page,
        slash: false,
        run: |d| d.session.remove_header_at_caret().map(|()| String::new()),
    },
    Action {
        id: "close-header",
        label: "Close header and footer",
        level: Level::Page,
        slash: false,
        run: |d| {
            let first = d.pages.pages.iter().find_map(|p| p.first.clone()).unwrap_or_default();
            d.session.select(first.clone(), first);
            done("")
        },
    },
    Action {
        id: "list-bullet",
        label: "List Bullet style",
        level: Level::Lists,
        slash: false,
        run: |d| style(d, "ListBullet"),
    },
    Action {
        id: "hanging-indent",
        label: "Hanging indent",
        level: Level::Paragraph,
        slash: false,
        run: |d| {
            d.session.hanging_indent(true);
            done("")
        },
    },
    Action {
        id: "reduce-hanging-indent",
        label: "Reduce hanging indent",
        level: Level::Paragraph,
        slash: false,
        run: |d| {
            d.session.hanging_indent(false);
            done("")
        },
    },
    Action {
        id: "reset-paragraph",
        label: "Reset paragraph formatting",
        level: Level::Paragraph,
        slash: false,
        run: |d| {
            d.session.reset_paragraph();
            done("")
        },
    },
    Action {
        id: "toggle-space-before",
        label: "Twelve points before",
        level: Level::Paragraph,
        slash: false,
        run: |d| {
            d.session.toggle_space_before();
            done("")
        },
    },
    Action {
        id: "double-underline",
        label: "Double underline",
        level: Level::Text,
        slash: false,
        run: |d| underline_kind(d, "double"),
    },
    Action {
        id: "underline-words",
        label: "Underline words only",
        level: Level::Text,
        slash: false,
        run: |d| underline_kind(d, "words"),
    },
    Action {
        id: "symbol-font",
        label: "Symbol font",
        level: Level::Text,
        slash: false,
        run: |d| {
            d.session.symbol_font();
            done("")
        },
    },
    Action {
        id: "cycle-case",
        label: "Change case",
        level: Level::Text,
        slash: false,
        run: |d| {
            let text = d.session.selected_text();
            let how = if text.chars().any(char::is_lowercase) && text.chars().any(char::is_uppercase) {
                "lower"
            } else if text.chars().any(char::is_lowercase) {
                "upper"
            } else {
                "title"
            };
            d.session.change_case(how);
            done("")
        },
    },
    Action {
        id: "grow-font-one",
        label: "Grow font one point",
        level: Level::Text,
        slash: false,
        run: |d| font_step(d, 1.0),
    },
    Action {
        id: "shrink-font-one",
        label: "Shrink font one point",
        level: Level::Text,
        slash: false,
        run: |d| font_step(d, -1.0),
    },
    Action {
        id: "bookmark-dialog",
        label: "Bookmark",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Insert];
            d.view.fields.insert("bookmark", String::new());
            done("Type the bookmark name in the panel")
        },
    },
    Action {
        id: "next-field",
        label: "Next field",
        level: Level::References,
        slash: false,
        run: |d| d.session.select_next_field(true).map(|()| String::new()),
    },
    Action {
        id: "previous-field",
        label: "Previous field",
        level: Level::References,
        slash: false,
        run: |d| d.session.select_next_field(false).map(|()| String::new()),
    },
    Action {
        id: "ellipsis",
        label: "Ellipsis",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.type_text("\u{2026}");
            done("")
        },
    },
    Action {
        id: "copyright",
        label: "Copyright sign",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.type_text("\u{a9}");
            done("")
        },
    },
    Action {
        id: "registered",
        label: "Registered sign",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.type_text("\u{ae}");
            done("")
        },
    },
    Action {
        id: "trademark",
        label: "Trademark sign",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.type_text("\u{2122}");
            done("")
        },
    },
    Action {
        id: "nonbreaking-space",
        label: "Nonbreaking space",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.type_text("\u{a0}");
            done("")
        },
    },
    Action {
        id: "em-dash",
        label: "Em dash",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.type_text("\u{2014}");
            done("")
        },
    },
    Action {
        id: "en-dash",
        label: "En dash",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.type_text("\u{2013}");
            done("")
        },
    },
    Action {
        id: "section-sign",
        label: "Section sign",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.type_text("\u{a7}");
            done("")
        },
    },
    Action {
        id: "paragraph-sign",
        label: "Paragraph sign",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.type_text("\u{b6}");
            done("")
        },
    },
    Action {
        id: "nonbreaking-hyphen",
        label: "Nonbreaking hyphen",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.insert_hyphen(false);
            done("")
        },
    },
    Action {
        id: "optional-hyphen",
        label: "Optional hyphen",
        level: Level::Insert,
        slash: false,
        run: |d| {
            d.session.insert_hyphen(true);
            done("")
        },
    },
    Action {
        id: "unicode-toggle",
        label: "Toggle character code",
        level: Level::Insert,
        slash: false,
        run: |d| d.session.toggle_unicode().map(|()| String::new()),
    },
    Action {
        id: "promote-heading",
        label: "Promote heading",
        level: Level::Styles,
        slash: false,
        run: |d| d.session.promote_heading(true).map(|()| String::new()),
    },
    Action {
        id: "demote-heading",
        label: "Demote heading",
        level: Level::Styles,
        slash: false,
        run: |d| d.session.promote_heading(false).map(|()| String::new()),
    },
    Action {
        id: "move-paragraph-up",
        label: "Move paragraph up",
        level: Level::Paragraph,
        slash: false,
        run: |d| d.session.move_paragraph(true).map(|()| String::new()),
    },
    Action {
        id: "move-paragraph-down",
        label: "Move paragraph down",
        level: Level::Paragraph,
        slash: false,
        run: |d| d.session.move_paragraph(false).map(|()| String::new()),
    },
    Action {
        id: "print",
        label: "Print",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Document, Level::Print];
            done("")
        },
    },
    Action {
        id: "print-now",
        label: "Print every page",
        level: Level::Document,
        slash: false,
        run: |d| super::print::print(d, &super::print::PrintOptions::default()),
    },
    Action {
        id: "ruler",
        label: "Ruler",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.ruler = !d.view.ruler;
            done("")
        },
    },
    Action {
        id: "read-mode",
        label: "Read Mode",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.mode = super::Mode::Read;
            d.laid = u64::MAX;
            done("")
        },
    },
    Action {
        id: "print-layout",
        label: "Print Layout",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.mode = super::Mode::Print;
            d.laid = u64::MAX;
            done("")
        },
    },
    Action {
        id: "outline-view",
        label: "Outline",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.mode = super::Mode::Outline;
            done("")
        },
    },
    Action {
        id: "fit-width",
        label: "Page width",
        level: Level::Document,
        slash: false,
        run: |d| {
            fit(d, false);
            done("")
        },
    },
    Action {
        id: "fit-page",
        label: "One page",
        level: Level::Document,
        slash: false,
        run: |d| {
            fit(d, true);
            done("")
        },
    },
    Action {
        id: "multiple-pages",
        label: "Multiple pages",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.multiple = !d.view.multiple;
            done("")
        },
    },
    Action {
        id: "gridlines",
        label: "Gridlines",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.gridlines = !d.view.gridlines;
            done("")
        },
    },
    Action {
        id: "new-window",
        label: "New Window",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.new_window = true;
            done("")
        },
    },
    Action {
        id: "zoom-dialog",
        label: "Zoom",
        level: Level::Document,
        slash: false,
        run: |d| {
            d.view.panel = vec![Level::Root, Level::Document];
            done("")
        },
    },
    Action {
        id: "outline-to-heading-1",
        label: "Promote to Heading 1",
        level: Level::Styles,
        slash: false,
        run: |d| style(d, "Heading1"),
    },
    Action {
        id: "outline-to-body",
        label: "Demote to body text",
        level: Level::Styles,
        slash: false,
        run: |d| style(d, "Normal"),
    },
    Action {
        id: "outline-move-up",
        label: "Move up",
        level: Level::Styles,
        slash: false,
        run: |d| outline_move(d, true),
    },
    Action {
        id: "outline-move-down",
        label: "Move down",
        level: Level::Styles,
        slash: false,
        run: |d| outline_move(d, false),
    },
    Action {
        id: "outline-expand",
        label: "Expand",
        level: Level::Document,
        slash: false,
        run: |d| {
            crate::components::word::outline::fold_at_caret(d, true);
            done("")
        },
    },
    Action {
        id: "outline-collapse",
        label: "Collapse",
        level: Level::Document,
        slash: false,
        run: |d| {
            crate::components::word::outline::fold_at_caret(d, false);
            done("")
        },
    },
];

/// The command with this id.
/// Whether an action can be used where the caret is. One that cannot is left out of the lists rather
/// than dimmed: a comment cannot be added in a header or footer, a link can only be removed from a link,
/// and the table of contents can only be updated or removed when there is one.
pub fn applies(id: &str, doc: &Doc) -> bool {
    use unluminous_word::position::StoryId;
    let head = &doc.session.selection.head;
    match id {
        "comment" => !matches!(head.story, StoryId::Header(_)),
        "remove-link" => doc.session.hyperlink_at(head).is_some(),
        "merge-cells" | "split-table" | "distribute-rows" | "distribute-columns"
        | "table-to-text" | "sort-table" | "select-row" | "select-column" | "select-table" => {
            doc.session.cell_at_caret().is_some()
        }
        id if id.starts_with("wrap-")
            || matches!(
                id,
                "bring-forward" | "send-backward" | "bring-to-front" | "send-to-back" | "picture-properties" | "alt-text"
            ) =>
        {
            doc.session.selected_object().is_some()
        }
        "crop" | "reset-picture" => doc.session.object_info().is_some_and(|i| i.kind == "Picture"),
        "insert-row-above" | "insert-row-below" | "insert-column-left" | "insert-column-right" | "delete-row"
        | "delete-column" | "delete-table" | "split-cells" | "table-properties" | "borders-and-shading" => {
            doc.session.cell_at_caret().is_some()
        }
        "edit-link" => doc.session.hyperlink_at(head).is_some(),
        "go-to-note" | "convert-note" => doc.session.note_at(head).is_some(),
        "remove-header" | "close-header" => matches!(head.story, StoryId::Header(_)),
        "update-table" | "remove-table-of-contents" => {
            !unluminous_word::references::find_tocs(&doc.session.document).is_empty()
        }
        _ => true,
    }
}

/// Whether F4 repeats an action: one that formats the selection or puts something in, not one that
/// moves, finds, undoes, opens a view or exports.
pub fn repeatable(action: &Action) -> bool {
    matches!(
        action.level,
        Level::Text | Level::Paragraph | Level::Styles | Level::Lists | Level::Insert
    ) && !matches!(action.id, "copy-formatting" | "find" | "replace" | "select-all")
}

pub fn find(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|action| action.id == id)
}

/// The commands whose name or label holds every word of `query`, best first: a label that starts with
/// the query, then one with a word that does, then the rest.
pub fn search(query: &str, slash_only: bool) -> Vec<&'static Action> {
    let query = query.trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut found: Vec<(u8, &'static Action)> = ACTIONS
        .iter()
        .filter(|a| !slash_only || a.slash)
        .filter_map(|action| {
            let label = action.label.to_lowercase();
            let haystack = format!("{label} {} {}", action.id, action.level.name().to_lowercase());
            if !words.iter().all(|w| haystack.contains(w)) {
                return None;
            }
            let rank = if query.is_empty() || label.starts_with(&query) {
                0
            } else if label
                .split_whitespace()
                .any(|w| w.starts_with(words.first().copied().unwrap_or("")))
            {
                1
            } else {
                2
            };
            Some((rank, action))
        })
        .collect();
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().map(|(_, a)| a).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_is_unique_and_kebab_case() {
        let mut seen = std::collections::HashSet::new();
        for action in ACTIONS {
            assert!(seen.insert(action.id), "{} twice", action.id);
            assert!(
                action.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                action.id
            );
        }
    }

    #[test]
    fn a_search_finds_by_any_word_and_puts_the_best_match_first() {
        assert_eq!(search("land", false).first().map(|a| a.id), Some("landscape"));
        assert_eq!(search("line spacing 1.5", false).first().map(|a| a.id), Some("line-15"));
        assert!(search("head", true).iter().all(|a| a.slash));
        assert!(search("zzzz", false).is_empty());
    }
}
