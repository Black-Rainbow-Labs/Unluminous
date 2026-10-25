//! Adding up the records of a pivot cell.
//!
//! One [`Acc`] holds everything every function needs, so a cell's records are read once and any function
//! can then be asked for, which is what a subtotal row with a different function from the value field does.

use super::cache::CacheValue;
use super::Func;
use crate::value::ErrorKind;

/// What a pivot cell shows before Show Values As changes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CellVal {
    /// No record had anything in the field.
    Empty,
    Num(f64),
    Err(ErrorKind),
}

impl CellVal {
    pub fn number(&self) -> Option<f64> {
        match self {
            CellVal::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, CellVal::Empty)
    }
}

/// The running figures for the records of one cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Acc {
    /// Records whose value is anything but blank.
    pub filled: u64,
    /// Records whose value is a number.
    pub numbers: u64,
    pub sum: f64,
    pub product: f64,
    pub min: f64,
    pub max: f64,
    mean: f64,
    m2: f64,
    pub error: Option<ErrorKind>,
}

impl Default for Acc {
    fn default() -> Self {
        Acc {
            filled: 0,
            numbers: 0,
            sum: 0.0,
            product: 1.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            mean: 0.0,
            m2: 0.0,
            error: None,
        }
    }
}

impl Acc {
    /// Take one record's value in.
    pub fn add(&mut self, value: &CacheValue) {
        match value {
            CacheValue::Blank => {}
            CacheValue::Number(n) | CacheValue::Date(n) => {
                self.filled += 1;
                self.numbers += 1;
                self.sum += n;
                self.product *= n;
                self.min = self.min.min(*n);
                self.max = self.max.max(*n);
                let delta = n - self.mean;
                self.mean += delta / self.numbers as f64;
                self.m2 += delta * (n - self.mean);
            }
            CacheValue::Error(e) => {
                self.filled += 1;
                self.error.get_or_insert(*e);
            }
            CacheValue::Text(_) | CacheValue::Bool(_) => self.filled += 1,
        }
    }

    /// Take a number in directly, which a calculated field's inputs are.
    pub fn add_number(&mut self, n: f64) {
        self.add(&CacheValue::Number(n));
    }

    /// The figure a function makes of them.
    pub fn result(&self, func: Func) -> CellVal {
        if self.filled == 0 {
            return CellVal::Empty;
        }
        if let Some(e) = self.error {
            if !matches!(func, Func::Count | Func::CountNums) {
                return CellVal::Err(e);
            }
        }
        let n = self.numbers as f64;
        match func {
            Func::Count => CellVal::Num(self.filled as f64),
            Func::CountNums => CellVal::Num(n),
            Func::Sum => finite(self.sum),
            Func::Average => {
                if self.numbers == 0 {
                    CellVal::Err(ErrorKind::Div0)
                } else {
                    finite(self.sum / n)
                }
            }
            Func::Max => CellVal::Num(if self.numbers == 0 { 0.0 } else { self.max }),
            Func::Min => CellVal::Num(if self.numbers == 0 { 0.0 } else { self.min }),
            Func::Product => finite(if self.numbers == 0 { 0.0 } else { self.product }),
            Func::StdDev => self.variance(true).map_or(CellVal::Err(ErrorKind::Div0), |v| finite(v.sqrt())),
            Func::StdDevP => self.variance(false).map_or(CellVal::Err(ErrorKind::Div0), |v| finite(v.sqrt())),
            Func::Var => self.variance(true).map_or(CellVal::Err(ErrorKind::Div0), finite),
            Func::VarP => self.variance(false).map_or(CellVal::Err(ErrorKind::Div0), finite),
        }
    }

    fn variance(&self, sample: bool) -> Option<f64> {
        let n = self.numbers as f64;
        if sample {
            (self.numbers >= 2).then(|| self.m2 / (n - 1.0))
        } else {
            (self.numbers >= 1).then(|| self.m2 / n)
        }
    }
}

fn finite(n: f64) -> CellVal {
    if n.is_finite() {
        CellVal::Num(if n == 0.0 { 0.0 } else { n })
    } else {
        CellVal::Err(ErrorKind::Num)
    }
}
