//! Which formulas to compute after a change, in what order, and what each computed value does to the
//! sheet.
//!
//! An [`Engine`] keeps an index from every cell to the formulas that name it, built from each formula's
//! tree. A change marks the formulas that depend on it, transitively, and every volatile one (NOW, RAND,
//! OFFSET, INDIRECT). The marked formulas are ordered so each comes after the formulas it reads, by Kahn's
//! algorithm with no recursion, so a column of a hundred thousand chained formulas cannot exhaust the
//! stack. Formulas left over when no more can be ordered are in a cycle or downstream of one; those in
//! the cycle are 0, which is what Excel shows with iteration off.
//!
//! A formula whose answer is an array spills it into the cells to its right and below. When a spill grows
//! or shrinks, the formulas that read the cells it covered before or after are computed again.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use crate::coord::{CellRef, Range, MAX_COLS};
use crate::eval::{Ctx, Env, Val};
use crate::formula::{Expr, Formula, Shape};
use crate::model::Workbook;
use crate::value::{Array, ErrorKind, Value};

/// A formula cell: sheet, row, column.
pub type Key = (usize, u32, u32);

#[derive(Debug, Clone, PartialEq)]
enum Precedent {
    Cell(Key),
    Range { first: usize, last: usize, range: Range },
}

/// The dependency index of one workbook.
#[derive(Debug, Default, Clone)]
pub struct Engine {
    formulas: BTreeSet<Key>,
    precedents: HashMap<Key, Vec<Precedent>>,
    single: HashMap<Key, Vec<Key>>,
    ranges: Vec<(usize, usize, Range, Key)>,
    volatile: HashSet<Key>,
    dirty: BTreeSet<Key>,
    /// How many formulas the last calculation computed, for tests and the frame trace.
    pub computed: usize,
}

impl Engine {
    /// Build the index for a workbook. Nothing is computed.
    pub fn new(book: &Workbook) -> Self {
        let mut engine = Engine::default();
        engine.rebuild(book);
        engine
    }

    /// Throw the index away and read every formula again, after a change that moves cells.
    pub fn rebuild(&mut self, book: &Workbook) {
        *self = Engine::default();
        for (sheet_index, sheet) in book.sheets.iter().enumerate() {
            for (&(row, col), cell) in &sheet.cells {
                if cell.formula.is_some() {
                    self.index((sheet_index, row, col), book);
                }
            }
        }
    }

    /// How many formulas the workbook has.
    pub fn formula_count(&self) -> usize {
        self.formulas.len()
    }

    fn unindex(&mut self, key: Key) {
        if let Some(old) = self.precedents.remove(&key) {
            for p in old {
                if let Precedent::Cell(c) = p {
                    if let Some(list) = self.single.get_mut(&c) {
                        list.retain(|k| *k != key);
                    }
                }
            }
            self.ranges.retain(|(.., k)| *k != key);
        }
        self.volatile.remove(&key);
        self.formulas.remove(&key);
    }

    fn index(&mut self, key: Key, book: &Workbook) {
        self.unindex(key);
        let Some(formula) = book
            .sheets
            .get(key.0)
            .and_then(|s| s.cells.get(&(key.1, key.2)))
            .and_then(|c| c.formula.as_ref())
        else {
            return;
        };
        if formula.array_member.is_some() {
            return;
        }
        self.formulas.insert(key);
        let mut found = Found::default();
        collect(&formula.expr, book, key.0, &mut found, &mut Vec::new(), 0);
        if found.volatile {
            self.volatile.insert(key);
        }
        for p in &found.precedents {
            match p {
                Precedent::Cell(c) => self.single.entry(*c).or_default().push(key),
                Precedent::Range { first, last, range } => {
                    self.ranges.push((*first, *last, *range, key))
                }
            }
        }
        self.precedents.insert(key, found.precedents);
    }

