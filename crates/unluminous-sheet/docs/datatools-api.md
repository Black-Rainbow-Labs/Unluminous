# Data tools: the `Session` methods

All of these are in `crates/unluminous-sheet/src/edit/datatools/` and re-exported from `unluminous_sheet::edit`.
Every method is one undo step, answers `Err(message)` with Excel's wording on bad input and leaves the
workbook unchanged when it refuses. `sheet` is the sheet index and `Range` is `unluminous_sheet::coord::Range`.

## Sort

- `sort(sheet, range, key, descending, header)` sorts by one column (the older call, kept).
- `sort_levels(sheet, range, &[SortLevel], &SortOptions)` sorts by several levels with no colour or icon lookups.
- `sort_with(sheet, range, &[SortLevel], &SortOptions, &dyn CellDisplay)` sorts by values, cell colour, font colour
  or icon; `CellDisplay` is how the app supplies the colours and icons that conditional formats give a cell.
- `guess_header(sheet, range, left_to_right) -> bool` is Excel's rule for whether the first line is a header.
- `add_differential_format(Dxf) -> u32` returns the index of an equal differential format, adding one if needed.

Types: `SortLevel`, `SortOn`, `SortOptions` (`header: HeaderMode`, `case_sensitive`, `left_to_right`, `remember`),
`HeaderMode` (`Guess`, `Yes`, `No`), `CellDisplay`, `DirectFormats` (the default `CellDisplay`).

## Filters

- `set_filter(sheet, Option<Range>)` turns the sheet's AutoFilter on over a range or off.
- `filter_column(sheet, column, FilterCriteria)` and `filter_column_with(.., &dyn CellDisplay)` set one column's
  criteria and apply them; `FilterCriteria` has `Values`, `Dates`, `Custom`, `Top10`, `Dynamic`, `Color`, `Icon`.
- `apply_filter(sheet)` and `apply_filter_with(sheet, &dyn CellDisplay)` are Reapply.
- `clear_filter(sheet)` shows every row and removes the criteria.
- `table_filter_column(name, column, FilterCriteria)`, `table_filter_column_with(..)` and `apply_table_filter(name)`
  do the same for the filter of a table.
- `advanced_filter(sheet, list, criteria, copy_to, unique)` is Advanced Filter in place or to another range.
- `show_all_rows(sheet)` shows every row hidden by an Advanced Filter.

## Remove Duplicates, Text to Columns, Consolidate

- `remove_duplicates(sheet, range, columns, header: bool) -> usize` and
  `remove_duplicates_with(.., HeaderMode)` answer how many rows went.
- `text_to_columns(sheet, range, delimiter)` splits at one delimiter. `text_to_columns_with(sheet, range,
  &TextToColumns)` takes `SplitMode` (`Delimited(Delimiters)` or `FixedWidth(Vec<usize>)`), a `ColumnFormat` per
  column (`General`, `Text`, `Date(DateOrder)`, `Skip`) and a destination.
- `consolidate(sheet, &Consolidation)` combines sources with a `ConsolidateFunction`, labels from the top row or
  left column, and with `create_links` writes outlined link rows and `SUM`, `AVERAGE` and similar summary rows.
  `book_name` is the workbook name written beside linked rows.

## Tables

- `add_table(sheet, range, name)` and `add_table_with(sheet, range, name, &TableOptions)` create a table, with or
  without a header row and with a style.
- `table_style(name)` and `set_table_style(name, &TableStyle)`: style name and the stripe and column options.
- `set_table_header_row(name, show)` and `set_table_totals_row(name, show)`.
- `table_total_function(name, column)` and `set_table_total_function(name, column, TotalFunction)`: `SUBTOTAL`
  101 to 111 and the `totalsRowFunction` value, or a custom formula.
- `resize_table(name, range)`, `table_to_range(name)`, `rename_table(name, new)`,
  `rename_table_column(name, column, new)`, `add_table_column(name, Option<&str>)`, `add_table_row(name)` and
  `delete_table_rows(name, first, count)`.
- `table_auto_fill: bool` on `Session` (default true): typing a formula in an empty table column fills the column
  and records `calculatedColumnFormula`. Typing in a header cell renames the column and the formulas that use it.

## Not covered by Excel checks

Sorting and filtering by icon cannot be set from Excel's COM interface in PowerShell, so those paths are covered by
unit tests with a stub `CellDisplay`. A table style is not copied into cell formats when a table becomes a range.
