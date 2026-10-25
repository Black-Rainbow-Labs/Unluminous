//! Equations (Office Math, `m:oMath`) as the linear text Word shows when an equation is switched to its
//! linear form: `x=(-b±√(b^2-4ac))/(2a)`.
//!
//! Round 2 draws an equation as this text in Cambria Math rather than laying out the two dimensional
//! form (TDD section 5.3). The element itself is kept and written back untouched, so nothing about the
//! equation changes in the file.

use crate::xml::El;

/// The family Word sets equations in.
pub const FONT: &str = "Cambria Math";

/// An equation's linear text: `m:oMath`, `m:oMathPara` (each equation on its own line) or any element
/// inside one.
/// The characters of an equation's text runs (`m:t`) in order, with no structure: what Word's
/// `Range.Text` holds for it once the structure marks are taken out.
pub fn plain_text(el: &El) -> String {
    let mut out = String::new();
    el.walk(&mut |node| {
        if node.name == "m:t" {
            out.push_str(&node.text());
        }
    });
    out
}

pub fn linear_text(el: &El) -> String {
    let mut out = String::new();
    write(el, &mut out);
    out
}

/// The text of `el`'s children, each in turn.
fn children(el: &El, out: &mut String) {
    for child in el.elements() {
        write(child, out);
    }
}

/// The text of one argument (`m:e`, `m:num`, `m:sup`...), bracketed when it is longer than one character
/// so `a/b` stays unambiguous, which is the linear format's own rule.
fn argument(el: Option<&El>, out: &mut String) {
    let mut inner = String::new();
    if let Some(el) = el {
        children(el, &mut inner);
    }
    if inner.chars().count() > 1 && !is_bracketed(&inner) {
        out.push('(');
        out.push_str(&inner);
        out.push(')');
    } else {
        out.push_str(&inner);
    }
}

fn is_bracketed(text: &str) -> bool {
    let open = text.chars().next();
    let close = text.chars().last();
    matches!(
        (open, close),
        (Some('('), Some(')')) | (Some('['), Some(']')) | (Some('{'), Some('}'))
    ) && text[1..text.len() - 1].chars().all(|c| c != '(' && c != ')')
}

/// The `m:val` of a property child, such as `m:chr` inside `m:naryPr`.
fn property(el: &El, props: &str, name: &str) -> Option<String> {
    el.child(props)?.child(name)?.attr("m:val").map(str::to_owned)
}

