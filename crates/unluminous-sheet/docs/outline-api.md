# Outline, subtotal and conditional format rules: the engine API

Every method below is on `unluminous_sheet::edit::Session` unless it says otherwise. Each one is a single undo
step, refuses what Excel refuses with an `Err(String)` and leaves the workbook unchanged when it does. Rows
and columns are counted from 0. `Dim` is `unluminous_sheet::outline::Dim` (`Rows` or `Columns`).

## Outline (`edit/outline.rs`, rules in `outline.rs`)

| Method | What it does |
|---|---|
| `group_rows(sheet, first, last)` | Data > Group (Shift+Alt+Right): every row one level deeper. Refused past outline level 7 (Excel's eighth level button). |
| `group_columns(sheet, first, last)` | The same for columns. |
| `ungroup_rows(sheet, first, last)` | Shift+Alt+Left. Refused when none of the rows is grouped. Hidden rows stay hidden and a summary row keeps its collapsed mark, as in Excel. |
| `ungroup_columns(sheet, first, last)` | The same for columns. |
| `show_detail(sheet, dim, summary, expanded)` | The plus and minus buttons: collapse (`expanded` false) or expand the group whose summary row or column is `summary`. Collapsing hides the detail and marks the summary line; expanding keeps an inner group folded. Refused when `summary` is not next to a group. |
| `show_levels(sheet, row_levels, column_levels)` | The level buttons 1 to 8. Level 1 shows only lines outside every group. 0 leaves that direction alone. Lines outside the outline keep their hidden state. |
| `auto_outline(sheet, within: Option<Range>)` | Data > Group > Auto Outline: replaces the outline (of the whole sheet, or of the rows and columns of `within`, using only the formulas inside it) with groups built from formulas that sum the rows or columns before or after them. Moves the summary setting to the side the formulas are on. |
| `clear_outline(sheet)` | Clear Outline for the sheet: groups removed, folded groups shown. Rows hidden by hand stay hidden. |
| `clear_outline_lines(sheet, dim, first, last)` | Clear Outline for some rows or columns. |
| `set_outline_summary(sheet, rows_below, columns_right)` | Settings dialog: `Some(false)` puts summary rows above (columns to the left). A folded group gains the mark on its new summary line. |
| `set_outline_symbols(sheet, shown)` | Ctrl+8: show or hide the outline buttons (`sheetPr/outlinePr showOutlineSymbols`). |
| `set_outline_styles(sheet, applied)` | Automatic styles setting (`applyStyles`). |
| `outline_depth(sheet) -> (rows, columns)` | The deepest level in use, to size the level buttons (draw `depth + 1` buttons). |

Pure functions, with no undo, are in `unluminous_sheet::outline`: `line(sheet, dim, index)` (level, hidden,
collapsed), `extent(sheet, dim, summary)` (the detail lines of a group), `summary_after`, `deepest`.
`Sheet.outline: OutlineSettings` holds `summary_below`, `summary_right`, `apply_styles`, `show_symbols`. The
file's `sheetFormatPr outlineLevelRow` and `outlineLevelCol` are computed when the sheet is written.

## Subtotal (`edit/subtotal.rs`)

| Item | What it does |
|---|---|
| `SubtotalSpec { range, group_by, function, total_columns, replace, page_breaks, summary_below }` | The Subtotal dialog. `range` is the list with its header row first. `group_by` and `total_columns` are offsets from the left of `range`. |
| `SubtotalFunction` | `Sum`, `Count`, `Average`, `Max`, `Min`, `Product`, `CountNumbers`, `StdDev`, `StdDevP`, `Var`, `VarP`, with `code()` (the `SUBTOTAL` number), `label()` (`Total`, `Count`, ...) and `parse(name)`. |
| `subtotal(sheet, &spec)` | Inserts a `SUBTOTAL` row at each change in the group column and a grand total, labels them (`East Total`, `Grand Total`, bold), groups the rows (details one level deeper than their subtotal row, the whole list one deeper than the grand total) and sets the summary row setting. With `replace` the subtotals already there are removed first, with `replace` false the new ones nest inside them. Refused for a list with no data rows. |
| `remove_subtotal(sheet, range)` | Remove All: deletes the subtotal rows and clears the outline of the list. |

Page breaks (`page_breaks`) are written as manual row breaks after each group's subtotal row. Excel stops
answering when asked for them on the machine this was written on, so that option has no oracle.

## Conditional format rules (`edit/cf_rules.rs`, evaluation in `conditional.rs`)

| Method | What it does |
|---|---|
| `conditional_rules(sheet, within: Option<Range>) -> Vec<RuleEntry>` | Manage Rules: the rules of the sheet, or those covering some of `within`, in the order they apply. A `RuleEntry` has `priority`, `ranges` and the `rule`. |
| `edit_conditional_rule(sheet, priority, rule)` | Replace a rule, keeping its place and its cells. |
| `set_conditional_ranges(sheet, priority, ranges)` | "Applies to". |
| `set_conditional_stop(sheet, priority, stop)` | Stop If True. |
| `delete_conditional_rule(sheet, priority)` | Delete one rule; the rest are numbered again from 1. |
| `move_conditional_rule(sheet, priority, RuleMove::{Up, Down, First, Last})` | Move Up, Move Down, and to either end. |
| `add_conditional(sheet, ranges, rule)` (existing) | Adds a rule with the lowest priority, which is where Excel's object model puts one; call `move_conditional_rule(.., First)` for what the dialog does. |

A rule is named by its priority, which is unique on the sheet and renumbered after a change.

Evaluation is `unluminous_sheet::conditional`: `RuleSet::prepare(&book, sheet)` once per revision, then
`rules.apply(&book, now, cell)` gives an `Applied` (`formats`, `scale`, `bar`, `icon`), and `applied.look(&book)`
merges the formats and the colour scale into one `Look` (fill, font colour, bold, italic, strike, underline,
number format), the first rule to set a property winning. `BarShape` has the bar's edges as fractions of the
cell, its colours, border and axis; `IconPick` the set and the icon. `default_formulas` and
`time_period_formula` give the formulas Excel writes for text, blank, error and time period rules.

`CfRule` gained `std_dev`, `equal_average`, `time_period`, `icon_set: Option<IconSetRule>`,
`data_bar_options: Option<DataBarExt>` and `x14: Option<X14Payload>`; `CfRule::new(kind)` makes an empty one.
A rule that Excel keeps only in the `x14` extension (the stars, triangles and boxes icon sets, custom icons,
formulas that name another sheet) is a rule in `sheet.conditional` like the others with `x14.only` set.
