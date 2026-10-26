# task-2338: Excel workbooks in Unluminous (PRD)

## The request

> We want unluminous to support creating and editing Microsoft excel documents. Research online, write a
> prd, then a tdd, and fully implement. We want full major feature parity with excel, and
> interoperability. We have excel installed locally that should be used as an oracle. We want extensive
> testing. We do not want to use the tool ribbon that excel has, and should focus on creating our own,
> amazing UX that is minimal, but allows users to drill down to access features. We want this to be a
> plugin that can be disabled/uninstalled, so no additional resources are used, if someone chooses.

## Who uses it

- **A person** opens an `.xlsx` file from the explorer and edits it the way they would in Excel: types
  values and formulas, formats cells, adds sheets, sorts and filters, and saves a file that Excel opens
  without a repair prompt and computes to the same values.
- **An agent** does the same through `plugins run spreadsheet …` and the generated MCP tool: reads a
  range as values or formulas, writes cells, formats them, evaluates a formula without writing it, and
  saves. Unluminous's rule is that every control a person has, an agent has too, through the same code.

## Research

What was read and what was measured:

- **ECMA-376 (Office Open XML), Part 1 SpreadsheetML.** An `.xlsx` file is a zip of XML parts:
  `[Content_Types].xml`, relationships in `_rels/*.rels`, `xl/workbook.xml`, one `xl/worksheets/sheetN.xml`
  a sheet, `xl/sharedStrings.xml`, `xl/styles.xml`, and optional parts for comments, tables, drawings,
  charts, pivot tables, VBA (`xl/vbaProject.bin`) and dynamic array metadata (`xl/metadata.xml`).
  Formulas are stored as text without the leading `=`, with newer functions written with a prefix
  (`_xlfn.XLOOKUP`, `_xlfn._xlws.FILTER`, `_xlfn.SEQUENCE`), and every formula cell carries the value
  Excel last computed for it.
- **Excel's own function reference** ("Excel functions (by category)", Microsoft Support) and "New
  dynamic array functions": FILTER, SORT, SORTBY, UNIQUE, SEQUENCE and RANDARRAY spill their result into
  the cells beside them, and a spill that is blocked shows `#SPILL!`. XLOOKUP, XMATCH, LET and LAMBDA
  came in the same generation.
- **IronCalc** (github.com/ironcalc/IronCalc), an open source spreadsheet engine in Rust with an xlsx
  reader and writer. Weighed as a dependency in the TDD §2 and not used.
- **The Excel installed on this machine**, version 16.0, driven through its COM interface from
  PowerShell. It is the oracle: it computes the expected value of every formula in the test corpus, it
  opens every file Unluminous writes and reports whether it had to repair it, and it writes the files
  Unluminous is tested reading.

Minimal spreadsheet interfaces looked at for the UX: Apple Numbers (a format sidebar that shows only
what applies to the selection), Google Sheets (a menu bar, a slim toolbar, and the function list while
typing `=`), and Excel's own status bar (sum, average and count of the selection). None of them needs a
ribbon to reach a feature, and that is the model here.

## Goals

1. Opening an `.xlsx` or `.xlsm` file shows the workbook: a grid of cells with the file's formatting,
   one tab a sheet, the formula bar above.
2. Every formula Excel computes, Unluminous computes to the same value, for every function in the
   library below. The test corpus is checked against values Excel itself computed.
3. Saving writes a file Excel opens with no repair prompt and that recalculates in Excel to the values
   Unluminous shows. Parts of the file Unluminous does not edit, such as charts, pivot tables, pictures
   and macros, are written back unchanged.
4. No ribbon. A thin bar over the grid, a right click menu, and one Format panel that shows the controls
   for what is selected and drills down into each group.
5. Everything is a plugin, `spreadsheet`. Switched off, `.xlsx` files open as "cannot be shown" tabs,
   no spreadsheet code runs, and no memory is held for it.
6. Everything a person can do, an agent can do through `plugins run spreadsheet`.

## Files

