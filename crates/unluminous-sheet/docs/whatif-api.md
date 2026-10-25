# The what if, protection, named style and theme lane: what the engine offers

Every method is on `edit::Session` unless it says otherwise. A method that changes the workbook is one undo
step, refuses bad input with an `Err(String)` worded as Excel words it, and leaves the workbook as it was when
it refuses. Sheets are 0 based indexes, cells are `CellRef`, ranges are `Range`.

## Goal Seek (`edit/whatif.rs`, arithmetic in `whatif.rs`)

| Method | What it does |
|---|---|
| `goal_seek(sheet, set: CellRef, to: f64, by: CellRef) -> Result<GoalSeekResult, String>` | Change `by` until the formula in `set` comes to `to`, taking the steps Excel takes and stopping within `calc.attrs["iterateDelta"]` (0.001 unless the workbook says otherwise). Leaves the last value tried in `by`, found or not. `GoalSeekResult { found, value, target, iterations }`. Undo puts the old value back. A UI that offers Cancel on "may not have found a solution" calls `undo()`. Errors: `Reference isn't valid.` |

`whatif::seek(x0, goal, tolerance, f)` is the same arithmetic over a closure, for tests.

## Data tables

| Method | What it does |
|---|---|
| `create_data_table(sheet, range, row_input: Option<CellRef>, col_input: Option<CellRef>) -> Result<(), String>` | `Range.Table`. `range` includes the header row and column. Both inputs: two variables, formula in the top left corner. Only `row_input`: values along the first row, formulas down the first column. Only `col_input`: values down the first column, formulas along the first row. Fills the results. Errors: `Input cell reference is not valid.`, `Cannot change part of a data table.` |
| `data_table_at(sheet, at) -> Option<&DataTable>` | The table a cell is a result of. `DataTable::formula()` is `TABLE(B2,B1)`, `TABLE(,B1)` or `TABLE(B2,)` as Excel shows it. |
| `remove_data_table(sheet, at) -> Result<(), String>` | Leave the results as plain values. |

Tables compute inside every recalculation (`calc.rs`), so editing an input cell or a header recomputes them,
and formulas that read the results follow. Editing part of a table, or clearing only part of it, is refused with
`Cannot change part of a data table.`; clearing the whole of it removes the table.

## Scenario Manager

| Method | What it does |
|---|---|
| `scenarios(sheet) -> Scenarios` | The sheet's scenarios: `list`, `current`, `show`, `results`. |
| `add_scenario(sheet, name, changing: &[Range], values: Option<&[Value]>, comment: Option<&str>, user, locked, hidden)` | Scenario Manager, Add. Values default to what the cells hold. Comment defaults to `Created by <user> on <M/D/YYYY>`. At most 32 cells. |
| `edit_scenario(sheet, name, new_name, changing, values, comment, user)` | Edit. Without a comment Excel appends `Modified by <user> on <date>` to the old one. Makes the edited scenario `current`. |
| `delete_scenario(sheet, name)` | Delete. |
| `show_scenario(sheet, name)` | Show: writes the values into the cells, sets `current` and `show`. |
| `scenario_summary(sheet, results: &[Range]) -> Result<usize, String>` | The standard summary report: a new sheet `Scenario Summary` (`... 2`) before this one, made active, laid out as Excel lays it out (changing cells, result cells, current values, gray fill, outline, hidden comment row, notes). Names are used for labels where a defined name is exactly one cell. |

A locked scenario on a sheet whose scenarios are protected is refused. `Sheet::scenarios()` and
`Sheet::set_scenarios()` read and write the stored element.

## Protection (`edit/protect.rs`, files in `protect.rs`)

| Method | What it does |
|---|---|
| `protect_sheet(sheet, password: Option<&str>, options: &SheetProtection) -> Result<(), String>` | Protect Sheet. `SheetProtection { sheet, objects, scenarios, allow: Allow { format_cells, ... , select_locked_cells, select_unlocked_cells }, .. }`; `SheetProtection::default()` is the dialog as it opens. The password is hashed with a random salt, SHA-512, 100,000 spins, and never kept. |
| `unprotect_sheet(sheet, password)` | Wrong password: `The password you supplied is not correct. Verify that the CAPS LOCK key is off and be sure to use the correct capitalization.` |
| `sheet_protection(sheet) -> Option<SheetProtection>` | |
| `add_edit_range(sheet, title, ranges, password)` / `remove_edit_range(sheet, title, password)` / `unlock_edit_range(sheet, title, password)` / `edit_ranges(sheet)` | Allow Edit Ranges. Ranges without a password are editable at once; with one, after `unlock_edit_range` until the sheet is protected again. Added and removed only while the sheet is unprotected. |
| `protect_workbook(password, lock_structure, lock_windows)` / `unprotect_workbook(password)` / `workbook_protection()` | Protect Workbook. While the structure is locked, adding, deleting, renaming, moving, copying, hiding and colouring sheets are refused with `Workbook is protected and cannot be changed.` (`allow_structure_change()`). |
| `allowed(sheet, Guard) -> Result<(), String>` | Whether a change is allowed on a protected sheet, with Excel's message. Use it to grey a command out. `Guard` is `Write(&[Range])`, `Format`, `FormatColumns`, `FormatRows`, `InsertRows`, `InsertColumns`, `DeleteRows(Range)`, `DeleteColumns(Range)`, `Sort(Range)`, `Hyperlink(Range)`, `Objects`, `Filter`, `Pivot`, `Scenarios`, `Table`, `Forbidden`. |
| `can_select(sheet, cell) -> bool` | Whether `selectLockedCells` and `selectUnlockedCells` allow selecting the cell. |

