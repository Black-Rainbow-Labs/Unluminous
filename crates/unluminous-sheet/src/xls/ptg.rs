//! The formula token decoder of the binary workbook formats.
//!
//! A formula in an `.xls` or `.xlsb` file is a list of tokens in reverse Polish order (`rgce`), with the
//! data some tokens need (array constants, cached ranges) after it (`rgcb`). This module turns the list
//! into the formula text Excel shows, in A1 style, which the formula parser reads like any other.
//!
//! Both formats use the same tokens. They differ in how wide a reference is: a row is 16 bits in `.xls`
//! and 32 in `.xlsb`, a column is 8 or 14 bits of a 16 bit field, and a string is cut by a one byte or a
//! two byte length. [`Dialect`] carries those differences.

use crate::coord::column_name;
use crate::formula::print::{number, sheet_name};

/// Which file format the tokens came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// `.xls`, BIFF8.
    Biff8,
    /// `.xlsb`, BIFF12.
    Biff12,
}

impl Dialect {
    fn last_row(self) -> u32 {
        match self {
            Dialect::Biff8 => 0xFFFF,
            Dialect::Biff12 => 0xF_FFFF,
        }
    }

    fn last_col(self) -> u32 {
        match self {
            Dialect::Biff8 => 0xFF,
            Dialect::Biff12 => 0x3FFF,
        }
    }
}

/// One entry of the extern sheet table: which book, and the first and last sheet of the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xti {
    pub book: u32,
    /// A sheet index, `-2` for names that belong to the workbook, `-1` for a deleted sheet.
    pub first: i32,
    pub last: i32,
}

/// What a supporting book is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupBook {
    /// This workbook.
    This,
    /// Another workbook, by its address and its sheet names.
    External { url: String, sheets: Vec<String> },
    /// The add-in functions.
    AddIn,
}

/// A table a structured reference can name: where it is and what its columns are called.
#[derive(Debug, Clone, PartialEq)]
pub struct TableInfo {
    /// The number a `ptgList` token names the table by.
    pub id: u32,
    pub name: String,
    pub columns: Vec<String>,
    pub sheet: usize,
    pub range: crate::coord::Range,
}

/// Everything outside the token list that a token can refer to.
#[derive(Debug, Default, Clone)]
pub struct Env<'a> {
    /// The sheet the formula is on, which decides how a structured reference to a table on it is written.
    pub sheet: Option<usize>,
    pub tables: &'a [TableInfo],
    /// This workbook's sheet names, in order.
    pub sheets: &'a [String],
    pub xti: &'a [Xti],
    pub supbooks: &'a [SupBook],
    /// The defined names, so `ptgName` index 1 is `names[0]`.
    pub names: &'a [String],
    /// For each supporting book, the names it lists, so `ptgNameX` can be written out.
    pub extern_names: &'a [Vec<String>],
}

/// The cell a formula is in, which relative tokens (`ptgRefN`, `ptgAreaN`) are measured from.
#[derive(Debug, Clone, Copy, Default)]
pub struct Origin {
    pub row: u32,
    pub col: u32,
}

/// What a token list turned out to be.
#[derive(Debug, Clone, PartialEq)]
pub enum Decoded {
    /// A formula, as text without the `=`.
    Text(String),
    /// `ptgExp`: this cell takes its formula from the shared or array formula that starts here.
    Exp { row: u32, col: u32 },
    /// `ptgTbl`: this cell is part of a data table.
    Table,
}

/// The error codes a token or a cell can hold, as the text Excel shows.
pub fn error_text(code: u8) -> &'static str {
    match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        0x2A => "#N/A",
        0x2B => "#GETTING_DATA",
        0x2C => "#SPILL!",
        0x2D => "#CALC!",
        _ => "#VALUE!",
    }
}

/// A reader over a byte slice that stops, rather than panics, at the end.
struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let slice =
            self.data.get(self.at..self.at + n).ok_or("a formula ends in the middle of a token")?;
        self.at += n;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f64(&mut self) -> Result<f64, String> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(f64::from_le_bytes(a))
    }

    fn done(&self) -> bool {
        self.at >= self.data.len()
    }
}

/// One corner of a reference as read: its row and column, and whether each is relative.
#[derive(Debug, Clone, Copy)]
struct Corner {
    row: u32,
    col: u32,
    row_rel: bool,
    col_rel: bool,
}

/// The decoder: the formula under way is a stack of the text of each operand.
struct Decoder<'a> {
    dialect: Dialect,
    env: &'a Env<'a>,
    origin: Origin,
    stack: Vec<String>,
    extra: Bytes<'a>,
}

