//! The pivot cache: what a pivot table knows about the source data.
//!
//! A cache is the fields of the source (named by its header row), the distinct items of each field, and a
//! copy of every record. A pivot table never reads the sheet it summarises; refreshing the cache reads it
//! again. Several tables can share one cache.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

use crate::coord::{CellRef, Range};
use crate::model::Workbook;
use crate::numfmt::Format;
use crate::value::{ErrorKind, Value};

/// One value of a record or one item of a field.
#[derive(Debug, Clone, PartialEq)]
pub enum CacheValue {
    /// `<m/>`: nothing in the cell.
    Blank,
    Number(f64),
    /// A number in a cell with a date format.
    Date(f64),
    Text(Arc<str>),
    Bool(bool),
    Error(ErrorKind),
}

impl CacheValue {
    pub fn text(text: &str) -> CacheValue {
        CacheValue::Text(Arc::from(text))
    }

    /// The value a sheet cell holds, with its date format taken into account.
    pub fn from_cell(value: &Value, is_date: bool) -> CacheValue {
        match value {
            Value::Empty => CacheValue::Blank,
            Value::Number(n) if is_date => CacheValue::Date(*n),
            Value::Number(n) => CacheValue::Number(*n),
            Value::Text(t) => CacheValue::Text(t.clone()),
            Value::Bool(b) => CacheValue::Bool(*b),
            Value::Error(e) => CacheValue::Error(*e),
        }
    }

    /// The value as a sheet value.
    pub fn to_value(&self) -> Value {
        match self {
            CacheValue::Blank => Value::Empty,
            CacheValue::Number(n) | CacheValue::Date(n) => Value::Number(*n),
            CacheValue::Text(t) => Value::Text(t.clone()),
            CacheValue::Bool(b) => Value::Bool(*b),
            CacheValue::Error(e) => Value::Error(*e),
        }
    }

    pub fn is_blank(&self) -> bool {
        matches!(self, CacheValue::Blank)
    }

    /// The number a calculation uses, if the value is one.
    pub fn number(&self) -> Option<f64> {
        match self {
            CacheValue::Number(n) | CacheValue::Date(n) => Some(*n),
            _ => None,
        }
    }

    /// The key two values are the same item by: text without regard to case, numbers by value.
    pub fn key(&self) -> String {
        match self {
            CacheValue::Blank => String::from("m"),
            CacheValue::Number(n) | CacheValue::Date(n) => format!("n{}", n.to_bits()),
            CacheValue::Text(t) => {
                let mut key = String::from("s");
                key.extend(t.chars().flat_map(char::to_lowercase));
                key
            }
            CacheValue::Bool(b) => format!("b{b}"),
            CacheValue::Error(e) => format!("e{}", e.text()),
        }
    }

    /// Excel's ascending order of items: numbers, then text without regard to case, then logicals and
    /// errors, with blank last.
    pub fn order(&self, other: &CacheValue) -> Ordering {
        match (self, other) {
            (CacheValue::Blank, CacheValue::Blank) => Ordering::Equal,
            (CacheValue::Blank, _) => Ordering::Greater,
            (_, CacheValue::Blank) => Ordering::Less,
            (a, b) => crate::eval::compare(&a.to_value(), &b.to_value()),
        }
    }
}

/// Where the records come from.
#[derive(Debug, Clone, PartialEq)]
pub enum CacheSource {
    /// A range of a sheet, header row included.
    Range { sheet: String, range: Range },
    /// A table or a defined name.
    Name(String),
}

impl CacheSource {
    /// The sheet and range this source is now, looking a name up in the workbook.
    pub fn resolve(&self, book: &Workbook) -> Result<(usize, Range), String> {
        match self {
            CacheSource::Range { sheet, range } => {
                let index = book
                    .sheet_index(sheet)
                    .ok_or_else(|| format!("The sheet {sheet} the pivot table reads is gone."))?;
                Ok((index, *range))
            }
            CacheSource::Name(name) => {
                if let Some(table) = book.table(name) {
                    return Ok((table.sheet, table.range));
                }
                let found = book
                    .names
                    .iter()
                    .find(|n| n.name.eq_ignore_ascii_case(name))
                    .ok_or_else(|| format!("The name {name} the pivot table reads is gone."))?;
                let text = found.formula.trim_start_matches('=');
                let (sheet_text, range_text) =
                    text.rsplit_once('!').ok_or("The name does not refer to a range.")?;
                let sheet_text = sheet_text.trim_matches('\'').replace("''", "'");
                let index = book.sheet_index(&sheet_text).ok_or("The name refers to no sheet.")?;
                let range = Range::parse(&range_text.replace('$', "")).ok_or("Not a range.")?;
                Ok((index, range))
            }
        }
    }
}

