//! Text that is not a Word document, made into one (task-2359, TDD 5.6 and Insert's Text from File):
//! plain text as paragraphs in Normal, Markdown as headings, lists, quotes, code, links and pictures,
//! and HTML through the clipboard's HTML reader, which Markdown goes through too once it is HTML.

use crate::clipboard;
use crate::model::*;

/// Plain text as paragraphs, one a line, in Normal.
pub fn from_text(text: &str) -> Vec<Paragraph> {
    text.replace("\r\n", "\n")
        .split('\n')
        .map(|line| {
            let mut paragraph = Paragraph::new();
            if !line.is_empty() {
                paragraph.runs.push(Run::text(line.replace('\t', "    "), no_props()));
            }
            paragraph
        })
        .collect()
}

/// Markdown as paragraphs with Word's styles: `#` headings, `-` and `1.` lists, `>` quotes, fenced code,
/// and bold, italic, code, links and pictures inside a line.
pub fn from_markdown(document: &mut Document, text: &str) -> Vec<Paragraph> {
    clipboard::from_html(document, &markdown_to_html(text))
}

/// HTML as paragraphs.
pub fn from_html(document: &mut Document, html: &str) -> Vec<Paragraph> {
    clipboard::from_html(document, html)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// One line's Markdown spans as HTML: `**bold**`, `*italic*` and `_italic_`, `` `code` ``,
/// `[words](address)` and `![alt](picture)`.
fn inline(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut bold = false;
    let mut italic = false;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("**") || rest.starts_with("__") {
            out.push_str(if bold { "</b>" } else { "<b>" });
            bold = !bold;
            i += 2;
            continue;
        }
        if (chars[i] == '*' || chars[i] == '_') && (italic || chars.get(i + 1).is_some_and(|c| !c.is_whitespace())) {
            out.push_str(if italic { "</i>" } else { "<i>" });
            italic = !italic;
            i += 1;
            continue;
        }
        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == '`') {
                let code: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push_str(&format!("<code>{}</code>", escape(&code)));
                i += end + 2;
                continue;
            }
        }
        let picture = chars[i] == '!' && chars.get(i + 1) == Some(&'[');
        if chars[i] == '[' || picture {
            let start = if picture { i + 1 } else { i };
            if let Some(close) = chars[start..].iter().position(|c| *c == ']') {
                let close = start + close;
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = chars[close + 2..].iter().position(|c| *c == ')') {
                        let words: String = chars[start + 1..close].iter().collect();
                        let address: String = chars[close + 2..close + 2 + end].iter().collect();
                        if picture {
                            out.push_str(&format!("<img alt=\"{}\" src=\"{}\">", escape(&words), escape(&address)));
                        } else {
                            out.push_str(&format!("<a href=\"{}\">{}</a>", escape(&address), escape(&words)));
                        }
                        i = close + 3 + end;
                        continue;
                    }
                }
            }
        }
        out.push_str(&escape(&chars[i].to_string()));
        i += 1;
    }
    if italic {
        out.push_str("</i>");
    }
    if bold {
        out.push_str("</b>");
    }
    out
}

