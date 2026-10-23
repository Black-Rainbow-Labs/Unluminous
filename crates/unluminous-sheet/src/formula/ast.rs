//! A formula as a tree.

use std::sync::Arc;

use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::value::ErrorKind;

/// One corner of a reference, with whether each half is absolute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub row: u32,
    pub col: u32,
    pub row_abs: bool,
    pub col_abs: bool,
}

impl Point {
    pub fn at(&self) -> CellRef {
        CellRef::new(self.row, self.col)
    }
}

/// What shape a reference was written in, which is how it is written back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    /// `A1`
    Cell,
    /// `A1:B2`
    Range,
    /// `A:C`
    Columns,
    /// `3:5`
    Rows,
}

/// A reference to cells, possibly on another sheet or across sheets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Reference {
    /// The sheet named, or the formula's own sheet when `None`.
    pub sheet: Option<Arc<str>>,
    /// The last sheet of a 3D reference: `Sheet1:Sheet3!A1`.
    pub sheet_last: Option<Arc<str>>,
    pub first: Point,
    pub last: Point,
    pub shape: Shape,
}

impl Reference {
    pub fn cell(row: u32, col: u32) -> Self {
        let p = Point { row, col, row_abs: false, col_abs: false };
        Self { sheet: None, sheet_last: None, first: p, last: p, shape: Shape::Cell }
    }

    pub fn range(&self) -> Range {
        Range::new(self.first.at(), self.last.at())
    }

    pub fn from_range(range: Range, sheet: Option<Arc<str>>) -> Self {
        let first = Point { row: range.first.row, col: range.first.col, row_abs: false, col_abs: false };
        let last = Point { row: range.last.row, col: range.last.col, row_abs: false, col_abs: false };
        let shape = if range.first == range.last {
            Shape::Cell
        } else if range.is_whole_columns() {
            Shape::Columns
        } else if range.is_whole_rows() {
            Shape::Rows
        } else {
            Shape::Range
        };
        Self { sheet, sheet_last: None, first, last, shape }
    }

    pub fn whole_columns(first: u32, last: u32) -> (Point, Point) {
        (
            Point { row: 0, col: first, row_abs: true, col_abs: false },
            Point { row: MAX_ROWS - 1, col: last, row_abs: true, col_abs: false },
        )
    }

    pub fn whole_rows(first: u32, last: u32) -> (Point, Point) {
        (
            Point { row: first, col: 0, row_abs: false, col_abs: true },
            Point { row: last, col: MAX_COLS - 1, row_abs: false, col_abs: true },
        )
    }
}

/// A structured reference into a table: `Table1[Amount]`, `Table1[[#Headers],[Name]]`, `[@Price]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableRef {
    /// `None` for `[@Price]` written inside the table.
    pub table: Option<Arc<str>>,
    /// `#All`, `#Data`, `#Headers`, `#Totals`, `#This Row`, in lower case without the `#`.
    pub items: Vec<Arc<str>>,
    pub first_column: Option<Arc<str>>,
    pub last_column: Option<Arc<str>>,
    /// As written after the table name, which is how it is printed back.
    pub text: Arc<str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// `A1:B2` built from two expressions, such as `A1:INDEX(B:B,3)`.
    Range,
}

impl BinOp {
    pub fn text(&self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Pow => "^",
            BinOp::Concat => "&",
            BinOp::Eq => "=",
            BinOp::Ne => "<>",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Range => ":",
        }
    }

    /// Lower binds looser.
    pub fn precedence(&self) -> u8 {
        match self {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 1,
            BinOp::Concat => 2,
            BinOp::Add | BinOp::Sub => 3,
            BinOp::Mul | BinOp::Div => 4,
            BinOp::Pow => 5,
            BinOp::Range => 9,
        }
    }
}

/// A formula's tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Text(Arc<str>),
    Bool(bool),
    Error(ErrorKind),
    /// `{1,2;3,4}`: rows of constants.
    Array(Vec<Vec<Expr>>),
    Ref(Reference),
    /// A defined name, a table name on its own, or a LET or LAMBDA parameter.
    Name(Arc<str>),
    /// A reference to another workbook, `[1]Sheet1!A1`, kept as written. It evaluates to `#REF!`, and a
    /// cell holding one keeps the value the file saved.
    External(Arc<str>),
    Neg(Box<Expr>),
    Plus(Box<Expr>),
    Percent(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// A function call, the name in upper case without any `_xlfn.` prefix.
    Call(Arc<str>, Vec<Expr>),
    /// An argument left out: `IF(A1,,2)`.
    Missing,
    /// A LAMBDA called where it is written: `LAMBDA(x,x+1)(2)`.
    Invoke(Box<Expr>, Vec<Expr>),
    /// `A1#`: everything the formula in A1 spilled.
    Spill(Box<Expr>),
    /// `@A1:A10`: one value from a range, by the row or column the formula is in.
    Implicit(Box<Expr>),
    Table(TableRef),
    /// `(A1,B1)`: several references as one.
    Union(Vec<Expr>),
    /// `A1:C3 B2:D4`: the cells both name.
    Intersect(Box<Expr>, Box<Expr>),
    /// `(expr)`, kept so the text prints back with the parentheses the person typed.
    Paren(Box<Expr>),
}

