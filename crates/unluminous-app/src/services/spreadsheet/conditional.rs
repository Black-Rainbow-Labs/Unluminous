//! What a sheet's conditional formats do to a cell, for drawing.
//!
//! The rules themselves are evaluated by `unluminous_sheet::conditional`, which has no window in it and is
//! tested against Excel. This turns what it answers into what the grid draws: the fill, the font and the
//! number format laid over the cell's own, a data bar's length, and the icon an icon set picks.

use unluminous_sheet::conditional::{BarShape, IconPick, RuleSet};
use unluminous_sheet::coord::CellRef;
use unluminous_sheet::Workbook;

use super::display::Rgb;

/// What the rules that apply to a cell change about how it is drawn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Effect {
    pub fill: Option<Rgb>,
    pub color: Option<Rgb>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strike: Option<bool>,
    pub underline: Option<bool>,
    pub number_format: Option<String>,
    /// A data bar: how much of the cell it fills, from 0 to 1, and its colour.
    pub bar: Option<(f32, Rgb)>,
    /// The whole data bar: where it starts and ends, its border and its axis.
    pub bar_shape: Option<BarShape>,
    /// The icon an icon set gives the cell.
    pub icon: Option<IconPick>,
}

impl Effect {
    pub fn is_none(&self) -> bool {
        *self == Effect::default()
    }
}

/// A sheet's rules, prepared.
#[derive(Debug, Clone, Default)]
pub struct Rules {
    pub revision: u64,
    pub sheet: usize,
    set: RuleSet,
}

impl Rules {
    /// Prepare the rules of one sheet.
    pub fn prepare(book: &Workbook, sheet: usize, revision: u64) -> Rules {
        Rules { revision, sheet, set: RuleSet::prepare(book, sheet) }
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// What the rules do to one cell.
    pub fn effect(&self, book: &Workbook, now: f64, at: CellRef) -> Effect {
        let applied = self.set.apply(book, now, at);
        let look = applied.look(book);
        Effect {
            fill: look.fill,
            color: look.font_color,
            bold: look.bold,
            italic: look.italic,
            strike: look.strike,
            underline: look.underline,
            number_format: look.number_format,
            bar: applied.bar.as_ref().map(|b| ((b.end - b.start) as f32, b.fill)),
            bar_shape: applied.bar,
            icon: applied.icon,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_sheet::coord::Range;
    use unluminous_sheet::edit::Session;
    use unluminous_sheet::model::{CfRule, Cfvo};
    use unluminous_sheet::styles::Color;

    #[test]
    fn a_colour_scale_runs_from_the_smallest_to_the_largest() {
        let mut s = Session::new(Workbook::new(), 46318.5);
        for (i, n) in ["1", "2", "3"].iter().enumerate() {
            s.set_input(0, CellRef::new(i as u32, 0), n).unwrap();
        }
        let cfvo = |k: &str| Cfvo { kind: k.to_owned(), value: None };
        let rule = CfRule {
            color_scale: Some(vec![
                (cfvo("min"), Color::rgb(0, 0, 0)),
                (cfvo("max"), Color::rgb(200, 100, 0)),
            ]),
            ..CfRule::new("colorScale")
        };
        s.add_conditional(0, vec![Range::parse("A1:A3").unwrap()], rule).unwrap();
        let rules = Rules::prepare(&s.book, 0, 1);
        assert_eq!(rules.effect(&s.book, 0.0, CellRef::new(1, 0)).fill, Some((100, 50, 0)));
        assert_eq!(rules.effect(&s.book, 0.0, CellRef::new(2, 0)).fill, Some((200, 100, 0)));
    }

    #[test]
    fn a_cell_rule_compares_with_its_formula_moved_to_each_cell() {
        let mut s = Session::new(Workbook::new(), 46318.5);
        s.set_input(0, CellRef::new(0, 0), "5").unwrap();
        s.set_input(0, CellRef::new(1, 0), "15").unwrap();
        s.set_input(0, CellRef::new(0, 1), "10").unwrap();
        s.set_input(0, CellRef::new(1, 1), "10").unwrap();
        s.book.styles.dxfs.push(unluminous_sheet::styles::Dxf {
            fill: Some(Color::rgb(255, 0, 0)),
            ..Default::default()
        });
        let rule = CfRule {
            formulas: vec!["A1>B1".to_owned()],
            dxf: Some(0),
            ..CfRule::new("expression")
        };
        s.add_conditional(0, vec![Range::parse("A1:A2").unwrap()], rule).unwrap();
        let rules = Rules::prepare(&s.book, 0, 1);
        assert_eq!(rules.effect(&s.book, 0.0, CellRef::new(0, 0)).fill, None);
        assert_eq!(rules.effect(&s.book, 0.0, CellRef::new(1, 0)).fill, Some((255, 0, 0)));
    }
}
