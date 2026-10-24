# task-2338: Excel workbooks in Unluminous (TDD)

`tasks/task-2338-spreadsheet-prd.md` is what is built. This is how.

## 1. Shape

Two halves, the split every feature of this size here already has:

- **`crates/unluminous-sheet`**, with no user interface dependency: the workbook model, the zip
  container, reading and writing SpreadsheetML, the formula language, the evaluator and its function
  library, recalculation, number formats, CSV, and every editing operation (insert rows, sort, fill,
  paste) as a function of a workbook. Its tests run with no window. It is `unluminous-jupyter`'s and
  `unluminous-db`'s arrangement.
- **The `spreadsheet` plugin** in `unluminous-app`: `plugins/spreadsheet/plugin.conf`, the provider
  `services::spreadsheet::Spreadsheet`, and the drawing in `components::spreadsheet`.

## 2. Why the engine is written here

IronCalc is an open source Rust spreadsheet engine with an xlsx reader and writer, and it was weighed.
Three things decided against it:

1. **It does not keep what it does not model.** It reads a file into its own model and writes that
   model out, so a chart, a pivot table, a picture or a VBA project in a file is gone after a save.
   Interoperability is half of the request, and a save that deletes somebody's chart is the one fault
   nobody forgives. The reader here keeps every part and every worksheet element it does not model
   and writes them back (§4.3).
2. **Its version is below 1.0 and its model changes between releases**, and the window would be built on
   that model's types.
3. **What is left is arithmetic that can be checked.** The hard part of a formula engine is agreeing with
   Excel, and Excel is on this machine. Every function is written against values Excel computed (§10),
   so an engine written here is checked the same way a dependency would have to be.

Dependencies: `flate2` and `crc32fast` (already in the tree, through `png` and `zip` users) for the zip
container, `roxmltree` (already in the tree) for reading XML. XML is written by hand with one escaping
function. Nothing new is added to the lock file.

## 3. The model

```rust
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub names: Vec<DefinedName>,        // workbook and sheet scoped
    pub styles: Styles,                 // fonts, fills, borders, number formats, cell formats
    pub tables: Vec<Table>,
    pub date1904: bool,
    pub calc: CalcSettings,             // automatic or manual, iterate flag kept
    pub kept: Kept,                     // every part not modelled, by path, as bytes
    pub active_sheet: usize,
}
pub struct Sheet {
    pub name: String,
    pub cells: BTreeMap<(u32, u32), Cell>,      // (row, column), zero based, row major
    pub columns: Vec<ColumnInfo>,               // widths, hidden, style, as ranges
    pub rows: BTreeMap<u32, RowInfo>,           // heights, hidden, style
    pub merges: Vec<Range>,
    pub frozen: Option<(u32, u32)>,
    pub conditional: Vec<ConditionalFormat>,
    pub validations: Vec<Validation>,
    pub notes: BTreeMap<(u32, u32), Note>,
    pub links: Vec<Hyperlink>,
    pub filter: Option<AutoFilter>,
    pub state: Visible | Hidden | VeryHidden,
    pub tab_color, zoom, show_grid,
    pub spills: Spills,                         // computed, never saved as cells
    pub raw: SheetRaw,                          // worksheet elements kept verbatim
}
pub struct Cell { pub value: Value, pub formula: Option<Formula>, pub style: u32 }
pub enum Value { Empty, Number(f64), Text(Rc<str>), Bool(bool), Error(ErrorKind) }
```

A `Formula` holds its parsed tree, the text as written, and whether it is a legacy array formula over a
range. A cell that holds a formula keeps the last value computed for it in `value`, which is also what
the file stores.

Coordinates are zero based in the model and one based only where text is read or written. Excel's
limits are kept: 1,048,576 rows and 16,384 columns.

## 4. The file

### 4.1 Zip

`zip.rs` reads the central directory, then each entry by its local header, stored or deflated. It
writes deflated entries with a CRC and a central directory, no zip64 (a part over 4 GB is refused,
which no real workbook has). Entries are written in the order they were read, new parts after, so a
file opened and saved changes as little as possible.

### 4.2 Reading

