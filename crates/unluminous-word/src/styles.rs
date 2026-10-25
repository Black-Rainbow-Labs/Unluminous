//! Style resolution: what a paragraph and a run look like once the document defaults, the styles, the
//! list and the direct formatting are applied, in Word's order.
//!
//! - A paragraph: the document defaults, the table style's paragraph properties when it is in a table,
//!   the paragraph style and everything it is based on (the root first), the list level's paragraph
//!   properties, then its own `w:pPr`.
//! - A run: the document defaults, the table style, the paragraph style chain's run properties, the
//!   character style chain, then its own `w:rPr`. The toggle properties (bold, italic, caps...) follow the
//!   specification's rule: a toggle set in a paragraph style and again in a character style cancels, which
//!   is why `Strong` inside a bold heading is not bolder. Direct formatting sets them outright.

use std::collections::HashMap;

use crate::model::Document;
use crate::package::{kind, Package};
use crate::props::{self, ParaLook, Rgb, RunLook};
use crate::xml::{self, El};

/// The theme's fonts and colours.
#[derive(Debug, Clone, Default)]
pub struct Theme {
    pub major: String,
    pub minor: String,
    pub colours: HashMap<String, Rgb>,
}

impl Theme {
    /// Read `word/theme/theme1.xml`, or Office's own theme when the document has none.
    pub fn of(document: &Document) -> Theme {
        let rel = document
            .package
            .rels
            .get(&document.main)
            .and_then(|rels| rels.of_kind(kind::THEME).next());
        let root = rel
            .map(|rel| Package::resolve(&document.main, &rel.target))
            .and_then(|name| document.package.part(&name))
            .and_then(|bytes| xml::parse_bytes(bytes).ok());
        let mut theme = Theme {
            major: "Calibri Light".into(),
            minor: "Calibri".into(),
            colours: office_colours(),
        };
        let Some(root) = root else { return theme };
        let mut visit = |el: &El| match el.name.as_str() {
            "a:majorFont" => {
                if let Some(face) = el.child("a:latin").and_then(|latin| latin.attr("typeface")) {
                    theme.major = face.to_owned();
                }
            }
            "a:minorFont" => {
                if let Some(face) = el.child("a:latin").and_then(|latin| latin.attr("typeface")) {
                    theme.minor = face.to_owned();
                }
            }
            "a:clrScheme" => {
                for slot in el.elements() {
                    let colour = slot.elements().find_map(|value| match value.name.as_str() {
                        "a:srgbClr" => value.attr("val").and_then(Rgb::parse),
                        "a:sysClr" => value.attr("lastClr").and_then(Rgb::parse),
                        _ => None,
                    });
                    if let Some(colour) = colour {
                        theme.colours.insert(slot.local().to_owned(), colour);
                    }
                }
            }
            _ => {}
        };
        root.walk(&mut visit);
        theme
    }

    /// The face a theme font name (`minorHAnsi`, `majorEastAsia`) stands for.
    pub fn font(&self, name: &str) -> String {
        match name.starts_with("major") {
            true => self.major.clone(),
            false => self.minor.clone(),
        }
    }

    /// The colour a theme colour name (`accent1`, `text1`, `hyperlink`) stands for.
    pub fn colour(&self, name: &str) -> Option<Rgb> {
        let slot = match name {
            "dark1" | "text1" => "dk1",
            "light1" | "background1" => "lt1",
            "dark2" | "text2" => "dk2",
            "light2" | "background2" => "lt2",
            "hyperlink" => "hlink",
            "followedHyperlink" => "folHlink",
            other => other,
        };
        self.colours.get(slot).copied()
    }
}

/// The Office theme's colours, for a document with no theme part.
fn office_colours() -> HashMap<String, Rgb> {
    [
        ("dk1", "000000"),
        ("lt1", "FFFFFF"),
        ("dk2", "44546A"),
        ("lt2", "E7E6E6"),
        ("accent1", "4472C4"),
        ("accent2", "ED7D31"),
        ("accent3", "A5A5A5"),
        ("accent4", "FFC000"),
        ("accent5", "5B9BD5"),
        ("accent6", "70AD47"),
        ("hlink", "0563C1"),
        ("folHlink", "954F72"),
    ]
    .into_iter()
    .map(|(name, hex)| (name.to_owned(), Rgb::parse(hex).expect("a fixed colour")))
    .collect()
}

