//! Paste Special's operations: add, subtract, multiply and divide the numbers pasted over.
//!
//! Excel's rules, all measured: a text, a logical value or an error pasted over a number changes nothing;
//! a blank cell counts as zero; a formula underneath becomes `=(old)+value`, and a formula pasted becomes
//! `old+(formula)`; a text, logical value or error underneath is left alone.

use std::sync::Arc;

use super::PasteOperation;
use crate::formula::{BinOp, Expr, Formula};
use crate::model::Cell;
use crate::value::{ErrorKind, Value};

/// What a cell ends up holding.
pub(super) struct Content {
    pub value: Value,
    pub formula: Option<Formula>,
    pub rich: Option<Arc<str>>,
}

impl Content {
    /// The content a cell has already.
    pub(super) fn keep(cell: &Cell) -> Self {
        Self { value: cell.value.clone(), formula: cell.formula.clone(), rich: cell.rich.clone() }
    }
}

/// The right hand side of an operation, taken from the cell copied.
pub(super) enum Operand {
    /// A blank cell, which counts as zero but leaves a blank cell blank.
    Blank,
    Number(f64),
    Formula(Expr),
}

/// The operand a copied cell gives, or `None` when it takes no part: text, a logical value or an error.
pub(super) fn operand(src: Option<&Cell>, formula: Option<&Formula>) -> Option<Operand> {
    let Some(cell) = src else { return Some(Operand::Blank) };
    if let Some(f) = formula {
        return Some(Operand::Formula((*f.expr).clone()));
    }
    match &cell.value {
        Value::Empty if cell.formula.is_none() => Some(Operand::Blank),
        Value::Number(n) => Some(Operand::Number(*n)),
        // A logical value that a formula made counts as 1 or 0; a typed one does not.
        Value::Bool(b) if cell.formula.is_some() => Some(Operand::Number(f64::from(*b))),
        _ => None,
    }
}

fn binary(op: PasteOperation) -> BinOp {
    match op {
        PasteOperation::Subtract => BinOp::Sub,
        PasteOperation::Multiply => BinOp::Mul,
        PasteOperation::Divide => BinOp::Div,
        _ => BinOp::Add,
    }
}

fn arithmetic(op: PasteOperation, left: f64, right: f64) -> Value {
    let result = match op {
        PasteOperation::Add => left + right,
        PasteOperation::Subtract => left - right,
        PasteOperation::Multiply => left * right,
        PasteOperation::Divide => {
            if right == 0.0 {
                return Value::Error(ErrorKind::Div0);
            }
            left / right
        }
        PasteOperation::None => right,
    };
    // Zero has no sign in a cell.
    Value::Number(if result == 0.0 { 0.0 } else { result })
}

/// The cell content after the operation, or `None` when the cell is left as it is.
pub(super) fn combine(op: PasteOperation, dst: &Cell, operand: &Operand) -> Option<Content> {
    let rhs = |o: &Operand| match o {
        Operand::Blank => Expr::Number(0.0),
        Operand::Number(n) => Expr::Number(*n),
        Operand::Formula(e) => Expr::Paren(Box::new(e.clone())),
    };
    let formula_of = |lhs: Expr, dynamic: bool| {
        Some(Content {
            value: Value::Empty,
            formula: Some(Formula::from_expr(
                Expr::Binary(binary(op), Box::new(lhs), Box::new(rhs(operand))),
                dynamic,
            )),
            rich: None,
        })
    };
    if let Some(f) = &dst.formula {
        return formula_of(Expr::Paren(Box::new((*f.expr).clone())), f.dynamic);
    }
    let left = match &dst.value {
        Value::Number(x) => *x,
        Value::Empty if !matches!(operand, Operand::Blank) => 0.0,
        _ => return None,
    };
    match operand {
        Operand::Formula(_) => formula_of(Expr::Number(left), true),
        Operand::Blank => Some(plain(arithmetic(op, left, 0.0))),
        Operand::Number(n) => Some(plain(arithmetic(op, left, *n))),
    }
}

fn plain(value: Value) -> Content {
    Content { value, formula: None, rich: None }
}
