//! A document as plain text, Markdown or HTML.
//!
//! Each walks the body in order and asks the styles what each paragraph is: a heading by its outline
//! level, a list item by its list and level, a table as a table. Run formatting that has a meaning in the
//! target (bold, italic, strikethrough, a link) is kept; the rest is dropped.

use crate::model::*;
use crate::position;
use crate::props::Align;
use crate::styles::Resolver;

/// The document's text, a paragraph a line, a table's cells separated by tabs.
pub fn to_text(document: &Document) -> String {
    let mut out = String::new();
    write_text(&document.body, &mut out);
    out
}

fn write_text(blocks: &[Block], out: &mut String) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                out.push_str(&crate::session::clean_text(&p.visible_text()));
                out.push('\n');
            }
            Block::Table(t) => {
                for row in &t.rows {
                    let cells: Vec<String> = row
                        .cells
                        .iter()
                        .map(|c| {
                            let mut text = String::new();
                            write_text(&c.blocks, &mut text);
                            text.trim_end().replace('\n', " ")
                        })
                        .collect();
                    out.push_str(&cells.join("\t"));
                    out.push('\n');
                }
            }
            Block::Sdt(s) => write_text(&s.blocks, out),
            Block::Kept(_) => {}
        }
    }
}

/// The runs of a paragraph as inline Markdown or HTML, by the function that wraps one run's text.
fn inline(document: &Document, resolver: &Resolver, paragraph: &Paragraph, wrap: &dyn Fn(&str, &crate::props::RunLook, Option<String>) -> String) -> String {
    let mut out = String::new();
    let mut in_code = 0usize;
    let mut after_separate = Vec::new();
    for run in &paragraph.runs {
        match &run.content {
            Content::FieldChar(el) => {
                match el.attr("w:fldCharType") {
                    Some("begin") => {
                        in_code += 1;
                        after_separate.push(false);
                    }
                    Some("separate") => {
                        if let Some(last) = after_separate.last_mut() {
                            *last = true;
                            in_code = in_code.saturating_sub(1);
                        }
                    }
                    _ => {
                        if after_separate.pop() == Some(false) {
                            in_code = in_code.saturating_sub(1);
                        }
                    }
                }
                continue;
            }
            Content::Instr(_) => continue,
            _ if in_code > 0 => continue,
            _ if run.is_deleted() => continue,
            _ => {}
        }
        let look = resolver.run(&paragraph.props, &run.props, None);
        if look.hidden {
            continue;
        }
        let text = match &run.content {
            Content::Text(text) => text.clone(),
            Content::Tab => "\t".into(),
            Content::Break(_) => "\n".into(),
            other => match other.character() {
                Some(c) if c != OBJECT => c.to_string(),
                _ => continue,
            },
        };
        let link = run.link().and_then(|el| match (el.attr("r:id"), el.attr("w:anchor")) {
            (Some(id), _) => document.package.rels.get(&document.main).and_then(|r| r.by_id(id)).map(|r| r.target.clone()),
            (None, Some(anchor)) => Some(format!("#{anchor}")),
            _ => None,
        });
        out.push_str(&wrap(&text, &look, link));
    }
    out
}

/// The document as Markdown: headings by outline level, lists with their markers and indents, tables as
/// pipe tables, bold, italic, strikethrough and links.
pub fn to_markdown(document: &Document) -> String {
    let resolver = Resolver::new(document);
    let mut counters = crate::numbering::Counters::default();
    let mut out = String::new();
    markdown_blocks(document, &resolver, &mut counters, &document.body, &mut out);
    out.trim_end().to_owned() + "\n"
}