/// Markdown as the HTML the clipboard reader understands.
pub fn markdown_to_html(text: &str) -> String {
    let mut out = String::from("<html><body>");
    let mut list: Option<&str> = None;
    let mut paragraph: Vec<String> = Vec::new();
    let mut code: Option<Vec<String>> = None;
    let flush = |out: &mut String, paragraph: &mut Vec<String>| {
        if !paragraph.is_empty() {
            out.push_str(&format!("<p>{}</p>", inline(&paragraph.join(" "))));
            paragraph.clear();
        }
    };
    let end_list = |out: &mut String, list: &mut Option<&str>| {
        if let Some(kind) = list.take() {
            out.push_str(&format!("</{kind}>"));
        }
    };
    for line in text.replace("\r\n", "\n").lines() {
        if let Some(lines) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                let body = lines.iter().map(|l| escape(l)).collect::<Vec<_>>().join("<br>");
                out.push_str(&format!("<pre>{body}</pre>"));
                code = None;
            } else {
                lines.push(line.to_owned());
            }
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            flush(&mut out, &mut paragraph);
            end_list(&mut out, &mut list);
            code = Some(Vec::new());
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut out, &mut paragraph);
            end_list(&mut out, &mut list);
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush(&mut out, &mut paragraph);
            end_list(&mut out, &mut list);
            out.push_str(&format!("<h{hashes}>{}</h{hashes}>", inline(trimmed[hashes..].trim())));
            continue;
        }
        if trimmed == "---" || trimmed == "***" {
            flush(&mut out, &mut paragraph);
            end_list(&mut out, &mut list);
            out.push_str("<hr>");
            continue;
        }
        let bullet = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ "));
        let number = trimmed.split_once(". ").filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).map(|(_, rest)| rest);
        if let Some((kind, item)) = bullet.map(|b| ("ul", b)).or(number.map(|n| ("ol", n))) {
            flush(&mut out, &mut paragraph);
            if list != Some(kind) {
                end_list(&mut out, &mut list);
                out.push_str(&format!("<{kind}>"));
                list = Some(kind);
            }
            out.push_str(&format!("<li>{}</li>", inline(item)));
            continue;
        }
        if let Some(quote) = trimmed.strip_prefix('>') {
            flush(&mut out, &mut paragraph);
            end_list(&mut out, &mut list);
            out.push_str(&format!("<blockquote>{}</blockquote>", inline(quote.trim())));
            continue;
        }
        // A table row: the cells, a tab between each.
        if trimmed.starts_with('|') {
            flush(&mut out, &mut paragraph);
            if trimmed.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                continue;
            }
            let cells: Vec<String> = trimmed.trim_matches('|').split('|').map(|c| inline(c.trim())).collect();
            out.push_str(&format!("<p>{}</p>", cells.join("&#9;")));
            continue;
        }
        end_list(&mut out, &mut list);
        paragraph.push(trimmed.to_owned());
    }
    flush(&mut out, &mut paragraph);
    end_list(&mut out, &mut list);
    if let Some(lines) = code {
        out.push_str(&format!("<pre>{}</pre>", lines.iter().map(|l| escape(l)).collect::<Vec<_>>().join("<br>")));
    }
    out.push_str("</body></html>");
    out
}

/// A new document holding these paragraphs, its styles brought from the built in template as the
/// paragraphs name them.
pub fn document_of(paragraphs: Vec<Paragraph>) -> Document {
    let mut document = crate::new::blank();
    let mut styles: Vec<String> = Vec::new();
    for paragraph in &paragraphs {
        if let Some(id) = paragraph.props.val_of("w:pStyle") {
            styles.push(id.to_owned());
        }
        for run in &paragraph.runs {
            if let Some(id) = run.props.val_of("w:rStyle") {
                styles.push(id.to_owned());
            }
        }
    }
    styles.sort();
    styles.dedup();
    for id in styles {
        let _ = crate::builtin::ensure_style(&mut document, &id);
    }
    document.body = paragraphs.into_iter().map(Block::Paragraph).collect();
    if document.body.is_empty() {
        document.body.push(Block::Paragraph(Paragraph::new()));
    }
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_becomes_headings_lists_quotes_and_code() {
        let mut document = crate::new::blank();
        let paragraphs = from_markdown(
            &mut document,
            "# A title\n\nSome **bold** and *italic* text with [a link](https://example.com).\n\n- one\n- two\n\n1. first\n\n> quoted\n\n```\nlet x = 1;\n```\n",
        );
        let styles: Vec<String> = paragraphs.iter().map(|p| p.props.val_of("w:pStyle").unwrap_or("Normal").to_owned()).collect();
        let texts: Vec<String> = paragraphs.iter().map(|p| p.visible_text()).collect();
        assert_eq!(texts[0], "A title");
        assert_eq!(styles[0], "Heading1");
        assert!(texts[1].starts_with("Some bold and italic text with a link"));
        assert!(paragraphs[1].runs.iter().any(|r| r.props.child("w:b").is_some()), "bold is bold");
        assert!(styles.iter().any(|s| s == "ListBullet"), "{styles:?}");
        assert!(texts.iter().any(|t| t.contains("let x = 1;")));
        let document = document_of(paragraphs);
        let saved = crate::write::save(&document).unwrap();
        assert!(crate::read::open(&saved).is_ok());
    }

    #[test]
    fn text_is_one_paragraph_a_line() {
        let paragraphs = from_text("one\r\ntwo\n\nfour");
        assert_eq!(paragraphs.len(), 4);
        assert_eq!(paragraphs[3].visible_text(), "four");
    }
}