`xlsx::read` follows the relationships from `_rels/.rels` to the workbook, from the workbook's rels to
each sheet, the shared strings, the styles and the theme, and from each sheet's rels to its comments,
tables, drawings and hyperlinks. Each modelled part is parsed with `roxmltree`. The theme is read for its
twelve colours only. A cell's `t` attribute decides its type (`s` shared string, `str` formula text,
`inlineStr`, `b`, `e`, `n`). Shared formulas (`t="shared"`, `si`) are expanded into one formula a cell,
moved by the offset from the master cell. Dynamic array formulas (`cm` pointing at the
`XLDAPR` metadata) are read as ordinary formulas whose result spills.

### 4.3 What is kept

- Every part that is not one the writer regenerates is kept as bytes in `Kept` and written back under
  the same name, with its relationships and its content type.
- Inside a worksheet, every element the model does not hold (`sheetProtection`, `pageMargins`,
  `pageSetup`, `headerFooter`, `drawing`, `legacyDrawing`, `tableParts`, `extLst`, `sheetPr` children,
  `phoneticPr`, `printOptions`, `rowBreaks`, `colBreaks`, `customProperties`, `oleObjects`, `controls`,
  ...) is kept as its XML text and written back in the place the schema puts it. The writer emits
  CT_Worksheet's elements in schema order from either the model or the kept text, which is what stops
  Excel calling the file corrupt.
- Workbook level elements likewise (`workbookPr` attributes, `bookViews`, `calcPr`, `extLst`,
  `fileVersion`, `workbookProtection`, `externalReferences`, `pivotCaches`).
- A cell's rich text runs are kept with the cell while its text is unchanged.

### 4.4 Writing

`xlsx::write` regenerates `[Content_Types].xml`, `xl/workbook.xml` and its rels, each worksheet, the
shared strings, the styles, the comments with their VML shapes, and the tables, and copies the kept
parts. Newer functions are written with the prefix Excel expects (`_xlfn.`, and `_xlfn._xlws.` for
FILTER and SORT), from one table beside the function library. A formula whose result spills is written
with `cm="1"` and `xl/metadata.xml` is written with the dynamic array entry, which is what makes Excel
read it as a dynamic array rather than as an implicit intersection. Every formula cell is written with
the value Unluminous computed, so a reader that does not calculate sees the right numbers.

### 4.5 CSV

RFC 4180 with Excel's habits: the separator is guessed from the first line (comma, semicolon or tab),
values are typed the way typing them would be, and saving writes each cell's formatted text.

## 5. Formulas

### 5.1 Parsing

`formula::lexer` then a precedence climbing `formula::parser` into an `Expr`:

```rust
enum Expr {
    Number(f64), Text(Rc<str>), Bool(bool), Error(ErrorKind), Array(Vec<Vec<Expr>>),
    Ref(Reference),                 // one cell, a range, whole columns or rows, on a sheet or 3D
    Name(String),                   // a defined name, a table, a LET or LAMBDA parameter
    Unary(Op, Box<Expr>), Binary(Op, Box<Expr>, Box<Expr>), Percent(Box<Expr>),
    Call(String, Vec<Expr>),        // upper case, prefix removed
    Invoke(Box<Expr>, Vec<Expr>),   // a LAMBDA called inline
    Spill(Reference),               // A1#
    Implicit(Box<Expr>),            // @
    Structured(TableRef),           // Table1[Amount], Table1[@Amount], Table1[#Totals]
    Union(Vec<Expr>), Intersect(Box<Expr>, Box<Expr>),
}
```

Precedence is Excel's: `:` then space (intersection) then `,` inside parentheses as union, then unary
minus, `%`, `^`, `* /`, `+ -`, `&`, comparisons. Unary minus binds tighter than `^`, which is why
`=-2^2` is 4 in Excel. `formula::print` writes an `Expr` back to text, and is what every reference
adjustment goes through: a copy, a fill, an inserted or deleted row, a renamed sheet. A deleted target
prints as `#REF!`.

### 5.2 Evaluating

`eval::Evaluator` walks the tree with a context: the workbook, the cell being computed, the LET and
LAMBDA bindings. An expression evaluates to a `Val`:

```rust
enum Val { Scalar(Value), Array(Array), Ref(RangeRef), Lambda(Lambda) }
```

