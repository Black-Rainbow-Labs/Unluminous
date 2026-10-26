//! The Page level's third level (task-2359, TDD 5.5): custom and mirrored margins, the gutter, the
//! header and footer distances, a custom paper size, vertical alignment, line numbers, automatic
//! hyphenation, a page border, a different first page and different odd and even pages, how pages are
//! numbered, and whether a section's header is linked to the one before it; and the footnote and
//! endnote options of the References level.
//!
//! The section ones change the `w:sectPr` of the section the caret is in; the document wide ones the
//! settings part, which is kept in the schema's order. Each is one undo step.

use crate::model::Document;
use crate::position;
use crate::props;
use crate::session::{Scope, Session};
use crate::xml::{El, Node};

fn twips(points: f32) -> String {
    ((points * 20.0).round() as i64).to_string()
}

/// The order Word writes `w:settings` in (ECMA-376 CT_Settings), for the children this module adds.
pub const SETTINGS: &[&str] = &[
    "writeProtection",
    "view",
    "zoom",
    "removePersonalInformation",
    "removeDateAndTime",
    "doNotDisplayPageBoundaries",
    "displayBackgroundShape",
    "printPostScriptOverText",
    "printFractionalCharacterWidth",
    "printFormsData",
    "embedTrueTypeFonts",
    "embedSystemFonts",
    "saveSubsetFonts",
    "saveFormsData",
    "mirrorMargins",
    "alignBordersAndEdges",
    "bordersDoNotSurroundHeader",
    "bordersDoNotSurroundFooter",
    "gutterAtTop",
    "hideSpellingErrors",
    "hideGrammaticalErrors",
    "activeWritingStyle",
    "proofState",
    "formsDesign",
    "attachedTemplate",
    "linkStyles",
    "stylePaneFormatFilter",
    "stylePaneSortMethod",
    "documentType",
    "mailMerge",
    "revisionView",
    "trackRevisions",
    "doNotTrackMoves",
    "doNotTrackFormatting",
    "documentProtection",
    "autoFormatOverride",
    "styleLockTheme",
    "styleLockQFSet",
    "defaultTabStop",
    "autoHyphenation",
    "consecutiveHyphenLimit",
    "hyphenationZone",
    "doNotHyphenateCaps",
    "showEnvelope",
    "summaryLength",
    "clickAndTypeStyle",
    "defaultTableStyle",
    "evenAndOddHeaders",
    "bookFoldRevPrinting",
    "bookFoldPrinting",
    "bookFoldPrintingSheets",
    "drawingGridHorizontalSpacing",
    "drawingGridVerticalSpacing",
    "displayHorizontalDrawingGridEvery",
    "displayVerticalDrawingGridEvery",
    "doNotUseMarginsForDrawingGridOrigin",
    "drawingGridHorizontalOrigin",
    "drawingGridVerticalOrigin",
    "doNotShadeFormData",
    "noPunctuationKerning",
    "characterSpacingControl",
    "printTwoOnOne",
    "strictFirstAndLastChars",
    "noLineBreaksAfter",
    "noLineBreaksBefore",
    "savePreviewPicture",
    "doNotValidateAgainstSchema",
    "saveInvalidXml",
    "ignoreMixedContent",
    "alwaysShowPlaceholderText",
    "doNotDemarcateInvalidXml",
    "saveXmlDataOnly",
    "useXSLTWhenSaving",
    "saveThroughXslt",
    "showXMLTags",
    "alwaysMergeEmptyNamespace",
    "updateFields",
    "hdrShapeDefaults",
    "footnotePr",
    "endnotePr",
    "compat",
    "docVars",
    "rsids",
    "mathPr",
    "attachedSchema",
    "themeFontLang",
    "clrSchemeMapping",
    "doNotIncludeSubdocsInStats",
    "doNotAutoCompressPictures",
    "forceUpgrade",
    "captions",
    "readModeInkLockDown",
    "smartTagType",
    "schemaLibrary",
    "shapeDefaults",
    "doNotEmbedSmartTags",
    "decimalSymbol",
    "listSeparator",
];