/// Decode a token list into text.
pub fn decode(
    dialect: Dialect,
    rgce: &[u8],
    rgcb: &[u8],
    env: &Env,
    origin: Origin,
) -> Result<Decoded, String> {
    if rgce.is_empty() {
        return Ok(Decoded::Text(String::new()));
    }
    if let Some(special) = special_first_token(dialect, rgce) {
        return Ok(special);
    }
    let mut decoder =
        Decoder { dialect, env, origin, stack: Vec::new(), extra: Bytes { data: rgcb, at: 0 } };
    let mut tokens = Bytes { data: rgce, at: 0 };
    while !tokens.done() {
        decoder.token(&mut tokens)?;
    }
    match decoder.stack.len() {
        1 => Ok(Decoded::Text(decoder.stack.remove(0))),
        0 => Ok(Decoded::Text(String::new())),
        n => Err(format!("a formula leaves {n} values on the stack")),
    }
}

/// `ptgExp` and `ptgTbl` stand for the whole formula when they are its only token.
fn special_first_token(dialect: Dialect, rgce: &[u8]) -> Option<Decoded> {
    let mut bytes = Bytes { data: rgce, at: 1 };
    match rgce[0] {
        0x01 => match dialect {
            Dialect::Biff8 => {
                let row = bytes.u16().ok()? as u32;
                let col = bytes.u16().ok()? as u32;
                Some(Decoded::Exp { row, col })
            }
            Dialect::Biff12 => {
                let row = bytes.u32().ok()?;
                Some(Decoded::Exp { row, col: u32::MAX })
            }
        },
        0x02 => Some(Decoded::Table),
        _ => None,
    }
}

impl<'a> Decoder<'a> {
    fn push(&mut self, text: String) {
        self.stack.push(text);
    }

    fn pop(&mut self) -> Result<String, String> {
        self.stack.pop().ok_or_else(|| "a formula uses more values than it has".to_owned())
    }

    /// Read one token and apply it to the stack.
    fn token(&mut self, t: &mut Bytes<'a>) -> Result<(), String> {
        let ptg = t.u8()?;
        match ptg {
            0x01 | 0x02 => {
                // An expression or table token that is not alone: nothing to write.
                t.take(4)?;
                self.push(String::new());
            }
            0x03..=0x11 => self.binary(ptg)?,
            0x12 => {
                let a = self.pop()?;
                self.push(format!("+{a}"));
            }
            0x13 => {
                let a = self.pop()?;
                self.push(format!("-{a}"));
            }
            0x14 => {
                let a = self.pop()?;
                self.push(format!("{a}%"));
            }
            0x15 => {
                let a = self.pop()?;
                self.push(format!("({a})"));
            }
            0x16 => self.push(String::new()),
            0x17 => {
                let text = self.string(t)?;
                self.push(format!("\"{}\"", text.replace('"', "\"\"")));
            }
            0x18 => self.extend(t)?,
            0x19 => self.attribute(t)?,
            0x1C => {
                let code = t.u8()?;
                self.push(error_text(code).to_owned());
            }
            0x1D => {
                let b = t.u8()?;
                self.push(if b == 0 { "FALSE" } else { "TRUE" }.to_owned());
            }
            0x1E => {
                let n = t.u16()?;
                self.push(n.to_string());
            }
            0x1F => {
                let n = t.f64()?;
                self.push(number(n));
            }
            _ => self.classed(ptg & 0x1F | 0x20, ptg, t)?,
        }
        Ok(())
    }