fn write(el: &El, out: &mut String) {
    match el.name.as_str() {
        "m:oMathPara" => {
            let mut first = true;
            for math in el.children_named("m:oMath") {
                if !first {
                    out.push('\u{000B}');
                }
                first = false;
                children(math, out);
            }
        }
        "m:t" | "w:t" => out.push_str(&el.text()),
        "m:r" => {
            for child in el.elements() {
                match child.name.as_str() {
                    "m:t" | "w:t" => out.push_str(&child.text()),
                    "w:tab" => out.push('\t'),
                    _ => {}
                }
            }
        }
        "m:f" => {
            argument(el.child("m:num"), out);
            match property(el, "m:fPr", "m:type").as_deref() {
                Some("lin") | Some("skw") | None | Some("bar") => out.push('/'),
                Some("noBar") => out.push('¦'),
                Some(_) => out.push('/'),
            }
            argument(el.child("m:den"), out);
        }
        "m:sSup" => {
            argument(el.child("m:e"), out);
            out.push('^');
            argument(el.child("m:sup"), out);
        }
        "m:sSub" => {
            argument(el.child("m:e"), out);
            out.push('_');
            argument(el.child("m:sub"), out);
        }
        "m:sSubSup" => {
            argument(el.child("m:e"), out);
            out.push('_');
            argument(el.child("m:sub"), out);
            out.push('^');
            argument(el.child("m:sup"), out);
        }
        "m:sPre" => {
            out.push('_');
            argument(el.child("m:sub"), out);
            out.push('^');
            argument(el.child("m:sup"), out);
            argument(el.child("m:e"), out);
        }
        "m:rad" => {
            let degree: String = el.child("m:deg").map(linear_text).unwrap_or_default();
            match degree.as_str() {
                "" => out.push('√'),
                "3" => out.push('∛'),
                "4" => out.push('∜'),
                _ => {
                    out.push_str("√(");
                    out.push_str(&degree);
                    out.push('&');
                    if let Some(e) = el.child("m:e") {
                        children(e, out);
                    }
                    out.push(')');
                    return;
                }
            }
            let mut inner = String::new();
            if let Some(e) = el.child("m:e") {
                children(e, &mut inner);
            }
            out.push('(');
            out.push_str(&inner);
            out.push(')');
        }
        "m:d" => {
            let open = property(el, "m:dPr", "m:begChr").unwrap_or_else(|| "(".into());
            let close = property(el, "m:dPr", "m:endChr").unwrap_or_else(|| ")".into());
            let separator = property(el, "m:dPr", "m:sepChr").unwrap_or_else(|| "|".into());
            out.push_str(&open);
            let mut first = true;
            for e in el.children_named("m:e") {
                if !first {
                    out.push_str(&separator);
                }
                first = false;
                children(e, out);
            }
            out.push_str(&close);
        }
        "m:nary" => {
            out.push_str(&property(el, "m:naryPr", "m:chr").unwrap_or_else(|| "∫".into()));
            let sub = el.child("m:sub").map(linear_text).unwrap_or_default();
            let sup = el.child("m:sup").map(linear_text).unwrap_or_default();
            if !sub.is_empty() {
                out.push('_');
                argument(el.child("m:sub"), out);
            }
            if !sup.is_empty() {
                out.push('^');
                argument(el.child("m:sup"), out);
            }
            out.push('▒');
            argument(el.child("m:e"), out);
        }
        "m:func" => {
            if let Some(name) = el.child("m:fName") {
                children(name, out);
            }
            out.push('⁡');
            argument(el.child("m:e"), out);
        }
        "m:acc" => {
            argument(el.child("m:e"), out);
            out.push_str(&property(el, "m:accPr", "m:chr").unwrap_or_else(|| "\u{0302}".into()));
        }
        "m:bar" => {
            out.push('▁');
            argument(el.child("m:e"), out);
        }
        "m:groupChr" => {
            out.push_str(&property(el, "m:groupChrPr", "m:chr").unwrap_or_else(|| "⏟".into()));
            argument(el.child("m:e"), out);
        }
        "m:limLow" => {
            argument(el.child("m:e"), out);
            out.push('┬');
            argument(el.child("m:lim"), out);
        }
        "m:limUpp" => {
            argument(el.child("m:e"), out);
            out.push('┴');
            argument(el.child("m:lim"), out);
        }
        "m:eqArr" => {
            out.push_str("█(");
            let mut first = true;
            for e in el.children_named("m:e") {
                if !first {
                    out.push('@');
                }
                first = false;
                children(e, out);
            }
            out.push(')');
        }
        "m:m" => {
            out.push_str("■(");
            let mut first_row = true;
            for row in el.children_named("m:mr") {
                if !first_row {
                    out.push('@');
                }
                first_row = false;
                let mut first = true;
                for e in row.children_named("m:e") {
                    if !first {
                        out.push('&');
                    }
                    first = false;
                    children(e, out);
                }
            }
            out.push(')');
        }
        // Properties say how a part looks and hold no text of their own.
        name if name.ends_with("Pr") => {}
        // `m:e`, `m:box`, `m:borderBox`, `m:phant`, `w:r` inside math, and anything newer: their content.
        _ => children(el, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml;

    fn math(inner: &str) -> El {
        xml::parse(&format!(
            "<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\">{inner}</m:oMath>"
        ))
        .unwrap()
    }

    #[test]
    fn a_quadratic_reads_as_words_linear_form() {
        let el = math(concat!(
            "<m:r><m:t>x=</m:t></m:r>",
            "<m:f><m:num><m:r><m:t>-b±</m:t></m:r><m:rad><m:radPr><m:degHide m:val=\"1\"/></m:radPr><m:deg/>",
            "<m:e><m:sSup><m:e><m:r><m:t>b</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup>",
            "<m:r><m:t>-4ac</m:t></m:r></m:e></m:rad></m:num>",
            "<m:den><m:r><m:t>2a</m:t></m:r></m:den></m:f>"
        ));
        assert_eq!(linear_text(&el), "x=(-b±√(b^2-4ac))/(2a)");
    }

    #[test]
    fn delimiters_sums_and_functions_keep_their_characters() {
        let el = math(concat!(
            "<m:d><m:dPr><m:begChr m:val=\"[\"/><m:endChr m:val=\"]\"/></m:dPr><m:e><m:r><m:t>a</m:t></m:r></m:e><m:e><m:r><m:t>b</m:t></m:r></m:e></m:d>",
            "<m:nary><m:naryPr><m:chr m:val=\"∑\"/></m:naryPr><m:sub><m:r><m:t>i=1</m:t></m:r></m:sub><m:sup><m:r><m:t>n</m:t></m:r></m:sup><m:e><m:r><m:t>i</m:t></m:r></m:e></m:nary>",
            "<m:func><m:fName><m:r><m:t>sin</m:t></m:r></m:fName><m:e><m:r><m:t>x</m:t></m:r></m:e></m:func>"
        ));
        assert_eq!(linear_text(&el), "[a|b]∑_(i=1)^n▒isin⁡x");
    }
}
