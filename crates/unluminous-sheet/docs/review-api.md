# Review lane: engine API

Every method is on `unluminous_sheet::edit::Session` unless it says otherwise. Types live under
`unluminous_sheet::edit::review::<module>`. A change is one undo step, refuses with an `Err(String)` worded
as Excel words it, and leaves the workbook unchanged when it refuses. Methods that only read take `&self`.

## Threaded comments and notes (`review::comments`, `review::notebox`)

Model: `Sheet::threads: BTreeMap<(row, col), Thread>`, `Workbook::persons: Vec<Person>`, `Note::bounds: Option<NoteBox>`.
A cell has a note or a thread, never both.

| Method | What it does |
|---|---|
| `thread_at(sheet, at) -> Option<&Thread>` | The thread on a cell. |
| `thread_rows(sheet: Option<usize>) -> Vec<ThreadRow>` | The Comments pane: every thread, with cell, author name, first text, reply count, resolved. |
| `add_thread(sheet, at, text, author) -> Result<()>` | New threaded comment. Refuses an empty text, a cell with a note, a cell with a thread. |
| `reply_to_thread(sheet, at, text, author)` | Add a reply. |
| `edit_thread_message(sheet, at, index, text)` | Change a message; 0 is the first, 1 the first reply. Mentions that no longer read are dropped. |
| `resolve_thread(sheet, at, resolved: bool)` | Resolve or reopen. |
| `delete_thread_message(sheet, at, index)` | Delete a reply; index 0 deletes the whole thread. |
| `delete_thread(sheet, at)` | Delete the thread. |
| `add_mention(sheet, at, index, person_display_name, start_char)` | Record the `@Name` at that place as a mention. |
| `person_for(name) -> String`, `person_name(id) -> String` | People by display name and id. |
| `show_note(sheet, at, shown)` | Show or hide one note. |
| `show_all_notes(Option<sheet>, shown) -> Result<usize>` | Show or hide every note of a sheet or the workbook. |
| `neighbouring_note(sheet, from, forward) -> Option<CellRef>` | Next and Previous Note, reading order, wrapping. |
| `note_box(sheet, at) -> Option<NoteBox>` | The note's box (anchor and points), or the box Excel would give it. |
| `set_note_box(sheet, at, left, top, width, height)` | Place and size a note box in points; the VML anchor is worked out from the column widths and row heights. |
| `set_note_size(sheet, at, width, height)` | Resize keeping the top left corner. |
| `convert_notes_to_threads(sheet, Option<Range>) -> Result<usize>` | Convert Notes to Comments. |

`Geometry { sheet, digit_width }` (`review::notebox`) does the pixel and point arithmetic; `digit_width` is 7 for Calibri 11 at 100%.

## Find and Replace (`review::search`)

`FindOptions { what, look_in: LookIn::{Formulas, Values, Notes, Comments}, by_columns, match_case, entire_cell, format: Option<xf index>, scope: Scope::{Sheet(i), Workbook}, within: Option<Vec<Range>> }`,
`FindOptions::new(what, scope)` for the defaults.

| Method | What it does |
|---|---|
| `find_cells(&FindOptions) -> Vec<Hit {sheet, at}>` | Find All, in the order Find and Find Next visit. |
| `find_next(&FindOptions, after: Option<Hit>, backwards) -> Option<Hit>` | Find Next and Previous, wrapping. |
| `replace_cells(&FindOptions, with) -> Result<usize>` | Replace All; answers the cells changed. |
| `replace_cell(&FindOptions, with, Hit) -> Result<bool>` | The Replace button. |

Free functions: `wildcard_find`, `wildcard_replace`, `formula_text`, `shown_value`.

## Go To Special (`review::special`)

`special_cells(sheet, selection: &[Range], active: CellRef, Special) -> Result<Vec<CellRef>>` (reading order; `Err("No cells were found.")`).
`Special::{Notes, Comments, Constants(Kinds), Formulas(Kinds), Blanks, CurrentRegion, CurrentArray, RowDifferences, ColumnDifferences, Precedents{all_levels}, Dependents{all_levels}, LastCell, VisibleCells, ConditionalFormats{same}, Validation{same}}`.
`Kinds::ALL`. `drawing_objects(sheet) -> Vec<DrawingObject>` is the Objects option. `areas_of(&[CellRef]) -> Vec<Range>` groups cells into rectangles. `current_region(&Sheet, CellRef) -> Range`.

## Auditing (`review::trace`, `review::errors`, `review::evaluate`, `review::stats`)

| Method | What it does |
|---|---|
| `direct_precedents(sheet, at) -> Vec<Reach>` | Ranges the formula reads. `Reach { sheet, range, other_sheet }`; `other_sheet` marks a reference to another sheet. |
| `precedents(sheet, Range, all_levels) -> Vec<Reach>` | Precedents of some cells, one level or all. |
| `dependents(sheet, Range, all_levels) -> Vec<Reach>` | Formulas that read some cells. |
| `trace_error(sheet, at) -> Result<ErrorTrace>` | `sources` and `arrows` of Trace Error. |
| `check_errors(Option<sheet>, Option<Range>, ErrorOptions) -> Vec<ErrorFinding {sheet, at, rule}>` | Error Checking. `ErrorOptions::default()` is Excel's (empty cell rule off), `::all()` has every rule on. `ErrorRule::{number, message, attribute}`. |
| `ignore_error(sheet, Range, ErrorRule)`, `reset_ignored_errors(sheet)` | Ignore Error, Reset Ignored Errors (saved as `ignoredErrors`). |
| `evaluator(sheet, at) -> Result<Evaluator>` | Evaluate Formula. `Evaluator::{view(&Workbook), evaluate, step_in, step_out, restart, finish}`; `EvaluateView` has the text, the underlined character range and which buttons work. |
| `add_watch(sheet, at)`, `delete_watch(sheet, at)`, `watch_rows() -> Vec<WatchRow>` | Watch Window (saved as `cellWatches`). |
| `workbook_statistics() -> WorkbookStatistics` | Workbook Statistics. |

Create Names from Selection is not in this lane.