| Excel | Unluminous |
|---|---|
| `.xlsx` open and save | Yes. |
| `.xlsm` open and save | Yes. The macros are kept in the file and written back. They are never run, and the workbook says so in a line over the grid. |
| `.csv` and `.tsv` | Yes, as a workbook of one sheet. Saving writes the values as text the way Excel does. Formats and other sheets are not saved to a CSV, and the tab says so before it writes. |
| New workbook | Yes. `File -> New -> Excel Workbook` and the explorer's `New` menu make `Book1.xlsx`. |
| `.xls` (the binary format before 2007), `.xlsb`, `.ods` | No. They are different file formats with nothing in common with the XML one. The tab says which format it is and that Excel can convert it. |
| Open a file another program changed | Yes, the rule every tab already follows: a workbook with no unsaved changes is read again. |
| Password protected files | No. They are encrypted with a scheme this version does not read; the tab says so. |
| Sheet and workbook protection | Kept in the file. A protected sheet's locked cells refuse an edit with a sentence saying it is protected. |
| Autosave and version history | No. Saving is the save every other tab has. |

## Cells and formulas

| Excel | Unluminous |
|---|---|
| Numbers, text, true and false, errors | Yes. |
| Typing a value: `12`, `1,234.5`, `50%`, `$3.00`, `2026-10-23`, `10/23/2026`, `14:30`, `TRUE` | Yes, read the way Excel reads them, setting a number format where Excel would. A leading apostrophe makes text. |
| Formulas with `+ - * / ^ & = <> < > <= >=`, `%`, unary minus, ranges `A1:B2`, union `,` inside functions, intersection ` ` | Yes. |
| Relative and absolute references, `$A$1`, F4 to cycle them | Yes. |
| Other sheets `Sheet2!A1`, `'My sheet'!A1`, 3D references `Sheet1:Sheet3!A1` | Yes. |
| Whole columns and rows `A:A`, `1:1` | Yes. |
| Defined names, workbook and sheet scoped | Yes, with a Names list in the Format panel's Data group. |
| Tables and structured references `Table1[Amount]` | Read and written; structured references in formulas evaluate. Creating a table: yes, from the selection. |
| Array constants `{1,2;3,4}` | Yes. |
| Dynamic arrays and spilling, `#SPILL!`, the `#` spill reference `A1#` | Yes. |
| Implicit intersection `@` | Yes. |
| LET, LAMBDA, and LAMBDA named in Name Manager | LET yes. LAMBDA yes, called inline and by name. |
| Legacy array formulas (Ctrl+Shift+Enter) | Read, evaluated, and written back. Ctrl+Shift+Enter enters one. |
| Automatic recalculation, only the cells that depend on a change | Yes. |
| Volatile functions: NOW, TODAY, RAND, RANDBETWEEN, OFFSET, INDIRECT | Yes, recalculated on every change, as Excel does. |
| Circular references | Detected; the cells show 0 and the bar names the cell, which is what Excel does with iteration off. Iterative calculation: kept in the file, not performed. |
| Manual calculation mode, F9 | Yes. |
| Error values `#DIV/0! #N/A #NAME? #NULL! #NUM! #REF! #VALUE! #SPILL! #CALC!` | Yes, with the same rules for which one a formula returns. |
| Formula errors shown to the person | Yes: a cell with an error has a mark, and the bar says why under the formula. |
| Function list while typing `=` and the arguments of the function the caret is in | Yes. |
| External links to other workbooks | The cached values are shown and kept; the links are not refreshed. |

### Function library

Every function below is implemented and checked against Excel. The list is the TDD's §6 table; in summary:

- **Math and trigonometry**: SUM, SUMIF, SUMIFS, SUMPRODUCT, SUMSQ, PRODUCT, ABS, SIGN, ROUND, ROUNDUP,
  ROUNDDOWN, MROUND, CEILING, CEILING.MATH, FLOOR, FLOOR.MATH, INT, TRUNC, MOD, QUOTIENT, POWER, SQRT,
  SQRTPI, EXP, LN, LOG, LOG10, PI, SIN, COS, TAN, ASIN, ACOS, ATAN, ATAN2, SINH, COSH, TANH, DEGREES,
  RADIANS, FACT, FACTDOUBLE, COMBIN, COMBINA, PERMUT, GCD, LCM, EVEN, ODD, RAND, RANDBETWEEN, RANDARRAY,
  SEQUENCE, SUBTOTAL, AGGREGATE, MMULT, MDETERM, MINVERSE, TRANSPOSE, ROMAN, ARABIC, BASE, DECIMAL.