    /// A cell's content changed: index its formula again if it has one, and mark what depends on it.
    pub fn changed(&mut self, book: &Workbook, sheet: usize, at: CellRef) {
        let key = (sheet, at.row, at.col);
        let is_formula = book
            .sheets
            .get(sheet)
            .and_then(|s| s.cells.get(&(at.row, at.col)))
            .is_some_and(|c| c.formula.is_some());
        if is_formula {
            self.index(key, book);
            self.dirty.insert(key);
        } else {
            self.unindex(key);
        }
        self.mark_dependents(key);
        // A cell a spill covers belongs to its anchor, which has to find out it is now blocked; and an
        // anchor that was blocked may be free to spill now.
        if let Some(s) = book.sheets.get(sheet) {
            if let Some(anchor) = s.spills.owner.get(&(at.row, at.col)) {
                let a = (sheet, anchor.0, anchor.1);
                self.dirty.insert(a);
                self.mark_dependents(a);
            }
            let blocked: Vec<Key> = s
                .cells
                .iter()
                .filter(|(_, c)| c.formula.is_some() && c.value == Value::Error(ErrorKind::Spill))
                .map(|(&(r, c), _)| (sheet, r, c))
                .collect();
            for b in blocked {
                self.dirty.insert(b);
                self.mark_dependents(b);
            }
        }
    }

    /// Mark everything that reads a cell.
    fn mark_dependents(&mut self, start: Key) {
        let mut queue = VecDeque::from([start]);
        let mut seen = HashSet::from([start]);
        while let Some(cell) = queue.pop_front() {
            for dependent in self.dependents(cell) {
                if seen.insert(dependent) {
                    self.dirty.insert(dependent);
                    queue.push_back(dependent);
                }
            }
        }
    }

    fn dependents(&self, cell: Key) -> Vec<Key> {
        let mut out: Vec<Key> = self.single.get(&cell).cloned().unwrap_or_default();
        let at = CellRef::new(cell.1, cell.2);
        for (first, last, range, key) in &self.ranges {
            if (*first..=*last).contains(&cell.0) && range.contains(at) {
                out.push(*key);
            }
        }
        out
    }

    /// Mark every formula, which is what opening a file and F9 with Shift and Control do.
    pub fn mark_all(&mut self) {
        self.dirty = self.formulas.clone();
    }

    /// Compute every marked formula and every volatile one. `now` is the serial NOW() answers.
    pub fn recalculate(&mut self, book: &mut Workbook, now: f64, seed: u64) {
        let env = Env::new(now, seed);
        self.computed = 0;
        book.circular = None;
        for v in self.volatile.clone() {
            self.dirty.insert(v);
            self.mark_dependents(v);
        }
        for _pass in 0..8 {
            if self.dirty.is_empty() {
                break;
            }
            let dirty: Vec<Key> = std::mem::take(&mut self.dirty)
                .into_iter()
                .filter(|k| self.formulas.contains(k))
                .collect();
            let (order, cycle) = self.order(&dirty, book);
            let iterating = book.calc.iterate;
            if let (Some(first), false) = (cycle.iter().next(), iterating) {
                book.circular = Some((first.0, CellRef::new(first.1, first.2)));
            }
            let mut moved_spills: Vec<(usize, Range)> = Vec::new();
            let mut iterated = false;
            for key in order {
                if cycle.contains(&key) {
                    if !iterating {
                        if let Some(cell) = book.sheets[key.0].cells.get_mut(&(key.1, key.2)) {
                            cell.value = Value::Number(0.0);
                        }
                    } else if !iterated {
                        iterated = true;
                        let keys: Vec<Key> = cycle.iter().copied().collect();
                        self.iterate(book, &keys, &env);
                    }
                    continue;
                }
                self.computed += 1;
                if let Some(moved) = compute(book, key, &env) {
                    moved_spills.push((key.0, moved));
                }
                round_to_shown(book, key);
            }
            // A spill that changed its extent changes cells formulas may read.
            for (sheet, range) in moved_spills {
                for at in cells_with_dependents(self, sheet, range) {
                    self.mark_dependents((sheet, at.row, at.col));
                }
            }
        }
        self.dirty.clear();
    }

    /// Calculate the formulas of a circular reference over and over, as Excel does with iteration on. Each
    /// pass computes every formula of the cycle in sheet, row and column order, starting from the values the
    /// cells hold now. It stops after `iterate_count` passes, or earlier once no cell moved by `iterate_delta`.
    fn iterate(&mut self, book: &mut Workbook, cycle: &[Key], env: &Env) {
        let passes = book.calc.iterate_count.max(1);
        let delta = book.calc.iterate_delta;
        for _ in 0..passes {
            let mut largest = 0.0f64;
            for &key in cycle {
                let before = cell_value(book, key);
                self.computed += 1;
                compute(book, key, env);
                round_to_shown(book, key);
                largest = largest.max(change_between(&before, &cell_value(book, key)));
            }
            if largest < delta {
                break;
            }
        }
    }

