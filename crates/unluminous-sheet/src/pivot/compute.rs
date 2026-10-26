//! Adding up a pivot table: which items each axis has, in which order, and the figure in every cell.
//!
//! The records are read once. Each belongs to one leaf of the row tree and one leaf of the column tree,
//! and is added into the cell of every pair of their ancestors, so a subtotal, a header row with its figure
//! and a grand total are all cells of the same grid.

use std::collections::{HashMap, HashSet};

use super::acc::{Acc, CellVal};
use super::cache::{CacheField, CacheValue, PivotCache};
use super::calcfield::{self, Expr};
use super::table::{AxisEntry, Filter, FilterKind, PivotTable};
use super::{Func, SortType};
use crate::numfmt::Format;
use crate::value::ErrorKind;

/// A node of an axis tree: the root, or one item of the field at its level.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub parent: Option<usize>,
    /// How many fields are down to this node. The root is 0.
    pub level: usize,
    /// The position of the item in its field's items. Meaningless for the root.
    pub item: usize,
    /// The nodes under this one, in the order they are shown.
    pub children: Vec<usize>,
}

/// One axis: the fields on it and the tree of items the records make.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Axis {
    pub fields: Vec<usize>,
    pub nodes: Vec<Node>,
}

impl Axis {
    fn new(fields: Vec<usize>) -> Axis {
        Axis {
            fields,
            nodes: vec![Node { parent: None, level: 0, item: usize::MAX, children: Vec::new() }],
        }
    }

    pub fn is_leaf(&self, node: usize) -> bool {
        self.nodes[node].level == self.fields.len()
    }

    /// The node reached from `from` by following some items down, if every one is there.
    pub fn follow(&self, from: usize, items: &[usize]) -> Option<usize> {
        let mut at = from;
        for item in items {
            at = *self.nodes[at].children.iter().find(|c| self.nodes[**c].item == *item)?;
        }
        Some(at)
    }

    /// The nodes from the root down to this one.
    pub fn chain(&self, node: usize) -> Vec<usize> {
        let mut out = vec![node];
        let mut at = node;
        while let Some(p) = self.nodes[at].parent {
            out.push(p);
            at = p;
        }
        out.reverse();
        out
    }

    /// The node at a level on the way to `node`, if it is that deep.
    pub fn ancestor_at(&self, node: usize, level: usize) -> Option<usize> {
        if self.nodes[node].level < level {
            return None;
        }
        let mut at = node;
        while self.nodes[at].level > level {
            at = self.nodes[at].parent?;
        }
        Some(at)
    }

    /// The items from the first field to this node's field.
    pub fn item_path(&self, node: usize) -> Vec<usize> {
        self.chain(node).iter().skip(1).map(|n| self.nodes[*n].item).collect()
    }
}

/// Where a value field's figures come from.
#[derive(Debug, Clone)]
enum Source {
    /// The accumulator in this slot.
    Slot(usize),
    /// A calculated field: a formula over sums, each name in it with its slot.
    Formula(Expr, Vec<(String, usize)>),
    /// Nothing sensible: the field is gone.
    Nothing,
}

/// The records of one cell, added up.
#[derive(Debug, Clone, Default)]
struct CellAcc {
    records: u64,
    accs: Vec<Acc>,
}

/// A pivot table's calculation.
pub struct Calc<'a> {
    pub table: &'a PivotTable,
    pub cache: &'a PivotCache,
    pub rows: Axis,
    pub cols: Axis,
    date1904: bool,
    /// For each used field, the item each record has.
    items_of: HashMap<usize, Vec<Option<usize>>>,
    /// Items each field lets through: not hidden, and passing its label filters.
    allowed: HashMap<usize, Vec<bool>>,
    slots: Vec<usize>,
    sources: Vec<Source>,
    cells: HashMap<(usize, usize), CellAcc>,
    leaf_of: Vec<Option<(usize, usize)>>,
    excluded_rows: HashSet<Vec<usize>>,
    excluded_cols: HashSet<Vec<usize>>,
    /// The nodes of calculated items on the rows and on the columns, with the formula of each.
    calc_rows: HashMap<usize, Expr>,
    calc_cols: HashMap<usize, Expr>,
}