- **Statistics**: AVERAGE, AVERAGEA, AVERAGEIF, AVERAGEIFS, COUNT, COUNTA, COUNTBLANK, COUNTIF, COUNTIFS,
  MAX, MAXA, MAXIFS, MIN, MINA, MINIFS, MEDIAN, MODE, MODE.SNGL, MODE.MULT, LARGE, SMALL, RANK, RANK.EQ,
  RANK.AVG, PERCENTILE, PERCENTILE.INC, PERCENTILE.EXC, QUARTILE, QUARTILE.INC, QUARTILE.EXC,
  PERCENTRANK, STDEV, STDEV.S, STDEV.P, STDEVP, STDEVA, VAR, VAR.S, VAR.P, VARP, AVEDEV, DEVSQ, GEOMEAN,
  HARMEAN, CORREL, PEARSON, COVAR, COVARIANCE.P, COVARIANCE.S, SLOPE, INTERCEPT, RSQ, FORECAST,
  FORECAST.LINEAR, TREND, STEYX, NORM.DIST, NORM.INV, NORM.S.DIST, NORM.S.INV, NORMDIST, NORMINV,
  STANDARDIZE, BINOM.DIST, POISSON.DIST, EXPON.DIST, FREQUENCY, SKEW, KURT, TRIMMEAN.
- **Logical**: IF, IFS, IFERROR, IFNA, AND, OR, XOR, NOT, TRUE, FALSE, SWITCH, LET, LAMBDA.
- **Lookup and reference**: VLOOKUP, HLOOKUP, LOOKUP, XLOOKUP, MATCH, XMATCH, INDEX, CHOOSE, CHOOSECOLS,
  CHOOSEROWS, OFFSET, INDIRECT, ADDRESS, ROW, ROWS, COLUMN, COLUMNS, AREAS, FILTER, SORT, SORTBY, UNIQUE,
  TAKE, DROP, VSTACK, HSTACK, TOCOL, TOROW, WRAPROWS, WRAPCOLS, EXPAND, HYPERLINK, FORMULATEXT.
- **Text**: CONCAT, CONCATENATE, TEXTJOIN, LEFT, RIGHT, MID, LEN, FIND, SEARCH, REPLACE, SUBSTITUTE,
  UPPER, LOWER, PROPER, TRIM, CLEAN, REPT, EXACT, TEXT, VALUE, NUMBERVALUE, FIXED, DOLLAR, CHAR, CODE,
  UNICHAR, UNICODE, T, N, TEXTBEFORE, TEXTAFTER, TEXTSPLIT, VALUETOTEXT.
- **Date and time**: DATE, DATEVALUE, TIME, TIMEVALUE, NOW, TODAY, YEAR, MONTH, DAY, HOUR, MINUTE, SECOND,
  WEEKDAY, WEEKNUM, ISOWEEKNUM, EDATE, EOMONTH, DATEDIF, DAYS, DAYS360, NETWORKDAYS, NETWORKDAYS.INTL,
  WORKDAY, WORKDAY.INTL, YEARFRAC.
- **Financial**: PMT, IPMT, PPMT, PV, FV, NPV, XNPV, IRR, XIRR, MIRR, RATE, NPER, SLN, SYD, DB, DDB,
  EFFECT, NOMINAL, CUMIPMT, CUMPRINC.
- **Information**: ISBLANK, ISNUMBER, ISTEXT, ISNONTEXT, ISLOGICAL, ISERROR, ISERR, ISNA, ISREF,
  ISFORMULA, ISEVEN, ISODD, NA, TYPE, ERROR.TYPE, CELL (the common info types), SHEET, SHEETS.
- **Engineering**: DEC2BIN, DEC2HEX, DEC2OCT, BIN2DEC, HEX2DEC, OCT2DEC, BIN2HEX, HEX2BIN, BITAND, BITOR,
  BITXOR, BITLSHIFT, BITRSHIFT, CONVERT (the common units), DELTA, GESTEP.

A function this version does not have evaluates to `#NAME?`, is kept in the file exactly as written,
and keeps the value Excel last saved for it until the cell is edited. The Format panel's function list
says which functions exist.

## Formatting

