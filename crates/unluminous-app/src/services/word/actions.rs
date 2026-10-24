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

fn style(doc: &mut Doc, id: &'static str) -> Result<String, String> {
    doc.session.apply_style(id)?;
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
            let shown = unluminous_word::ops::format_date(&now, "\\@ \"d MMMM yyyy\"", "DATE");
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
            let _ = d.session.apply_style("Caption");
            d.session.type_text("Figure ");
            let count = 1 + unluminous_word::position::paragraphs(&d.session.document.body).iter().filter(|(_, p)| p.runs.iter().any(|r| matches!(&r.content, unluminous_word::model::Content::Instr(c) if c.contains("SEQ Figure")))).count();
            d.session.insert_field("SEQ Figure \\* ARABIC", &count.to_string());
            Ok(String::new())
        },
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
];

/// The command with this id.
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
