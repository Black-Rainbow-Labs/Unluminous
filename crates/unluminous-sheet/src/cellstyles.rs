//! Named cell styles: Normal, Good, Heading 1, Accent1 and the rest of Home, Cell Styles, and the styles
//! a person makes.
//!
//! A style is a name for one entry of `cellStyleXfs`, which holds the formatting the style defines. A cell
//! format points at its style with `xfId` and carries a copy of the style's formatting. A group the style does
//! not include (`applyFont="0"`) is left alone when the style is applied. A cell format whose value for a
//! group differs from its style's is a cell with direct formatting in that group, and a change to the style
//! does not touch it, which is how Excel keeps a bold cell bold when the style it uses changes colour.
//!
//! The 47 styles Excel ships are defined by `builtin_styles.xml`, the `styles.xml` Excel 16 wrote for a
//! workbook that used every one of them. A built in style enters a workbook the first time it is applied,
//! with its fonts set in the workbook's theme fonts, as Excel does.

use std::sync::OnceLock;

use crate::styles::{CellStyle, Font, Styles, Xf};

/// Which groups of formatting a style defines. The Style dialog's six check boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Includes {
    pub number: bool,
    pub font: bool,
    pub alignment: bool,
    pub border: bool,
    pub fill: bool,
    pub protection: bool,
}

impl Includes {
    /// Every group, which is what a style made from a cell includes.
    pub const ALL: Includes = Includes {
        number: true,
        font: true,
        alignment: true,
        border: true,
        fill: true,
        protection: true,
    };

    /// The groups a style's format includes: all of them but the ones its `apply` flags switch off.
    pub fn of(xf: &Xf) -> Includes {
        let on =
            |flag: &str| !xf.applies.iter().any(|(k, v)| k == flag && (v == "0" || v == "false"));
        Includes {
            number: on("applyNumberFormat"),
            font: on("applyFont"),
            alignment: on("applyAlignment"),
            border: on("applyBorder"),
            fill: on("applyFill"),
            protection: on("applyProtection"),
        }
    }

    /// The `apply` flags that write these inclusions: `0` for each group left out, in Excel's order.
    pub fn applies(&self) -> Vec<(String, String)> {
        [
            ("applyNumberFormat", self.number),
            ("applyFont", self.font),
            ("applyFill", self.fill),
            ("applyBorder", self.border),
            ("applyAlignment", self.alignment),
            ("applyProtection", self.protection),
        ]
        .into_iter()
        .filter(|(_, on)| !on)
        .map(|(flag, _)| (flag.to_owned(), "0".to_owned()))
        .collect()
    }

    fn has(&self, group: Group) -> bool {
        match group {
            Group::Number => self.number,
            Group::Font => self.font,
            Group::Fill => self.fill,
            Group::Border => self.border,
            Group::Alignment => self.alignment,
            Group::Protection => self.protection,
        }
    }
}

/// One group of the formatting a cell format holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Number,
    Font,
    Fill,
    Border,
    Alignment,
    Protection,
}

const GROUPS: [Group; 6] =
    [Group::Number, Group::Font, Group::Fill, Group::Border, Group::Alignment, Group::Protection];

/// Whether two formats have the same value for a group.
fn same(a: &Xf, b: &Xf, group: Group) -> bool {
    match group {
        Group::Number => a.num_fmt == b.num_fmt,
        Group::Font => a.font == b.font,
        Group::Fill => a.fill == b.fill,
        Group::Border => a.border == b.border,
        Group::Alignment => a.align == b.align,
        Group::Protection => a.locked == b.locked && a.hidden == b.hidden,
    }
}

/// Copy one group from one format to another.
fn copy(to: &mut Xf, from: &Xf, group: Group) {
    match group {
        Group::Number => to.num_fmt = from.num_fmt,
        Group::Font => to.font = from.font,
        Group::Fill => to.fill = from.fill,
        Group::Border => to.border = from.border,
        Group::Alignment => to.align = from.align.clone(),
        Group::Protection => {
            to.locked = from.locked;
            to.hidden = from.hidden;
        }
    }
}