/// What the styles part says, indexed, for resolving many paragraphs.
pub struct Resolver<'a> {
    styles: HashMap<&'a str, &'a El>,
    /// The default paragraph style's id, `Normal` in every file Word writes.
    default_paragraph: Option<&'a str>,
    default_character: Option<&'a str>,
    default_table: Option<&'a str>,
    run_defaults: Option<&'a El>,
    paragraph_defaults: Option<&'a El>,
    pub numbering: crate::numbering::Numbering<'a>,
    pub theme: Theme,
}

impl<'a> Resolver<'a> {
    pub fn new(document: &'a Document) -> Resolver<'a> {
        let mut resolver = Resolver {
            styles: HashMap::new(),
            default_paragraph: None,
            default_character: None,
            default_table: None,
            run_defaults: None,
            paragraph_defaults: None,
            numbering: crate::numbering::Numbering::new(
                document.numbering.as_ref().map(|part| &part.root),
            )
            .with_styles(document.styles.as_ref().map(|part| &part.root)),
            theme: Theme::of(document),
        };
        let Some(root) = document.styles.as_ref().map(|part| &part.root) else { return resolver };
        if let Some(defaults) = root.child("w:docDefaults") {
            resolver.run_defaults = defaults.child("w:rPrDefault").and_then(|d| d.child("w:rPr"));
            resolver.paragraph_defaults =
                defaults.child("w:pPrDefault").and_then(|d| d.child("w:pPr"));
        }
        for style in root.children_named("w:style") {
            let Some(id) = style.attr("w:styleId") else { continue };
            resolver.styles.insert(id, style);
            if props::on_off_attr(style.attr("w:default")) {
                match style.attr("w:type") {
                    Some("paragraph") => resolver.default_paragraph = Some(id),
                    Some("character") => resolver.default_character = Some(id),
                    Some("table") => resolver.default_table = Some(id),
                    _ => {}
                }
            }
        }
        resolver
    }

    /// The style with this id.
    pub fn style(&self, id: &str) -> Option<&'a El> {
        self.styles.get(id).copied()
    }

    /// A style's name, as Word shows it (`heading 1` is shown as `Heading 1`).
    pub fn name_of(&self, id: &str) -> String {
        let name = self.style(id).and_then(|style| style.val_of("w:name")).unwrap_or(id);
        display_name(name)
    }