A reference stays a reference until a function or operator needs values, so `ROW`, `OFFSET`, `INDEX` and
`ISREF` see the reference itself. Operators lift over arrays element by element, broadcasting a single
row or column the way Excel does and filling a mismatch with `#N/A`. Coercion follows Excel: text that
reads as a number is a number in arithmetic and not in SUM's range arguments; a blank is 0 or "".

### 5.3 Recalculation

- **Precedents** are collected from each formula's tree when it is set: the cells and ranges it names.
  Formulas that use OFFSET, INDIRECT, a volatile function, or a name that does are marked volatile.
- **Dependents** are found with an index from a cell to the formulas that name it directly, and a list of
  range precedents scanned for containment. A change marks its dependents dirty, transitively, plus every
  volatile formula.
- The dirty set is ordered by an iterative depth first walk of the precedents (no recursion, so a column
  of a hundred thousand chained formulas does not exhaust the stack), then evaluated in that order. A
  dependency found only while evaluating (through INDIRECT) that is still dirty is computed then.
- A cycle found while ordering leaves its cells at 0 and the workbook records the first cell of it, which
  the bar shows.
- **Spills**: a formula whose result is an array of more than one cell writes it into `Sheet::spills`,
  anchored at the formula. Reading a cell asks the cell map, then the spills. A spill that would cover a
  non empty cell or another spill is `#SPILL!`. A change to a spill's size marks the formulas that name
  the cells it covered, before and after.

### 5.4 The function library

`functions/` holds one module a category, and a table `FUNCTIONS: &[(name, min args, max args, kind,
fn)]` is the one list: the evaluator, the formula completion list, the argument hints, the `functions`
verb and the oracle corpus are all made from it. Criteria (`">=10"`, `"a*"`, `"<>"`) are one module used by
every *IF and *IFS function and by the database functions. Statistics distributions use their own
implementations of the error function and the inverse normal (Acklam's algorithm refined by one Halley
step), checked to fifteen digits against Excel.

## 6. Number formats

`numfmt::Format::parse` reads a format code into up to four sections, each a list of tokens: digit
placeholders `0 # ?`, the decimal point, thousands separators and scaling commas, percent, exponents,
literal text, `@`, fill `*`, padding `_`, colours `[Red]`, conditions `[>=100]`, and the date and time
codes `y m d h s AM/PM [h] [mm] [ss] .0`. `render(value, &format, width) -> Rendered { text, colour,
align }`. General follows Excel: up to eleven characters, switching to scientific and rounding to the
width the column has. The built in formats 0 to 49 are a table. A date is a serial number from the 1900
system with its phantom 29 February 1900, or the 1904 system when the workbook says so.

## 7. Editing operations

Each is a function in `unluminous-sheet::edit` taking the workbook and returning what the undo needs:

| Operation | Notes |
|---|---|
| set a cell (value or formula), clear contents, clear formats | Typing is parsed by `input::read`, which applies a number format the way Excel does for `50%`, `$3`, dates. |
| insert and delete rows, columns, cells | Every formula in the workbook is moved through `print` with an adjustment; merges, validations, conditional formats, notes, links, names, tables, filters and the frozen pane move too. |
| copy and paste, cut and paste | Copy keeps cells; paste moves relative references by the offset. Cut moves the cells and rewrites references to them. |
| fill down, right, and series | Series find a step from the selection: numbers, dates, months, weekdays, text ending in a number. |
| sort | Stable, by several keys, Excel's order: numbers, text (case insensitive), logicals, errors, blanks last. |
| filter | Rows hidden by the filter, the criteria kept in `autoFilter`. |
| merge, unmerge, set style properties, widths, heights, hide | |
| find and replace | |
| sheets | add, rename (references rewritten), delete (references become `#REF!`), move, copy, hide. |

**Undo** is a stack of entries. A cell operation records the cells it changed as they were. A structural
operation (insert, delete, sort, a sheet change) records a snapshot of the workbook's sheets and names,
which is cheap because the kept parts are shared (`Arc`) and a sheet of a hundred thousand cells clones
in a few milliseconds.

## 8. The plugin in the window

### 8.1 Manifest and opening

