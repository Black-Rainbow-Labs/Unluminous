# Objects API: pictures, shapes, sparklines

Written for the coordinator who wires the pointer, the command tree and the verbs. Everything here is in
`unluminous-sheet` unless it says `unluminous-app`. Every `Session` method is one undo step, refuses what Excel
refuses with the workbook unchanged, and takes the sheet index first. `ObjectId` is the `id` of the object's
`cNvPr` (`u32`), unique on a sheet. Drawing units are EMU (914,400 to the inch, 12,700 to the point, 9,525 to
the pixel). `Rect { x, y, w, h }` in `objects::geometry` is from the top left corner of the sheet.

## Model (`unluminous_sheet::objects`)

- `Sheet::drawing: Drawing { objects: Vec<DrawingObject>, changed, char_px }`. `objects` is the stacking order,
  bottom first. Charts are not in it (they stay XML in the drawing part). `Sheet::pictures()` still answers the
  old `Picture { from, to, part }` list for the existing grid code.
- `DrawingObject`: `id`, `name`, `descr` (alt text), `hidden`, `kind: ObjectKind`, `anchor: Anchor`, `rotation`
  (degrees clockwise), `flip_h`, `flip_v`, `lock_aspect`, `locked`, `prints`, `picture: Option<PictureData>`,
  `shape: Option<ShapeData>`, `children` (a group), `fallback` (the stored picture of a kept object), `tree`.
  `frame_rect(sheet)` is the unturned frame (Excel's Left, Top, Width, Height); `bounds(sheet)` is the box round
  it turned.
- `ObjectKind`: `Picture, Shape, TextBox, Connector, Group, FormControl, SmartArt, WordArt, Icon, Equation,
  OleObject, Model3D, Other`. The kinds from `FormControl` on are kept byte for byte, located by their anchor, and
  drawn from `fallback` or as a card (`ObjectKind::label()`).
- `Anchor { kind: TwoCell | OneCell | Absolute, placement: MoveAndSize | Move | Fixed, from, to, pos, ext }`.
  `Anchor::rect(sheet)` and `set_rect(sheet, rect)` convert with the sheet's column widths and row heights
  (`objects::geometry`; a character of the default font is 7 pixels for Calibri and 8 for Aptos Narrow, as Excel
  measured).
- `ShapeData { preset, adjust, fill: Fill, line: Line, text: Option<TextBody>, style_* }`; `Fill::{Inherit, None,
  Solid(ShapeColor), Other}`; `ShapeColor::resolve(theme) -> (r, g, b, a)`.
- Sparklines: `Sheet::sparklines: Vec<SparklineGroup>` (`objects::sparkline`). A group has `kind`, `weight`,
  `empty`, `markers/high/low/first/last/negative`, `show_axis`, `hidden_cells`, `right_to_left`, `min/max:
  AxisBound::{Automatic, SameForAll, Custom(f64)}` (+ the kept `min_value/max_value`), `date_axis`, `colors`,
  `sparklines: Vec<Sparkline { data, location }>`.
- `sparkline::sparkline_data(book, sheet, group, index) -> Option<SparklineData>`: `values` (`None` for a gap),
  `axis_min`, `axis_max`, `baseline`, `high/low/first/last` indexes, `negatives`; `fraction_down(v)` and
  `bar_span(i)` give positions down the plot. The automatic axis runs from the lowest value to the highest (a
  flat series sits at the top of an axis one unit tall); this was measured against Excel's drawing of column
  sparklines (`tests/sparkline_scale.rs`).
- `richdata::cell_image(book, sheet, cell) -> Option<part>`: the image part a cell shows when a picture was placed
  in it.

## Session methods (`edit/objects.rs`, `edit/sparklines.rs`)

Pictures
- `insert_picture(sheet, bytes, at: CellRef) -> Result<ObjectId>`: PNG, JPEG, GIF or BMP bytes, natural size.
- `paste_picture(sheet, bytes, at)`: the clipboard's image bytes (same as a file).
- `insert_svg_picture(sheet, svg, png_fallback, at)`: an SVG with the PNG that stands in for it.
- `crop_picture(sheet, id, Crop { left, top, right, bottom })`: thousandths of a percent; the picture keeps its
  scale and its frame shrinks, as Excel's crop does.
- `reset_picture(sheet, id, size: bool)`: remove the crop, and with `size` restore the natural size.
- `place_in_cell(sheet, id, at)`: Picture > Place in Cell. Writes the rich value parts Excel writes; the cell must
  be empty. `cell_image(sheet, at)` reads it back.

Shapes (`objects::make::NewShape`: `Rectangle, RoundedRectangle, Ellipse, Line, Arrow, TextBox`)
- `insert_shape(sheet, kind, rect: Rect) -> ObjectId`
- `set_shape_fill(sheet, id, Fill)`, `set_shape_line(sheet, id, Line)`, `set_shape_transparency(sheet, id, percent)`
- `set_shape_text(sheet, id, text)`, `format_shape_text(sheet, id, TextFormat { size, bold, italic, underline,
  color, font, align, valign })`

Every kind of object
- `move_object(sheet, id, dx, dy)`, `set_object_frame(sheet, id, Rect)`, `resize_object(sheet, id, w, h, keep_aspect)`
- `set_object_anchor(sheet, id, Anchor)`: the pointer path. The window works out cell and offsets in the columns it
  draws (`unluminous-app` `anchor_for_screen_rect`) and sets them, so a drag lands where the pointer was.
- `rotate_object(sheet, id, degrees)`, `flip_object(sheet, id, horizontal)`
- `set_alt_text`, `set_object_name`, `set_placement(Placement)`, `set_aspect_lock`, `delete_object`
- `reorder_object(sheet, id, ZOrder::{ToFront, Forward, Backward, ToBack})`
- `align_objects(sheet, &ids, Alignment::{Left, Centre, Right, Top, Middle, Bottom})`
- `distribute_objects(sheet, &ids, Distribution::{Horizontally, Vertically})` (three or more)
- `group_objects(sheet, &ids) -> ObjectId`, `ungroup_object(sheet, id) -> Vec<ObjectId>`

Sparklines
- `insert_sparklines(sheet, kind, data_sheet, data: Range, location: Range) -> group index`: a column of locations
  takes a row of data each, a row of locations a column each, one cell the whole range; refuses a location that
  already holds one.
- `sparkline_at(sheet, cell) -> Option<(group, index)>`
- `update_sparkline_group(sheet, group, |g| ..)` and the named setters `set_sparkline_kind`,
  `set_sparkline_points(SparklinePoints)`, `set_sparkline_colors(SparklineColors)`, `set_sparkline_weight`,
  `set_sparkline_axis(min, max)`, `set_sparkline_horizontal_axis(show, right_to_left)`,
  `set_sparkline_date_axis(Option<&str>)`, `set_sparkline_hidden_and_empty(EmptyCells, hidden)`,
  `set_sparkline_data(sheet, cell, data_sheet, Range)`
- `group_sparklines(sheet, &cells)`, `ungroup_sparkline(sheet, cell)`, `clear_sparklines(sheet, Range)`,
  `clear_sparkline_group(sheet, cell)`

A structural edit moves anchors (an object that moves but does not size keeps its size; one that does neither
stays), sparkline data ranges (also on a sheet rename) and sparkline locations (a sparkline whose cell is deleted
goes). `refshift.rs` does this on the model; the registry rows for `from`, `to`, `f` and `sqref` stay because
charts and other kept XML still use them.

## Drawing and pointer helpers (`unluminous-app` `components/spreadsheet`)

`objects.rs`
- `draw_sheet_objects(ui, book, frame, ink, chosen: Option<ObjectId>)`: the once a frame loop (replaces
  `chart::show_pictures`). Loads textures, draws objects bottom first, then the selection on `chosen`.
- `draw_object(painter, frame_rect, object, &DrawContext)`: one object into the rectangle its unturned frame has.
- `draw_selection(painter, frame, degrees, accent, ground)`: outline, eight handles, turning handle on its stem.
- `object_frames(frame, book) -> Vec<(ObjectId, Rect, rotation)>`: bottom first, for the helpers below.
- `hit_test(frames, p) -> Option<ObjectId>`: the topmost object under a point (turned frames honoured).
- `handle_at(frame, degrees, p, reach) -> Option<Handle>`; `Handle::{NorthWest, North, NorthEast, East, SouthEast,
  South, SouthWest, West, Rotate}` with `cursor()`; `handle_positions(frame, degrees)`.
- `resize_by_handle(frame, degrees, handle, pointer_delta, keep_aspect) -> Rect`: the opposite side stays put,
  also for a turned frame.
- `rotation_from_pointer(centre, p, snap) -> degrees`: 0 is straight up; `snap` goes to 15 degrees.
- `screen_rect(frame, book, &Anchor) -> Rect` and `anchor_for_screen_rect(frame, book, &like, rect) -> Anchor`.

`sparkline.rs`
- `draw_sparklines(ui, book, frame, ink)`: the once a frame loop.
- `draw_sparkline(painter, cell, &SparklineData, &SparklineGroup, theme, zoom, ink)`, with the geometry in
  `plot_points` and `bar_rects`.

Wiring suggested for `grid.rs`: call `draw_sheet_objects` where `show_pictures` is called, `draw_sparklines`
after the cell text; on a press, `handle_at` for the chosen object first, then `hit_test`; a drag of a handle
gives `resize_by_handle`/`rotation_from_pointer`, and the result goes through `anchor_for_screen_rect` and
`set_object_anchor` (or `rotate_object`) when the pointer is released.

## Excel checks

`tests/objects.rs`, `tests/picture_in_cell.rs` and `tests/sparkline_scale.rs`, with fixtures in
`tests/fixtures/objects/` and the COM scripts `tools/excel-oracle/objects-inspect.ps1` (what Excel sees of every
shape and sparkline group), `place-in-cell.ps1` and `sparkline-pixels.ps1`.