/// How a grouped field is grouped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    /// Numbers in ranges.
    Range,
    Seconds,
    Minutes,
    Hours,
    Days,
    Months,
    Quarters,
    Years,
}

impl GroupBy {
    pub fn xml(&self) -> &'static str {
        match self {
            GroupBy::Range => "range",
            GroupBy::Seconds => "seconds",
            GroupBy::Minutes => "minutes",
            GroupBy::Hours => "hours",
            GroupBy::Days => "days",
            GroupBy::Months => "months",
            GroupBy::Quarters => "quarters",
            GroupBy::Years => "years",
        }
    }

    pub fn from_xml(text: &str) -> Option<GroupBy> {
        [
            GroupBy::Range,
            GroupBy::Seconds,
            GroupBy::Minutes,
            GroupBy::Hours,
            GroupBy::Days,
            GroupBy::Months,
            GroupBy::Quarters,
            GroupBy::Years,
        ]
        .into_iter()
        .find(|g| g.xml() == text)
    }

    /// The word in the name of a field made by grouping: `Months (Date)`.
    pub fn field_word(&self) -> &'static str {
        match self {
            GroupBy::Range => "",
            GroupBy::Seconds => "Seconds",
            GroupBy::Minutes => "Minutes",
            GroupBy::Hours => "Hours",
            GroupBy::Days => "Days",
            GroupBy::Months => "Months",
            GroupBy::Quarters => "Quarters",
            GroupBy::Years => "Years",
        }
    }
}

/// `rangePr`: the limits and the step of a grouping by period or by range.
#[derive(Debug, Clone, PartialEq)]
pub struct RangePr {
    pub group_by: GroupBy,
    pub start_date: Option<f64>,
    pub end_date: Option<f64>,
    pub start_num: Option<f64>,
    pub end_num: Option<f64>,
    pub interval: f64,
    pub auto_start: bool,
    pub auto_end: bool,
}

/// `fieldGroup`: how a field is derived from another one, or the marker that others are derived from it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FieldGroup {
    /// The field this field's group field is, when it has one.
    pub par: Option<usize>,
    /// The field this one groups.
    pub base: Option<usize>,
    pub range: Option<RangePr>,
    /// For each item of the base field, the group item it belongs to.
    pub discrete: Option<Vec<usize>>,
    /// The items of the group field, which a pivot field lists instead of the shared items.
    pub items: Vec<CacheValue>,
}

/// One field of the source.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheField {
    pub name: String,
    pub num_fmt_id: u32,
    /// The code of a custom `num_fmt_id`, so a pivot over another workbook's styles can show items.
    pub num_fmt_code: Option<String>,
    /// False for a calculated field and for a field made by grouping: records hold nothing for it.
    pub database: bool,
    /// The distinct items, in the order of first appearance. Empty when the file did not list them and
    /// nothing has needed them yet.
    pub shared: Vec<CacheValue>,
    pub formula: Option<String>,
    pub group: Option<FieldGroup>,
    /// Other attributes of the `cacheField`, kept.
    pub attrs: Vec<(String, String)>,
}

impl CacheField {
    pub fn new(name: &str) -> CacheField {
        CacheField {
            name: name.to_owned(),
            num_fmt_id: 0,
            num_fmt_code: None,
            database: true,
            shared: Vec::new(),
            formula: None,
            group: None,
            attrs: Vec::new(),
        }
    }

    /// The items a pivot field over this field lists: the group items when the field is grouped, the
    /// shared items otherwise.
    pub fn items(&self) -> &[CacheValue] {
        match &self.group {
            Some(g) if !g.items.is_empty() => &g.items,
            _ => &self.shared,
        }
    }

    /// Whether this field's items are the items of a grouping.
    pub fn has_group_items(&self) -> bool {
        self.group.as_ref().is_some_and(|g| !g.items.is_empty())
    }
}

/// A calculated item: a formula over the other items of its field.
#[derive(Debug, Clone, PartialEq)]
pub struct CalculatedItem {
    pub field: usize,
    pub formula: String,
    /// Its place in the field's items.
    pub item: usize,
}

/// The cache.
#[derive(Debug, Clone, PartialEq)]
pub struct PivotCache {
    pub id: u32,
    pub source: CacheSource,
    pub fields: Vec<CacheField>,
    /// One entry per field; a field with no database column holds `Blank`.
    pub records: Vec<Vec<CacheValue>>,
    /// `refreshedBy`, `refreshedDate`, `createdVersion` and the other attributes of the definition, kept.
    pub attrs: Vec<(String, String)>,
    pub refresh_on_load: bool,
    pub calculated_items: Vec<CalculatedItem>,
    /// Where the definition and the records were read from or last written.
    pub definition_part: String,
    pub records_part: String,
}