| Excel | Unluminous |
|---|---|
| Number formats: General, number, currency, accounting, date, time, percent, fraction, scientific, text, and custom format codes with sections, colours and conditions | Yes. The format code language is implemented, so any code Excel accepts is shown the way Excel shows it. |
| Font: family, size, bold, italic, underline, strikethrough, colour | Yes. |
| Fill colour | Yes. Pattern fills other than solid are kept and drawn solid. |
| Borders: each side, line styles thin, medium, thick, dashed, dotted, double, colours | Yes. |
| Alignment: horizontal, vertical, wrap text, indent, shrink to fit | Yes. Text rotation is kept and drawn horizontally. |
| Merge and unmerge cells, merge and centre | Yes. |
| Column widths and row heights, double click to fit, hide and unhide | Yes. |
| Freeze panes | Yes. |
| Gridlines on and off, zoom per sheet | Yes. |
| Cell styles (Good, Bad, Heading 1…) | The ones in the file are kept and drawn. A short list of built in styles is offered in the Format panel. |
| Format painter | Yes: `Copy Format` and `Paste Format`, and the brush in the Format panel. |
| Conditional formatting: cell value rules, formula rules, top and bottom, above average, duplicates, text contains, colour scales, data bars, icon sets | Drawn: every type but icon sets, which are kept and not drawn. Created from the panel: cell value, text contains, top and bottom, duplicates, colour scale, data bar. |
| Themes and theme colours | Theme colours in the file are read and drawn. The file's theme part is kept. |

## Data

| Excel | Unluminous |
|---|---|
| Sort a range by one or more columns, with headers | Yes. |
| AutoFilter: filter by values, by condition, sort from the header | Yes. |
| Data validation: list dropdown, whole number, decimal, date, text length, custom formula, input and error messages | Yes. A list cell has a dropdown. |
| Remove duplicates | Yes. |
| Text to columns | Yes, by a delimiter. |
| Fill handle: copy, series of numbers, dates, weekdays and months, and formulas with relative references moved | Yes. Ctrl+D and Ctrl+R as well. |
| Flash fill, Power Query, data types, What If analysis, Solver | No. |
| Pivot tables | Kept in the file with their last values. Building or refreshing one: no. |
| Group and outline rows and columns | Kept and drawn as hidden rows and columns; the outline buttons are not drawn. |

## Editing

| Excel | Unluminous |
|---|---|
| Type into a cell, F2 to edit, Escape to cancel, Enter and Tab to move | Yes. |
| Formula bar, name box, Go To (Ctrl+G) | Yes. |
| Point at cells while typing a formula, coloured outlines on the references | Yes. |
| Cut, copy and paste within the workbook, formulas adjusted | Yes. |
| Copy to Excel and paste from Excel | Yes: tab separated text, which is what Excel puts on the clipboard and reads. |
| Paste special: values, formulas, formats, transpose | Yes. |
| Insert and delete rows, columns and cells, references adjusted, `#REF!` where a reference is deleted | Yes. |
| Undo and redo | Yes, every change is one step. |
| Find and replace, in a sheet or the workbook, values or formulas | Yes. |
| AutoSum (Alt+=) | Yes. |
| Comments and notes | Notes are shown with the red corner and edited. Threaded comments are kept and shown read only. |
| Hyperlinks | Yes. Ctrl+Click opens a web address in a browser tab and a link to a cell goes to it. |
| Pictures and shapes | Kept in the file. Pictures are drawn over the grid; shapes are not. |
| Charts | Kept in the file and drawn: column, bar, line, area, pie, doughnut and scatter, from the chart part. A new chart of those types is made from the selection. Other chart types are kept and shown as a frame naming the type. |
| Sparklines, slicers, form controls | Kept, not drawn. |
| Spell check, translate, Copilot, collaboration | No. |
| Print, page setup, headers and footers | Kept in the file. Printing: no, the editor has no printing anywhere. |

## Sheets

Add, rename, delete, duplicate, move by dragging the tab, hide and unhide, tab colour. Ctrl+PageUp and
Ctrl+PageDown move between them.

## The interface: no ribbon

The grid fills the tab. Above it, **one bar** 32 points tall:

```
[ B4      ] fx  =SUM(B1:B3)                                         [ Format ▸ ]  [ ⋯ ]
```

- the **name box**, which shows the selection and takes a reference or a name to go to,
- the **formula bar**, which grows to several lines while a long formula is edited,
- **Format**, which opens the Format panel at the right of the grid,
- **⋯**, the workbook menu: sheets, names, find and replace, calculation, save as CSV.

Under the grid, the **sheet tabs** and, at the right, the **summary of the selection**: sum, average and
count, as Excel's status bar has. Clicking it copies the number.