    /// The paragraph style a `w:pPr` names, or the default one.
    pub fn paragraph_style_id(&self, ppr: &El) -> Option<&'a str> {
        match ppr.val_of("w:pStyle") {
            Some(id) => {
                self.styles.get_key_value(id).map(|(key, _)| *key).or(self.default_paragraph)
            }
            None => self.default_paragraph,
        }
    }

    /// A style and every style it is based on, the root first. A loop in `basedOn` is cut where it repeats.
    pub fn chain(&self, id: &str) -> Vec<&'a El> {
        let mut out: Vec<&'a El> = Vec::new();
        let mut next = Some(id.to_owned());
        while let Some(id) = next {
            let Some(style) = self.style(&id) else { break };
            if out.iter().any(|seen| std::ptr::eq(*seen, style)) || out.len() > 32 {
                break;
            }
            out.push(style);
            next = style.val_of("w:basedOn").map(str::to_owned);
        }
        out.reverse();
        out
    }

    /// The table style and the conditional formats of the cell a paragraph is in: the style itself, then
    /// each `w:tblStylePr` the cell takes, in the order they are applied (TDD 5.2, table styles).
    fn table_levels(&self, table_style: Option<&str>) -> Vec<&'a El> {
        let Some(context) = table_style else { return Vec::new() };
        let (id, kinds) = split_cell_style(context);
        let Some(style) = self.style(id) else { return Vec::new() };
        let mut levels = vec![style];
        for kind in CONDITION_ORDER {
            if kinds.contains(&kind) {
                levels.extend(style.children_named("w:tblStylePr").filter(|el| el.attr("w:type") == Some(kind)));
            }
        }
        levels
    }

    /// A paragraph's text frame (`w:framePr`): its style chain's and its own, merged attribute by
    /// attribute, as Word merges them (builtin-styles.docx's Envelope Address paragraph says only
    /// `w:wrap="auto"` and takes its size and place from the style).
    pub fn frame_of(&self, ppr: &El) -> Option<El> {
        let mut frames: Vec<&El> = self
            .paragraph_style_id(ppr)
            .map(|style| self.chain(style).into_iter().filter_map(|el| el.child("w:pPr")?.child("w:framePr")).collect())
            .unwrap_or_default();
        frames.extend(ppr.child("w:framePr"));
        let mut merged: Option<El> = None;
        for frame in frames {
            let into = merged.get_or_insert_with(|| El::new("w:framePr"));
            for (key, value) in &frame.attrs {
                into.set_attr(key, value.clone());
            }
        }
        merged
    }

    /// The table style a table names, or the default one.
    pub fn table_style_id(&self, tblpr: &El) -> Option<&'a str> {
        match tblpr.val_of("w:tblStyle") {
            Some(id) => self.styles.get_key_value(id).map(|(key, _)| *key),
            None => self.default_table,
        }
    }

    /// The list a paragraph is in, as `(num id, level)`: its own `w:numPr`, or its style chain's.
    pub fn numbering_of(&self, ppr: &El) -> Option<(i64, u8)> {
        let read = |el: &El| -> Option<(Option<i64>, Option<u8>)> {
            let num = el.child("w:numPr")?;
            Some((
                num.val_of("w:numId").and_then(|v| v.parse().ok()),
                num.val_of("w:ilvl").and_then(|v| v.parse().ok()),
            ))
        };
        let mut found: (Option<i64>, Option<u8>) = (None, None);
        let style = self.paragraph_style_id(ppr);
        if let Some(style) = style {
            for el in self.chain(style) {
                if let Some(ppr) = el.child("w:pPr") {
                    if let Some((num, level)) = read(ppr) {
                        found = (num.or(found.0), level.or(found.1));
                    }
                }
            }
        }
        if let Some((num, level)) = read(ppr) {
            found = (num.or(found.0), level.or(found.1));
        }
        let num = found.0.filter(|num| *num > 0)?;
        // A list a style links to takes its level from the level that names that style.
        let level = found
            .1
            .or_else(|| style.and_then(|style| self.numbering.level_for_style(num, style)))
            .unwrap_or(0);
        Some((num, level.min(8)))
    }

    /// Everything about how a paragraph looks, `table_style` being the style of the table it is in.
    pub fn paragraph(&self, ppr: &El, table_style: Option<&str>) -> ParaLook {
        let mut look = ParaLook::default();
        if let Some(defaults) = self.paragraph_defaults {
            props::apply_paragraph(&mut look, defaults);
        }
        for table in self.table_levels(table_style) {
            if let Some(ppr) = table.child("w:pPr") {
                props::apply_paragraph(&mut look, ppr);
            }
        }
        let style = self.paragraph_style_id(ppr);
        if let Some(style) = style {
            for el in self.chain(style) {
                if let Some(ppr) = el.child("w:pPr") {
                    props::apply_paragraph(&mut look, ppr);
                }
            }
            look.style = style.to_owned();
        }
        look.numbering = self.numbering_of(ppr);
        if let Some((num, level)) = look.numbering {
            if let Some(ppr) = self.numbering.level(num, level).and_then(|lvl| lvl.child("w:pPr")) {
                props::apply_paragraph(&mut look, ppr);
            }
        }
        props::apply_paragraph(&mut look, ppr);
        look
    }

    /// Everything about how a run looks inside a paragraph with properties `ppr`.
    pub fn run(&self, ppr: &El, rpr: &El, table_style: Option<&str>) -> RunLook {
        let mut look = RunLook::default();
        if let Some(defaults) = self.run_defaults {
            props::apply_run(&mut look, defaults, &self.theme);
            props::apply_toggles_absolute(&mut look, defaults);
        }
        let levels: Vec<&El> =
            self.table_levels(table_style).into_iter().filter_map(|el| el.child("w:rPr")).collect();
        let paragraph_chain: Vec<&El> = self
            .paragraph_style_id(ppr)
            .map(|id| self.chain(id).into_iter().filter_map(|style| style.child("w:rPr")).collect())
            .unwrap_or_default();
        let character_id = rpr.val_of("w:rStyle").or(self.default_character);
        let character_chain: Vec<&El> = character_id
            .map(|id| self.chain(id).into_iter().filter_map(|style| style.child("w:rPr")).collect())
            .unwrap_or_default();
        for el in levels.iter().chain(&paragraph_chain).chain(&character_chain) {
            props::apply_run(&mut look, el, &self.theme);
        }
        // A table style's bold or italic (a header row's) sets the toggle outright, under the styles.
        for el in &levels {
            props::apply_toggles_absolute(&mut look, el);
        }
        // Toggles: each chain inherits normally inside itself, and the two chains combine by flipping.
        for name in props::TOGGLES {
            let in_chain =
                |chain: &[&El]| chain.iter().rev().find_map(|el| props::on_off(el, name));
            let combined = match (in_chain(&paragraph_chain), in_chain(&character_chain)) {
                (Some(a), Some(b)) => Some(a ^ b),
                (a, b) => a.or(b),
            };
            if let Some(value) = combined {
                *props::toggle_slot(&mut look, name) = value;
            }
        }
        props::apply_run(&mut look, rpr, &self.theme);
        props::apply_toggles_absolute(&mut look, rpr);
        look
    }

    /// Every style of a kind (`paragraph`, `character`, `table`), as `(id, name)`, hidden ones left out,
    /// in the order of their `w:uiPriority`, then name, which is how Word's style gallery orders them.
    pub fn listed(&self, kind: &str) -> Vec<(String, String)> {
        let mut styles: Vec<(i64, String, String)> = self
            .styles
            .values()
            .filter(|style| style.attr("w:type") == Some(kind))
            .filter(|style| {
                props::on_off(style, "w:semiHidden") != Some(true)
                    && props::on_off(style, "w:hidden") != Some(true)
            })
            .map(|style| {
                let id = style.attr("w:styleId").unwrap_or_default().to_owned();
                let priority =
                    style.val_of("w:uiPriority").and_then(|p| p.parse().ok()).unwrap_or(99);
                (priority, id.clone(), self.name_of(&id))
            })
            .collect();
        styles.sort_by(|a, b| (a.0, &a.2).cmp(&(b.0, &b.2)));
        styles.into_iter().map(|(_, id, name)| (id, name)).collect()
    }

    /// The id of the style named `name` (as Word shows it) or with that id.
    pub fn find(&self, name_or_id: &str) -> Option<&'a str> {
        if let Some((id, _)) = self.styles.get_key_value(name_or_id) {
            return Some(id);
        }
        let wanted = name_or_id.to_lowercase();
        self.styles
            .iter()
            .find(|(id, _)| self.name_of(id).to_lowercase() == wanted)
            .map(|(id, _)| *id)
    }
}