/// What the Cell Styles gallery shows for one style.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleInfo {
    pub name: String,
    /// Excel's number for a style it ships.
    pub builtin_id: Option<u32>,
    /// The workbook has the style. A built in style that is not defined enters the workbook when applied.
    pub defined: bool,
    /// A built in style that was deleted, which the gallery leaves out.
    pub hidden: bool,
    /// A built in style whose definition was changed.
    pub modified: bool,
}

/// The styles Excel 16 wrote for a workbook that used all 47 built in styles.
fn reference() -> &'static Styles {
    static REFERENCE: OnceLock<Styles> = OnceLock::new();
    REFERENCE.get_or_init(|| {
        let mut styles = Styles::default();
        crate::xlsx::read::read_styles(include_str!("builtin_styles.xml"), &mut styles)
            .expect("the built in styles part is well formed");
        styles
    })
}

/// The longest name Excel accepts for a style.
pub const LONGEST_NAME: usize = 255;

impl Styles {
    /// The index in [`Styles::cell_styles`] of the style with a name. Names compare without regard to case.
    pub fn style_index(&self, name: &str) -> Option<usize> {
        self.cell_styles.iter().position(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// The name of the style a cell format uses.
    pub fn style_name_of(&self, xf: u32) -> String {
        let id = self.xf(xf).xf_id;
        self.cell_styles
            .iter()
            .filter(|s| s.xf_id == id)
            .min_by_key(|s| s.hidden)
            .map_or_else(|| "Normal".to_owned(), |s| s.name.clone())
    }

    /// Give a format the formatting of a style for each group the style includes, and point it at the style.
    pub(crate) fn take_cell_style(&self, next: &mut Xf, index: usize) {
        let Some(style) = self.cell_styles.get(index) else { return };
        let sxf = self.style_xfs.get(style.xf_id as usize).cloned().unwrap_or_default();
        let includes = Includes::of(&sxf);
        for group in GROUPS {
            if includes.has(group) {
                copy(next, &sxf, group);
            }
        }
        next.xf_id = style.xf_id;
    }

    /// The index of a style by name, bringing a built in style into the workbook when it is not there
    /// yet and showing one that was deleted.
    pub fn ensure_style(&mut self, name: &str) -> Option<usize> {
        if let Some(i) = self.style_index(name) {
            self.cell_styles[i].hidden = false;
            return Some(i);
        }
        let reference = reference();
        let rs = reference
            .cell_styles
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name) && s.builtin_id.is_some())?;
        let rxf = reference.style_xfs.get(rs.xf_id as usize)?.clone();
        let xf = self.import_xf(reference, &rxf);
        let mut style = CellStyle::named(&rs.name, 0, rs.builtin_id);
        style.i_level = rs.i_level;
        Some(self.add_style(style, xf))
    }

    /// A format from another set of styles, with its fonts, fills, borders and number format brought into
    /// this one. A font set in the theme's fonts takes this workbook's theme fonts.
    pub fn import_xf(&mut self, from: &Styles, xf: &Xf) -> Xf {
        let mut out = xf.clone();
        out.xf_id = 0;
        if xf.num_fmt >= 164 {
            if let Some(code) = from.num_fmts.get(&xf.num_fmt) {
                out.num_fmt = self.format_id(code);
            }
        }
        let mut font = from.fonts.get(xf.font as usize).cloned().unwrap_or_default();
        self.adapt_scheme_font(&mut font);
        out.font = Self::intern(&mut self.fonts, font);
        let fill = from.fills.get(xf.fill as usize).cloned().unwrap_or_default();
        out.fill = Self::intern(&mut self.fills, fill);
        let border = from.borders.get(xf.border as usize).cloned().unwrap_or_default();
        out.border = Self::intern(&mut self.borders, border);
        out
    }

    /// Set a font that follows the theme (`scheme` minor or major) in this workbook's theme font.
    fn adapt_scheme_font(&self, font: &mut Font) {
        let theme = match font.scheme.as_deref() {
            Some("minor") => self.theme_font.as_ref(),
            Some("major") => self.theme_major_font.as_ref(),
            _ => None,
        };
        if let Some(name) = theme {
            font.name = name.clone();
        }
    }