**Every method that changes a sheet calls `allowed` first.** A new `Session` method that changes cells, formats,
rows, columns, objects, filters or tables must do the same with the `Guard` that names it; the matrix in
`tests/fixtures/protection/protect-matrix.tsv` says which flag governs which operation. `pivotTables` is in
the model and in `Guard::Pivot` for the pivot lane to call.

`crate::protect` has `SheetProtection`, `ProtectedRange`, `WorkbookProtection`, `Secret` (`new`, `with_random_salt`,
`matches`), `legacy_hash` and the messages. The legacy 16 bit `password` is read, checked and written back only
for an element that had one.

## Named cell styles (`edit/cellstyles.rs`, rules in `cellstyles.rs`)

| Method | What it does |
|---|---|
| `style_gallery() -> Vec<StyleInfo>` | The 47 built in styles (defined in the workbook or not) then the custom ones. |
| `style_of(sheet, cell) -> String` | `Range.Style.Name`. |
| `style_includes(name) -> Option<Includes>` | Number, Font, Alignment, Border, Fill, Protection. |
| `apply_style(sheet, ranges, name)` | A built in style enters the workbook on first use, in the workbook's theme fonts. The cell takes the style's formatting for each group it includes and points at the style (`xfId`). Needs `formatCells` on a protected sheet. |
| `new_style(name, from: Option<(sheet, cell)>, Includes)` | New Cell Style from a cell's format. |
| `modify_style(name, &[StyleChange], Option<Includes>)` | Cells that use the style change in each group they do not format themselves. A built in style becomes `customBuiltin`. |
| `delete_style(name)` | Cells return to Normal. A built in style is hidden, a custom one removed. Normal: `You can't delete the Normal style.` |
| `merge_styles(&Workbook) -> Result<usize, String>` | Merge Styles: same name is replaced. |

`StyleChange::CellStyle(index)` applies a style through `set_style`, so whole rows and columns work. The apply
flags of a cell format are worked out on writing, as Excel writes them: a flag is on when the value differs from
the style's.

## Themes (`edit/theme.rs`, data in `theme.rs` and `themes/`)

| Method | What it does |
|---|---|
| `apply_theme(name)` | 11 built in themes (`theme::BUILT_IN`): Office, Office Theme, Office 2013 - 2022 Theme, Facet, Retrospect, Slice, Wisp, Gallery, Frame, Dividend, Modulo. |
| `apply_theme_colors(name)` / `apply_theme_fonts(name)` | A scheme of Page Layout, Colors (23: `theme::color_scheme_names()`) or Fonts (25: `font_scheme_names()`), or the half of a theme. |
| `set_theme_colors(&[(slot, 0xRRGGBB)], name)` / `set_theme_fonts(major, minor)` | By hand. Slot 0 is Dark 1, 4 is Accent 1, 10 is Hyperlink (`theme::SLOTS`, `SLOT_NAMES`). |
| `theme_colors()` / `theme_fonts()` / `theme_xml()` | What the workbook has. |

Changing the theme redraws every theme colour reference (`Color::Theme`) and renames every font that follows the
theme (`scheme="minor"` or `"major"`), and drops `defaultThemeVersion` from `workbookPr`, which Excel does.

## Files

`xlsx/read.rs` and `write.rs` read and write `cellStyleXfs` and `cellStyles` as `Styles::style_xfs` and
`Styles::cell_styles`. Sheet protection, edit ranges and scenarios stay as kept XML on the sheet and workbook
(`SheetRaw.elements`, `Workbook.raw`), so a structural edit moves their references with the registry in
`refshift.rs`; the typed forms read and write that XML. The writer also names a sheet made in the session so that
it cannot collide with the part of an existing sheet.
