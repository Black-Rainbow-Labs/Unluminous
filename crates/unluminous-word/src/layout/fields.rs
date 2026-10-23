//! Fields whose result depends on the page they are drawn on.
//!
//! Every other field shows the result the file stored, the way Word shows a field until it is updated.
//! `PAGE`, `NUMPAGES` and `SECTIONPAGES` are worked out while a header or footer is laid out for a page,
//! because Word does exactly that and their stored result is whatever page was last printed.

/// A field code split into its type and the rest: ` PAGE  \* roman ` is `("PAGE", "\* roman")`.
pub fn split(code: &str) -> (String, String) {
    let code = code.trim();
    let (kind, rest) = code.split_once(char::is_whitespace).unwrap_or((code, ""));
    (kind.to_uppercase(), rest.trim().to_owned())
}

/// The `\*` format switch's argument, upper cased: `ROMAN`, `ARABIC`, `ALPHABETIC`, `MERGEFORMAT`.
pub fn format_switch(rest: &str) -> Option<String> {
    let at = rest.find("\\*")?;
    rest[at + 2..].split_whitespace().next().map(str::to_owned)
}

/// The result of a page field for page `number` of `pages`, or `None` for a field that keeps its result.
pub fn page_result(code: &str, page: Option<(u32, u32)>) -> Option<String> {
    let (number, pages) = page?;
    let (kind, rest) = split(code);
    let value = match kind.as_str() {
        "PAGE" => number,
        "NUMPAGES" | "SECTIONPAGES" => pages,
        _ => return None,
    };
    let format = match format_switch(&rest).as_deref() {
        Some("roman") => "lowerRoman",
        Some("ROMAN") | Some("Roman") => "upperRoman",
        Some("alphabetic") => "lowerLetter",
        Some("ALPHABETIC") | Some("Alphabetic") => "upperLetter",
        Some("Ordinal") | Some("ordinal") => "ordinal",
        Some("CardText") | Some("cardtext") => "cardinalText",
        _ => "decimal",
    };
    Some(crate::numbering::format_number(i64::from(value), format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_fields_follow_their_format_switch() {
        assert_eq!(page_result(" PAGE   \\* MERGEFORMAT ", Some((4, 9))).as_deref(), Some("4"));
        assert_eq!(page_result("PAGE \\* roman", Some((4, 9))).as_deref(), Some("iv"));
        assert_eq!(page_result("NUMPAGES", Some((4, 9))).as_deref(), Some("9"));
        assert_eq!(page_result("DATE", Some((4, 9))), None);
        assert_eq!(page_result("PAGE", None), None);
    }
}