    /// Set every font that follows the theme in the theme's fonts, after the theme changed.
    pub fn refresh_scheme_fonts(&mut self) {
        let (minor, major) = (self.theme_font.clone(), self.theme_major_font.clone());
        for font in &mut self.fonts {
            let theme = match font.scheme.as_deref() {
                Some("minor") => minor.as_ref(),
                Some("major") => major.as_ref(),
                _ => None,
            };
            if let Some(name) = theme {
                font.name = name.clone();
            }
        }
    }

    /// Add a style with its format, placed among the others in name order as Excel writes them. Answers
    /// the style's index in [`Styles::cell_styles`].
    pub fn add_style(&mut self, mut style: CellStyle, mut xf: Xf) -> usize {
        xf.xf_id = 0;
        self.style_xfs.push(xf);
        style.xf_id = (self.style_xfs.len() - 1) as u32;
        let key = style.name.to_lowercase();
        let at = self.cell_styles.iter().position(|s| s.name.to_lowercase() > key);
        let at = at.unwrap_or(self.cell_styles.len());
        self.cell_styles.insert(at, style);
        at
    }

    /// Replace the formatting a style defines. Every cell format that uses the style and has not
    /// overridden a group, which is to say whose value for the group is the style's old value, takes the
    /// new value; a cell with its own formatting for a group keeps it.
    pub fn redefine_style(&mut self, index: usize, new: Xf) {
        let xf_id = self.cell_styles[index].xf_id;
        let old = self.style_xfs.get(xf_id as usize).cloned().unwrap_or_default();
        let includes = Includes::of(&new);
        for cell in self.xfs.iter_mut().filter(|x| x.xf_id == xf_id) {
            let mut changed = false;
            for group in GROUPS {
                if includes.has(group) && same(cell, &old, group) && !same(cell, &new, group) {
                    copy(cell, &new, group);
                    changed = true;
                }
            }
            if changed {
                cell.applies.clear();
            }
        }
        if let Some(slot) = self.style_xfs.get_mut(xf_id as usize) {
            *slot = Xf { xf_id: 0, ..new };
        }
        if self.cell_styles[index].builtin_id.is_some() {
            self.cell_styles[index].custom_builtin = true;
        }
    }

    /// Delete a style. Cells that used it return to the Normal style: a group of a cell that was the style's
    /// takes Normal's value, and a group the cell overrode stays. A built in style is hidden rather than
    /// removed, as Excel does, and a style a person made is removed.
    pub fn remove_style(&mut self, name: &str) -> Result<(), String> {
        let Some(index) = self.style_index(name) else {
            return Err(format!("There is no style named {name}."));
        };
        let style = self.cell_styles[index].clone();
        if style.builtin_id == Some(0) {
            return Err("You can't delete the Normal style.".to_owned());
        }
        let normal_id =
            self.cell_styles.iter().find(|s| s.builtin_id == Some(0)).map_or(0, |s| s.xf_id);
        let old = self.style_xfs.get(style.xf_id as usize).cloned().unwrap_or_default();
        let normal = self.style_xfs.get(normal_id as usize).cloned().unwrap_or_default();
        for cell in self.xfs.iter_mut().filter(|x| x.xf_id == style.xf_id) {
            for group in GROUPS {
                if same(cell, &old, group) {
                    copy(cell, &normal, group);
                }
            }
            cell.xf_id = normal_id;
            cell.applies.clear();
        }
        if style.builtin_id.is_some() {
            self.cell_styles[index].hidden = true;
            return Ok(());
        }
        self.cell_styles.remove(index);
        let gone = style.xf_id;
        self.style_xfs.remove(gone as usize);
        for s in &mut self.cell_styles {
            if s.xf_id > gone {
                s.xf_id -= 1;
            }
        }
        for x in &mut self.xfs {
            if x.xf_id > gone {
                x.xf_id -= 1;
            }
        }
        Ok(())
    }