/// Put a settings part's children in the schema's order: a child it does not name stays after the one
/// before it.
pub(crate) fn sort_settings(root: &mut El) {
    let rank =
        |el: &El| el.name.strip_prefix("w:").and_then(|l| SETTINGS.iter().position(|k| *k == l));
    let mut last = 0usize;
    let mut keyed: Vec<(usize, usize, Node)> = Vec::new();
    for (index, node) in std::mem::take(&mut root.children).into_iter().enumerate() {
        let key = match &node {
            Node::El(el) => match rank(el) {
                Some(r) => {
                    last = r;
                    r * 2
                }
                None => last * 2 + 1,
            },
            _ => last * 2 + 1,
        };
        keyed.push((key, index, node));
    }
    keyed.sort_by_key(|(k, i, _)| (*k, *i));
    root.children = keyed.into_iter().map(|(_, _, n)| n).collect();
}

/// The document's settings, made when it has none.
fn settings(document: &mut Document) -> &mut El {
    if document.settings.is_none() {
        document.settings = Some(crate::model::XmlPart {
            name: "word/settings.xml".into(),
            root: El::new("w:settings"),
            dirty: true,
        });
        let main = document.main.clone();
        let target = crate::package::Package::relative(&main, "word/settings.xml");
        document.package.rels_mut(&main).add(crate::package::kind::SETTINGS, &target, false);
        document.package.set_override(
            "word/settings.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
        );
    }
    let part = document.settings.as_mut().expect("made above");
    part.dirty = true;
    &mut part.root
}

impl Session {
    /// Change the settings part in one undo step, keeping the schema's order.
    fn change_settings(&mut self, change: impl FnOnce(&mut El) + 'static) {
        self.change(Scope::Whole, move |document, selection, _| {
            let root = settings(document);
            change(root);
            sort_settings(root);
            selection.clone()
        });
    }

    /// Every margin, the gutter and the header and footer distances of the caret's section, in points.
    pub fn set_custom_margins(&mut self, margins: [f32; 4], gutter: f32, header: f32, footer: f32) {
        self.change_section(move |sect| {
            let mar = sect.child_or_insert("w:pgMar");
            for (name, value) in ["w:top", "w:bottom", "w:left", "w:right"].iter().zip(margins) {
                mar.set_attr(name, twips(value));
            }
            mar.set_attr("w:gutter", twips(gutter));
            mar.set_attr("w:header", twips(header));
            mar.set_attr("w:footer", twips(footer));
            crate::order::sort(sect);
        });
    }

    /// Mirrored margins: the inside and outside margins swap on even pages, as for a bound book.
    pub fn set_mirror_margins(&mut self, on: bool) {
        self.change_settings(move |root| {
            props::set_on_off(root, "w:mirrorMargins", on.then_some(true))
        });
    }

    /// The paper of the caret's section: `width` by `height` points.
    pub fn set_paper(&mut self, width: f32, height: f32) {
        self.change_section(move |sect| {
            let pg = sect.child_or_insert("w:pgSz");
            pg.set_attr("w:w", twips(width));
            pg.set_attr("w:h", twips(height));
            if width > height {
                pg.set_attr("w:orient", "landscape");
            } else {
                pg.remove_attr("w:orient");
            }
            crate::order::sort(sect);
        });
    }

    /// Where the text sits on the page between the top and bottom margins: `top`, `center`, `both`
    /// (justified) or `bottom`.
    pub fn set_vertical_alignment(&mut self, align: &str) -> Result<(), String> {
        let align = match align {
            "top" | "center" | "both" | "bottom" => align.to_owned(),
            "justified" => "both".to_owned(),
            other => {
                return Err(format!(
                    "vertical alignment is top, center, justified or bottom, not {other}"
                ))
            }
        };
        self.change_section(move |sect| {
            if align == "top" {
                sect.remove_children("w:vAlign");
            } else {
                sect.put(El::new("w:vAlign").with("w:val", align));
            }
            crate::order::sort(sect);
        });
        Ok(())
    }

    /// Line numbers in the margin: every `count_by` lines from `start`, restarting `newPage`,
    /// `newSection` or `continuous`, `distance` points from the text; `None` takes them away.
    pub fn set_line_numbers(&mut self, numbers: Option<(u32, u32, &str, f32)>) {
        let numbers = numbers.map(|(c, s, r, d)| (c, s, r.to_owned(), d));
        self.change_section(move |sect| {
            sect.remove_children("w:lnNumType");
            if let Some((count_by, start, restart, distance)) = numbers {
                let mut el = El::new("w:lnNumType").with("w:countBy", count_by.max(1).to_string());
                if start > 1 {
                    el.set_attr("w:start", (start - 1).to_string());
                }
                if distance > 0.0 {
                    el.set_attr("w:distance", twips(distance));
                }
                el.set_attr("w:restart", restart);
                sect.children.push(Node::El(el));
            }
            crate::order::sort(sect);
        });
    }

