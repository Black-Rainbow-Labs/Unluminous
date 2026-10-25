//! Show Values As: a cell shown as a share of a total, a difference from another item, a running total or
//! a rank, worked out from the cells the records made.

use super::acc::CellVal;
use super::compute::{Axis, Calc};
use super::{Func, ShowAs, BASE_NEXT, BASE_PREVIOUS};
use crate::value::ErrorKind;

/// Which axis a base field is on and at what level of it.
#[derive(Debug, Clone, Copy)]
struct BaseField {
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
            ShowAs::PercentOfTotal => share_of_total(v, base(0, 0)),
            ShowAs::PercentOfRow => share_of_total(v, base(r, 0)),
            ShowAs::PercentOfCol => share_of_total(v, base(0, c)),
            ShowAs::Index => index(v, base(0, 0), base(r, 0), base(0, c)),
            ShowAs::PercentOfParentRow => {
                if self.rows.fields.is_empty() {
                    return CellVal::Empty;
                }
                blank_errors(ratio(v, base(self.rows.nodes[r].parent.unwrap_or(0), c)))
            }
            ShowAs::PercentOfParentCol => {
                if self.cols.fields.is_empty() {
                    return CellVal::Empty;
                }
                blank_errors(ratio(v, base(r, self.cols.nodes[c].parent.unwrap_or(0))))
            }
            _ => {
                let Some(b) = self.base_of(field.base_field) else { return v };
                self.relative(r, c, d, b)
            }
        }
    }

    fn base_of(&self, field: usize) -> Option<BaseField> {
        if let Some(i) = self.rows.fields.iter().position(|f| *f == field) {
            return Some(BaseField { rows: true, level: i + 1 });
        }
        self.cols.fields.iter().position(|f| *f == field).map(|i| BaseField { rows: false, level: i + 1 })
    }

    fn axis_of(&self, rows: bool) -> &Axis {
        if rows {
            &self.rows
        } else {
            &self.cols
        }
    }

    /// The settings that compare a cell with other items of the base field. A cell deeper than the base
    /// field is compared with the cell at the same place under the other item.
    fn relative(&self, r: usize, c: usize, d: usize, b: BaseField) -> CellVal {
        let field = &self.table.data[d];
        let axis = self.axis_of(b.rows);
        let node = if b.rows { r } else { c };
        let at = |n: usize| if b.rows { self.raw(n, c, d, None) } else { self.raw(r, n, d, None) };
        let v = self.raw(r, c, d, None);
        // Settings about one item's own parent need only the ancestor at the base field's level.
        if field.show == ShowAs::PercentOfParent {
            return match axis.ancestor_at(node, b.level) {
                Some(a) => blank_errors(ratio(v, at(a))),
                None => CellVal::Empty,
            };
        }
        let Some(a) = axis.ancestor_at(node, b.level) else {
            // Above the base field there is nothing to compare, except that a share of a running total
            // that is an error says so.
            return match (field.show, v) {
                (ShowAs::PercentRunTotal, CellVal::Err(e)) => CellVal::Err(e),
                _ => CellVal::Empty,
            };
        };
        let below: Vec<usize> = axis.chain(node).iter().skip(b.level + 1).map(|n| axis.nodes[*n].item).collect();
        let under = |s: usize| -> Option<CellVal> { axis.follow(s, &below).map(at) };
        let parent = axis.nodes[a].parent.unwrap_or(0);
        let siblings = &axis.nodes[parent].children;
        let place = siblings.iter().position(|s| *s == a).unwrap_or(0);
        match field.show {
            ShowAs::Difference | ShowAs::Percent | ShowAs::PercentDiff => {
                match self.base_sibling(axis, siblings, place, field.base_item) {
                    Base::Nothing => CellVal::Empty,
                    Base::Missing => CellVal::Err(ErrorKind::NA),
                    Base::Node(t) => match under(t) {
                        Some(base_value) => compare(field.show, v, base_value, t == a),
                        None => CellVal::Empty,
                    },
                }
            }
            ShowAs::RunTotal | ShowAs::PercentRunTotal => {
                // The sum of the figures there are, and whether there were any.
                let sum_of = |list: &[usize]| -> Result<(f64, bool), ErrorKind> {
                    let (mut sum, mut any) = (0.0, false);
                    for s in list {
                        match under(*s) {
                            Some(CellVal::Num(n)) => {
                                sum += n;
                                any = true;
                            }
                            Some(CellVal::Err(e)) => return Err(e),
                            _ => {}
                        }
                    }
                    Ok((sum, any))
                };
                let running = match sum_of(&siblings[..=place]) {
                    Ok((n, _)) => n,
                    Err(e) => return CellVal::Err(e),
                };
                if field.show == ShowAs::RunTotal {
                    return CellVal::Num(running);
                }
                if below.is_empty() {
                    return match at(parent) {
                        CellVal::Err(_) => CellVal::Err(ErrorKind::Div0),
                        total => ratio(CellVal::Num(running), total),
                    };
                }
                match sum_of(siblings) {
                    Ok((total, true)) => ratio(CellVal::Num(running), CellVal::Num(total)),
                    Ok((_, false)) => CellVal::Empty,
                    Err(e) => CellVal::Err(e),
                }
            }
            _ => {
                let CellVal::Num(x) = v else { return CellVal::Empty };
                // Equal figures share a rank and the next figure takes the next rank.
                let mut ahead: Vec<f64> = Vec::new();
                for s in siblings.iter() {
                    if let Some(CellVal::Num(y)) = under(*s) {
                        let before = if field.show == ShowAs::RankAscending { y < x } else { y > x };
                        if before && !ahead.contains(&y) {
                            ahead.push(y);
                        }
                    }
                }
                CellVal::Num(ahead.len() as f64 + 1.0)
            }
        }
    }

    /// The sibling a base item names: the previous or the next one, or the item itself.
    fn base_sibling(&self, axis: &Axis, siblings: &[usize], place: usize, base_item: u32) -> Base {
        let found = |n: Option<usize>| n.map_or(Base::Nothing, Base::Node);
        match base_item {
            BASE_PREVIOUS => found(place.checked_sub(1).map(|p| siblings[p])),
            BASE_NEXT => found(siblings.get(place + 1).copied()),
            item => match siblings.iter().copied().find(|s| axis.nodes[*s].item == item as usize) {
                Some(n) => Base::Node(n),
                None => Base::Missing,
            },
        }
    }
}

