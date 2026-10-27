# task-2382: the spreadsheet tab in the design system, and the Excel conveniences it lacked

## What was asked

> Our excel plugin isn't styled like the rest of the app. Look at the styles and themes of the rest of the
> app, including our Word plugin. Create a world class design and UX that matches our design.

And, while the ticket was being worked:

> our ui needs to have similar convenience UX like excel. For example, when I enter "=sum(" I should be able
> to drag a whole column of cells, but currently it only selects one at a time. Research online for other UX
> for excel, use our excel as an oracle, and fully implement.

## Part 1: Excel's conveniences

### The fault behind `=SUM(` and a drag

`grid::read_the_pointer` started Point mode on the press by putting the clicked cell in `GridOut::point`,
and on each later frame of the drag made the range from `out.point`'s first cell. `GridOut` is built new
every frame, so `out.point` was `None` on every frame after the press and the drag changed nothing. The cell
the drag started from now lives in `Drag::Point(CellRef, PointOver)`, which lasts as long as the drag.

### Point mode, as Excel has it

| Gesture | What it does | Where |
| --- | --- | --- |
| Drag over cells | the whole range, `=SUM(B2:B4` | `grid::start_pointing`, `Drag::Point` |
| Press or drag on column or row headers | whole columns or rows, `=SUM(B:C` | `PointOver::Columns`, `Rows` |
| Shift and a click | grows the reference pointed at from where it started | `PointHow::Again` |
| Ctrl and a click | adds another reference after a comma, `=SUM(B2:C3,A2` | `PointHow::Add` |
| Arrow, Shift and arrow | moves or grows the reference (was there) | `keys::typing` |
| Ctrl and arrow, Ctrl+Shift and arrow | to the edge of the data, as the grid's own Ctrl and arrow | `keys::typing`, `edge_of_data` |
| A click on another sheet's tab | the formula stays open in the formula bar and gets `Summary!`; a click on a cell there finishes `Summary!A1`; Enter goes back to the formula's own sheet | `point_on_another_sheet`, `keys::pointing_prefix` |
| Drag a reference's coloured outline by its border | carries the reference; `$` marks and the sheet's name are kept | `Drag::Finder`, `formula_text::replace_reference` |
| Drag a corner square of the outline | resizes the reference with the opposite corner kept | `editor::FinderGrip::Corner` |

The references in the formula being typed are coloured like their outlines, in the cell and in the formula
bar (`editor::coloured_formula`). Whole column and row references (`B:C`, `3:5`) are now read as
references, so they are outlined and coloured too.

### The other conveniences

| Keys | What Excel does | Before | Now |
| --- | --- | --- | --- |
| Tab, Tab, Enter | Enter goes to the next row of the column the Tabs started in | Enter went down from the last cell | `View::tab_start`, `keys::back_to_where_the_tabs_started`, in the grid and while typing |
| Alt+= on one cell | AutoSum's guess opens in the cell with the range pointed at; Enter takes it, an arrow or a drag changes it | wrote the formula at once | `keys::start_autosum` |
| Alt+= on a block | a total under each column, in the block's empty last row or the row under it; across for one row | one total in the active cell | `actions::autosum_totals` |
| Ctrl+Shift+A after a function's name | writes the argument names, `=SUM(number1,[number2],...)` | nothing | `keys::typing`, `formula_text::argument_names` |
| F9 while typing | the chosen part, or all of it, replaced by its value | nothing | `keys::evaluate_part` |
| Formula AutoComplete | functions and defined names, starting with the letters first, then holding them | functions starting with the letters | `editor::offered_names`; never for a cell address such as `C2` |

`plugins run spreadsheet where <cell>` says where a cell was drawn, so a test or an agent can aim the
pointer, and `plugins view spreadsheet` says the cell mode (`Ready`, `Enter`, `Edit`, `Point`).

A cell being typed into takes the keyboard back if its box lost it while the tab has the keyboard and no
other box took it. Found by the Ctrl+Shift+A test: afterwards Escape and Enter reached nothing.

## Part 2: the design

The spreadsheet tab is drawn with the parts the Word tab already uses (`task-2366`), from the Claude Design
canvas "Unluminous". `components::word::kit` is shared by both tabs; nothing in it is about Word. Every size
is the design's points times the tab's own zoom (`Look::zoom`), never the editor's font.

### The bar (44 points, the pane ground, a groove under it)

1. **Name box**: a carved well 132 points wide and 30 tall, round ends, the address placed by its capitals.
   When the workbook has defined names a chevron at its right opens them in a menu card; a row chooses the
   name's range, as Excel's name box list does.
2. **Insert function**: a round chip with Lucide's `square-function` mark, opening Insert Function in the
   panel (Shift+F3).
3. **Formula well**: a carved well with round ends taking the rest of the bar. While the formula bar is typed
   into it carries a ring in the accent. Its references are coloured as in the cell.
4. **Format** (`Palette`) and **Workbook menu** (Lucide `menu`): round chips at the right, carved in while
   what they open is open.

### The sheet strip (40 points, the pane ground, a groove over it)

A round chip with `Plus` adds a sheet. The sheets are pills in one carved track, the sheet showing raised
out of it, as the realm bar and the Database pane's pages are. A sheet's tab colour is a dot before its
name. A double click renames in place; a drag along the strip moves the sheet.

### The window's status bar

The mode and the figures for the selection move to the window's status bar through
`UiProvider::tab_status`, as the Word tab's page and words did: `Workbook` (or `CSV file`), then `Ready`,
`Average 3.25`, `Count 3`, `Sum 9.75`, and the zoom. Pressing a figure copies its number
(`plugins run spreadsheet copy-figure sum`); pressing the zoom sets it back to 100%.

### The Format panel (320 points)

The head is the Word panel's: a back chip when deeper than the root, the level's name in 15 point semibold
with the path above it, a close chip. The root's levels are raised rows with a mark in a carved disc, the
name and the current value for the chosen cells, and a chevron. A carved search well finds any command.
Deeper levels keep their menu rows. Marks: Number `hash`, Font `type`, Fill `paint-bucket`, Borders `grid`,
Alignment `text-align-start`, Styles `heading`, Cell size `ruler` (as columns), Rules `palette`, Data
`database`, Sort and Filter `filter`, and a list mark for the rest.

### Headers and the grid

The column and row headers are drawn on the pane ground with hairlines in the divider colour at 60%, the
letters at 10.5 points placed by their capitals. A chosen header carries a wash of the accent and a 2 point
accent bar on the side facing the cells, as before.

### Popups over the grid

The input message, the note and the argument hint are menu cards (`controls::menu_frame`) with the design's
ink.

## Tests

- `tests/spreadsheet.rs`: the drag points at B2:B4, whole columns from the headers, Shift and Ctrl clicks;
  Tab and Enter; AutoSum in Point mode and on a block; Ctrl+Shift+A, F9, Ctrl+Shift+Down; the range finder
  moves and resizes; pointing on another sheet.
- `services::spreadsheet::tests`: AutoSum totals, the function list's order.
- `formula_text` tests: whole column references, `replace_reference`, argument names and sheet prefixes.
- Every spreadsheet picture is taken again and looked at.