    /// The tokens whose three classes (reference, value, array) read the same: the class is the top bits
    /// of the token, so `base` is the token with the class set to reference.
    fn classed(&mut self, base: u8, ptg: u8, t: &mut Bytes<'a>) -> Result<(), String> {
        if ptg & 0x60 == 0 {
            return Err(format!(
                "the formula holds a token this version does not know ({ptg:#04x})"
            ));
        }
        match base {
            0x20 => self.array_token(t),
            0x21 => {
                let index = t.u16()? as usize;
                let (name, fixed) = function(index)?;
                self.call(name, fixed, None)
            }
            0x22 => {
                let count = (t.u8()? & 0x7F) as usize;
                let index = (t.u16()? & 0x7FFF) as usize;
                let (name, _) = function(index)?;
                self.call(name, count, Some(index))
            }
            0x23 => {
                let index = t.u32()? as usize;
                let name = self
                    .env
                    .names
                    .get(index.wrapping_sub(1))
                    .cloned()
                    .unwrap_or_else(|| "#NAME?".to_owned());
                self.push(name);
                Ok(())
            }
            0x24 => {
                let corner = self.read_corner(t, false)?;
                self.push(self.corner_text(corner));
                Ok(())
            }
            0x25 => {
                let text = self.area(t, false)?;
                self.push(text);
                Ok(())
            }
            0x26 => {
                // PtgMemArea: its subexpression follows in the token list and is read as tokens; the
                // cached ranges are in the extra data.
                t.take(6)?;
                self.skip_cached_ranges();
                Ok(())
            }
            0x27 | 0x28 => {
                t.take(6)?;
                Ok(())
            }
            0x29 => {
                t.take(2)?;
                Ok(())
            }
            0x2A => {
                t.take(if self.dialect == Dialect::Biff8 { 4 } else { 6 })?;
                self.push("#REF!".to_owned());
                Ok(())
            }
            0x2B => {
                t.take(if self.dialect == Dialect::Biff8 { 8 } else { 12 })?;
                self.push("#REF!".to_owned());
                Ok(())
            }
            0x2C => {
                let corner = self.read_corner(t, true)?;
                self.push(self.corner_text(corner));
                Ok(())
            }
            0x2D => {
                let text = self.area(t, true)?;
                self.push(text);
                Ok(())
            }
            0x39 => self.name_x(t),
            0x3A => self.ref_3d(t),
            0x3B => self.area_3d(t),
            0x3C => {
                let prefix = self.sheet_prefix(t.u16()?);
                t.take(if self.dialect == Dialect::Biff8 { 4 } else { 6 })?;
                self.push(format!("{prefix}#REF!"));
                Ok(())
            }
            0x3D => {
                let prefix = self.sheet_prefix(t.u16()?);
                t.take(if self.dialect == Dialect::Biff8 { 8 } else { 12 })?;
                self.push(format!("{prefix}#REF!"));
                Ok(())
            }
            _ => Err(format!("the formula holds a token this version does not know ({ptg:#04x})")),
        }
    }

    fn binary(&mut self, ptg: u8) -> Result<(), String> {
        let b = self.pop()?;
        let a = self.pop()?;
        let op = match ptg {
            0x03 => "+",
            0x04 => "-",
            0x05 => "*",
            0x06 => "/",
            0x07 => "^",
            0x08 => "&",
            0x09 => "<",
            0x0A => "<=",
            0x0B => "=",
            0x0C => ">=",
            0x0D => ">",
            0x0E => "<>",
            0x0F => " ",
            0x10 => ",",
            _ => ":",
        };
        self.push(format!("{a}{op}{b}"));
        Ok(())
    }

    /// The text of a `ptgStr`.
    fn string(&mut self, t: &mut Bytes<'a>) -> Result<String, String> {
        match self.dialect {
            Dialect::Biff8 => {
                let cch = t.u8()? as usize;
                let wide = t.u8()? & 1 != 0;
                if wide {
                    let units: Vec<u16> = t
                        .take(cch * 2)?
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    Ok(String::from_utf16_lossy(&units))
                } else {
                    Ok(t.take(cch)?.iter().map(|&b| b as char).collect())
                }
            }
            Dialect::Biff12 => {
                let cch = t.u16()? as usize;
                let units: Vec<u16> = t
                    .take(cch * 2)?
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                Ok(String::from_utf16_lossy(&units))
            }
        }
    }

    /// `ptgExtend`: structured references and pivot names, which have no A1 text, are skipped.
    fn extend(&mut self, t: &mut Bytes<'a>) -> Result<(), String> {
        let kind = t.u8()?;
        match kind {
            0x19 => {
                let _ixti = t.u16()?;
                let flags = t.u16()?;
                let list = t.u32()?;
                let first = t.u16()?;
                let last = t.u16()?;
                let text = self.structured_reference(list, flags, first, last);
                self.push(text);
            }
            0x1D => {
                t.take(4)?;
                self.push("#REF!".to_owned());
            }
            other => {
                return Err(format!(
                    "the formula holds an extended token this version does not know ({other:#04x})"
                ))
            }
        }
        Ok(())
    }