/// What a refresh changed in the items of a field, so a table can follow: for each old item, its new place.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemRemap {
    pub field: usize,
    pub moved: Vec<Option<usize>>,
    /// How many items the field has now.
    pub count: usize,
}

impl PivotCache {
    /// A cache over a range whose first row holds the field names.
    pub fn from_range(
        book: &Workbook,
        sheet: usize,
        range: Range,
        source: CacheSource,
        id: u32,
    ) -> Result<PivotCache, String> {
        if range.rows() < 2 {
            return Err("A pivot table needs a header row and at least one row of data.".to_owned());
        }
        let mut cache = PivotCache {
            id,
            source,
            fields: Vec::new(),
            records: Vec::new(),
            attrs: Vec::new(),
            refresh_on_load: false,
            calculated_items: Vec::new(),
            definition_part: String::new(),
            records_part: String::new(),
        };
        cache.read_source(book, sheet, range, false)?;
        Ok(cache)
    }

    /// Read the header row and the records from the sheet. With `keep`, the fields that are not in the
    /// database (grouping and calculated fields) stay and the others are matched by name.
    fn read_source(
        &mut self,
        book: &Workbook,
        sheet: usize,
        range: Range,
        keep: bool,
    ) -> Result<Vec<ItemRemap>, String> {
        let headers = header_names(book, sheet, range)?;
        let old: Vec<CacheField> = std::mem::take(&mut self.fields);
        let mut fields: Vec<CacheField> = Vec::new();
        for (c, name) in headers.iter().enumerate() {
            let mut field = old
                .iter()
                .find(|f| keep && f.database && f.name == *name)
                .cloned()
                .unwrap_or_else(|| CacheField::new(name));
            field.num_fmt_id = first_format(book, sheet, range, c as u32);
            field.num_fmt_code = custom_code(book, field.num_fmt_id);
            fields.push(field);
        }
        let database_count = fields.len();
        let mut records = Vec::with_capacity(range.rows() as usize - 1);
        let date_formats = DateFormats::new(book);
        for r in range.first.row + 1..=range.last.row {
            let mut record = Vec::with_capacity(database_count);
            for c in range.first.col..=range.last.col {
                let at = CellRef::new(r, c);
                let (value, style) = match book.sheets[sheet].cell(at) {
                    Some(cell) => (book.sheets[sheet].value_at(at), cell.style),
                    None => (book.sheets[sheet].value_at(at), 0),
                };
                let is_date = matches!(value, Value::Number(_)) && date_formats.is_date(book, style);
                record.push(CacheValue::from_cell(&value, is_date));
            }
            records.push(record);
        }
        // Fields that are not in the database keep their place after the database ones.
        if keep {
            for f in old.iter().filter(|f| !f.database) {
                fields.push(f.clone());
            }
        }
        let remaps = rebuild_shared(&old, &mut fields, &records, keep);
        let total = fields.len();
        for record in &mut records {
            record.resize(total, CacheValue::Blank);
        }
        self.fields = fields;
        self.records = records;
        Ok(remaps)
    }

    /// Read the source again. The items already known keep their order and new ones follow them. The
    /// answer says, for each field whose items changed, where each old item went.
    pub fn refresh(&mut self, book: &Workbook) -> Result<Vec<ItemRemap>, String> {
        let (sheet, range) = self.source.resolve(book)?;
        if range.rows() < 2 {
            return Err("The source of the pivot table has no data.".to_owned());
        }
        self.read_source(book, sheet, range, true)
    }

    /// Make a field's shared items from the records when nothing has listed them.
    pub fn ensure_shared(&mut self, field: usize) {
        if !self.fields[field].shared.is_empty() || !self.fields[field].database {
            return;
        }
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut shared = Vec::new();
        for record in &self.records {
            let value = &record[field];
            if let std::collections::hash_map::Entry::Vacant(slot) = seen.entry(value.key()) {
                slot.insert(shared.len());
                shared.push(value.clone());
            }
        }
        self.fields[field].shared = shared;
    }

    /// The index of a value among a field's shared items.
    pub fn shared_index(&self, field: usize, value: &CacheValue) -> Option<usize> {
        let key = value.key();
        self.fields[field].shared.iter().position(|v| v.key() == key)
    }

