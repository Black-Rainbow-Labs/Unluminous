//! The Evaluate Formula stepper.
//!
//! An [`Evaluator`] holds a formula as a tree and the part that is evaluated next, which Excel underlines.
//! Each step replaces that part with its value. The part is the first, reading the formula the way Excel
//! does, whose operands are all values: a cell reference, a name, an operator or a function whose
//! arguments have been worked out. A number or text written in the formula is already a value and is not a
//! step. `IF`, `IFERROR`, `IFNA` and `CHOOSE` work out only the argument they use, so the other branch is
//! left as written, as Excel leaves it.
//!
//! A reference to a cell that holds a formula can be stepped into, which shows that cell's formula and
//! steps through it. Stepping out finishes the inner formula and puts its value where the reference was.
//! The evaluator keeps no borrow of the workbook: every call is given the workbook to read.

use crate::coord::CellRef;
use crate::edit::Session;
use crate::eval::{Ctx, Env, Val};
use crate::formula::print::print;
use crate::formula::{Expr, Shape};
use crate::model::Workbook;
use crate::value::{Array, ErrorKind, Value};

/// A name used to mark the part being evaluated while the formula is printed.
const MARK: &str = "Z_EVALUATE_NEXT_Z";

/// One formula being stepped through: the top one, or one stepped into.
#[derive(Debug, Clone)]
struct Frame {
    sheet: usize,
    at: CellRef,
    root: Expr,
}

/// What the Evaluate Formula dialog shows.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateView {
    /// The cell whose formula is shown, as `Sheet1!$D$2`.
    pub reference: String,
    /// The formula as it stands, with a leading `=`.
    pub text: String,
    /// The characters of `text` that are underlined: the part evaluated next. `None` once there is a value.
    pub underlined: Option<(usize, usize)>,
    /// The Evaluate button works.
    pub can_evaluate: bool,
    /// The Step In button works: the underlined part is a cell that has a formula of its own.
    pub can_step_in: bool,
    /// The Step Out button works: a formula was stepped into.
    pub can_step_out: bool,
    /// The value, when the whole formula has been worked out.
    pub result: Option<Value>,
}

/// A stepper over a cell's formula.
#[derive(Debug, Clone)]
pub struct Evaluator {
    frames: Vec<Frame>,
    start: Frame,
    now: f64,
}

impl Session {
    /// Start Evaluate Formula on a cell. Fails when the cell has no formula.
    pub fn evaluator(&self, sheet: usize, at: CellRef) -> Result<Evaluator, String> {
        Evaluator::new(&self.book, sheet, at, self.now)
    }
}

impl Evaluator {
    /// Start on the formula in a cell.
    pub fn new(book: &Workbook, sheet: usize, at: CellRef, now: f64) -> Result<Self, String> {
        let formula = book
            .sheets
            .get(sheet)
            .and_then(|s| s.cell(at))
            .and_then(|c| c.formula.as_ref())
            .ok_or("The cell does not contain a formula.")?;
        let start = Frame { sheet, at, root: (*formula.expr).clone() };
        Ok(Self { frames: vec![start.clone()], start, now })
    }

    /// Begin again from the formula the stepper was started on.
    pub fn restart(&mut self) {
        self.frames = vec![self.start.clone()];
    }

    fn frame(&self) -> &Frame {
        self.frames.last().expect("a stepper always has a formula")
    }

    /// What the dialog shows now.
    pub fn view(&self, book: &Workbook) -> EvaluateView {
        let frame = self.frame();
        let path = target_path(&frame.root);
        let (text, underlined) = match &path {
            Some(path) => marked_text(&frame.root, path),
            None => (format!("={}", print(&frame.root, false)), None),
        };
        let can_step_in = path.as_ref().is_some_and(|p| self.step_in_target(book, p).is_some());
        EvaluateView {
            reference: reference_text(book, frame),
            text,
            underlined,
            can_evaluate: path.is_some(),
            can_step_in,
            can_step_out: self.frames.len() > 1,
            result: path.is_none().then(|| literal_value(&frame.root)),
        }
    }

    /// The formula of the cell a reference names, when the underlined part is a single cell holding one.
    fn step_in_target(&self, book: &Workbook, path: &[usize]) -> Option<Frame> {
        let frame = self.frame();
        let Expr::Ref(r) = node_at(&frame.root, path) else { return None };
        if r.shape != Shape::Cell || r.sheet_last.is_some() {
            return None;
        }
        let sheet = match &r.sheet {
            Some(name) => book.sheet_index(name)?,
            None => frame.sheet,
        };
        let at = r.first.at();
        let formula = book.sheets.get(sheet)?.cell(at)?.formula.as_ref()?;
        Some(Frame { sheet, at, root: (*formula.expr).clone() })
    }