    /// Order formulas so each comes after the marked formulas it reads. Answers the order, and the formulas
    /// that sit on a cycle.
    fn order(&self, dirty: &[Key], book: &Workbook) -> (Vec<Key>, BTreeSet<Key>) {
        let set: HashSet<Key> = dirty.iter().copied().collect();
        let mut indegree: HashMap<Key, usize> = dirty.iter().map(|k| (*k, 0)).collect();
        let mut successors: HashMap<Key, Vec<Key>> = HashMap::new();
        for &key in dirty {
            for p in self.precedent_formulas(key, &set, book) {
                if p == key {
                    // A formula that reads itself is a cycle on its own.
                    *indegree.entry(key).or_default() += 1;
                    successors.entry(p).or_default().push(key);
                    continue;
                }
                *indegree.entry(key).or_default() += 1;
                successors.entry(p).or_default().push(key);
            }
        }
        // Volatile formulas last among the formulas that are ready, so the cells OFFSET and INDIRECT read
        // have been computed when they can be.
        let mut ready: VecDeque<Key> = dirty
            .iter()
            .filter(|k| indegree[k] == 0 && !self.volatile.contains(k))
            .copied()
            .collect();
        let mut ready_volatile: VecDeque<Key> = dirty
            .iter()
            .filter(|k| indegree[k] == 0 && self.volatile.contains(k))
            .copied()
            .collect();
        let mut order = Vec::with_capacity(dirty.len());
        while let Some(key) = ready.pop_front().or_else(|| ready_volatile.pop_front()) {
            order.push(key);
            if let Some(next) = successors.get(&key) {
                for n in next {
                    let d = indegree.get_mut(n).unwrap();
                    *d -= 1;
                    if *d == 0 {
                        if self.volatile.contains(n) {
                            ready_volatile.push_back(*n);
                        } else {
                            ready.push_back(*n);
                        }
                    }
                }
            }
        }
        if order.len() == dirty.len() {
            return (order, BTreeSet::new());
        }
        // What is left is on a cycle or downstream of one. Peel the formulas nothing left depends on; what
        // cannot be peeled is the cycle.
        let ordered: HashSet<Key> = order.iter().copied().collect();
        let mut left: BTreeSet<Key> =
            dirty.iter().filter(|k| !ordered.contains(k)).copied().collect();
        let mut peeled = Vec::new();
        loop {
            let sinks: Vec<Key> = left
                .iter()
                .filter(|k| {
                    successors.get(k).is_none_or(|next| next.iter().all(|n| !left.contains(n)))
                })
                .copied()
                .collect();
            if sinks.is_empty() {
                break;
            }
            for s in sinks {
                left.remove(&s);
                peeled.push(s);
            }
        }
        let cycle = left.clone();
        order.extend(left);
        order.extend(peeled.into_iter().rev());
        (order, cycle)
    }