    /// A structured reference: a table, the rows and the columns it names. Inside the table the name is
    /// left out where Excel leaves it out.
    fn structured_reference(&self, list: u32, flags: u16, first: u16, last: u16) -> String {
        let tables = self.env.tables;
        let by_number = tables.get((list as usize).wrapping_sub(1));
        let Some(table) = tables.iter().find(|t| t.id == list).or(by_number) else {
            return "#REF!".to_owned();
        };
        let column = |i: u16| table.columns.get(i as usize).map(|c| escape_column(c));
        let (Some(from), Some(to)) = (column(first), column(last)) else {
            return "#REF!".to_owned();
        };
        let at = crate::coord::CellRef::new(self.origin.row, self.origin.col);
        let inside = self.env.sheet == Some(table.sheet) && table.range.contains(at);
        let rows = (flags >> 2) & 0x1F;
        let whole = flags & 3 == 0;
        let columns = if first == last { format!("[{from}]") } else { format!("[{from}]:[{to}]") };
        let name = if inside { "" } else { table.name.as_str() };
        if rows == 0x10 {
            return match (whole, first == last) {
                (true, _) => format!("{name}[@]"),
                (false, true) => format!("{name}[@{from}]"),
                (false, false) => format!("{name}[@{columns}]"),
            };
        }
        if rows == 0 {
            return match (whole, first == last) {
                (true, _) => format!("{}[]", table.name),
                (false, true) => format!("{name}[{from}]"),
                (false, false) => format!("{name}[{columns}]"),
            };
        }
        let items: Vec<&str> = [
            (0x01, "[#All]"),
            (0x02, "[#Data]"),
            (0x04, "[#Headers]"),
            (0x08, "[#Totals]"),
            (0x10, "[#This Row]"),
        ]
        .into_iter()
        .filter(|(bit, _)| rows & bit != 0)
        .map(|(_, text)| text)
        .collect();
        match (whole, items.len()) {
            (true, 1) => format!("{}{}", table.name, items[0]),
            (true, _) => format!("{}[{}]", table.name, items.join(",")),
            (false, _) => format!("{}[{},{}]", table.name, items.join(","), columns),
        }
    }

    /// `ptgAttr`: hints for the calculation engine, of which only the sum shorthand changes the text.
    fn attribute(&mut self, t: &mut Bytes<'a>) -> Result<(), String> {
        let kind = t.u8()?;
        if kind & 0x04 != 0 && kind != 0x40 && kind != 0x41 {
            // Choose: a count of choices, then one offset for each, and one more.
            let count = t.u16()? as usize;
            t.take(2 * (count + 1))?;
            return Ok(());
        }
        t.take(2)?;
        if kind & 0x10 != 0 {
            let a = self.pop()?;
            self.push(format!("SUM({a})"));
        }
        Ok(())
    }

    /// Apply a function to the arguments on the stack. `index` is set for a variable argument call, where
    /// function number 255 means the first argument is the name of a user function.
    fn call(
        &mut self,
        name: &'static str,
        count: usize,
        index: Option<usize>,
    ) -> Result<(), String> {
        if self.stack.len() < count {
            return Err(
                "a function is called with more arguments than the formula holds".to_owned()
            );
        }
        let at = self.stack.len() - count;
        let mut args = self.stack.split_off(at);
        if index == Some(255) {
            if args.is_empty() {
                return Err("a user function is called without its name".to_owned());
            }
            let user = args.remove(0);
            self.push(format!("{}({})", future_name(&user), args.join(",")));
            return Ok(());
        }
        if name.is_empty() {
            return Err("the formula calls a function this version does not know".to_owned());
        }
        self.push(format!("{name}({})", args.join(",")));
        Ok(())
    }

    /// The cached ranges of a `ptgMemArea` are skipped in the extra data.
    fn skip_cached_ranges(&mut self) {
        if let Ok(count) = self.extra.u16() {
            let _ = self.extra.take(8 * count as usize);
        }
    }

    /// A `ptgArray`: the token holds nothing, and the constants are next in the extra data.
    fn array_token(&mut self, t: &mut Bytes<'a>) -> Result<(), String> {
        t.take(if self.dialect == Dialect::Biff8 { 7 } else { 14 })?;
        let (rows, cols) = match self.dialect {
            Dialect::Biff8 => {
                let cols = self.extra.u8()? as usize + 1;
                let rows = self.extra.u16()? as usize + 1;
                (rows, cols)
            }
            Dialect::Biff12 => {
                let rows = self.extra.u32()? as usize;
                let cols = self.extra.u32()? as usize;
                (rows, cols)
            }
        };
        if rows.checked_mul(cols).is_none_or(|n| n > 1 << 20) {
            return Err("an array constant is too large".to_owned());
        }
        let mut text = String::from("{");
        for r in 0..rows {
            if r > 0 {
                text.push(';');
            }
            for c in 0..cols {
                if c > 0 {
                    text.push(',');
                }
                text.push_str(&self.array_value()?);
            }
        }
        text.push('}');
        self.push(text);
        Ok(())
    }