fn markdown_blocks(document: &Document, resolver: &Resolver, counters: &mut crate::numbering::Counters, blocks: &[Block], out: &mut String) {
    let escape = |text: &str| text.replace('\\', "\\\\").replace('*', "\\*").replace('_', "\\_").replace('`', "\\`");
    let wrap = |text: &str, look: &crate::props::RunLook, link: Option<String>| {
        let core = escape(text);
        let trimmed = core.trim();
        if trimmed.is_empty() {
            return core;
        }
        let (lead, trail) = (&core[..core.len() - core.trim_start().len()], &core[core.trim_end().len()..]);
        let mut body = trimmed.to_owned();
        if look.strike {
            body = format!("~~{body}~~");
        }
        if look.italic {
            body = format!("*{body}*");
        }
        if look.bold {
            body = format!("**{body}**");
        }
        if let Some(link) = link {
            body = format!("[{body}]({link})");
        }
        format!("{lead}{body}{trail}")
    };
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                let look = resolver.paragraph(&p.props, None);
                let text = inline(document, resolver, p, &wrap);
                let text = text.replace('\n', "  \n");
                if let Some((num, level)) = look.numbering {
                    let label = counters.next(&resolver.numbering, num, level);
                    let bullet = label.as_ref().is_some_and(|l| !l.text.chars().any(|c| c.is_ascii_alphanumeric()));
                    let marker = if bullet { "-".to_owned() } else { "1.".to_owned() };
                    out.push_str(&"   ".repeat(usize::from(level)));
                    out.push_str(&marker);
                    out.push(' ');
                    out.push_str(text.trim());
                    out.push('\n');
                    continue;
                }
                if text.trim().is_empty() {
                    out.push('\n');
                    continue;
                }
                match look.outline_level {
                    Some(level) if level < 6 => out.push_str(&format!("{} {}\n\n", "#".repeat(usize::from(level) + 1), text.trim())),
                    _ if look.style == "Title" => out.push_str(&format!("# {}\n\n", text.trim())),
                    _ if look.style.contains("Quote") => out.push_str(&format!("> {}\n\n", text.trim())),
                    _ => out.push_str(&format!("{}\n\n", text.trim())),
                }
            }
            Block::Table(t) => {
                for (index, row) in t.rows.iter().enumerate() {
                    let cells: Vec<String> = row
                        .cells
                        .iter()
                        .map(|c| {
                            let mut inner = String::new();
                            markdown_blocks(document, resolver, counters, &c.blocks, &mut inner);
                            inner.trim().replace('\n', "<br>").replace('|', "\\|")
                        })
                        .collect();
                    out.push_str(&format!("| {} |\n", cells.join(" | ")));
                    if index == 0 {
                        out.push_str(&format!("|{}\n", " --- |".repeat(cells.len())));
                    }
                }
                out.push('\n');
            }
            Block::Sdt(s) => markdown_blocks(document, resolver, counters, &s.blocks, out),
            Block::Kept(_) => {}
        }
    }
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The document as one HTML page, with its fonts, sizes, colours and alignment inline, and pictures
/// embedded, so it opens in a browser as it looks.
pub fn to_html(document: &Document) -> String {
    let resolver = Resolver::new(document);
    let mut counters = crate::numbering::Counters::default();
    let title = crate::new::property(document, "dc:title").unwrap_or_else(|| "Document".to_owned());
    let mut body = String::new();
    html_blocks(document, &resolver, &mut counters, &document.body, &mut body);
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title>\n<style>body{{max-width:46em;margin:3em auto;padding:0 1em;line-height:1.3}}table{{border-collapse:collapse}}td,th{{border:1px solid #999;padding:.2em .5em;vertical-align:top}}p{{margin:0 0 .5em}}</style></head>\n<body>\n{}</body></html>\n",
        html_escape(&title),
        body
    )
}

fn html_blocks(document: &Document, resolver: &Resolver, counters: &mut crate::numbering::Counters, blocks: &[Block], out: &mut String) {
    let wrap = |text: &str, look: &crate::props::RunLook, link: Option<String>| {
        let mut style = format!("font-family:'{}';font-size:{}pt", look.font, look.size);
        if let Some(c) = look.color {
            style.push_str(&format!(";color:#{}", c.hex()));
        }
        if let Some(h) = look.highlight.or(look.shading) {
            style.push_str(&format!(";background:#{}", h.hex()));
        }
        if look.caps {
            style.push_str(";text-transform:uppercase");
        }
        if look.small_caps {
            style.push_str(";font-variant:small-caps");
        }
        let mut body = format!("<span style=\"{style}\">{}</span>", html_escape(text).replace('\n', "<br>").replace('\t', "&emsp;"));
        if look.bold {
            body = format!("<b>{body}</b>");
        }
        if look.italic {
            body = format!("<i>{body}</i>");
        }
        if look.underline != crate::props::Underline::None {
            body = format!("<u>{body}</u>");
        }
        if look.strike || look.double_strike {
            body = format!("<s>{body}</s>");
        }
        match look.vert {
            crate::props::VertAlign::Superscript => body = format!("<sup>{body}</sup>"),
            crate::props::VertAlign::Subscript => body = format!("<sub>{body}</sub>"),
            _ => {}
        }
        if let Some(link) = link {
            body = format!("<a href=\"{}\">{body}</a>", html_escape(&link));
        }
        body
    };
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                let look = resolver.paragraph(&p.props, None);
                let mut text = inline(document, resolver, p, &wrap);
                for run in &p.runs {
                    if let Some(picture) = run.content.picture() {
                        if let Some(img) = picture_tag(document, picture) {
                            text.push_str(&img);
                        }
                    }
                }
                let align = match look.align {
                    Align::Left => "left",
                    Align::Center => "center",
                    Align::Right => "right",
                    Align::Justify => "justify",
                };
                let mut style = format!("text-align:{align};margin-top:{}pt;margin-bottom:{}pt", look.space_before, look.space_after);
                if look.indent_left != 0.0 {
                    style.push_str(&format!(";margin-left:{}pt", look.indent_left));
                }
                if look.indent_first != 0.0 {
                    style.push_str(&format!(";text-indent:{}pt", look.indent_first));
                }
                let label = look.numbering.and_then(|(num, level)| counters.next(&resolver.numbering, num, level)).map(|l| format!("{}&emsp;", html_escape(&l.text))).unwrap_or_default();
                let tag = match look.outline_level {
                    Some(level) if level < 6 => format!("h{}", level + 1),
                    _ => "p".to_owned(),
                };
                out.push_str(&format!("<{tag} style=\"{style}\">{label}{}</{tag}>\n", if text.is_empty() { "&nbsp;".to_owned() } else { text }));
            }
            Block::Table(t) => {
                out.push_str("<table>\n");
                for row in &t.rows {
                    out.push_str("<tr>");
                    for cell in &row.cells {
                        if cell.props.child("w:vMerge").is_some_and(|m| m.attr("w:val").is_none_or(|v| v == "continue")) {
                            continue;
                        }
                        let span = cell.props.val_of("w:gridSpan").unwrap_or("1");
                        let mut inner = String::new();
                        html_blocks(document, resolver, counters, &cell.blocks, &mut inner);
                        out.push_str(&format!("<td colspan=\"{span}\">{inner}</td>"));
                    }
                    out.push_str("</tr>\n");
                }
                out.push_str("</table>\n");
            }
            Block::Sdt(s) => html_blocks(document, resolver, counters, &s.blocks, out),
            Block::Kept(_) => {}
        }
    }
}