impl Expr {
    /// Visit every node, children before the parent's next sibling.
    pub fn walk(&self, visit: &mut impl FnMut(&Expr)) {
        visit(self);
        match self {
            Expr::Array(rows) => rows.iter().flatten().for_each(|e| e.walk(visit)),
            Expr::Neg(e) | Expr::Plus(e) | Expr::Percent(e) | Expr::Spill(e) | Expr::Implicit(e) | Expr::Paren(e) => {
                e.walk(visit)
            }
            Expr::Binary(_, a, b) | Expr::Intersect(a, b) => {
                a.walk(visit);
                b.walk(visit);
            }
            Expr::Call(_, args) | Expr::Union(args) => args.iter().for_each(|e| e.walk(visit)),
            Expr::Invoke(f, args) => {
                f.walk(visit);
                args.iter().for_each(|e| e.walk(visit));
            }
            _ => {}
        }
    }

    /// The same tree with every reference passed through `change`. `None` from `change` makes the
    /// reference `#REF!`.
    pub fn map_refs(&self, change: &mut impl FnMut(&Reference) -> Option<Reference>) -> Expr {
        let mut map = |e: &Expr| e.map_refs(change);
        match self {
            Expr::Ref(r) => match change(r) {
                Some(r) => Expr::Ref(r),
                None => Expr::Error(ErrorKind::Ref),
            },
            Expr::Array(rows) => Expr::Array(rows.clone()),
            Expr::Neg(e) => Expr::Neg(Box::new(map(e))),
            Expr::Plus(e) => Expr::Plus(Box::new(map(e))),
            Expr::Percent(e) => Expr::Percent(Box::new(map(e))),
            Expr::Spill(e) => Expr::Spill(Box::new(map(e))),
            Expr::Implicit(e) => Expr::Implicit(Box::new(map(e))),
            Expr::Paren(e) => Expr::Paren(Box::new(map(e))),
            Expr::Binary(op, a, b) => Expr::Binary(*op, Box::new(map(a)), Box::new(map(b))),
            Expr::Intersect(a, b) => Expr::Intersect(Box::new(map(a)), Box::new(map(b))),
            Expr::Call(name, args) => Expr::Call(name.clone(), args.iter().map(map).collect()),
            Expr::Union(args) => Expr::Union(args.iter().map(map).collect()),
            Expr::Invoke(f, args) => Expr::Invoke(Box::new(map(f)), args.iter().map(map).collect()),
            other => other.clone(),
        }
    }

    /// The same tree with every sheet name `from` written as `to`, which is what renaming a sheet does.
    pub fn rename_sheet(&self, from: &str, to: &str) -> Expr {
        let rename = |name: &Option<Arc<str>>| match name {
            Some(n) if n.eq_ignore_ascii_case(from) => Some(Arc::from(to)),
            other => other.clone(),
        };
        self.map_refs(&mut |r| {
            let mut r = r.clone();
            r.sheet = rename(&r.sheet);
            r.sheet_last = rename(&r.sheet_last);
            Some(r)
        })
    }
}

/// A formula in a cell: its tree and the text it was read from.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    /// Without the leading `=`, as the person or the file wrote it.
    pub text: Arc<str>,
    pub expr: Arc<Expr>,
    /// A dynamic array formula, which spills. Formulas from a file without the dynamic array flag are
    /// legacy formulas, which take one value from a range by implicit intersection.
    pub dynamic: bool,
    /// The range of a legacy array formula entered with Ctrl+Shift+Enter, set on its top left cell.
    pub array: Option<Range>,
    /// Part of a legacy array formula whose top left cell is elsewhere.
    pub array_member: Option<CellRef>,
}

impl Formula {
    /// Parse formula text, with or without the leading `=`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let body = text.strip_prefix('=').unwrap_or(text);
        let expr = super::parser::parse(body)?;
        Ok(Self {
            text: Arc::from(super::print::print(&expr, false).as_str()),
            expr: Arc::new(expr),
            dynamic: true,
            array: None,
            array_member: None,
        })
    }

    /// Parse text exactly as it stands, keeping it as the formula's text: what reading a file does.
    pub fn parse_kept(text: &str) -> Result<Self, String> {
        let expr = super::parser::parse(text)?;
        Ok(Self { text: Arc::from(text), expr: Arc::new(expr), dynamic: true, array: None, array_member: None })
    }

    /// A formula from a tree, printing the text.
    pub fn from_expr(expr: Expr, dynamic: bool) -> Self {
        Self {
            text: Arc::from(super::print::print(&expr, false).as_str()),
            expr: Arc::new(expr),
            dynamic,
            array: None,
            array_member: None,
        }
    }

    /// The text with the leading `=`, as the formula bar shows it.
    pub fn display(&self) -> String {
        let shown = super::print::print(&self.expr, false);
        match self.array {
            Some(_) => format!("{{={shown}}}"),
            None => format!("={shown}"),
        }
    }
}
