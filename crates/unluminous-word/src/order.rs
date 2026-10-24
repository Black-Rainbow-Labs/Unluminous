//! The order the schema puts each properties element's children in.
//!
//! WordprocessingML's properties elements are `xsd:sequence`s, and Word treats a child out of its place as
//! a damaged file. A child Unluminous added is appended wherever it was convenient, so before a properties
//! element is written its children are put in the schema's order by [`sort`]. A child the tables below do
//! not name (an extension such as `w14:ligatures`) keeps its place after the named child it followed when
//! it was read, which is where the program that wrote it put it.

use crate::xml::{El, Node};

/// `CT_PPr` (with `CT_PPrBase` first).
pub const PARAGRAPH: &[&str] = &[
    "pStyle",
    "keepNext",
    "keepLines",
    "pageBreakBefore",
    "framePr",
    "widowControl",
    "numPr",
    "suppressLineNumbers",
    "pBdr",
    "shd",
    "tabs",
    "suppressAutoHyphens",
    "kinsoku",
    "wordWrap",
    "overflowPunct",
    "topLinePunct",
    "autoSpaceDE",
    "autoSpaceDN",
    "bidi",
    "adjustRightInd",
    "snapToGrid",
    "spacing",
    "ind",
    "contextualSpacing",
    "mirrorIndents",
    "suppressOverlap",
    "jc",
    "textDirection",
    "textAlignment",
    "textboxTightWrap",
    "outlineLvl",
    "divId",
    "cnfStyle",
    "rPr",
    "sectPr",
    "pPrChange",
];

/// `CT_RPr`, and the paragraph mark's `CT_ParaRPr`, which adds the revision marks at its front.
pub const RUN: &[&str] = &[
    "ins",
    "del",
    "moveFrom",
    "moveTo",
    "rStyle",
    "rFonts",
    "b",
    "bCs",
    "i",
    "iCs",
    "caps",
    "smallCaps",
    "strike",
    "dstrike",
    "outline",
    "shadow",
    "emboss",
    "imprint",
    "noProof",
    "snapToGrid",
    "vanish",
    "webHidden",
    "color",
    "spacing",
    "w",
    "kern",
    "position",
    "sz",
    "szCs",
    "highlight",
    "u",
    "effect",
    "bdr",
    "shd",
    "fitText",
    "vertAlign",
    "rtl",
    "cs",
    "em",
    "lang",
    "eastAsianLayout",
    "specVanish",
    "oMath",
    "rPrChange",
];

/// `CT_SectPr`. Header and footer references are one choice group in the schema, in any order, so they
/// share a place.
pub const SECTION: &[&str] = &[
    "headerReference|footerReference",
    "footnotePr",
    "endnotePr",
    "type",
    "pgSz",
    "pgMar",
    "paperSrc",
    "pgBorders",
    "lnNumType",
    "pgNumType",
    "cols",
    "formProt",
    "vAlign",
    "noEndnote",
    "titlePg",
    "textDirection",
    "bidi",
    "rtlGutter",
    "docGrid",
    "printerSettings",
    "sectPrChange",
];

/// `CT_TblPr`.
pub const TABLE: &[&str] = &[
    "tblStyle",
    "tblpPr",
    "tblOverlap",
    "bidiVisual",
    "tblStyleRowBandSize",
    "tblStyleColBandSize",
    "tblW",
    "jc",
    "tblCellSpacing",
    "tblInd",
    "tblBorders",
    "shd",
    "tblLayout",
    "tblCellMar",
    "tblLook",
    "tblCaption",
    "tblDescription",
    "tblPrChange",
];

/// `CT_TrPr`, which is a choice rather than a sequence; Word writes it in this order.
pub const ROW: &[&str] = &[
    "cnfStyle",
    "divId",
    "gridBefore",
    "gridAfter",
    "wBefore",
    "wAfter",
    "cantSplit",
    "trHeight",
    "tblHeader",
    "tblCellSpacing",
    "jc",
    "hidden",
    "ins",
    "del",
    "trPrChange",
];

/// `CT_TcPr`.
pub const CELL: &[&str] = &[
    "cnfStyle",
    "tcW",
    "gridSpan",
    "hMerge",
    "vMerge",
    "tcBorders",
    "shd",
    "noWrap",
    "tcMar",
    "textDirection",
    "tcFitText",
    "vAlign",
    "hideMark",
    "headers",
    "cellIns",
    "cellDel",
    "cellMerge",
    "tcPrChange",
];

/// `CT_PBdr`, `CT_TblBorders` and `CT_TcBorders` share one order.
pub const BORDERS: &[&str] = &[
    "top", "left", "start", "bottom", "right", "end", "insideH", "insideV", "tl2br", "tr2bl",
    "bar", "between",
];