/// An `<img>` with the picture's bytes in a data address, when they can be found.
fn picture_tag(document: &Document, picture: &crate::xml::El) -> Option<String> {
    let mut embed = None;
    picture.walk(&mut |el| {
        if embed.is_none() && el.name == "a:blip" {
            embed = el.attr("r:embed").map(str::to_owned);
        }
    });
    let rel = document.package.rels.get(&document.main)?.by_id(&embed?)?.clone();
    let name = crate::package::Package::resolve(&document.main, &rel.target);
    let bytes = document.package.part(&name)?;
    let mime = document.package.content_type(&name).unwrap_or("image/png");
    Some(format!("<img src=\"data:{mime};base64,{}\">", base64(bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Every paragraph of the body with what an agent needs to know about it, for `plugins run word read`.
pub fn paragraphs_value(document: &Document, from: usize, to: usize, with_runs: bool) -> Vec<serde_json::Value> {
    let resolver = Resolver::new(document);
    let mut counters = crate::numbering::Counters::default();
    let mut out = Vec::new();
    for (index, (path, p)) in position::paragraphs(&document.body).into_iter().enumerate() {
        let look = resolver.paragraph(&p.props, None);
        let label = look.numbering.and_then(|(num, level)| counters.next(&resolver.numbering, num, level));
        if index + 1 < from || index + 1 > to {
            continue;
        }
        let mut value = serde_json::json!({
            "index": index + 1,
            "path": path,
            "style": resolver.name_of(&look.style),
            "text": crate::session::clean_text(&p.visible_text()),
            "length": p.len(),
        });
        if let Some(label) = label {
            value["list"] = serde_json::json!({ "label": label.text, "level": look.numbering.map(|(_, l)| l) });
        }
        if let Some((_, row, cell)) = position::cell_of(&document.body, &path) {
            value["cell"] = serde_json::json!({ "row": row + 1, "column": cell + 1 });
        }
        if look.align != Align::Left {
            value["align"] = serde_json::json!(look.align.name());
        }
        if with_runs {
            let mut at = 0;
            let runs: Vec<serde_json::Value> = p
                .runs
                .iter()
                .map(|run| {
                    let r = resolver.run(&p.props, &run.props, None);
                    let start = at;
                    at += run.len();
                    let text = match &run.content {
                        Content::Text(t) | Content::Instr(t) => t.clone(),
                        other => other.character().map(String::from).unwrap_or_default(),
                    };
                    serde_json::json!({
                        "offset": start, "text": text, "font": r.font.as_ref(), "size": r.size, "bold": r.bold, "italic": r.italic,
                        "underline": r.underline.name(), "color": r.color.map(|c| c.hex()), "highlight": r.highlight.map(|c| c.hex()),
                        "inserted": run.is_inserted(), "deleted": run.is_deleted(), "link": run.link().is_some(),
                    })
                })
                .collect();
            value["runs"] = serde_json::Value::Array(runs);
        }
        out.push(value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> crate::session::Session {
        let mut s = crate::session::Session::new(crate::new::blank());
        s.type_text("Report\nSome *plain* & bold text\nitem one\nitem two");
        s.select(crate::position::Pos::new(vec![0], 0), crate::position::Pos::new(vec![0], 0));
        let _ = s.apply_style("Heading1");
        s.select(crate::position::Pos::new(vec![1], 15), crate::position::Pos::new(vec![1], 19));
        s.toggle("w:b");
        s.select(crate::position::Pos::new(vec![2], 0), crate::position::Pos::new(vec![3], 0));
        s.toggle_list(crate::builtin::ListKind::Bullet);
        s
    }

    #[test]
    fn markdown_has_headings_lists_bold_and_escapes() {
        let md = to_markdown(&sample().document);
        assert!(md.contains("# Report"), "{md}");
        assert!(md.contains("**bold**"), "{md}");
        assert!(md.contains("\\*plain\\*"), "{md}");
        assert!(md.contains("- item one\n- item two"), "{md}");
    }

    #[test]
    fn html_and_text_are_whole_documents() {
        let s = sample();
        let html = to_html(&s.document);
        assert!(html.starts_with("<!doctype html>") && html.contains("<h1") && html.contains("<b>"));
        assert!(html.contains("&amp;"));
        assert_eq!(to_text(&s.document).lines().count(), 4);
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }
}