    /// Evaluate the underlined part: the Evaluate button.
    pub fn evaluate(&mut self, book: &Workbook) -> Result<(), String> {
        let now = self.now;
        let frame = self.frames.last_mut().expect("a stepper always has a formula");
        let path = target_path(&frame.root).ok_or("The formula has been evaluated.")?;
        let value = evaluate_node(book, frame, &path, now);
        replace_at(&mut frame.root, &path, value);
        Ok(())
    }

    /// Show the formula of the cell the underlined reference names: the Step In button.
    pub fn step_in(&mut self, book: &Workbook) -> Result<(), String> {
        let path = target_path(&self.frame().root).ok_or("The formula has been evaluated.")?;
        let inner = self
            .step_in_target(book, &path)
            .ok_or("The underlined part is not a cell with a formula.")?;
        self.frames.push(inner);
        Ok(())
    }

    /// Finish the formula stepped into and put its value where the reference was: the Step Out button.
    pub fn step_out(&mut self, book: &Workbook) -> Result<(), String> {
        if self.frames.len() < 2 {
            return Err("No formula was stepped into.".to_owned());
        }
        while target_path(&self.frame().root).is_some() {
            self.evaluate(book)?;
        }
        let inner = self.frames.pop().expect("checked above");
        let parent = self.frames.last_mut().expect("a stepper always has a formula");
        let path = target_path(&parent.root).ok_or("The formula has been evaluated.")?;
        replace_at(&mut parent.root, &path, inner.root);
        Ok(())
    }

    /// Evaluate until the formula is a value, and answer it.
    pub fn finish(&mut self, book: &Workbook) -> Value {
        self.frames.truncate(1);
        while target_path(&self.frame().root).is_some() {
            if self.evaluate(book).is_err() {
                break;
            }
        }
        literal_value(&self.frame().root)
    }
}

/// `Sheet1!$D$2`.
fn reference_text(book: &Workbook, frame: &Frame) -> String {
    let name = book.sheets.get(frame.sheet).map(|s| s.name.as_str()).unwrap_or("");
    format!(
        "{}!${}${}",
        crate::formula::print::sheet_name(name),
        crate::coord::column_name(frame.at.col),
        frame.at.row + 1
    )
}

/// Whether an expression is already a value: nothing is left to work out in it.
fn is_value(e: &Expr) -> bool {
    match e {
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Missing => true,
        Expr::Array(rows) => rows.iter().flatten().all(is_value),
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) => matches!(**x, Expr::Number(_)),
        Expr::Paren(x) => is_value(x),
        _ => false,
    }
}

/// The children of an expression that must be values before it can be worked out, by position among
/// `children`, and the children in the order Excel works them out.
fn needed(e: &Expr) -> Vec<usize> {
    let all = || (0..children(e).len()).collect::<Vec<_>>();
    let Expr::Call(name, args) = e else { return all() };
    let upper = name.to_ascii_uppercase();
    let first = args.first();
    match (upper.as_str(), first) {
        ("IF", Some(cond)) if is_value(cond) => match truthy(cond) {
            Some(true) if args.len() > 1 => vec![0, 1],
            Some(false) if args.len() > 2 => vec![0, 2],
            _ => vec![0],
        },
        ("IFERROR" | "IFNA", Some(first)) if is_value(first) => {
            let caught = matches!(
                (upper.as_str(), first),
                ("IFNA", Expr::Error(ErrorKind::NA)) | ("IFERROR", Expr::Error(_))
            );
            if caught && args.len() > 1 {
                vec![0, 1]
            } else {
                vec![0]
            }
        }
        ("CHOOSE", Some(index)) if is_value(index) => match number_of(index) {
            Some(n) if n >= 1.0 && (n as usize) < args.len() => vec![0, n as usize],
            _ => vec![0],
        },
        ("IF" | "IFERROR" | "IFNA" | "CHOOSE", _) => vec![0],
        _ => all(),
    }
}

fn truthy(e: &Expr) -> Option<bool> {
    match e {
        Expr::Bool(b) => Some(*b),
        Expr::Number(n) => Some(*n != 0.0),
        Expr::Paren(x) => truthy(x),
        _ => None,
    }
}

fn number_of(e: &Expr) -> Option<f64> {
    match e {
        Expr::Number(n) => Some(*n),
        Expr::Paren(x) => number_of(x),
        _ => None,
    }
}