    /// The fields that are in the database, with the index each has among the record's entries.
    pub fn database_fields(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.fields.len()).filter(|&i| self.fields[i].database)
    }

    /// Whether a field is a date field: every value in it is a date or blank.
    pub fn is_date_field(&self, field: usize) -> bool {
        let f = &self.fields[field];
        let values: Vec<&CacheValue> = if f.shared.is_empty() {
            Vec::new()
        } else {
            f.shared.iter().collect()
        };
        let mut any = false;
        for v in values {
            match v {
                CacheValue::Date(_) => any = true,
                CacheValue::Blank => {}
                _ => return false,
            }
        }
        if any {
            return true;
        }
        let mut any = false;
        for record in &self.records {
            match &record[field] {
                CacheValue::Date(_) => any = true,
                CacheValue::Blank => {}
                _ => return false,
            }
        }
        any
    }

    /// The smallest and largest number or date in a field, if it has any.
    pub fn number_bounds(&self, field: usize) -> Option<(f64, f64)> {
        let mut bounds: Option<(f64, f64)> = None;
        for record in &self.records {
            if let Some(n) = record[field].number() {
                bounds = Some(match bounds {
                    Some((lo, hi)) => (lo.min(n), hi.max(n)),
                    None => (n, n),
                });
            }
        }
        bounds
    }
}

/// The names of the fields: the header cells' text, a blank one named `ColumnN` and a repeated one
/// numbered.
fn header_names(book: &Workbook, sheet: usize, range: Range) -> Result<Vec<String>, String> {
    let mut names: Vec<String> = Vec::new();
    for c in range.first.col..=range.last.col {
        let value = book.sheets[sheet].value_at(CellRef::new(range.first.row, c));
        if value.is_empty() {
            return Err("The header row of the source has an empty cell. Every field needs a name.".to_owned());
        }
        let mut name = crate::eval::to_text(&value).unwrap_or_default();
        if names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            let base = name.clone();
            let mut n = 2;
            while names.iter().any(|x| x.eq_ignore_ascii_case(&format!("{base}{n}"))) {
                n += 1;
            }
            name = format!("{base}{n}");
        }
        names.push(name);
    }
    Ok(names)
}

/// The number format id of the first cell with something in it in a column of the source.
fn first_format(book: &Workbook, sheet: usize, range: Range, col: u32) -> u32 {
    for r in range.first.row + 1..=range.last.row {
        let at = CellRef::new(r, range.first.col + col);
        if let Some(cell) = book.sheets[sheet].cell(at) {
            if !cell.value.is_empty() {
                return book.styles.xf(cell.style).num_fmt;
            }
        }
    }
    0
}

fn custom_code(book: &Workbook, id: u32) -> Option<String> {
    (id >= 164).then(|| book.styles.code_for_id(id))
}

/// Which cell formats show dates, worked out once for each format used.
struct DateFormats {
    cache: std::cell::RefCell<HashMap<u32, bool>>,
}

impl DateFormats {
    fn new(_book: &Workbook) -> DateFormats {
        DateFormats { cache: std::cell::RefCell::new(HashMap::new()) }
    }

    fn is_date(&self, book: &Workbook, style: u32) -> bool {
        let id = book.styles.xf(style).num_fmt;
        *self.cache.borrow_mut().entry(id).or_insert_with(|| {
            let code = book.styles.code_for_id(id);
            Format::parse(&code).is_date()
        })
    }
}

/// Give each database field its shared items: the old ones that are still in the data, in their old order,
/// then the new ones in order of appearance. Fields whose items were never listed stay unlisted.
fn rebuild_shared(
    old: &[CacheField],
    fields: &mut [CacheField],
    records: &[Vec<CacheValue>],
    keep: bool,
) -> Vec<ItemRemap> {
    let mut remaps = Vec::new();
    for (i, field) in fields.iter_mut().enumerate() {
        if !field.database {
            continue;
        }
        let was_listed = keep && old.iter().any(|f| f.database && f.name == field.name && !f.shared.is_empty());
        if !was_listed {
            field.shared.clear();
            continue;
        }
        let before = field.shared.clone();
        let mut present: HashMap<String, CacheValue> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for record in records {
            let value = &record[i];
            if let std::collections::hash_map::Entry::Vacant(slot) = present.entry(value.key()) {
                order.push(value.key());
                slot.insert(value.clone());
            }
        }
        let mut shared = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for value in &before {
            let key = value.key();
            if present.contains_key(&key) && !index.contains_key(&key) {
                index.insert(key.clone(), shared.len());
                shared.push(present[&key].clone());
            }
        }
        for key in &order {
            if !index.contains_key(key) {
                index.insert(key.clone(), shared.len());
                shared.push(present[key].clone());
            }
        }
        let moved: Vec<Option<usize>> = before.iter().map(|v| index.get(&v.key()).copied()).collect();
        remaps.push(ItemRemap { field: i, moved, count: shared.len() });
        field.shared = shared;
    }
    remaps
}