    /// Automatic hyphenation for the whole document.
    pub fn set_auto_hyphenation(&mut self, on: bool) {
        self.change_settings(move |root| {
            props::set_on_off(root, "w:autoHyphenation", on.then_some(true))
        });
    }

    /// A box round every page of the caret's section, `width` points of line in `colour`, or none.
    pub fn set_page_border(&mut self, border: Option<(f32, &str)>) {
        let border = border.map(|(w, c)| (w, c.to_owned()));
        self.change_section(move |sect| {
            sect.remove_children("w:pgBorders");
            if let Some((width, colour)) = border {
                let side = |name: &str| {
                    El::new(name)
                        .with("w:val", "single")
                        .with("w:sz", ((width * 8.0).round() as i64).clamp(2, 96).to_string())
                        .with("w:space", "24")
                        .with("w:color", colour.clone())
                };
                let el = El::new("w:pgBorders")
                    .with("w:offsetFrom", "page")
                    .with_child(side("w:top"))
                    .with_child(side("w:left"))
                    .with_child(side("w:bottom"))
                    .with_child(side("w:right"));
                sect.children.push(Node::El(el));
            }
            crate::order::sort(sect);
        });
    }

    /// A different header and footer on the first page of the caret's section.
    pub fn set_different_first_page(&mut self, on: bool) {
        self.change_section(move |sect| {
            props::set_on_off(sect, "w:titlePg", on.then_some(true));
            crate::order::sort(sect);
        });
    }

    /// Different headers and footers on odd and even pages, for the whole document.
    pub fn set_different_odd_even(&mut self, on: bool) {
        self.change_settings(move |root| {
            props::set_on_off(root, "w:evenAndOddHeaders", on.then_some(true))
        });
    }

    /// How the caret's section numbers its pages: `decimal`, `lowerRoman`, `upperRoman`, `lowerLetter`
    /// or `upperLetter`, starting at `start` or going on from the section before.
    pub fn set_page_numbering(&mut self, format: &str, start: Option<u32>) {
        let format = format.to_owned();
        self.change_section(move |sect| {
            let pg = sect.child_or_insert("w:pgNumType");
            if format == "decimal" {
                pg.remove_attr("w:fmt");
            } else {
                pg.set_attr("w:fmt", format);
            }
            match start {
                Some(n) => pg.set_attr("w:start", n.to_string()),
                None => pg.remove_attr("w:start"),
            }
            if pg.attrs.is_empty() {
                sect.remove_children("w:pgNumType");
            }
            crate::order::sort(sect);
        });
    }

    /// Link the caret's section's header (or footer) of `kind` to the section before it, which removes
    /// its own; or unlink it, which gives it a copy of the one it was showing.
    pub fn link_to_previous(&mut self, footer: bool, kind: &str, link: bool) -> Result<(), String> {
        let reference = if footer { "w:footerReference" } else { "w:headerReference" };
        let kind = kind.to_owned();
        let sect = self.section_at_caret().unwrap_or_default();
        let own = sect
            .children_named(reference)
            .find(|r| r.attr("w:type").unwrap_or("default") == kind)
            .is_some();
        if link && !own {
            return Err("this section already shows the header of the section before it".into());
        }
        if !link && own {
            return Err("this section already has a header of its own".into());
        }
        let inherited = (!link).then(|| self.inherited_header(footer, &kind)).flatten();
        self.change_section(move |sect| {
            if link {
                sect.children.retain(|n| !matches!(n, Node::El(e) if e.name == reference && e.attr("w:type").unwrap_or("default") == kind));
            } else if let Some(id) = &inherited {
                sect.children.insert(0, Node::El(El::new(reference).with("w:type", kind.clone()).with("r:id", id.clone())));
            }
            crate::order::sort(sect);
        });
        Ok(())
    }

