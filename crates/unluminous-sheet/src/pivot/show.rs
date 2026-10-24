//! Show Values As: a cell shown as a share of a total, a difference from another item, a running total or
//! a rank, worked out from the cells the records made.

use super::acc::CellVal;
use super::compute::{Axis, Calc};
use super::{Func, ShowAs, BASE_NEXT, BASE_PREVIOUS};
use crate::value::ErrorKind;

/// Which axis a base field is on and at what level of it.
#[derive(Debug, Clone, Copy)]
struct Base {
    rows: bool,
    /// The level of the nodes whose items the base field has.
    level: usize,
}

impl Calc<'_> {
    /// The figure of value field `d` in the cell of two nodes, as Show Values As makes it.
    pub fn shown(&self, r: usize, c: usize, d: usize, func: Option<Func>) -> CellVal {
        let v = self.raw(r, c, d, func);
        let Some(field) = self.table.data.get(d) else { return v };
        if func.is_some() {
            return v;
        }
        let base = |r: usize, c: usize| self.raw(r, c, d, None);
        match field.show {
            ShowAs::Normal => v,
            ShowAs::PercentOfTotal => ratio(v, base(0, 0)),
            ShowAs::PercentOfRow => ratio(v, base(r, 0)),
            ShowAs::PercentOfCol => ratio(v, base(0, c)),
            ShowAs::Index => index(v, base(0, 0), base(r, 0), base(0, c)),
            ShowAs::PercentOfParentRow => {
                ratio(v, base(self.rows.nodes[r].parent.unwrap_or(0), c))
            }
            ShowAs::PercentOfParentCol => {
                ratio(v, base(r, self.cols.nodes[c].parent.unwrap_or(0)))
            }
            _ => {
                let Some(b) = self.base_of(field.base_field) else { return v };
                self.relative(r, c, d, b)
            }
        }
    }

    fn base_of(&self, field: usize) -> Option<Base> {
        if let Some(i) = self.rows.fields.iter().position(|f| *f == field) {
            return Some(Base { rows: true, level: i + 1 });
        }
        self.cols.fields.iter().position(|f| *f == field).map(|i| Base { rows: false, level: i + 1 })
    }

    fn axis_of(&self, rows: bool) -> &Axis {
        if rows {
            &self.rows
        } else {
            &self.cols
        }
    }

    /// The settings that compare a cell with other items of the base field.
    fn relative(&self, r: usize, c: usize, d: usize, b: Base) -> CellVal {
        let field = &self.table.data[d];
        let axis = self.axis_of(b.rows);
        let node = if b.rows { r } else { c };
        let at = |n: usize| if b.rows { self.raw(n, c, d, None) } else { self.raw(r, n, d, None) };
        let v = self.raw(r, c, d, None);
        // Settings about one item's own parent need only the ancestor at the base field's level.
        if field.show == ShowAs::PercentOfParent {
            return match axis.ancestor_at(node, b.level) {
                Some(a) => ratio(v, at(a)),
                None => CellVal::Empty,
            };
        }
        if axis.nodes[node].level != b.level {
            return CellVal::Empty;
        }
        let parent = axis.nodes[node].parent.unwrap_or(0);
        let siblings = &axis.nodes[parent].children;
        let place = siblings.iter().position(|s| *s == node).unwrap_or(0);
        match field.show {
            ShowAs::Difference | ShowAs::Percent | ShowAs::PercentDiff => {
                let target = self.base_sibling(axis, siblings, place, field.base_item);
                let Some(t) = target else { return CellVal::Empty };
                let base_value = at(t);
                compare(field.show, v, base_value, t == node)
            }
            ShowAs::RunTotal | ShowAs::PercentRunTotal => {
                let mut sum = 0.0;
                for s in &siblings[..=place] {
                    match at(*s) {
                        CellVal::Num(n) => sum += n,
                        CellVal::Err(e) => return CellVal::Err(e),
                        CellVal::Empty => {}
                    }
                }
                if field.show == ShowAs::RunTotal {
                    CellVal::Num(sum)
                } else {
                    ratio(CellVal::Num(sum), at(parent))
                }
            }
            _ => {
                let CellVal::Num(x) = v else { return CellVal::Empty };
                let mut rank = 1;
                for s in siblings.iter() {
                    if let CellVal::Num(y) = at(*s) {
                        let ahead = if field.show == ShowAs::RankAscending { y < x } else { y > x };
                        if ahead {
                            rank += 1;
                        }
                    }
                }
                CellVal::Num(rank as f64)
            }
        }
    }

    /// The sibling a base item names: the previous or the next one, or the item itself.
    fn base_sibling(&self, axis: &Axis, siblings: &[usize], place: usize, base_item: u32) -> Option<usize> {
        match base_item {
            BASE_PREVIOUS => place.checked_sub(1).map(|p| siblings[p]),
            BASE_NEXT => siblings.get(place + 1).copied(),
            item => siblings.iter().copied().find(|s| axis.nodes[*s].item == item as usize),
        }
    }
}

/// A figure as a share of another. A total with nothing in it leaves the cell empty, a total of nothing
/// but zero is a division by zero, and a cell with nothing in it is a share of nothing.
fn ratio(x: CellVal, den: CellVal) -> CellVal {
    match (x, den) {
        (CellVal::Err(e), _) | (_, CellVal::Err(e)) => CellVal::Err(e),
        (_, CellVal::Empty) => CellVal::Empty,
        (x, CellVal::Num(den)) => {
            if den == 0.0 {
                CellVal::Err(ErrorKind::Div0)
            } else {
                CellVal::Num(x.number().unwrap_or(0.0) / den)
            }
        }
    }
}

fn index(v: CellVal, grand: CellVal, row: CellVal, col: CellVal) -> CellVal {
    for part in [v, grand, row, col] {
        if let CellVal::Err(e) = part {
            return CellVal::Err(e);
        }
    }
    let (CellVal::Num(g), CellVal::Num(r), CellVal::Num(c)) = (grand, row, col) else {
        return CellVal::Empty;
    };
    if r * c == 0.0 {
        return CellVal::Err(ErrorKind::Div0);
    }
    CellVal::Num(v.number().unwrap_or(0.0) * g / (r * c))
}

/// Difference From, % Of and % Difference From against the base item's cell.
fn compare(show: ShowAs, x: CellVal, base: CellVal, is_base: bool) -> CellVal {
    if let CellVal::Err(e) = x {
        return CellVal::Err(e);
    }
    if let CellVal::Err(e) = base {
        return CellVal::Err(e);
    }
    match show {
        ShowAs::Difference => {
            CellVal::Num(x.number().unwrap_or(0.0) - base.number().unwrap_or(0.0))
        }
        _ => {
            if x.is_empty() && !is_base {
                return CellVal::Err(ErrorKind::Null);
            }
            let (CellVal::Num(xv), CellVal::Num(b)) = (x, base) else { return CellVal::Empty };
            if b == 0.0 {
                return CellVal::Err(ErrorKind::Div0);
            }
            if show == ShowAs::Percent {
                CellVal::Num(xv / b)
            } else {
                CellVal::Num((xv - b) / b)
            }
        }
    }
}