    /// Bring the styles of another workbook into this one, as Styles.Merge does: a style this workbook does
    /// not have is added, and one it has by the same name takes the other workbook's definition. Answers how
    /// many styles were brought in. Normal is left as it is unless the other workbook changed it.
    pub fn merge_styles_from(&mut self, from: &Styles) -> usize {
        let mut count = 0;
        for style in &from.cell_styles {
            if style.builtin_id == Some(0) && !style.custom_builtin {
                continue;
            }
            let Some(sxf) = from.style_xfs.get(style.xf_id as usize) else { continue };
            let xf = self.import_xf(from, sxf);
            match self.style_index(&style.name) {
                Some(i) => {
                    self.redefine_style(i, xf);
                    self.cell_styles[i].hidden = style.hidden;
                    self.cell_styles[i].custom_builtin = style.custom_builtin;
                }
                None => {
                    let mut added = style.clone();
                    added.other.clear();
                    self.add_style(added, xf);
                }
            }
            count += 1;
        }
        count
    }

    /// Every style the gallery lists: the 47 built in styles in the gallery's order, then the styles a
    /// person made in name order. Each says whether this workbook defines it yet.
    pub fn style_gallery(&self) -> Vec<StyleInfo> {
        let reference = reference();
        let mut order: Vec<&CellStyle> = reference.cell_styles.iter().collect();
        order.sort_by_key(|s| s.xf_id);
        let mut out: Vec<StyleInfo> = order
            .into_iter()
            .map(|r| {
                let own = self.cell_styles.iter().find(|s| s.name.eq_ignore_ascii_case(&r.name));
                StyleInfo {
                    name: r.name.clone(),
                    builtin_id: r.builtin_id,
                    defined: own.is_some(),
                    hidden: own.is_some_and(|s| s.hidden),
                    modified: own.is_some_and(|s| s.custom_builtin),
                }
            })
            .collect();
        let mut custom: Vec<&CellStyle> = self
            .cell_styles
            .iter()
            .filter(|s| !out.iter().any(|o| o.name.eq_ignore_ascii_case(&s.name)))
            .collect();
        custom.sort_by_key(|s| s.name.to_lowercase());
        out.extend(custom.into_iter().map(|s| StyleInfo {
            name: s.name.clone(),
            builtin_id: s.builtin_id,
            defined: true,
            hidden: s.hidden,
            modified: s.custom_builtin,
        }));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_holds_the_47_styles_excel_ships() {
        let reference = reference();
        assert_eq!(reference.cell_styles.len(), 47);
        assert!(reference.cell_styles.iter().all(|s| s.builtin_id.is_some()));
        let good = reference.cell_styles.iter().find(|s| s.name == "Good").unwrap();
        assert_eq!(good.builtin_id, Some(26));
        let heading = reference.cell_styles.iter().find(|s| s.name == "Heading 1").unwrap();
        assert_eq!(heading.builtin_id, Some(16));
    }

    #[test]
    fn a_built_in_style_enters_the_workbook_in_the_workbooks_theme_font() {
        let mut styles = Styles::default_workbook();
        styles.theme_font = Some("Calibri".to_owned());
        styles.theme_major_font = Some("Calibri Light".to_owned());
        let i = styles.ensure_style("title").unwrap();
        assert_eq!(styles.cell_styles[i].name, "Title");
        let xf = styles.style_xfs[styles.cell_styles[i].xf_id as usize].clone();
        let font = &styles.fonts[xf.font as usize];
        assert_eq!(font.name, "Calibri Light");
        assert_eq!(font.size, 18.0);
    }

    #[test]
    fn styles_are_kept_in_name_order() {
        let mut styles = Styles::default_workbook();
        for name in ["Good", "Accent1", "Bad", "20% - Accent1"] {
            styles.ensure_style(name).unwrap();
        }
        let names: Vec<&str> = styles.cell_styles.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["20% - Accent1", "Accent1", "Bad", "Good", "Normal"]);
    }

    #[test]
    fn the_gallery_lists_the_built_in_styles_then_the_custom_ones() {
        let mut styles = Styles::default_workbook();
        let normal_xf = styles.style_xfs[0].clone();
        styles.add_style(CellStyle::named("Mine", 0, None), normal_xf);
        let gallery = styles.style_gallery();
        assert_eq!(gallery.len(), 48);
        assert_eq!(gallery[0].name, "Normal");
        assert!(gallery[0].defined && !gallery[1].defined);
        assert_eq!(gallery.last().unwrap().name, "Mine");
    }
}