`plugins/spreadsheet/plugin.conf`: `plugin.kind = ui`, `ui.provider = spreadsheet`, `ui.extensions =
.xlsx, .xlsm, .csv, .tsv`, a menu and a settings page, no pane. `opening.rs` sends a file the plugin
claims to `open_a_file_in_a_plugin_tab` (`app/plugin_files.rs`), which opens a plugin tab keyed `spreadsheet:<path relative to the
project>` and tells the provider to load it. Switched off, `opens_elsewhere` answers nothing, so `.csv`
opens as text and `.xlsx` opens as "cannot be shown" with a line naming the plugin.

### 8.2 Two changes to the plugin contract

- **`UiProvider::tab_keyed(key, ui, look)`**, defaulting to `tab`, because a provider that draws one tab a
  file has to know which file the tab is. Realm drew its tabs in the window to get round this; a second
  provider needing it is the reason to make it part of the contract.
- **`UiProvider::modified(key) -> bool`** and **`save(key)`**, defaulting to false and a refusal, so a
  plugin tab can have the unsaved dot and be saved by `Ctrl+S` and `File -> Save`. Closing a modified
  workbook tab writes it, which is what closing a modified document tab already does in Unluminous, and a
  write that fails keeps the tab open with a notice. `closed(key)` lets the provider drop the book.
- **`UiProvider::claims_key(key, modifiers)`**, so the menu watcher in `frame.rs` leaves Excel's chords to
  the tab: `Ctrl+D` is Fill Down there and Show Diff on the Git menu, `Ctrl+Z` undoes a cell.

### 8.3 The provider