**The Format panel drills down.** Its top level is six rows, each showing the current value for the
selection in one line: Number (`Currency, 2 places`), Text (`Calibri 11, Bold`), Fill, Borders,
Alignment, and Rules (conditional formatting and validation). Pressing a row opens that group in place
with a back arrow: the Number group lists the formats with a live example of the selected cell in each,
and `Custom` opens the format code with a preview. Data (sort, filter, remove duplicates, text to
columns, names) is a seventh row. The panel is a pane of the tab, not a modal, so the grid can be
watched as a format is chosen, and it closes with Escape or the Format button.

**The right click menu** is about the cells under the pointer: Cut, Copy, Paste, Paste Special ▸, Insert
▸, Delete ▸, Sort ▸, Filter, Clear ▸, Format Cells…, Note, Link. On a column or row header it adds
width or height, hide and unhide.

**Every command is searchable.** Typing in the `Find a command` field at the top of the Format panel, or
Ctrl+Shift+P, lists the spreadsheet's commands by name, so a feature with no place on screen is still one
search away. Every Excel keyboard shortcut in the table in TDD §8 works.

**While typing a formula**, a list of functions narrows under the cell, and the arguments of the function
the caret is in are shown above it, with the current argument in bold.

## The plugin

- Plugin id `spreadsheet`, name **Spreadsheet**, `plugin.kind = ui`, claiming `.xlsx`, `.xlsm`, `.csv`
  and `.tsv` through `ui.extensions`.
- It has no pane, which would take room from people who never use it. It has a menu, `Spreadsheet`, a
  Settings page, and its tabs.
- **Switched off**, the provider is not built, no workbook is held, and `.xlsx` files open as tabs saying
  the Spreadsheet plugin is off. `.csv` files open as text, as they do today.
- **Uninstalled**: the Plugins page's Remove. The code is in the binary, as every bundled plugin's is,
  and does nothing until the plugin is on and a workbook is opened. A workbook's memory is let go when
  its last tab closes.

## The agent's half

`plugins run spreadsheet <verb>` and the `unluminous_spreadsheet` MCP tool generated from it. The verbs
are listed in the TDD §9; they cover opening, the sheets, reading a range as values, formulas or
formats, writing values and formulas, formatting, inserting and deleting rows and columns, sorting,
filtering, evaluating a formula without writing it, the selection, undo and redo, and saving.

## What is not in this version, and why

- **Running VBA macros.** A macro is a program; Unluminous runs no code from a document.
- **The binary formats `.xls` and `.xlsb`, and `.ods`.** Different formats sharing nothing with the XML
  one. Excel converts each in one step.
- **Building or refreshing a pivot table, Power Query, Solver, What If analysis, Flash Fill.** Each is a
  product of its own; their results in a file are kept and shown.
- **Printing.** The editor prints nothing anywhere.
- **Shapes, SmartArt, sparklines, slicers, form controls and icon sets** are kept in the file and not
  drawn.

**Round 2 (`task-2360`, design in `tasks/task-2357-spreadsheet-round2-tdd.md`) built most of this list.**
`.xls`, `.xlsb` and `.ods` are read, and `.ods` is written. Pivot tables are built and refreshed,
with slicers and timelines. Goal Seek, data tables, scenarios, Flash Fill, printing, print preview and
PDF export, pictures, shapes, text boxes, sparklines and icon sets are in. What is still not in this
version: running VBA macros, Power Query, Solver, SmartArt and form controls, for the reasons above.

## How it is checked

- **The Excel oracle.** A corpus workbook of formulas covering every function is computed by Excel and
  saved; the test reads the values Excel wrote and compares Unluminous's own result for every cell.
- **Round trips through Excel.** Files written by Unluminous are opened by Excel through COM, which
  reports whether it repaired them and what each formula computes; files written by Excel are read by
  Unluminous. Tests that need Excel run when it is installed and skip with a printed line otherwise, and
  the files Excel wrote are committed so the comparison runs everywhere.
- **Unit tests** for the zip reader and writer, the parser, every function, the format code renderer,
  reference adjustment on insert and delete, and recalculation order.
- **Window tests and screenshots** of the grid, the formula bar, the Format panel at each level, the
  right click menu, a chart and conditional formatting.
- **An agent run**: a scenario in `tools/agent-study` that asks for a budget sheet in plain words and is
  graded from the saved file.