/// What a base item picks out among the siblings of a cell.
enum Base {
    /// There is no such sibling: the first item has no previous one.
    Nothing,
    /// The item is not among them.
    Missing,
    Node(usize),
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

/// A share that is an error shows nothing.
fn blank_errors(v: CellVal) -> CellVal {
    match v {
        CellVal::Err(_) => CellVal::Empty,
        other => other,
    }
}

/// A share of a total: a total with nothing in it is a division by zero.
fn share_of_total(x: CellVal, total: CellVal) -> CellVal {
    match total {
        CellVal::Empty => match x {
            CellVal::Err(e) => CellVal::Err(e),
            _ => CellVal::Err(ErrorKind::Div0),
        },
        total => ratio(x, total),
    }
}

fn index(v: CellVal, grand: CellVal, row: CellVal, col: CellVal) -> CellVal {
    for part in [v, grand, row, col] {
        if let CellVal::Err(e) = part {
            return CellVal::Err(e);
        }
    }
    let (CellVal::Num(g), CellVal::Num(r), CellVal::Num(c)) = (grand, row, col) else {
        return CellVal::Err(ErrorKind::Div0);
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
        // A difference carries the error on; a share of an item that is an error shows nothing.
        return if show == ShowAs::Difference { CellVal::Err(e) } else { CellVal::Empty };
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
