# Clipboard and fill API (task-2360, lane clipboard)

All in `unluminous_sheet::edit` (`edit::clipboard` for the types). Every method is one undo step and refuses with an Excel worded `Err(String)`.

Paste
- `Session::paste_special(&Clip, sheet, to: Range, &PasteSpecial) -> Result<Range,String>`: `PasteSpecial { kind: PasteKind (12 kinds, Excel codes), operation: PasteOperation, skip_blanks, transpose }`.
- `Session::paste(clip, sheet, to, PasteMode)`: the old entry, now calls the above.
- `Session::paste_link(&Clip, sheet, at) -> Result<Range,String>`.
- `Session::insert_copied(&Clip, sheet, at, Shift::Down|Right) -> Result<Range,String>`: copied or cut cells.
- `Session::copy(sheet, range, cut) -> Clip`: carries notes, validation, conditional formats, merges, links, widths, formats; a filtered range copies visible rows only. `Clip::to_text(&Workbook)` is Excel's text for other programs (hidden rows and columns left out).
- `Session::paste_text(sheet, at, text, ExternalPaste{formulas})`, `Session::paste_html(sheet, at, html, ExternalPaste)`, `parse_text_grid(text)`. Cells starting `=` are text with a quote prefix unless `formulas`.

Fill
- `fill_series(sheet, source, target)` and `fill_series_with(sheet, source, target, FillKind, &CustomLists)`; `FillKind` has Excel's `XlAutoFillType` codes (0..10) and `parse`.
- `fill_series_with_control(...)`: Ctrl+drag. `fill_handle_extent(sheet, source) -> Option<Range>`: double click.
- `data_series(sheet, range, &SeriesSpec{by_rows, kind: Linear|Growth|Date(Day|Weekday|Month|Year)|AutoFill, step, stop, trend})`.
- `fill_across_sheets(sheet, range, &[usize], AcrossSheets::All|Contents|Formats)`.
- `justify(sheet, range) -> Result<Range,String>`.
- `flash_fill(sheet, output_column_range)`, `flash_fill_down(sheet, at)` (Ctrl+E), `flash_fill_examples(examples, rows)`.
- `CustomLists` (Excel's four lists by default; `add`, `remove`, `lists`).

Measured against Excel: pasting into a filtered range writes hidden rows too (only a copy from a filtered range leaves hidden rows out).