`services::spreadsheet::Spreadsheet` holds `books: Vec<Book>`, each with its path, `Workbook`, undo
stack, and view (sheet, selection, scroll, the cell being edited, the Format panel's level). A book is
dropped when its tab closes. `open` reads nothing; a workbook is read when its tab is first drawn or
when `open` is asked for.

### 8.3.1 One tree of commands

`services::spreadsheet::catalogue` is every command a tab offers, as a tree of entries that each name the
`plugins run spreadsheet` words they run. The workbook menu under `...` (with its search box), the Format
panel's levels, the right click menu and a sheet tab's menu are all drawn from it, so a press on an entry
and an agent sending the same words reach `commands::run` alike. A test walks the tree and runs every entry.
The keyboard and the pointer on the grid make `actions::Act` values directly, and `commands::run` turns
words into the same values, so both reach `actions::apply`.

### 8.4 Drawing

`components::spreadsheet`:

- `grid.rs` draws only the rows and columns that are on the screen, from cumulative offsets kept per
  sheet and rebuilt when a width or height changes. Each visible cell's text is formatted once per value
  and cached by `(value, style)`. Frozen rows and columns are drawn in their own clipped strips.
  Gridlines, fills, borders, conditional format results, data bars, the note corner, the validation
  arrow, the selection with its fill handle, the reference outlines while a formula is typed, and the
  copy marquee.
- `bar.rs`: the name box, the formula bar, Format and the workbook menu.
- `panel.rs`: the Format panel, a stack of levels.
- `sheets.rs`: the sheet tabs and the selection summary.
- `menu.rs`: the right click menu.
- `chart.rs`: charts. A chart read from a file is drawn from its kind, title and series formulas, and its
  parts are written back as they were; a chart made here (`chart <kind>`) is written as new drawing and
  chart parts in Excel's default shape (`xlsx/chart.rs`).
- `completion.rs`: the function list and the argument hint.

All of it is drawn with egui and the window's `controls`, `modal` and `rux` furniture, sized by the tab's
own zoom (Ctrl with the wheel).

### 8.5 Keys

Arrow keys, Shift to extend, Ctrl to jump to the edge of data, Home, Ctrl+Home, Ctrl+End, PageUp and
PageDown, Tab and Enter (Shift to go back), F2, Escape, Delete, Backspace, Ctrl+Z, Ctrl+Y, Ctrl+C, Ctrl+X,
Ctrl+V, Ctrl+Alt+V (paste special), Ctrl+D, Ctrl+R, Ctrl+B, Ctrl+I, Ctrl+U, Ctrl+5 (strikethrough),
Ctrl+1 (Format panel), Ctrl+Shift+1 to 6 (number formats), Ctrl+; and Ctrl+Shift+; (date and time),
Ctrl+Space and Shift+Space (column and row), Ctrl+A, Ctrl+F, Ctrl+H, Ctrl+G, Alt+= (AutoSum), F4 (cycle
reference), Ctrl+Shift+Enter (array formula), Alt+Enter (line break), Ctrl+PageUp and Ctrl+PageDown
(sheets), Ctrl+- and Ctrl+Shift++ (delete and insert), Ctrl+9 and Ctrl+0 (hide row, column), Ctrl+Shift+L
(filter), F9 (calculate).

### 8.6 Clipboard

Copy writes tab separated text, which Excel reads; cut is copy with a marquee and moves on paste. Paste
reads the clipboard's text: when it is the text this tab copied, the cells are pasted with their formulas
and formats; otherwise it is split into cells by tabs and lines and each is typed.

## 9. The agent's half

`unluminous-cli/src/plugins.rs` gains `SPREADSHEET`, so `plugins run spreadsheet` and the MCP tool are
generated from one list:

`open`, `new`, `books`, `save`, `save-as`, `close`, `sheets`, `sheet` (show, add, rename, delete, move,
hide), `read` (values, formulas or formats over a range, as rows), `write` (cells from a JSON array of
rows or one value), `formula`, `eval`, `clear`, `format` (number, font, fill, border, align, wrap),
`merge`, `unmerge`, `width`, `height`, `insert`, `delete`, `sort`, `filter`, `fill`, `copy`, `paste`,
`find`, `replace`, `select`, `selection`, `undo`, `redo`, `freeze`, `name`, `validate`, `conditional`,
`chart`, `note`, `link`, `recalc`, `functions`, `view`, `panel`.

Each verb is a call into the same `edit` functions the interface calls, through the provider's `command`.

## 10. The Excel oracle

`tools/excel-oracle/` holds PowerShell scripts that drive Excel through COM:

- `compute.ps1 <in> <out>` opens a workbook, does a full calculation and saves it, so the file carries
  Excel's own value for every formula.
- `inspect.ps1 <file>` opens a workbook and prints JSON: whether Excel repaired it, and for each sheet the
  used range's values, formulas and number formats as Excel reads them.

The corpus is generated by `cargo run -p unluminous-sheet --example oracle_corpus`, which writes
`corpus.xlsx` with thousands of formulas, one row a case, from the function table and a list of edge
cases. Excel computes it into `crates/unluminous-sheet/tests/oracle/corpus.excel.xlsx`, which is
committed. `tests/oracle.rs` reads that file, recalculates every cell with Unluminous's engine, and
compares with the value Excel saved: numbers to 1e-9 relative, text exactly, errors by kind. Volatile
functions are checked for shape (a RAND is a number in [0,1)), not value.

The same scripts make the round trip tests: workbooks Unluminous writes are inspected by Excel (no repair,
same values), and workbooks Excel writes (`fixtures/*.xlsx`, made with `make-fixtures.ps1`) are read by
Unluminous and compared against `inspect.ps1`'s JSON, also committed. The tests that start Excel run
when it is installed and print a line and pass otherwise; `UNLUMINOUS_REQUIRE_EXCEL=1` makes a missing
Excel a failure, the rule `UNLUMINOUS_REQUIRE_KERNEL` set for notebooks.

## 11. Tests

- `unluminous-sheet`: zip, reader, writer, keeping unknown parts byte for byte, parser and printer round
  trips, every function, number formats against Excel's own rendering of the same values (`inspect.ps1`
  returns the `.Text` of each cell), reference adjustment, recalculation order, spills, undo.
- `unluminous-app`: provider command tests driving `Spreadsheet::command` like the Database plugin's,
  window tests in `tests/spreadsheet.rs` with screenshots of the grid, the formula bar, the Format panel
  at its levels, the right click menu, conditional formats and a chart.

## 12. Order of work

1. Crate skeleton, values, references, A1 parsing, the model.
2. Zip, xlsx reading and writing with keeping; round trip tests.
3. Formula lexer, parser, printer; evaluator; recalculation; spills.
4. Function library by category, with the oracle corpus growing beside it.
5. Number formats, styles, input parsing, CSV.
6. Editing operations and undo.
7. The plugin: manifest, opening, the provider, the grid, the bar, the sheet tabs, keys, clipboard.
8. The Format panel and the right click menu; conditional formats, validation, notes, links, filters.
9. Charts.
10. The agent's half, documentation, window tests, release.