    /// A copy of the header of `kind` the caret's section shows from an earlier section, as a new part.
    fn inherited_header(&mut self, footer: bool, kind: &str) -> Option<String> {
        let reference = if footer { "w:footerReference" } else { "w:headerReference" };
        let caret_section = self.section_at_caret().unwrap_or_default();
        let mut found: Option<String> = None;
        for (_, paragraph) in position::paragraphs(&self.document.body) {
            let Some(sect) = paragraph.props.child("w:sectPr") else { continue };
            if *sect == caret_section {
                break;
            }
            if let Some(id) = sect
                .children_named(reference)
                .find(|r| r.attr("w:type").unwrap_or("default") == kind)
                .and_then(|r| r.attr("r:id"))
            {
                found = Some(id.to_owned());
            }
        }
        let source = self.document.headers.iter().find(|h| Some(&h.id) == found.as_ref())?.clone();
        let id = crate::ops::new_header_part_for(&mut self.document, footer);
        if let Some(part) = self.document.headers.iter_mut().find(|h| h.id == id) {
            part.story.blocks = source.story.blocks.clone();
            part.dirty = true;
        }
        Some(id)
    }

    /// Footnote or endnote options: the number format, the first number, when numbering restarts
    /// (`continuous`, `eachSect` or `eachPage`, footnotes only) and where the notes go (`pageBottom` or
    /// `beneathText` for footnotes, `docEnd` or `sectEnd` for endnotes).
    pub fn set_note_options(
        &mut self,
        endnote: bool,
        format: &str,
        start: u32,
        restart: &str,
        place: &str,
    ) {
        let (name, format, restart, place) = (
            if endnote { "w:endnotePr" } else { "w:footnotePr" },
            format.to_owned(),
            restart.to_owned(),
            place.to_owned(),
        );
        self.change_settings(move |root| {
            let pr = root.child_or_insert(name);
            let keep: Vec<Node> = pr
                .children
                .iter()
                .filter(|n| matches!(n, Node::El(e) if e.name.ends_with("note")))
                .cloned()
                .collect();
            pr.children = Vec::new();
            pr.children.push(Node::El(El::new("w:pos").with("w:val", place.clone())));
            pr.children.push(Node::El(El::new("w:numFmt").with("w:val", format.clone())));
            pr.children
                .push(Node::El(El::new("w:numStart").with("w:val", start.max(1).to_string())));
            if !endnote || restart != "eachPage" {
                pr.children.push(Node::El(El::new("w:numRestart").with("w:val", restart.clone())));
            }
            pr.children.extend(keep);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_setup_writes_the_section_and_the_settings() {
        let mut session = Session::new(crate::new::blank());
        session.set_custom_margins([36.0, 36.0, 54.0, 54.0], 18.0, 24.0, 24.0);
        session.set_vertical_alignment("center").unwrap();
        session.set_line_numbers(Some((5, 1, "newPage", 18.0)));
        session.set_page_border(Some((1.0, "FF0000")));
        session.set_different_first_page(true);
        session.set_page_numbering("lowerRoman", Some(3));
        session.set_mirror_margins(true);
        session.set_auto_hyphenation(true);
        session.set_note_options(false, "lowerLetter", 1, "eachPage", "pageBottom");
        let sect = session.section_at_caret().unwrap();
        let geometry = crate::layout::section::Geometry::of(&sect);
        assert_eq!(
            (geometry.top, geometry.left, geometry.gutter, geometry.header),
            (36.0, 54.0, 18.0, 24.0)
        );
        assert!(geometry.vertical_center);
        assert_eq!(sect.child("w:lnNumType").and_then(|l| l.attr("w:countBy")), Some("5"));
        assert_eq!(sect.child("w:pgNumType").and_then(|l| l.attr("w:fmt")), Some("lowerRoman"));
        assert!(sect.child("w:titlePg").is_some());
        let root = &session.document.settings.as_ref().unwrap().root;
        let names: Vec<&str> = root.elements().map(|e| e.name.as_str()).collect();
        let at = |n: &str| names.iter().position(|x| *x == n).unwrap();
        assert!(
            at("w:mirrorMargins") < at("w:autoHyphenation"),
            "the settings are in the schema's order"
        );
        assert!(at("w:autoHyphenation") < at("w:footnotePr"));
        // The file Word reads back has them.
        let saved = crate::write::save(&session.document).unwrap();
        let back = crate::read::open(&saved).unwrap();
        assert!(back.settings.unwrap().root.child("w:mirrorMargins").is_some());
    }
}