impl<'a> Calc<'a> {
    pub fn build(table: &'a PivotTable, cache: &'a PivotCache, date1904: bool) -> Calc<'a> {
        let real = |axis: Vec<AxisEntry>| -> Vec<usize> {
            axis.into_iter()
                .filter_map(|e| match e {
                    AxisEntry::Field(f) => Some(f),
                    AxisEntry::Values => None,
                })
                .collect()
        };
        let mut calc = Calc {
            table,
            cache,
            rows: Axis::new(real(table.row_axis())),
            cols: Axis::new(real(table.col_axis())),
            date1904,
            items_of: HashMap::new(),
            allowed: HashMap::new(),
            slots: Vec::new(),
            sources: Vec::new(),
            cells: HashMap::new(),
            leaf_of: Vec::new(),
            excluded_rows: HashSet::new(),
            excluded_cols: HashSet::new(),
            calc_rows: HashMap::new(),
            calc_cols: HashMap::new(),
        };
        calc.prepare();
        calc.run();
        calc
    }

    /// The text of an item as the pivot shows it.
    pub fn item_text(&self, field: usize, x: usize) -> String {
        let value = self.item_value(field, x);
        match &value {
            CacheValue::Blank => "(blank)".to_owned(),
            CacheValue::Text(t) => t.to_string(),
            CacheValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
            CacheValue::Error(e) => e.text().to_owned(),
            CacheValue::Number(_) | CacheValue::Date(_) => {
                let code = self.field_format(field);
                Format::parse(&code).render(&value.to_value(), self.date1904, 255).text
            }
        }
    }

    /// The format code the items of a field are shown with.
    pub fn field_format(&self, field: usize) -> String {
        if let Some(code) = &self.table.fields[field].num_fmt {
            return code.clone();
        }
        let cf = &self.cache.fields[field];
        if let Some(code) = &cf.num_fmt_code {
            return code.clone();
        }
        crate::styles::builtin_format(cf.num_fmt_id).unwrap_or("General").to_owned()
    }

    /// The value of the item at a position in a pivot field's items.
    pub fn item_at(&self, field: usize, position: usize) -> CacheValue {
        let entry = &self.table.fields[field].items[position];
        if let Some(name) = &entry.name {
            return CacheValue::text(name);
        }
        self.item_value(field, entry.x)
    }

    fn item_value(&self, field: usize, x: usize) -> CacheValue {
        self.cache.fields[field].items().get(x).cloned().unwrap_or(CacheValue::Blank)
    }

    /// The text of the item at a position in a pivot field's items.
    pub fn label(&self, field: usize, position: usize) -> String {
        let entry = &self.table.fields[field].items[position];
        match &entry.name {
            Some(name) => name.clone(),
            None => self.item_text(field, entry.x),
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Setting up.

    fn prepare(&mut self) {
        let mut used: Vec<usize> = self.rows.fields.clone();
        used.extend(self.cols.fields.iter().copied());
        used.extend(self.table.pages.iter().map(|p| p.field));
        used.extend(filter_only_fields(self.table));
        for &f in &used {
            if self.items_of.contains_key(&f) {
                continue;
            }
            self.items_of.insert(f, super::group::record_items(self.cache, f));
            let allowed = self.allowed_items(f);
            self.allowed.insert(f, allowed);
        }
        self.prepare_data();
    }

    /// Which of a field's items its hidden flags and its label filters let through.
    fn allowed_items(&self, field: usize) -> Vec<bool> {
        let pf = &self.table.fields[field];
        let mut out = Vec::with_capacity(pf.items.len());
        let page = self.table.pages.iter().find(|p| p.field == field);
        for (i, entry) in pf.items.iter().enumerate() {
            let mut ok = !entry.hidden;
            if let Some(p) = page {
                if let Some(chosen) = p.item {
                    ok = chosen == i;
                }
            }
            if ok {
                let text = self.label(field, i);
                ok = pf.filters.iter().filter(|f| !f.kind.is_value()).all(|f| match f.kind {
                    FilterKind::DateBetween => {
                        let item = self.cache.fields[field].items().get(entry.x);
                        date_matches(f, item.and_then(super::cache::CacheValue::number))
                    }
                    _ => caption_matches(f, &text),
                });
            }
            out.push(ok);
        }
        out
    }

    fn prepare_data(&mut self) {
        for d in &self.table.data {
            let field = &self.cache.fields[d.field];
            let source = match &field.formula {
                None if field.database => Source::Slot(self.slot_for(d.field)),
                None => Source::Nothing,
                Some(text) => match calcfield::parse(text) {
                    Ok(expr) => {
                        let mut names = Vec::new();
                        calcfield::names_in(&expr, &mut names);
                        let refs: Vec<(String, usize)> = names
                            .iter()
                            .filter_map(|n| {
                                let f =
                                    self.cache.fields.iter().position(|f| {
                                        f.database && f.name.eq_ignore_ascii_case(n)
                                    })?;
                                Some((n.to_ascii_lowercase(), f))
                            })
                            .collect();
                        let refs = refs.into_iter().map(|(n, f)| (n, self.slot_for(f))).collect();
                        Source::Formula(expr, refs)
                    }
                    Err(_) => Source::Nothing,
                },
            };
            self.sources.push(source);
        }
    }

    fn slot_for(&mut self, field: usize) -> usize {
        match self.slots.iter().position(|f| *f == field) {
            Some(s) => s,
            None => {
                self.slots.push(field);
                self.slots.len() - 1
            }
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Reading the records.

    fn run(&mut self) {
        self.scan();
        // The value filters look at the figures as they are before any of them is applied.
        if self.apply_value_filters() {
            self.scan();
        }
        self.fill_empty_items();
        self.add_calculated_items();
        self.sort_all();
    }

    /// Under every parent, a node for each calculated item of the next field.
    fn add_calculated_items(&mut self) {
        for rows in [true, false] {
            let fields = if rows { self.rows.fields.clone() } else { self.cols.fields.clone() };
            for (i, &f) in fields.iter().enumerate() {
                let entries: Vec<(usize, Expr)> = self.table.fields[f]
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.calculated)
                    .filter_map(|(pos, e)| {
                        let item = self
                            .cache
                            .calculated_items
                            .iter()
                            .find(|c| c.field == f && c.item == e.x)?;
                        Some((pos, calcfield::parse(&item.formula).ok()?))
                    })
                    .collect();
                if entries.is_empty() {
                    continue;
                }
                let parents: Vec<usize> = {
                    let axis = if rows { &self.rows } else { &self.cols };
                    (0..axis.nodes.len())
                        .filter(|&n| axis.nodes[n].level == i && !axis.nodes[n].children.is_empty())
                        .collect()
                };
                for p in parents {
                    for (pos, expr) in &entries {
                        let axis = if rows { &mut self.rows } else { &mut self.cols };
                        let id = axis.nodes.len();
                        axis.nodes.push(Node {
                            parent: Some(p),
                            level: i + 1,
                            item: *pos,
                            children: Vec::new(),
                        });
                        axis.nodes[p].children.push(id);
                        let map = if rows { &mut self.calc_rows } else { &mut self.calc_cols };
                        map.insert(id, expr.clone());
                    }
                }
            }
        }
    }

    /// The figure of a calculated item: its formula over the figures of the other items under the same parent.
    fn calculated_value(&self, r: usize, c: usize, d: usize) -> Option<CellVal> {
        let (rows, node, expr) = if let Some(e) = self.calc_rows.get(&r) {
            (true, r, e)
        } else if let Some(e) = self.calc_cols.get(&c) {
            (false, c, e)
        } else {
            return None;
        };
        let axis = if rows { &self.rows } else { &self.cols };
        let parent = axis.nodes[node].parent?;
        let field = axis.fields[axis.nodes[node].level - 1];
        let lookup = |_f: Option<&str>, name: &str| -> Result<f64, ErrorKind> {
            // An item the parent does not have adds up to nothing.
            let Some(sibling) = axis.nodes[parent].children.iter().copied().find(|s| {
                *s != node && self.label(field, axis.nodes[*s].item).eq_ignore_ascii_case(name)
            }) else {
                return Ok(0.0);
            };
            let v =
                if rows { self.raw(sibling, c, d, None) } else { self.raw(r, sibling, d, None) };
            match v {
                CellVal::Num(n) => Ok(n),
                CellVal::Empty => Ok(0.0),
                CellVal::Err(e) => Err(e),
            }
        };
        Some(match calcfield::evaluate(expr, &lookup) {
            Ok(n) => CellVal::Num(n),
            Err(e) => CellVal::Err(e),
        })
    }

    /// The calculated items under a node, however deep, add their figures to its total.
    fn calculated_extra(&self, r: usize, c: usize, d: usize) -> f64 {
        let mut extra = 0.0;
        if !self.calc_rows.is_empty() {
            extra += self.extra_below(true, r, r, c, d);
        }
        if !self.calc_cols.is_empty() {
            extra += self.extra_below(false, c, r, c, d);
        }
        extra
    }

    fn extra_below(&self, rows: bool, node: usize, r: usize, c: usize, d: usize) -> f64 {
        let axis = if rows { &self.rows } else { &self.cols };
        let calc = if rows { &self.calc_rows } else { &self.calc_cols };
        let mut extra = 0.0;
        for &k in &axis.nodes[node].children {
            if calc.contains_key(&k) {
                let value = if rows {
                    self.calculated_value(k, c, d)
                } else {
                    self.calculated_value(r, k, d)
                };
                if let Some(CellVal::Num(n)) = value {
                    extra += n;
                }
            } else {
                extra += self.extra_below(rows, k, r, c, d);
            }
        }
        extra
    }

    /// Whether a record is let through by the filter fields and by every field on an axis.
    fn item_path_of(&self, fields: &[usize], rec: usize) -> Option<Vec<usize>> {
        let mut path = Vec::with_capacity(fields.len());
        for &f in fields {
            let x = self.items_of[&f][rec]?;
            let pos = self.table.fields[f].position_of(x)?;
            if !self.allowed[&f][pos] {
                return None;
            }
            path.push(pos);
        }
        Some(path)
    }

    fn page_ok(&self, rec: usize) -> bool {
        let pages_ok = self.table.pages.iter().all(|p| {
            let Some(x) = self.items_of[&p.field][rec] else { return false };
            match self.table.fields[p.field].position_of(x) {
                Some(pos) => self.allowed[&p.field][pos],
                None => false,
            }
        });
        // A field with a filter that is in no area, as a timeline's date field is, still filters the records.
        pages_ok
            && filter_only_fields(self.table).into_iter().all(|f| {
                let Some(x) = self.items_of[&f][rec] else { return false };
                self.table.fields[f].position_of(x).is_some_and(|pos| self.allowed[&f][pos])
            })
    }

    fn scan(&mut self) {
        self.rows = Axis::new(self.rows.fields.clone());
        self.cols = Axis::new(self.cols.fields.clone());
        self.cells.clear();
        self.leaf_of = vec![None; self.cache.records.len()];
        let mut row_index: HashMap<(usize, usize), usize> = HashMap::new();
        let mut col_index: HashMap<(usize, usize), usize> = HashMap::new();
        for rec in 0..self.cache.records.len() {
            if !self.page_ok(rec) {
                continue;
            }
            let Some(rp) = self.item_path_of(&self.rows.fields.clone(), rec) else { continue };
            let Some(cp) = self.item_path_of(&self.cols.fields.clone(), rec) else { continue };
            if self.is_excluded(&rp, true) || self.is_excluded(&cp, false) {
                continue;
            }
            let rl = descend(&mut self.rows, &mut row_index, &rp);
            let cl = descend(&mut self.cols, &mut col_index, &cp);
            self.leaf_of[rec] = Some((rl, cl));
            self.accumulate(rec, rl, cl);
        }
    }

    fn is_excluded(&self, path: &[usize], rows: bool) -> bool {
        let set = if rows { &self.excluded_rows } else { &self.excluded_cols };
        !set.is_empty() && (1..=path.len()).any(|n| set.contains(&path[..n]))
    }

    fn accumulate(&mut self, rec: usize, rl: usize, cl: usize) {
        let row_chain = self.rows.chain(rl);
        let col_chain = self.cols.chain(cl);
        let record = &self.cache.records[rec];
        for &r in &row_chain {
            for &c in &col_chain {
                let cell = self.cells.entry((r, c)).or_insert_with(|| CellAcc {
                    records: 0,
                    accs: vec![Acc::default(); self.slots.len()],
                });
                cell.records += 1;
                for (s, &field) in self.slots.iter().enumerate() {
                    cell.accs[s].add(&record[field]);
                }
            }
        }
    }

    /// With ShowAll, the items of a field that no record has appear under every parent.
    fn fill_empty_items(&mut self) {
        for rows in [true, false] {
            let fields = if rows { self.rows.fields.clone() } else { self.cols.fields.clone() };
            for (i, &f) in fields.iter().enumerate() {
                if !self.table.fields[f].show_all {
                    continue;
                }
                let parents: Vec<usize> = {
                    let axis = if rows { &self.rows } else { &self.cols };
                    (0..axis.nodes.len()).filter(|&n| axis.nodes[n].level == i).collect()
                };
                for p in parents {
                    for pos in 0..self.table.fields[f].items.len() {
                        if !self.allowed[&f][pos] {
                            continue;
                        }
                        let axis = if rows { &mut self.rows } else { &mut self.cols };
                        if axis.nodes[p].children.iter().any(|c| axis.nodes[*c].item == pos) {
                            continue;
                        }
                        let id = axis.nodes.len();
                        axis.nodes.push(Node {
                            parent: Some(p),
                            level: i + 1,
                            item: pos,
                            children: Vec::new(),
                        });
                        axis.nodes[p].children.push(id);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Filters and order.

    /// Apply the value filters and the top N filters. Answers whether any item went, so the records
    /// are read again without it.
    fn apply_value_filters(&mut self) -> bool {
        let mut changed = false;
        for rows in [true, false] {
            let fields = if rows { self.rows.fields.clone() } else { self.cols.fields.clone() };
            for (level, &f) in fields.iter().enumerate() {
                let filters: Vec<Filter> = self.table.fields[f]
                    .filters
                    .iter()
                    .filter(|x| x.kind.is_value())
                    .cloned()
                    .collect();
                for filter in filters {
                    changed |= self.apply_value_filter(rows, level, f, &filter);
                }
            }
        }
        changed
    }

    fn apply_value_filter(
        &mut self,
        rows: bool,
        level: usize,
        field: usize,
        filter: &Filter,
    ) -> bool {
        let d = filter.data.unwrap_or(0).min(self.table.data.len().saturating_sub(1));
        if self.table.data.is_empty() {
            return false;
        }
        let axis_nodes: Vec<usize> = {
            let axis = if rows { &self.rows } else { &self.cols };
            (0..axis.nodes.len()).filter(|&n| axis.nodes[n].level == level + 1).collect()
        };
        let _ = field;
        let mut by_parent: Vec<(usize, Vec<usize>)> = Vec::new();
        {
            let axis = if rows { &self.rows } else { &self.cols };
            for n in axis_nodes {
                let p = axis.nodes[n].parent.unwrap_or(0);
                match by_parent.iter_mut().find(|(q, _)| *q == p) {
                    Some((_, list)) => list.push(n),
                    None => by_parent.push((p, vec![n])),
                }
            }
        }
        let mut changed = false;
        for (_, siblings) in by_parent {
            let values: Vec<Option<f64>> = siblings
                .iter()
                .map(|&n| {
                    let (r, c) = if rows { (n, 0) } else { (0, n) };
                    self.raw(r, c, d, None).number()
                })
                .collect();
            let keep = keep_by_value(filter, &values);
            for (i, &n) in siblings.iter().enumerate() {
                if keep[i] {
                    continue;
                }
                let path = {
                    let axis = if rows { &self.rows } else { &self.cols };
                    axis.item_path(n)
                };
                let set = if rows { &mut self.excluded_rows } else { &mut self.excluded_cols };
                changed |= set.insert(path);
            }
        }
        changed
    }

    fn sort_all(&mut self) {
        for rows in [true, false] {
            let count = if rows { self.rows.nodes.len() } else { self.cols.nodes.len() };
            for n in 0..count {
                self.sort_children(rows, n);
            }
        }
    }

    fn sort_children(&mut self, rows: bool, node: usize) {
        let (level, mut children, field) = {
            let axis = if rows { &self.rows } else { &self.cols };
            let level = axis.nodes[node].level;
            let Some(&field) = axis.fields.get(level) else { return };
            (level, axis.nodes[node].children.clone(), field)
        };
        let _ = level;
        let pf = &self.table.fields[field];
        let auto = pf.auto_sort.as_ref().filter(|_| pf.sort != SortType::Manual);
        let item_of = |this: &Self, n: usize| -> usize {
            let axis = if rows { &this.rows } else { &this.cols };
            axis.nodes[n].item
        };
        match (pf.sort, auto) {
            (SortType::Manual, _) => children.sort_by_key(|&n| item_of(self, n)),
            (order, Some(auto)) => {
                let d = auto.data.min(self.table.data.len().saturating_sub(1));
                let mut keyed: Vec<(usize, Option<f64>)> = children
                    .iter()
                    .map(|&n| {
                        let (r, c) = if rows { (n, 0) } else { (0, n) };
                        (n, self.raw(r, c, d, None).number())
                    })
                    .collect();
                keyed.sort_by(|a, b| {
                    let ord = match (a.1, b.1) {
                        (Some(x), Some(y)) => {
                            x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        (Some(_), None) => return std::cmp::Ordering::Less,
                        (None, Some(_)) => return std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    };
                    let ord = if order == SortType::Descending { ord.reverse() } else { ord };
                    ord.then_with(|| item_of(self, a.0).cmp(&item_of(self, b.0)))
                });
                children = keyed.into_iter().map(|(n, _)| n).collect();
            }
            (order, None) => {
                let values: HashMap<usize, CacheValue> =
                    children.iter().map(|&n| (n, self.item_at(field, item_of(self, n)))).collect();
                children.sort_by(|a, b| {
                    let ord = values[a].order(&values[b]);
                    if order == SortType::Descending {
                        ord.reverse()
                    } else {
                        ord
                    }
                });
            }
        }
        let axis = if rows { &mut self.rows } else { &mut self.cols };
        axis.nodes[node].children = children;
    }

    // ------------------------------------------------------------------------------------------------
    // Figures.

    /// How many records a cell adds up.
    pub fn records(&self, r: usize, c: usize) -> u64 {
        self.cells.get(&(r, c)).map_or(0, |c| c.records)
    }

    /// The figure of value field `d` in the cell of two nodes, before Show Values As. A function other than
    /// the value field's own can be asked for, which is what a subtotal row of another function does.
    pub fn raw(&self, r: usize, c: usize, d: usize, func: Option<Func>) -> CellVal {
        if !self.calc_rows.is_empty() || !self.calc_cols.is_empty() {
            if let Some(v) = self.calculated_value(r, c, d) {
                return v;
            }
            let plain = self.raw_plain(r, c, d, func);
            let extra = self.calculated_extra(r, c, d);
            if extra != 0.0 && func.unwrap_or(self.table.data[d].func) == Func::Sum {
                return match plain {
                    CellVal::Num(n) => CellVal::Num(n + extra),
                    CellVal::Empty => CellVal::Num(extra),
                    e => e,
                };
            }
            return plain;
        }
        self.raw_plain(r, c, d, func)
    }

    fn raw_plain(&self, r: usize, c: usize, d: usize, func: Option<Func>) -> CellVal {
        let empty = CellAcc { records: 0, accs: vec![Acc::default(); self.slots.len()] };
        let Some(field) = self.table.data.get(d) else { return CellVal::Empty };
        let cell = match self.cells.get(&(r, c)) {
            Some(cell) => cell,
            None if matches!(self.sources[d], Source::Formula(..)) => &empty,
            None => return CellVal::Empty,
        };
        let func = func.unwrap_or(field.func);
        match &self.sources[d] {
            Source::Slot(s) => cell.accs[*s].result(func),
            Source::Formula(expr, refs) => {
                let lookup = |_item_field: Option<&str>, name: &str| -> Result<f64, ErrorKind> {
                    let key = name.to_ascii_lowercase();
                    refs.iter()
                        .find(|(n, _)| *n == key)
                        .map(|(_, slot)| cell.accs[*slot].sum)
                        .ok_or(ErrorKind::Name)
                };
                match calcfield::evaluate(expr, &lookup) {
                    Ok(n) => CellVal::Num(n),
                    Err(e) => CellVal::Err(e),
                }
            }
            Source::Nothing => CellVal::Empty,
        }
    }

    /// The text of the item a page field shows.
    pub fn page_text(&self, page: &super::table::PageField) -> String {
        let pf = &self.table.fields[page.field];
        if let Some(pos) = page.item {
            return self.label(page.field, pos);
        }
        if pf.items.iter().any(|i| i.hidden) && pf.multi_page {
            "(Multiple Items)".to_owned()
        } else {
            "(All)".to_owned()
        }
    }

    /// The cache field behind a field of the table.
    pub fn cache_field(&self, field: usize) -> &CacheField {
        &self.cache.fields[field]
    }
}

/// Walk down an axis tree following a path of items, making the nodes that are missing.
fn descend(axis: &mut Axis, index: &mut HashMap<(usize, usize), usize>, path: &[usize]) -> usize {
    let mut at = 0;
    for (i, &item) in path.iter().enumerate() {
        at = match index.get(&(at, item)) {
            Some(&n) => n,
            None => {
                let id = axis.nodes.len();
                axis.nodes.push(Node {
                    parent: Some(at),
                    level: i + 1,
                    item,
                    children: Vec::new(),
                });
                axis.nodes[at].children.push(id);
                index.insert((at, item), id);
                id
            }
        };
    }
    at
}

/// The fields in no area that carry a label or date filter.
fn filter_only_fields(table: &PivotTable) -> Vec<usize> {
    let used = table.used_fields();
    (0..table.fields.len())
        .filter(|f| !used.contains(f))
        .filter(|f| {
            let pf = &table.fields[*f];
            !pf.items.is_empty() && pf.filters.iter().any(|x| !x.kind.is_value())
        })
        .collect()
}

/// Whether the date of an item is in the days a timeline's filter keeps, the last day included whole.
fn date_matches(filter: &Filter, date: Option<f64>) -> bool {
    let first: f64 = filter.value1.trim().parse().unwrap_or(f64::NEG_INFINITY);
    let last: f64 = filter.value2.as_deref().unwrap_or("").trim().parse().unwrap_or(f64::INFINITY);
    date.is_some_and(|d| d >= first && d < last.floor() + 1.0)
}

/// Whether the text of an item passes a label filter.
fn caption_matches(filter: &Filter, text: &str) -> bool {
    let lower = |s: &str| s.to_lowercase();
    let t = lower(text);
    let a = lower(&filter.value1);
    let b = lower(filter.value2.as_deref().unwrap_or(""));
    let cmp = |x: &str, y: &str| crate::eval::compare_text(x, y);
    use std::cmp::Ordering::*;
    match filter.kind {
        FilterKind::CaptionEqual => t == a,
        FilterKind::CaptionNotEqual => t != a,
        FilterKind::CaptionBeginsWith => t.starts_with(&a),
        FilterKind::CaptionNotBeginsWith => !t.starts_with(&a),
        FilterKind::CaptionEndsWith => t.ends_with(&a),
        FilterKind::CaptionNotEndsWith => !t.ends_with(&a),
        FilterKind::CaptionContains => t.contains(&a),
        FilterKind::CaptionNotContains => !t.contains(&a),
        FilterKind::CaptionGreaterThan => cmp(&t, &a) == Greater,
        FilterKind::CaptionGreaterThanOrEqual => cmp(&t, &a) != Less,
        FilterKind::CaptionLessThan => cmp(&t, &a) == Less,
        FilterKind::CaptionLessThanOrEqual => cmp(&t, &a) != Greater,
        FilterKind::CaptionBetween => cmp(&t, &a) != Less && cmp(&t, &b) != Greater,
        FilterKind::CaptionNotBetween => cmp(&t, &a) == Less || cmp(&t, &b) == Greater,
        _ => true,
    }
}

/// Which of some sibling items a value filter keeps, given their figures.
fn keep_by_value(filter: &Filter, values: &[Option<f64>]) -> Vec<bool> {
    let n: f64 = filter.value1.trim().parse().unwrap_or(0.0);
    let m: f64 = filter.value2.as_deref().unwrap_or("").trim().parse().unwrap_or(0.0);
    match filter.kind {
        FilterKind::Count | FilterKind::Percent | FilterKind::Sum => keep_top(filter, values, n),
        kind => values
            .iter()
            .map(|v| match (kind, v) {
                (FilterKind::ValueEqual, Some(x)) => *x == n,
                (FilterKind::ValueNotEqual, Some(x)) => *x != n,
                (FilterKind::ValueNotEqual, None) => true,
                (FilterKind::ValueGreaterThan, Some(x)) => *x > n,
                (FilterKind::ValueGreaterThanOrEqual, Some(x)) => *x >= n,
                (FilterKind::ValueLessThan, Some(x)) => *x < n,
                (FilterKind::ValueLessThanOrEqual, Some(x)) => *x <= n,
                (FilterKind::ValueBetween, Some(x)) => *x >= n && *x <= m,
                (FilterKind::ValueNotBetween, Some(x)) => *x < n || *x > m,
                (FilterKind::ValueNotBetween, None) => true,
                _ => false,
            })
            .collect(),
    }
}

/// Top and bottom filters: the N largest or smallest, or enough of them to reach N percent or a sum.
fn keep_top(filter: &Filter, values: &[Option<f64>], n: f64) -> Vec<bool> {
    let mut order: Vec<usize> = (0..values.len()).filter(|&i| values[i].is_some()).collect();
    order.sort_by(|&a, &b| {
        let (x, y) = (values[a].unwrap_or(0.0), values[b].unwrap_or(0.0));
        let ord = x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
        if filter.top {
            ord.reverse()
        } else {
            ord
        }
    });
    let total: f64 = order.iter().map(|&i| values[i].unwrap_or(0.0)).sum();
    let mut keep = vec![false; values.len()];
    let mut running = 0.0;
    for (taken, &i) in order.iter().enumerate() {
        let enough = match filter.kind {
            FilterKind::Count => taken as f64 >= n,
            FilterKind::Percent => total != 0.0 && running / total * 100.0 >= n - 1e-9,
            _ => running >= n,
        };
        if enough {
            break;
        }
        keep[i] = true;
        running += values[i].unwrap_or(0.0);
    }
    keep
}