/// `CT_TblCellMar` and `CT_TcMar`.
pub const MARGINS: &[&str] = &["top", "left", "start", "bottom", "right", "end"];

/// `CT_Style`.
pub const STYLE: &[&str] = &[
    "name",
    "aliases",
    "basedOn",
    "next",
    "link",
    "autoRedefine",
    "hidden",
    "uiPriority",
    "semiHidden",
    "unhideWhenUsed",
    "qFormat",
    "locked",
    "personal",
    "personalCompose",
    "personalReply",
    "rsid",
    "pPr",
    "rPr",
    "tblPr",
    "trPr",
    "tcPr",
    "tblStylePr",
];

/// `CT_Lvl`, a numbering level.
pub const LEVEL: &[&str] = &[
    "start",
    "numFmt",
    "lvlRestart",
    "pStyle",
    "isLgl",
    "suff",
    "lvlText",
    "lvlPicBulletId",
    "legacy",
    "lvlJc",
    "pPr",
    "rPr",
];

/// The order table for a properties element called `name`, when there is one.
pub fn table_for(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "w:pPr" => PARAGRAPH,
        "w:rPr" => RUN,
        "w:sectPr" => SECTION,
        "w:tblPr" => TABLE,
        "w:trPr" => ROW,
        "w:tcPr" => CELL,
        "w:pBdr" | "w:tblBorders" | "w:tcBorders" => BORDERS,
        "w:tblCellMar" | "w:tcMar" => MARGINS,
        "w:style" => STYLE,
        "w:lvl" => LEVEL,
        _ => return None,
    })
}

/// Put `el`'s children in the schema's order, and the children of every properties element inside it.
pub fn sort(el: &mut El) {
    for child in el.elements_mut() {
        if table_for(&child.name).is_some() || child.local() == "pPr" || child.local() == "rPr" {
            sort(child);
        }
    }
    let Some(table) = table_for(&el.name) else { return };
    let rank = |child: &El| -> Option<usize> {
        child.name.strip_prefix("w:").and_then(|local| {
            table.iter().position(|known| known.split('|').any(|name| name == local))
        })
    };
    // A child the table does not name sorts just after whatever named child came before it.
    let mut last = 0usize;
    let mut keyed: Vec<(usize, usize, Node)> = Vec::with_capacity(el.children.len());
    for (index, node) in std::mem::take(&mut el.children).into_iter().enumerate() {
        let key = match &node {
            Node::El(child) => match rank(child) {
                Some(found) => {
                    last = found;
                    found * 2
                }
                None => last * 2 + 1,
            },
            Node::Text(_) => last * 2 + 1,
        };
        keyed.push((key, index, node));
    }
    keyed.sort_by_key(|(key, index, _)| (*key, *index));
    el.children = keyed.into_iter().map(|(_, _, node)| node).collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn children_added_at_the_end_are_put_in_the_schemas_place() {
        let mut props = El::new("w:rPr")
            .with_child(El::new("w:sz").with("w:val", "28"))
            .with_child(El::new("w:b"))
            .with_child(El::new("w:rStyle").with("w:val", "Strong"));
        sort(&mut props);
        let names: Vec<&str> = props.elements().map(|el| el.name.as_str()).collect();
        assert_eq!(names, ["w:rStyle", "w:b", "w:sz"]);
    }

    #[test]
    fn an_extension_stays_after_the_element_it_followed() {
        let mut props = El::new("w:rPr")
            .with_child(El::new("w:lang"))
            .with_child(El::new("w14:ligatures"))
            .with_child(El::new("w:b"));
        sort(&mut props);
        let names: Vec<&str> = props.elements().map(|el| el.name.as_str()).collect();
        assert_eq!(names, ["w:b", "w:lang", "w14:ligatures"]);
    }

    #[test]
    fn the_paragraph_marks_run_properties_are_sorted_inside_the_paragraph_properties() {
        let mut props = El::new("w:pPr")
            .with_child(El::new("w:rPr").with_child(El::new("w:i")).with_child(El::new("w:b")))
            .with_child(El::new("w:jc"))
            .with_child(El::new("w:pStyle"));
        sort(&mut props);
        let names: Vec<&str> = props.elements().map(|el| el.name.as_str()).collect();
        assert_eq!(names, ["w:pStyle", "w:jc", "w:rPr"]);
        let inner: Vec<&str> =
            props.child("w:rPr").unwrap().elements().map(|el| el.name.as_str()).collect();
        assert_eq!(inner, ["w:b", "w:i"]);
    }
}
