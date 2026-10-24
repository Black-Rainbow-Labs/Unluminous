//! Named cell styles as an editing operation: apply a style to cells, make a style from a cell, change a
//! style, delete one and merge the styles of another workbook. The rules for what a style does to the cells
//! that use it are in [`crate::cellstyles`].

use crate::cellstyles::{Includes, StyleInfo, LONGEST_NAME};
use crate::coord::{CellRef, Range};
use crate::model::Workbook;
use crate::styles::{CellStyle, StyleChange};

use super::{Guard, Session};

impl Session {
    /// The styles the gallery lists: Excel's 47 and the ones a person made.
    pub fn style_gallery(&self) -> Vec<StyleInfo> {
        self.book.styles.style_gallery()
    }

    /// The name of the style a cell uses (`Range.Style.Name`).
    pub fn style_of(&self, sheet: usize, at: CellRef) -> String {
        let Some(s) = self.book.sheets.get(sheet) else { return "Normal".to_owned() };
        let xf =
            s.cell(at).map_or_else(|| super::inherited_style(s, (at.row, at.col)), |c| c.style);
        self.book.styles.style_name_of(xf)
    }

    /// Which groups of formatting a style defines, or `None` for a style that does not exist.
    pub fn style_includes(&self, name: &str) -> Option<Includes> {
        let styles = &self.book.styles;
        let style = styles.cell_styles.get(styles.style_index(name)?)?;
        styles.style_xfs.get(style.xf_id as usize).map(Includes::of)
    }

    /// Apply a style to the cells of some ranges, as Home, Cell Styles does. A built in style the workbook
    /// does not have yet enters it. The cells take the style's formatting for each group it includes and keep
    /// the rest.
    pub fn apply_style(
        &mut self,
        sheet: usize,
        ranges: &[Range],
        name: &str,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Format)?;
        let index = self
            .book
            .styles
            .ensure_style(name)
            .ok_or_else(|| format!("There is no style named {name}."))?;
        self.set_style(sheet, ranges, &StyleChange::CellStyle(index))
    }

    /// Make a style (Home, Cell Styles, New Cell Style) from the format of a cell, or from Normal when no
    /// cell is given. The cells keep their format.
    pub fn new_style(
        &mut self,
        name: &str,
        from: Option<(usize, CellRef)>,
        includes: Includes,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > LONGEST_NAME {
            return Err(format!("A style name must be 1 to {LONGEST_NAME} characters long."));
        }
        if self.book.styles.style_index(name).is_some() {
            return Err(format!("A style named {name} already exists."));
        }
        let mut xf = match from {
            Some((sheet, at)) => {
                self.check_sheet(sheet)?;
                let s = &self.book.sheets[sheet];
                let style = s
                    .cell(at)
                    .map_or_else(|| super::inherited_style(s, (at.row, at.col)), |c| c.style);
                self.book.styles.xf(style)
            }
            None => self.book.styles.style_xfs.first().cloned().unwrap_or_default(),
        };
        xf.applies = includes.applies();
        xf.quote_prefix = false;
        xf.pivot_button = false;
        self.remember_whole("New style");
        self.book.styles.add_style(CellStyle::named(name, 0, None), xf);
        Ok(())
    }

    /// Change what a style defines and which groups it includes (Modify Style). Cells that use the style
    /// change with it, except for the groups they format themselves.
    pub fn modify_style(
        &mut self,
        name: &str,
        changes: &[StyleChange],
        includes: Option<Includes>,
    ) -> Result<(), String> {
        if self.book.styles.style_index(name).is_none() && !self.is_built_in_style(name) {
            return Err(format!("There is no style named {name}."));
        }
        self.remember_whole("Modify style");
        let styles = &mut self.book.styles;
        let Some(index) = styles.ensure_style(name) else {
            return Err(format!("There is no style named {name}."));
        };
        let old = styles
            .style_xfs
            .get(styles.cell_styles[index].xf_id as usize)
            .cloned()
            .unwrap_or_default();
        let kept = includes.unwrap_or_else(|| Includes::of(&old));
        let mut xf = old;
        for change in changes {
            xf = styles.derive_xf(xf, change);
        }
        xf.applies = kept.applies();
        styles.redefine_style(index, xf);
        Ok(())
    }

    fn is_built_in_style(&self, name: &str) -> bool {
        self.book
            .styles
            .style_gallery()
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(name) && s.builtin_id.is_some())
    }

    /// Delete a style. Cells that used it return to Normal. Normal cannot be deleted.
    pub fn delete_style(&mut self, name: &str) -> Result<(), String> {
        let Some(index) = self.book.styles.style_index(name) else {
            return Err(format!("There is no style named {name}."));
        };
        if self.book.styles.cell_styles[index].builtin_id == Some(0) {
            return Err("You can't delete the Normal style.".to_owned());
        }
        self.remember_whole("Delete style");
        self.book.styles.remove_style(name)
    }

    /// Bring in the styles of another workbook (Merge Styles). A style with the same name is replaced by the
    /// other workbook's. Answers how many styles came in.
    pub fn merge_styles(&mut self, from: &Workbook) -> Result<usize, String> {
        self.remember_whole("Merge styles");
        Ok(self.book.styles.merge_styles_from(&from.styles))
    }
}