    /// The marked formulas a formula reads, through its single cells and its ranges.
    fn precedent_formulas(&self, key: Key, marked: &HashSet<Key>, book: &Workbook) -> Vec<Key> {
        let mut out = Vec::new();
        let Some(precedents) = self.precedents.get(&key) else { return out };
        for p in precedents {
            match p {
                Precedent::Cell(c) => {
                    if marked.contains(c) {
                        out.push(*c);
                    }
                    // A cell a spill covers is computed by the spill's anchor.
                    if let Some(anchor) =
                        book.sheets.get(c.0).and_then(|s| s.spills.owner.get(&(c.1, c.2)))
                    {
                        let a = (c.0, anchor.0, anchor.1);
                        if marked.contains(&a) {
                            out.push(a);
                        }
                    }
                }
                Precedent::Range { first, last, range } => {
                    for sheet in *first..=*last {
                        let lo = (sheet, range.first.row, 0);
                        let hi = (sheet, range.last.row, MAX_COLS);
                        for k in self.formulas.range(lo..=hi) {
                            if k.2 >= range.first.col && k.2 <= range.last.col && marked.contains(k)
                            {
                                out.push(*k);
                            }
                        }
                        // Spills reaching into the range come from anchors that may be outside it.
                        if let Some(s) = book.sheets.get(sheet) {
                            for (anchor, (rows, cols)) in &s.spills.anchors {
                                let spill = Range::new(
                                    CellRef::new(anchor.0, anchor.1),
                                    CellRef::new(anchor.0 + rows - 1, anchor.1 + cols - 1),
                                );
                                let a = (sheet, anchor.0, anchor.1);
                                if spill.intersects(range) && marked.contains(&a) {
                                    out.push(a);
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

/// Cells in a range, so their dependents can be marked.
/// The value a formula cell holds now.
fn cell_value(book: &Workbook, key: Key) -> Value {
    book.sheets[key.0].cells.get(&(key.1, key.2)).map_or(Value::Empty, |c| c.value.clone())
}

/// How far a value moved between two passes: the difference of two numbers, none for two values that are
/// the same, and as far as can be for anything else that changed.
fn change_between(before: &Value, after: &Value) -> f64 {
    match (before, after) {
        (Value::Number(a), Value::Number(b)) => (a - b).abs(),
        (a, b) if a == b => 0.0,
        _ => f64::INFINITY,
    }
}

/// With precision as displayed, a number a formula computed is kept as its cell shows it.
fn round_to_shown(book: &mut Workbook, key: Key) {
    if book.calc.full_precision {
        return;
    }
    let date1904 = book.date1904;
    let Some(cell) = book.sheets[key.0].cells.get(&(key.1, key.2)) else {
        return;
    };
    let Value::Number(n) = cell.value else {
        return;
    };
    let shown = book.shown_number(cell.style, n, date1904);
    if shown != n {
        if let Some(cell) = book.sheets[key.0].cells.get_mut(&(key.1, key.2)) {
            cell.value = Value::Number(shown);
        }
    }
}

fn cells_with_dependents(engine: &Engine, sheet: usize, range: Range) -> Vec<CellRef> {
    let mut out: Vec<CellRef> = engine
        .single
        .keys()
        .filter(|k| k.0 == sheet && range.contains(CellRef::new(k.1, k.2)))
        .map(|k| CellRef::new(k.1, k.2))
        .collect();
    for (first, last, r, _) in &engine.ranges {
        if (*first..=*last).contains(&sheet) {
            if let Some(i) = r.intersection(&range) {
                out.push(i.first);
            }
        }
    }
    out
}

#[derive(Default)]
struct Found {
    precedents: Vec<Precedent>,
    volatile: bool,
}

/// The cells and ranges an expression reads, following defined names, tables and spill references.
fn collect(
    expr: &Expr,
    book: &Workbook,
    sheet: usize,
    found: &mut Found,
    names: &mut Vec<String>,
    depth: usize,
) {
    if depth > 64 {
        found.volatile = true;
        return;
    }
    expr.walk(&mut |e| match e {
        Expr::Ref(r) => {
            let resolve = |name: &Option<std::sync::Arc<str>>| match name {
                None => Some(sheet),
                Some(n) => book.sheet_index(n),
            };
            let first = resolve(&r.sheet);
            let last = match &r.sheet_last {
                Some(_) => resolve(&r.sheet_last),
                None => first,
            };
            let (Some(first), Some(last)) = (first, last) else { return };
            let (first, last) = (first.min(last), first.max(last));
            if r.shape == Shape::Cell && first == last {
                found.precedents.push(Precedent::Cell((first, r.first.row, r.first.col)));
            } else {
                let range = match r.shape {
                    Shape::Columns => Range::new(
                        CellRef::new(0, r.first.col),
                        CellRef::new(crate::coord::MAX_ROWS - 1, r.last.col),
                    ),
                    Shape::Rows => Range::new(
                        CellRef::new(r.first.row, 0),
                        CellRef::new(r.last.row, MAX_COLS - 1),
                    ),
                    _ => r.range(),
                };
                found.precedents.push(Precedent::Range { first, last, range });
            }
        }
        Expr::Name(name) => {
            let upper = name.to_ascii_uppercase();
            if names.contains(&upper) {
                return;
            }
            let (scope, bare) = match name.split_once('!') {
                Some((s, b)) => (book.sheet_index(s.trim_matches('\'')).unwrap_or(sheet), b),
                None => (sheet, &**name),
            };
            if let Some(defined) = book.name(bare, scope) {
                if let Ok(parsed) =
                    crate::formula::parser::parse(defined.formula.trim_start_matches('='))
                {
                    names.push(upper);
                    collect(&parsed, book, scope, found, names, depth + 1);
                    names.pop();
                }
            } else if let Some(table) = book.table(bare) {
                found.precedents.push(Precedent::Range {
                    first: table.sheet,
                    last: table.sheet,
                    range: table.range,
                });
            }
        }
        Expr::Table(t) => {
            match t.table.as_ref().and_then(|_| crate::eval::table_area(book, t, sheet, None).ok())
            {
                Some((on, range)) => {
                    found.precedents.push(Precedent::Range { first: on, last: on, range })
                }
                None => found.volatile = true,
            }
        }
        Expr::Spill(inner) => {
            if let Expr::Ref(r) = &**inner {
                let target = r.sheet.as_ref().and_then(|n| book.sheet_index(n)).unwrap_or(sheet);
                if let Some(s) = book.sheets.get(target) {
                    // The anchor, and whatever it spills into now; a change of extent is caught by
                    // the engine marking the cells a moved spill covered.
                    found.precedents.push(Precedent::Cell((target, r.first.row, r.first.col)));
                    let spill = s
                        .spills
                        .range_of((r.first.row, r.first.col))
                        .unwrap_or(Range::cell(r.first.at()));
                    found.precedents.push(Precedent::Range {
                        first: target,
                        last: target,
                        range: spill,
                    });
                }
            }
        }
        Expr::Call(name, _) => {
            if crate::functions::VOLATILE.contains(&&**name) {
                found.volatile = true;
            }
            if crate::functions::find(name).is_none() {
                // A LAMBDA called by name.
                let upper = name.to_ascii_uppercase();
                if !names.contains(&upper) {
                    if let Some(defined) = book.name(name, sheet) {
                        if let Ok(parsed) =
                            crate::formula::parser::parse(defined.formula.trim_start_matches('='))
                        {
                            names.push(upper);
                            collect(&parsed, book, sheet, found, names, depth + 1);
                            names.pop();
                        }
                    }
                }
            }
        }
        _ => {}
    });
}

/// What a formula's value is in its cell, given what it evaluated to.
enum Outcome {
    Single(Value),
    Spill(Array),
}

/// Compute one formula and store its value. Answers the extent of the spill when it changed.
fn compute(book: &mut Workbook, key: Key, env: &Env) -> Option<Range> {
    let (sheet, at) = (key.0, CellRef::new(key.1, key.2));
    let formula: Formula = book.sheets[sheet].cells.get(&(key.1, key.2))?.formula.clone()?;
    if formula.frozen {
        return None;
    }
    let outcome = {
        let mut ctx = Ctx::new(book, sheet, at, env);
        ctx.legacy = !formula.dynamic && formula.array.is_none();
        let v = ctx.eval(&formula.expr);
        if let Some(range) = formula.array {
            // A legacy array formula fills its own range.
            let array = match v {
                Val::Lambda(_) => Array::single(Value::Error(ErrorKind::Calc)),
                other => ctx.array(other).unwrap_or_else(|e| Array::single(Value::Error(e))),
            };
            let mut stretched = Vec::with_capacity(range.len() as usize);
            for r in 0..range.rows() as usize {
                for c in 0..range.cols() as usize {
                    stretched.push(array.stretched(r, c));
                }
            }
            Outcome::Spill(Array::new(range.rows() as usize, range.cols() as usize, stretched))
        } else {
            match v {
                Val::Array(a) if a.rows * a.cols > 1 && formula.dynamic => Outcome::Spill(a),
                Val::Ref(area) if area.range.len() > 1 && formula.dynamic && !area.is_3d() => {
                    match ctx.array(Val::Ref(area)) {
                        Ok(a) => Outcome::Spill(a),
                        Err(e) => Outcome::Single(Value::Error(e)),
                    }
                }
                Val::Lambda(_) => Outcome::Single(Value::Error(ErrorKind::Calc)),
                Val::Refs(ref areas) if areas.is_empty() => {
                    Outcome::Single(Value::Error(ErrorKind::Calc))
                }
                other => {
                    let single =
                        if formula.dynamic { ctx.single(other) } else { ctx.intersect(other) };
                    Outcome::Single(single)
                }
            }
        }
    };
    let s = &mut book.sheets[sheet];
    let before = s.spills.range_of((key.1, key.2));
    match outcome {
        Outcome::Single(v) => {
            s.spills.clear_anchor((key.1, key.2));
            // A formula that reads an empty cell shows 0.
            let v = if v.is_empty() { Value::Number(0.0) } else { v };
            if let Some(cell) = s.cells.get_mut(&(key.1, key.2)) {
                cell.value = v;
            }
            before
        }
        Outcome::Spill(array) => {
            if let Some(range) = formula.array {
                // Each cell of a legacy array formula holds its own value.
                for (i, cell_at) in range.cells().enumerate() {
                    let v = array.data.get(i).cloned().unwrap_or(Value::Error(ErrorKind::NA));
                    let v = if v.is_empty() { Value::Number(0.0) } else { v };
                    if let Some(cell) = s.cells.get_mut(&(cell_at.row, cell_at.col)) {
                        cell.value = v;
                    }
                }
                return None;
            }
            let rows = array.rows as u32;
            let cols = array.cols as u32;
            let target =
                at.offset(rows as i64 - 1, cols as i64 - 1).map(|last| Range::new(at, last));
            s.spills.clear_anchor((key.1, key.2));
            let blocked = match target {
                None => true,
                Some(range) => {
                    s.cells.range((range.first.row, 0)..=(range.last.row, MAX_COLS)).any(
                        |(&(r, c), cell)| {
                            (r, c) != (key.1, key.2)
                                && range.contains(CellRef::new(r, c))
                                && !(cell.value.is_empty() && cell.formula.is_none())
                        },
                    ) || s
                        .spills
                        .owner
                        .range((range.first.row, 0)..=(range.last.row, MAX_COLS))
                        .any(|(&(r, c), owner)| {
                            range.contains(CellRef::new(r, c)) && *owner != (key.1, key.2)
                        })
                }
            };
            let first =
                if blocked { Value::Error(ErrorKind::Spill) } else { array.data[0].clone() };
            let first = if first.is_empty() { Value::Number(0.0) } else { first };
            if let Some(cell) = s.cells.get_mut(&(key.1, key.2)) {
                cell.value = first;
            }
            if !blocked {
                s.spills.anchors.insert((key.1, key.2), (rows, cols));
                for (i, v) in array.data.into_iter().enumerate() {
                    let (r, c) = (key.1 + i as u32 / cols, key.2 + i as u32 % cols);
                    if (r, c) == (key.1, key.2) {
                        continue;
                    }
                    s.spills.owner.insert((r, c), (key.1, key.2));
                    // An empty value in a spill reads as 0, as it shows.
                    s.spills
                        .values
                        .insert((r, c), if v.is_empty() { Value::Number(0.0) } else { v });
                }
            }
            let after = s.spills.range_of((key.1, key.2));
            match (before, after) {
                (b, a) if b == a => None,
                (Some(b), Some(a)) => Some(b.union(&a)),
                (Some(b), None) => Some(b),
                (None, Some(a)) => Some(a),
                (None, None) => None,
            }
        }
    }
}

/// Every spill on a sheet, by its anchor, for the grid to draw a frame round.
pub fn spill_ranges(book: &Workbook, sheet: usize) -> BTreeMap<(u32, u32), Range> {
    let mut out = BTreeMap::new();
    if let Some(s) = book.sheets.get(sheet) {
        for anchor in s.spills.anchors.keys() {
            if let Some(r) = s.spills.range_of(*anchor) {
                out.insert(*anchor, r);
            }
        }
    }
    out
}

/// Recalculate a whole workbook: what opening a file does.
pub fn recalculate_all(book: &mut Workbook, now: f64) -> Engine {
    let mut engine = Engine::new(book);
    engine.mark_all();
    engine.recalculate(book, now, 0x9E37_79B9_7F4A_7C15);
    engine
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Cell;

    fn set(book: &mut Workbook, at: &str, text: &str) {
        let at = CellRef::parse(at).unwrap();
        let cell = match text.strip_prefix('=') {
            Some(_) => Cell { formula: Some(Formula::parse(text).unwrap()), ..Default::default() },
            None => Cell::value(crate::input::read(text, false, 2026).value),
        };
        book.sheets[0].set(at, cell);
    }

    fn get(book: &Workbook, at: &str) -> String {
        book.sheets[0].value_at(CellRef::parse(at).unwrap()).to_text()
    }

    #[test]
    fn formulas_are_computed_after_what_they_read() {
        let mut book = Workbook::new();
        set(&mut book, "A3", "=A2*2");
        set(&mut book, "A2", "=A1+1");
        set(&mut book, "A1", "5");
        set(&mut book, "B1", "=SUM(A1:A3)");
        let mut engine = recalculate_all(&mut book, 0.0);
        assert_eq!(get(&book, "A3"), "12");
        assert_eq!(get(&book, "B1"), "23");
        set(&mut book, "A1", "10");
        engine.changed(&book, 0, CellRef::parse("A1").unwrap());
        engine.recalculate(&mut book, 0.0, 1);
        assert_eq!(get(&book, "B1"), "43");
        assert_eq!(engine.computed, 3, "A2, A3 and B1, nothing else");
    }

    #[test]
    fn a_long_chain_does_not_exhaust_the_stack() {
        let mut book = Workbook::new();
        set(&mut book, "A1", "1");
        for row in 2..=20_000 {
            set(&mut book, &format!("A{row}"), &format!("=A{}+1", row - 1));
        }
        recalculate_all(&mut book, 0.0);
        assert_eq!(get(&book, "A20000"), "20000");
    }

    #[test]
    fn a_cycle_is_zero_and_named() {
        let mut book = Workbook::new();
        set(&mut book, "A1", "=B1+1");
        set(&mut book, "B1", "=A1+1");
        set(&mut book, "C1", "=A1+5");
        recalculate_all(&mut book, 0.0);
        assert_eq!(get(&book, "A1"), "0");
        assert!(book.circular.is_some());
        assert_eq!(get(&book, "C1"), "5");
    }

    #[test]
    fn an_array_spills_and_a_blocked_spill_is_an_error() {
        let mut book = Workbook::new();
        set(&mut book, "A1", "=SEQUENCE(3)");
        set(&mut book, "B1", "=SUM(A1#)");
        set(&mut book, "C1", "=A3*10");
        let mut engine = recalculate_all(&mut book, 0.0);
        assert_eq!(get(&book, "A3"), "3");
        assert_eq!(get(&book, "B1"), "6");
        assert_eq!(get(&book, "C1"), "30");
        set(&mut book, "A2", "x");
        engine.changed(&book, 0, CellRef::parse("A2").unwrap());
        engine.recalculate(&mut book, 0.0, 1);
        assert_eq!(get(&book, "A1"), "#SPILL!");
        assert_eq!(get(&book, "A2"), "x");
        assert_eq!(get(&book, "A3"), "");
    }

    #[test]
    fn a_spill_that_grows_recomputes_what_reads_its_new_cells() {
        let mut book = Workbook::new();
        set(&mut book, "Z1", "2");
        set(&mut book, "A1", "=SEQUENCE(Z1)");
        set(&mut book, "C1", "=A4");
        let mut engine = recalculate_all(&mut book, 0.0);
        assert_eq!(get(&book, "C1"), "0");
        set(&mut book, "Z1", "5");
        engine.changed(&book, 0, CellRef::parse("Z1").unwrap());
        engine.recalculate(&mut book, 0.0, 1);
        assert_eq!(get(&book, "C1"), "4");
    }

    #[test]
    fn indirect_and_now_are_recomputed_every_time() {
        let mut book = Workbook::new();
        set(&mut book, "A1", "1");
        set(&mut book, "B1", "=INDIRECT(\"A1\")*2");
        let mut engine = recalculate_all(&mut book, 0.0);
        assert_eq!(get(&book, "B1"), "2");
        set(&mut book, "A1", "7");
        engine.changed(&book, 0, CellRef::parse("A1").unwrap());
        engine.recalculate(&mut book, 0.0, 1);
        assert_eq!(get(&book, "B1"), "14");
    }

    #[test]
    fn a_legacy_formula_takes_one_value_by_intersection() {
        let mut book = Workbook::new();
        for row in 1..=3 {
            set(&mut book, &format!("A{row}"), &row.to_string());
        }
        let mut legacy = Formula::parse("=A1:A3*10").unwrap();
        legacy.dynamic = false;
        book.sheets[0].set(
            CellRef::parse("B2").unwrap(),
            Cell { formula: Some(legacy), ..Default::default() },
        );
        recalculate_all(&mut book, 0.0);
        assert_eq!(get(&book, "B2"), "20");
        assert_eq!(get(&book, "B3"), "");
    }
}