    /// One constant of an array, from the extra data.
    fn array_value(&mut self) -> Result<String, String> {
        let kind = self.extra.u8()?;
        match (self.dialect, kind) {
            (Dialect::Biff8, 0x00) => {
                self.extra.take(8)?;
                Ok(String::new())
            }
            (Dialect::Biff8, 0x01) | (Dialect::Biff12, 0x00) => Ok(number(self.extra.f64()?)),
            (Dialect::Biff8, 0x02) => {
                let cch = self.extra.u16()? as usize;
                let wide = self.extra.u8()? & 1 != 0;
                let text: String = if wide {
                    let units: Vec<u16> = self
                        .extra
                        .take(cch * 2)?
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    String::from_utf16_lossy(&units)
                } else {
                    self.extra.take(cch)?.iter().map(|&b| b as char).collect()
                };
                Ok(format!("\"{}\"", text.replace('"', "\"\"")))
            }
            (Dialect::Biff12, 0x01) => {
                let cch = self.extra.u16()? as usize;
                let units: Vec<u16> = self
                    .extra
                    .take(cch * 2)?
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                Ok(format!("\"{}\"", String::from_utf16_lossy(&units).replace('"', "\"\"")))
            }
            (Dialect::Biff8, 0x04) => {
                let b = self.extra.u8()?;
                self.extra.take(7)?;
                Ok(if b == 0 { "FALSE" } else { "TRUE" }.to_owned())
            }
            (Dialect::Biff12, 0x02) => {
                let b = self.extra.u8()?;
                Ok(if b == 0 { "FALSE" } else { "TRUE" }.to_owned())
            }
            (Dialect::Biff8, 0x10) => {
                let code = self.extra.u8()?;
                self.extra.take(7)?;
                Ok(error_text(code).to_owned())
            }
            (Dialect::Biff12, 0x04) => Ok(error_text(self.extra.u8()?).to_owned()),
            (_, other) => Err(format!("an array constant holds a value of kind {other:#04x}")),
        }
    }

    /// A cell reference as it is stored: a row and a column with flags for which are relative.
    fn read_corner(
        &mut self,
        t: &mut Bytes<'a>,
        relative_to_origin: bool,
    ) -> Result<Corner, String> {
        let (row, col_field) = match self.dialect {
            Dialect::Biff8 => (t.u16()? as u32, t.u16()? as u32),
            Dialect::Biff12 => (t.u32()?, t.u16()? as u32),
        };
        Ok(self.corner(row, col_field, relative_to_origin))
    }

    /// Build a corner from a stored row and a column field with the two flag bits on top. A relative
    /// token of the `N` kind stores offsets from the cell the formula is in, which are added back here.
    fn corner(&self, row: u32, col_field: u32, relative_to_origin: bool) -> Corner {
        let row_rel = col_field & 0x8000 != 0;
        let col_rel = col_field & 0x4000 != 0;
        let (row_modulus, col_bits): (i64, u32) = match self.dialect {
            Dialect::Biff8 => (1 << 16, 8),
            Dialect::Biff12 => (1 << 20, 14),
        };
        let col_modulus = 1i64 << col_bits;
        let stored_col = col_field & (col_modulus as u32 - 1);
        let (mut row, mut col) = (row, stored_col);
        if relative_to_origin {
            if row_rel {
                let offset = match self.dialect {
                    Dialect::Biff8 => row as u16 as i16 as i64,
                    Dialect::Biff12 => row as i32 as i64,
                };
                row = (self.origin.row as i64 + offset).rem_euclid(row_modulus) as u32;
            }
            if col_rel {
                let sign = 1i64 << (col_bits - 1);
                let raw = stored_col as i64;
                let offset = if raw & sign != 0 { raw - col_modulus } else { raw };
                col = (self.origin.col as i64 + offset).rem_euclid(col_modulus) as u32;
            }
        }
        Corner { row, col, row_rel, col_rel }
    }

    fn corner_text(&self, c: Corner) -> String {
        format!(
            "{}{}{}{}",
            if c.col_rel { "" } else { "$" },
            column_name(c.col),
            if c.row_rel { "" } else { "$" },
            c.row + 1
        )
    }

    /// A `ptgArea` or `ptgAreaN`.
    fn area(&mut self, t: &mut Bytes<'a>, relative_to_origin: bool) -> Result<String, String> {
        let (row_first, row_last, col_first, col_last) = match self.dialect {
            Dialect::Biff8 => {
                let rf = t.u16()? as u32;
                let rl = t.u16()? as u32;
                let cf = t.u16()? as u32;
                let cl = t.u16()? as u32;
                (rf, rl, cf, cl)
            }
            Dialect::Biff12 => {
                let rf = t.u32()?;
                let rl = t.u32()?;
                let cf = t.u16()? as u32;
                let cl = t.u16()? as u32;
                (rf, rl, cf, cl)
            }
        };
        let first = self.corner(row_first, col_first, relative_to_origin);
        let last = self.corner(row_last, col_last, relative_to_origin);
        Ok(self.area_text(first, last))
    }