/// The child expressions of a node, in the order they are written.
fn children(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Neg(x)
        | Expr::Plus(x)
        | Expr::Percent(x)
        | Expr::Spill(x)
        | Expr::Implicit(x)
        | Expr::Paren(x) => vec![x],
        Expr::Binary(_, a, b) | Expr::Intersect(a, b) => vec![a, b],
        Expr::Call(_, args) | Expr::Union(args) => args.iter().collect(),
        Expr::Invoke(f, args) => std::iter::once(&**f).chain(args.iter()).collect(),
        _ => Vec::new(),
    }
}

fn children_mut(e: &mut Expr) -> Vec<&mut Expr> {
    match e {
        Expr::Neg(x)
        | Expr::Plus(x)
        | Expr::Percent(x)
        | Expr::Spill(x)
        | Expr::Implicit(x)
        | Expr::Paren(x) => vec![x],
        Expr::Binary(_, a, b) | Expr::Intersect(a, b) => vec![a, b],
        Expr::Call(_, args) | Expr::Union(args) => args.iter_mut().collect(),
        Expr::Invoke(f, args) => std::iter::once(&mut **f).chain(args.iter_mut()).collect(),
        _ => Vec::new(),
    }
}

/// The way to the part evaluated next, as the child positions to follow from the root.
fn target_path(root: &Expr) -> Option<Vec<usize>> {
    if is_value(root) {
        return None;
    }
    let mut path = Vec::new();
    let mut node = root;
    loop {
        let kids = children(node);
        let pending = needed(node).into_iter().find(|i| kids.get(*i).is_some_and(|k| !is_value(k)));
        match pending {
            Some(i) => {
                path.push(i);
                node = kids[i];
            }
            None => return Some(path),
        }
    }
}

fn node_at<'a>(root: &'a Expr, path: &[usize]) -> &'a Expr {
    path.iter().fold(root, |node, i| children(node)[*i])
}

fn replace_at(root: &mut Expr, path: &[usize], with: Expr) {
    let mut node = root;
    for i in path {
        node = children_mut(node).swap_remove(*i);
    }
    *node = with;
}

/// The formula printed with the part evaluated next marked: the text and the characters it covers.
fn marked_text(root: &Expr, path: &[usize]) -> (String, Option<(usize, usize)>) {
    let target = print(node_at(root, path), false);
    let mut marked = root.clone();
    replace_at(&mut marked, path, Expr::Name(MARK.into()));
    let text = format!("={}", print(&marked, false));
    let at = text.find(MARK).unwrap_or(0);
    let start = text[..at].chars().count();
    let end = start + target.chars().count();
    (text.replacen(MARK, &target, 1), Some((start, end)))
}

/// Work out one part of a formula.
fn evaluate_node(book: &Workbook, frame: &Frame, path: &[usize], now: f64) -> Expr {
    let env = Env::new(now, 0x2545_F491_4F6C_DD1D);
    let mut ctx = Ctx::new(book, frame.sheet, frame.at, &env);
    let val = ctx.eval(node_at(&frame.root, path));
    match val {
        Val::Lambda(_) => Expr::Error(ErrorKind::Calc),
        Val::Value(v) => expr_of(v),
        Val::Array(a) => array_expr(&a),
        other => match ctx.array(other) {
            Ok(a) if a.rows * a.cols == 1 => expr_of(a.data[0].clone()),
            Ok(a) => array_expr(&a),
            Err(e) => Expr::Error(e),
        },
    }
}

/// A value written in a formula.
fn expr_of(v: Value) -> Expr {
    match v {
        Value::Number(n) => Expr::Number(n),
        Value::Text(t) => Expr::Text(t),
        Value::Bool(b) => Expr::Bool(b),
        Value::Error(e) => Expr::Error(e),
        Value::Empty => Expr::Number(0.0),
    }
}

fn array_expr(a: &Array) -> Expr {
    Expr::Array(
        (0..a.rows)
            .map(|r| (0..a.cols).map(|c| expr_of(a.data[r * a.cols + c].clone())).collect())
            .collect(),
    )
}

/// The value of an expression that is a value.
fn literal_value(e: &Expr) -> Value {
    match e {
        Expr::Number(n) => Value::Number(*n),
        Expr::Text(t) => Value::Text(t.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Error(e) => Value::Error(*e),
        Expr::Neg(x) => match literal_value(x) {
            Value::Number(n) => Value::Number(-n),
            other => other,
        },
        Expr::Percent(x) => match literal_value(x) {
            Value::Number(n) => Value::Number(n / 100.0),
            other => other,
        },
        Expr::Plus(x) | Expr::Paren(x) => literal_value(x),
        Expr::Array(rows) => {
            rows.first().and_then(|r| r.first()).map_or(Value::Empty, literal_value)
        }
        _ => Value::Empty,
    }
}