/// The name Word shows for a style. The built in styles are stored by their English names in lower case
/// (`heading 1`, `toc 2`, `footnote text`) and Word shows each word with a capital, with a few names of
/// its own: `annotation text` is shown as `Comment Text`.
pub fn display_name(name: &str) -> String {
    if !name.starts_with(char::is_lowercase) {
        return name.to_owned();
    }
    let renamed = match name {
        "annotation text" => "comment text",
        "annotation reference" => "comment reference",
        "annotation subject" => "comment subject",
        other => other,
    };
    renamed
        .split(' ')
        .map(|word| match word {
            "toc" => "TOC".to_owned(),
            "of" | "and" => word.to_owned(),
            _ => {
                let mut characters = word.chars();
                match characters.next() {
                    Some(first) => first.to_uppercase().chain(characters).collect(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl props::RunLook {
    /// The point size a raised or lowered run is drawn at, which is two thirds of its size as Word does.
    pub fn drawn_size(&self) -> f32 {
        match self.vert {
            props::VertAlign::Baseline => self.size,
            _ => self.size * 2.0 / 3.0,
        }
    }
}

/// The order Word applies a table style's conditional formats in, later ones over earlier ones
/// (ECMA-376 17.7.6): the whole table, the column bands, the row bands, the first and last columns,
/// the first and last rows, then the corner cells.
pub const CONDITION_ORDER: [&str; 13] = [
    "wholeTable",
    "band1Vert",
    "band2Vert",
    "band1Horz",
    "band2Horz",
    "firstCol",
    "lastCol",
    "firstRow",
    "lastRow",
    "neCell",
    "nwCell",
    "seCell",
    "swCell",
];

/// A table style named with the conditional formats one cell takes, which is what a paragraph inside a
/// cell is resolved against: the style id, then each `w:tblStylePr` type, joined by U+0001.
pub fn cell_style(id: &str, kinds: &[&str]) -> String {
    let mut out = id.to_owned();
    for kind in kinds {
        out.push('\u{1}');
        out.push_str(kind);
    }
    out
}

/// The style id and the conditional formats of a [`cell_style`] name.
pub fn split_cell_style(name: &str) -> (&str, Vec<&str>) {
    let mut parts = name.split('\u{1}');
    let id = parts.next().unwrap_or_default();
    (id, parts.collect())
}