    /// An area as text: `A1:B2`, or `A:B` and `1:2` when it covers whole columns or rows.
    fn area_text(&self, first: Corner, last: Corner) -> String {
        let whole_columns = first.row == 0 && last.row == self.dialect.last_row() && !first.row_rel;
        let whole_rows = first.col == 0 && last.col == self.dialect.last_col() && !first.col_rel;
        if whole_columns && !whole_rows {
            return format!(
                "{}{}:{}{}",
                if first.col_rel { "" } else { "$" },
                column_name(first.col),
                if last.col_rel { "" } else { "$" },
                column_name(last.col)
            );
        }
        if whole_rows && !whole_columns {
            return format!(
                "{}{}:{}{}",
                if first.row_rel { "" } else { "$" },
                first.row + 1,
                if last.row_rel { "" } else { "$" },
                last.row + 1
            );
        }
        format!("{}:{}", self.corner_text(first), self.corner_text(last))
    }

    /// `Sheet1!` or `Sheet1:Sheet3!` for an entry of the extern sheet table.
    fn sheet_prefix(&self, ixti: u16) -> String {
        let Some(xti) = self.env.xti.get(ixti as usize) else { return "#REF!".to_owned() };
        let book = self.env.supbooks.get(xti.book as usize);
        let (first, last) = (xti.first, xti.last);
        if first < 0 && first != -2 {
            return "#REF!".to_owned();
        }
        match book {
            Some(SupBook::External { sheets, .. }) => {
                let ordinal = self
                    .env
                    .supbooks
                    .iter()
                    .take(xti.book as usize + 1)
                    .filter(|b| matches!(b, SupBook::External { .. }))
                    .count();
                let name = |i: i32| sheets.get(i as usize).cloned().unwrap_or_default();
                if first == last {
                    format!("[{ordinal}]{}!", sheet_name(&name(first)))
                } else {
                    format!("[{ordinal}]{}:{}!", name(first), name(last))
                }
            }
            _ => {
                let name = |i: i32| self.env.sheets.get(i as usize).cloned();
                match (name(first), name(last)) {
                    (Some(a), Some(_)) if first == last => format!("{}!", sheet_name(&a)),
                    (Some(a), Some(b)) => {
                        if sheet_name(&a) == a && sheet_name(&b) == b {
                            format!("{a}:{b}!")
                        } else {
                            format!("'{}:{}'!", a.replace('\'', "''"), b.replace('\'', "''"))
                        }
                    }
                    _ => String::new(),
                }
            }
        }
    }

    fn ref_3d(&mut self, t: &mut Bytes<'a>) -> Result<(), String> {
        let prefix = self.sheet_prefix(t.u16()?);
        let corner = self.read_corner(t, false)?;
        self.push(format!("{prefix}{}", self.corner_text(corner)));
        Ok(())
    }

    fn area_3d(&mut self, t: &mut Bytes<'a>) -> Result<(), String> {
        let prefix = self.sheet_prefix(t.u16()?);
        let area = self.area(t, false)?;
        self.push(format!("{prefix}{area}"));
        Ok(())
    }

    /// `ptgNameX`: a name that lives in another book, or an add-in function.
    fn name_x(&mut self, t: &mut Bytes<'a>) -> Result<(), String> {
        let ixti = t.u16()?;
        let index = t.u32()? as usize;
        let xti = self.env.xti.get(ixti as usize);
        let name = xti
            .and_then(|x| self.env.extern_names.get(x.book as usize))
            .and_then(|names| names.get(index.wrapping_sub(1)))
            .cloned()
            .unwrap_or_else(|| "#NAME?".to_owned());
        let external = xti.is_some_and(|x| {
            matches!(self.env.supbooks.get(x.book as usize), Some(SupBook::External { .. }))
        });
        if external {
            let prefix = self.sheet_prefix(ixti);
            let book = prefix.split('!').next().unwrap_or("").to_owned();
            self.push(format!("{book}!{name}"));
        } else {
            self.push(name);
        }
        Ok(())
    }
}

/// The name and fixed argument count of a function number.
fn function(index: usize) -> Result<(&'static str, usize), String> {
    let (name, count) = crate::ftab::FUNCTIONS.get(index).copied().ok_or_else(|| {
        format!("the formula calls function number {index}, which this version does not know")
    })?;
    Ok((name, count as usize))
}

/// The name of a function Excel added after the file format was fixed, which the file stores as a name
/// such as `_xlfn.IFS`, written as the function name alone.
fn future_name(stored: &str) -> String {
    let mut name = stored;
    loop {
        if let Some(rest) = name.strip_prefix("_xlfn.") {
            name = rest;
        } else if let Some(rest) = name.strip_prefix("_xlws.") {
            name = rest;
        } else {
            break;
        }
    }
    name.to_owned()
}

