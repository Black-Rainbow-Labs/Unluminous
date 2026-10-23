//! A formula's tree back into text.
//!
//! Two forms. The one a person sees, which is what the formula bar shows, and the one a file holds, which
//! writes newer functions with the prefix Excel reads them by (`_xlfn.XLOOKUP`) and LET and LAMBDA
//! parameters with `_xlpm.`. Every change to a formula's references, a copy, a fill, an inserted row,
//! a renamed sheet, goes through here.

use std::collections::HashSet;

use super::ast::{Expr, Point, Reference, Shape};
use crate::coord::column_name;

/// The formula as text without its `=`. `for_file` adds the prefixes a file needs.
pub fn print(expr: &Expr, for_file: bool) -> String {
    let mut out = String::new();
    let mut printer = Printer { for_file, params: Vec::new() };
    printer.expr(expr, &mut out, 0);
    out
}

struct Printer {
    for_file: bool,
    params: Vec<HashSet<String>>,
}

impl Printer {
    fn is_param(&self, name: &str) -> bool {
        let upper = name.to_ascii_uppercase();
        self.params.iter().any(|set| set.contains(&upper))
    }

    /// `min` is the precedence the context needs: a binary expression looser than it is parenthesised.
    fn expr(&mut self, expr: &Expr, out: &mut String, min: u8) {
        match expr {
            Expr::Number(n) => out.push_str(&number(*n)),
            Expr::Text(t) => {
                out.push('"');
                out.push_str(&t.replace('"', "\"\""));
                out.push('"');
            }
            Expr::Bool(b) => out.push_str(if *b { "TRUE" } else { "FALSE" }),
            Expr::Error(e) => out.push_str(e.text()),
            Expr::Array(rows) => {
                out.push('{');
                for (r, row) in rows.iter().enumerate() {
                    if r > 0 {
                        out.push(';');
                    }
                    for (c, item) in row.iter().enumerate() {
                        if c > 0 {
                            out.push(',');
                        }
                        self.expr(item, out, 0);
                    }
                }
                out.push('}');
            }
            Expr::Ref(r) => out.push_str(&reference(r)),
            Expr::Name(n) => {
                if self.for_file && self.is_param(n) {
                    out.push_str("_xlpm.");
                }
                out.push_str(n);
            }
            Expr::External(text) => out.push_str(text),
            Expr::Neg(e) => {
                out.push('-');
                self.expr(e, out, 6);
            }
            Expr::Plus(e) => {
                out.push('+');
                self.expr(e, out, 6);
            }
            Expr::Percent(e) => {
                self.expr(e, out, 6);
                out.push('%');
            }
            Expr::Binary(op, a, b) => {
                let p = op.precedence();
                let wrap = p < min;
                if wrap {
                    out.push('(');
                }
                self.expr(a, out, p);
                out.push_str(op.text());
                // Left associative: the right side needs one more to keep `1-(2-3)` its parentheses.
                self.expr(b, out, p + 1);
                if wrap {
                    out.push(')');
                }
            }
            Expr::Call(name, args) => {
                if self.for_file {
                    out.push_str(file_prefix(name));
                }
                out.push_str(name);
                out.push('(');
                let params = binding_names(name, args);
                let pushed = !params.is_empty();
                if pushed {
                    self.params.push(params);
                }
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    self.expr(arg, out, 0);
                }
                if pushed {
                    self.params.pop();
                }
                out.push(')');
            }
            Expr::Missing => {}
            Expr::Invoke(f, args) => {
                self.expr(f, out, 10);
                out.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    self.expr(arg, out, 0);
                }
                out.push(')');
            }
            Expr::Spill(e) if self.for_file => {
                out.push_str("_xlfn.ANCHORARRAY(");
                self.expr(e, out, 0);
                out.push(')');
            }
            Expr::Spill(e) => {
                self.expr(e, out, 10);
                out.push('#');
            }
            Expr::Implicit(e) if self.for_file => {
                out.push_str("_xlfn.SINGLE(");
                self.expr(e, out, 0);
                out.push(')');
            }
            Expr::Implicit(e) => {
                out.push('@');
                self.expr(e, out, 10);
            }
            Expr::Table(t) => {
                if let Some(table) = &t.table {
                    out.push_str(table);
                }
                out.push_str(&t.text);
            }
            Expr::Union(items) => {
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    self.expr(item, out, 0);
                }
                out.push(')');
            }
            Expr::Intersect(a, b) => {
                self.expr(a, out, 8);
                out.push(' ');
                self.expr(b, out, 8);
            }
            Expr::Paren(e) => {
                out.push('(');
                self.expr(e, out, 0);
                out.push(')');
            }
        }
    }
}