/// A column name as it is written inside the brackets of a structured reference.
fn escape_column(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '[' | ']' | '#' | '\'') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(dialect: Dialect, rgce: &[u8], rgcb: &[u8], env: &Env, origin: Origin) -> String {
        match decode(dialect, rgce, rgcb, env, origin).unwrap() {
            Decoded::Text(t) => t,
            other => panic!("{other:?}"),
        }
    }

    fn plain(rgce: &[u8]) -> String {
        text(Dialect::Biff8, rgce, &[], &Env::default(), Origin::default())
    }

    #[test]
    fn operators_keep_the_order_the_tokens_give_them() {
        // 1+2*3
        assert_eq!(plain(&[0x1E, 1, 0, 0x1E, 2, 0, 0x1E, 3, 0, 0x05, 0x03]), "1+2*3");
        // (1+2)*3, with the parenthesis token
        assert_eq!(plain(&[0x1E, 1, 0, 0x1E, 2, 0, 0x03, 0x15, 0x1E, 3, 0, 0x05]), "(1+2)*3");
        // -4%
        assert_eq!(plain(&[0x1E, 4, 0, 0x14, 0x13]), "-4%");
    }

    #[test]
    fn greater_than_and_greater_or_equal_are_not_swapped() {
        assert_eq!(plain(&[0x1E, 1, 0, 0x1E, 2, 0, 0x0C]), "1>=2");
        assert_eq!(plain(&[0x1E, 1, 0, 0x1E, 2, 0, 0x0D]), "1>2");
        assert_eq!(plain(&[0x1E, 1, 0, 0x1E, 2, 0, 0x0E]), "1<>2");
    }

    #[test]
    fn a_reference_is_absolute_unless_its_flags_say_it_is_relative() {
        // $B$2, then B2, then $B2, with the column flag 0x4000 and the row flag 0x8000.
        assert_eq!(plain(&[0x24, 1, 0, 1, 0]), "$B$2");
        assert_eq!(plain(&[0x24, 1, 0, 1, 0xC0]), "B2");
        assert_eq!(plain(&[0x24, 1, 0, 1, 0x80]), "$B2");
    }

    #[test]
    fn whole_columns_and_whole_rows_are_written_the_short_way() {
        // $A:$C
        assert_eq!(plain(&[0x25, 0, 0, 0xFF, 0xFF, 0, 0, 2, 0]), "$A:$C");
        // $2:$4
        assert_eq!(plain(&[0x25, 1, 0, 3, 0, 0, 0, 0xFF, 0]), "$2:$4");
    }

    #[test]
    fn a_function_takes_the_arguments_the_count_says_and_a_missing_argument_is_empty() {
        // IF(A1,,2): ptgFuncVar with three arguments and function number 1.
        let tokens = [0x24, 0, 0, 0, 0xC0, 0x16, 0x1E, 2, 0, 0x42, 3, 1, 0];
        assert_eq!(plain(&tokens), "IF(A1,,2)");
        // PI(): ptgFunc with no arguments.
        assert_eq!(plain(&[0x21, 19, 0]), "PI()");
    }

    #[test]
    fn a_string_doubles_its_quotes_and_reads_wide_characters() {
        assert_eq!(plain(&[0x17, 3, 0, b'a', b'"', b'b']), "\"a\"\"b\"");
        assert_eq!(plain(&[0x17, 1, 1, 0xE9, 0x00]), "\"\u{e9}\"");
    }

    #[test]
    fn a_sheet_name_is_quoted_when_it_needs_it_and_a_range_of_sheets_has_one_prefix() {
        let sheets = vec!["Data".to_owned(), "Two words".to_owned(), "Last".to_owned()];
        let xti = [
            Xti { book: 0, first: 0, last: 0 },
            Xti { book: 0, first: 1, last: 1 },
            Xti { book: 0, first: 0, last: 2 },
        ];
        let books = [SupBook::This];
        let env = Env { sheets: &sheets, xti: &xti, supbooks: &books, ..Default::default() };
        let one = |ixti: u8| [0x3A, ixti, 0, 0, 0, 0, 0xC0];
        assert_eq!(text(Dialect::Biff8, &one(0), &[], &env, Origin::default()), "Data!A1");
        assert_eq!(text(Dialect::Biff8, &one(1), &[], &env, Origin::default()), "'Two words'!A1");
        assert_eq!(text(Dialect::Biff8, &one(2), &[], &env, Origin::default()), "Data:Last!A1");
    }

    #[test]
    fn a_relative_token_is_measured_from_the_cell_it_is_in() {
        // ptgRefN one row up and one column left: row offset -1, column offset -1, both relative.
        let tokens = [0x2C, 0xFF, 0xFF, 0xFF, 0xC0];
        let origin = Origin { row: 2, col: 2 };
        assert_eq!(text(Dialect::Biff8, &tokens, &[], &Env::default(), origin), "B2");
        let origin = Origin { row: 10, col: 5 };
        assert_eq!(text(Dialect::Biff8, &tokens, &[], &Env::default(), origin), "E10");
    }

    #[test]
    fn an_array_constant_is_read_from_the_data_after_the_tokens() {
        // {1,"a";TRUE,#N/A}: 2 columns and 2 rows, the first value a number.
        let mut extra = vec![1, 1, 0];
        extra.push(1);
        extra.extend(1f64.to_le_bytes());
        extra.extend([2, 1, 0, 0, b'a']);
        extra.extend([4, 1, 0, 0, 0, 0, 0, 0, 0]);
        extra.extend([0x10, 0x2A, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            text(
                Dialect::Biff8,
                &[0x20, 0, 0, 0, 0, 0, 0, 0],
                &extra,
                &Env::default(),
                Origin::default()
            ),
            "{1,\"a\";TRUE,#N/A}"
        );
    }

    #[test]
    fn the_binary_workbook_format_has_wider_rows_and_columns() {
        // A reference to XFD1048576 with nothing relative: the row is 32 bits and the column 14.
        let mut tokens = vec![0x24];
        tokens.extend(1_048_575u32.to_le_bytes());
        tokens.extend(0x3FFFu16.to_le_bytes());
        assert_eq!(
            text(Dialect::Biff12, &tokens, &[], &Env::default(), Origin::default()),
            "$XFD$1048576"
        );
        // Whole columns: the last row of the sheet.
        let mut area = vec![0x25];
        area.extend(0u32.to_le_bytes());
        area.extend(1_048_575u32.to_le_bytes());
        area.extend(0u16.to_le_bytes());
        area.extend(1u16.to_le_bytes());
        assert_eq!(text(Dialect::Biff12, &area, &[], &Env::default(), Origin::default()), "$A:$B");
    }

    #[test]
    fn a_shared_or_array_formula_stands_for_the_formula_stored_elsewhere() {
        assert_eq!(
            decode(Dialect::Biff8, &[0x01, 5, 0, 3, 0], &[], &Env::default(), Origin::default())
                .unwrap(),
            Decoded::Exp { row: 5, col: 3 }
        );
    }

    #[test]
    fn a_token_this_version_does_not_know_is_an_error_and_not_a_guess() {
        assert!(decode(Dialect::Biff8, &[0x1A], &[], &Env::default(), Origin::default()).is_err());
        assert!(
            decode(Dialect::Biff8, &[0x1E, 1], &[], &Env::default(), Origin::default()).is_err()
        );
        // An operator with nothing to work on.
        assert!(decode(Dialect::Biff8, &[0x03], &[], &Env::default(), Origin::default()).is_err());
    }

    #[test]
    fn a_function_added_after_the_format_is_called_by_its_name() {
        let names = vec!["_xlfn.IFS".to_owned()];
        let env = Env { names: &names, ..Default::default() };
        // The name, an argument, and the user function call: ptgName 1, ptgInt 5, ptgFuncVar 2 arguments, 255.
        let tokens = [0x23, 1, 0, 0, 0, 0x1E, 5, 0, 0x42, 2, 0xFF, 0];
        assert_eq!(text(Dialect::Biff8, &tokens, &[], &env, Origin::default()), "IFS(5)");
    }

    #[test]
    fn a_structured_reference_names_the_table_outside_it_and_leaves_the_name_out_inside() {
        let tables = [TableInfo {
            id: 1,
            name: "Fruit".to_owned(),
            columns: vec!["Item".to_owned(), "Units".to_owned()],
            sheet: 0,
            range: crate::coord::Range::parse("A1:B5").unwrap(),
        }];
        let env = Env { sheet: Some(0), tables: &tables, ..Default::default() };
        // ptgExtend, ptgList; flags 1 (a single column); table 1; column 2 only.
        let list = |flags: u16| {
            let mut t = vec![0x18, 0x19, 0, 0];
            t.extend(flags.to_le_bytes());
            t.extend(1u32.to_le_bytes());
            t.extend([1, 0, 1, 0]);
            t
        };
        let inside = Origin { row: 2, col: 0 };
        let outside = Origin { row: 2, col: 5 };
        assert_eq!(text(Dialect::Biff8, &list(1), &[], &env, inside), "[Units]");
        assert_eq!(text(Dialect::Biff8, &list(1), &[], &env, outside), "Fruit[Units]");
        assert_eq!(text(Dialect::Biff8, &list(0x41), &[], &env, inside), "[@Units]");
        assert_eq!(text(Dialect::Biff8, &list(0x41), &[], &env, outside), "Fruit[@Units]");
        assert_eq!(
            text(Dialect::Biff8, &list(0x11), &[], &env, outside),
            "Fruit[[#Headers],[Units]]"
        );
    }
}