/// The parameter names a LET or a LAMBDA binds, upper case.
fn binding_names(name: &str, args: &[Expr]) -> HashSet<String> {
    let mut names = HashSet::new();
    let take = |e: &Expr, names: &mut HashSet<String>| {
        if let Expr::Name(n) = e {
            names.insert(n.to_ascii_uppercase());
        }
    };
    match name {
        "LET" => {
            for (i, arg) in args.iter().enumerate() {
                if i % 2 == 0 && i + 1 < args.len() {
                    take(arg, &mut names);
                }
            }
        }
        "LAMBDA" => {
            for arg in args.iter().take(args.len().saturating_sub(1)) {
                take(arg, &mut names);
            }
        }
        _ => {}
    }
    names
}

/// A number as a formula writes it: as short as it reads back exactly, with `E+` for large ones.
pub fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let text = format!("{n:?}");
    if let Some((mantissa, exponent)) = text.split_once('e') {
        let (sign, digits) = match exponent.strip_prefix('-') {
            Some(d) => ('-', d),
            None => ('+', exponent),
        };
        return format!("{mantissa}E{sign}{digits}");
    }
    text
}

/// A sheet name as a reference writes it, quoted when it has to be.
pub fn sheet_name(name: &str) -> String {
    let plain = !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
        && crate::coord::CellRef::parse(name).is_none()
        && !looks_like_r1c1(name);
    if plain {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

fn looks_like_r1c1(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper == "R" || upper == "C" || upper.starts_with('R') && upper[1..].chars().all(|c| c.is_ascii_digit() || c == 'C')
}

fn point(p: &Point) -> String {
    format!(
        "{}{}{}{}",
        if p.col_abs { "$" } else { "" },
        column_name(p.col),
        if p.row_abs { "$" } else { "" },
        p.row + 1
    )
}

/// A reference as text, with its sheet when it names one.
pub fn reference(r: &Reference) -> String {
    let mut out = String::new();
    if let Some(sheet) = &r.sheet {
        match &r.sheet_last {
            Some(last) => {
                let joined = format!("{sheet}:{last}");
                let quoted = sheet_name(sheet) != **sheet || sheet_name(last) != **last;
                if quoted {
                    out.push_str(&format!("'{}'", joined.replace('\'', "''")));
                } else {
                    out.push_str(&joined);
                }
            }
            None => out.push_str(&sheet_name(sheet)),
        }
        out.push('!');
    }
    match r.shape {
        Shape::Cell => out.push_str(&point(&r.first)),
        Shape::Range => {
            out.push_str(&point(&r.first));
            out.push(':');
            out.push_str(&point(&r.last));
        }
        Shape::Columns => {
            out.push_str(&format!(
                "{}{}:{}{}",
                if r.first.col_abs { "$" } else { "" },
                column_name(r.first.col),
                if r.last.col_abs { "$" } else { "" },
                column_name(r.last.col)
            ));
        }
        Shape::Rows => {
            out.push_str(&format!(
                "{}{}:{}{}",
                if r.first.row_abs { "$" } else { "" },
                r.first.row + 1,
                if r.last.row_abs { "$" } else { "" },
                r.last.row + 1
            ));
        }
    }
    out
}

/// The prefix a file writes a function with, which is how older versions of Excel tell a function they
/// do not have from a name.
pub fn file_prefix(name: &str) -> &'static str {
    match name {
        "FILTER" | "SORT" => "_xlfn._xlws.",
        _ if FUTURE_FUNCTIONS.contains(&name) => "_xlfn.",
        _ => "",
    }
}

/// Functions added after Excel 2007, which a file writes with `_xlfn.`.
pub const FUTURE_FUNCTIONS: &[&str] = &[
    "ACOT", "ACOTH", "AGGREGATE", "ARABIC", "ARRAYTOTEXT", "BASE", "BETA.DIST", "BETA.INV", "BINOM.DIST",
    "BINOM.DIST.RANGE", "BINOM.INV", "BITAND", "BITLSHIFT", "BITOR", "BITRSHIFT", "BITXOR", "BYCOL", "BYROW",
    "CEILING.MATH", "CEILING.PRECISE", "CHISQ.DIST", "CHISQ.DIST.RT", "CHISQ.INV", "CHISQ.INV.RT", "CHISQ.TEST",
    "CHOOSECOLS", "CHOOSEROWS", "COMBINA", "CONCAT", "CONFIDENCE.NORM", "CONFIDENCE.T", "COT", "COTH",
    "COVARIANCE.P", "COVARIANCE.S", "CSC", "CSCH", "DAYS", "DECIMAL", "DROP", "ERF.PRECISE", "ERFC.PRECISE",
    "EXPAND", "EXPON.DIST", "F.DIST", "F.DIST.RT", "F.INV", "F.INV.RT", "F.TEST", "FLOOR.MATH", "FLOOR.PRECISE",
    "FORECAST.LINEAR", "FORMULATEXT", "GAMMA", "GAMMA.DIST", "GAMMA.INV", "GAMMALN.PRECISE", "GAUSS", "HSTACK",
    "IFNA", "IFS", "IMCOSH", "IMCOT", "IMCSC", "IMCSCH", "IMSEC", "IMSECH", "IMSINH", "IMTAN", "ISFORMULA",
    "ISOMITTED", "ISOWEEKNUM", "LAMBDA", "LET", "LOGNORM.DIST", "LOGNORM.INV", "MAKEARRAY", "MAP", "MAXIFS",
    "MINIFS", "MODE.MULT", "MODE.SNGL", "NEGBINOM.DIST", "NETWORKDAYS.INTL", "NORM.DIST", "NORM.INV",
    "NORM.S.DIST", "NORM.S.INV", "NUMBERVALUE", "PDURATION", "PERCENTILE.EXC", "PERCENTILE.INC",
    "PERCENTRANK.EXC", "PERCENTRANK.INC", "PERMUTATIONA", "PHI", "POISSON.DIST", "QUARTILE.EXC",
    "QUARTILE.INC", "RANDARRAY", "RANK.AVG", "RANK.EQ", "REDUCE", "RRI", "SCAN", "SEC", "SECH", "SEQUENCE",
    "SHEET", "SHEETS", "SINGLE", "SKEW.P", "SORTBY", "STDEV.P", "STDEV.S", "SWITCH", "T.DIST", "T.DIST.2T",
    "T.DIST.RT", "T.INV", "T.INV.2T", "T.TEST", "TAKE", "TEXTAFTER", "TEXTBEFORE", "TEXTJOIN", "TEXTSPLIT",
    "TOCOL", "TOROW", "UNICHAR", "UNICODE", "UNIQUE", "VALUETOTEXT", "VAR.P", "VAR.S", "VSTACK", "WEIBULL.DIST",
    "WORKDAY.INTL", "WRAPCOLS", "WRAPROWS", "XLOOKUP", "XMATCH", "XOR", "Z.TEST",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::parser::parse;

    #[test]
    fn a_file_writes_new_functions_and_let_parameters_with_their_prefixes() {
        let expr = parse("LET(x,XLOOKUP(1,A:A,B:B),FILTER(x,x>0)+SUM(1))").unwrap();
        assert_eq!(
            print(&expr, true),
            "_xlfn.LET(_xlpm.x,_xlfn.XLOOKUP(1,A:A,B:B),_xlfn._xlws.FILTER(_xlpm.x,_xlpm.x>0)+SUM(1))"
        );
        assert_eq!(print(&parse(&print(&expr, true)).unwrap(), false), print(&expr, false));
    }

    #[test]
    fn parentheses_are_kept_where_the_order_needs_them() {
        for text in ["1-(2-3)", "(1+2)*3", "2^(1+1)", "-(A1+B1)", "(A1)"] {
            assert_eq!(print(&parse(text).unwrap(), false), text);
        }
    }

    #[test]
    fn sheet_names_are_quoted_when_a_plain_name_would_misread() {
        assert_eq!(sheet_name("Sheet1"), "Sheet1");
        assert_eq!(sheet_name("My Sheet"), "'My Sheet'");
        assert_eq!(sheet_name("A1"), "'A1'");
        assert_eq!(sheet_name("2024"), "'2024'");
        assert_eq!(sheet_name("R1C1"), "'R1C1'");
    }
}
