# Charts: the engine API for the window and the verbs

Written for the coordinator who wires the pointer, the command tree and the verbs. Everything is in
`unluminous-sheet`; the drawing is in `unluminous-app`. Nothing here reads the window.

## The model

`model::Chart` keeps what most code needs (`from`, `to`, `kind`, `title`, `series`, `part`, `new`) and has one
new field, `spec: chartspec::ChartSpec`, with everything else:

- `spec.groups: Vec<Group>`: the chart groups of the plot area (a column group, a line group on the secondary
  axes, a pie). `Group.kind`, `three_d`, `horizontal`, `grouping`, `gap_width`, `overlap`, `first_slice_angle`,
  `hole_size`, `axes` (ids), `labels`, the line elements, bubble, scatter and radar styles, pie of pie options.
- `spec.axes: Vec<Axis>`: `kind`, `position`, `min`, `max`, `log_base`, `reverse`, gridlines, `title`,
  `num_fmt`, tick marks, `tick_labels`, `crosses`, `major_unit`, `minor_unit`, date axis units.
- `spec.legend`, `spec.data_table`, `spec.plot_visible_only`, `spec.blanks`, `spec.style`, `spec.title`
  (formatting; the words are `Chart.title`, `=formula` when it follows a cell), `spec.name`, `spec.alt_text`.
- `spec.ex: Option<ExSpec>`: a `chartEx` chart (treemap, sunburst, histogram, Pareto, box and whisker,
  waterfall, funnel, map): per series layout, binning, subtotals, quartile method, parent labels.
- `spec.dirty`, `spec.moved`: set by the Session methods. A chart read from a file and left alone is written
  back byte for byte; a dirty one is written from the model with everything the model does not name kept as
  the XML it was; a moved one has its anchor rewritten.

`ChartSeries` gained `sizes` (bubble sizes), `group` (index into `spec.groups`) and `fmt: SeriesFormat`
(labels, trendlines, error bars, point formats, marker, smooth, explosion, `filtered`, `filtered_categories`).
`ChartSeries::new(name, categories, values)` builds a plain one. `Chart::chart_type() -> ChartType` is the
precise type (`ChartType::ALL` lists the 60 types of Insert Chart in Excel's order, `name()` and `parse()`
give the verb word, `excel_number()` the `XlChartType`). `Chart.kind` is the coarse family and is kept in step
by `sync_kind()`.

`Sheet.chart_removed: Vec<String>` lists chart parts read from the file whose charts were deleted or moved.
A chart sheet is a `Sheet` whose `kept_part` is a chart sheet (`xlsx::chart_drawing::REL_CHART_SHEET`) and
whose `charts` holds its one chart; `charts` of a chart sheet read from a file are read too.

`add_chart(sheet, chart)` and `delete_chart(sheet, index)` stay, and now complete a chart given only a coarse
`kind` and delete charts read from files.

## Session methods (`edit::charts`)

All are one undo step, take `sheet` (a worksheet or a chart sheet) and `index` (the chart on it), and refuse
with Excel's wording leaving the workbook unchanged.

| Method | Does |
|---|---|
| `insert_chart(sheet, range, ChartType, title) -> usize` | Insert Chart over a range: series laid out as Excel lays out a selection (header row, label column, direction with more points; date labels give a date axis; scatter and bubble take X from the first column). |
| `insert_chart_sheet(sheet, range, ChartType) -> usize` | F11: a new chart sheet. |
| `recommended_charts(sheet, range) -> Vec<ChartType>` | The five charts offered for the data's shape. |
| `change_chart_type(sheet, index, ChartType)` | Change Chart Type for the whole chart. |
| `change_series_type(sheet, index, series, ChartType)` | One series as another type (combination chart). |
| `set_series_secondary(sheet, index, series, bool)` | Secondary axis on or off. |
| `add_series(sheet, index, SeriesSource) -> usize`, `edit_series(.., series, SeriesSource)`, `remove_series(.., series)`, `move_series(.., series, later)` | Select Data. |
| `set_chart_source(sheet, index, range, Option<PlotBy>)` | Chart data range. |
| `switch_row_column(sheet, index)` | Switch Row/Column. |
| `set_hidden_and_empty_cells(sheet, index, show_hidden, Blanks)` | Hidden and Empty Cells. |
| `move_chart(sheet, index, from, to)` | Move and resize (cell and offset in EMUs for both corners). |
| `set_chart_name(sheet, index, name, alt_text)` | Selection pane name and alt text. |
| `move_chart_to_sheet(sheet, index, target) -> usize` | Move Chart, object in. |
| `move_chart_to_new_sheet(sheet, index, name) -> usize` | Move Chart, new sheet. |
| `set_chart_title(sheet, index, Option<String>, overlay)` | Chart Title. |
| `set_axis_options(sheet, index, AxisSelector, AxisOptions)` | Bounds, units, log base, reverse, crosses, number format, tick marks, labels, visibility, gridlines, title. |
| `set_legend(sheet, index, Option<LegendPosition>, overlay)` | Legend. |
| `set_data_table(sheet, index, Option<DataTable>)` | Data table. |
| `set_data_labels(sheet, index, Option<series>, show, LabelOptions)` | Data labels: value, category, series name, percentage, bubble size, legend key, position, separator, number format. |
| `add_trendline(.., series, Trendline) -> usize`, `edit_trendline(.., series, which, Trendline)`, `remove_trendline(.., series, which)` | Trendlines (linear, exponential, logarithmic, polynomial 2 to 6, power, moving average; forward, backward, intercept, equation, R squared). |
| `set_error_bars(.., series, ErrorBars)`, `remove_error_bars(.., series, Option<ErrorDirection>)` | Error bars (fixed, percentage, standard deviation, standard error, custom; plus, minus, both; cap). |
| `set_group_options(sheet, index, group, GroupOptions)` | Gap width, overlap, first slice angle, hole size, vary colours, pie of pie plot size and split, bubble scale, drop lines, high low lines, up down bars, series lines. |
| `set_series_format(sheet, index, series, Option<point>, SeriesOptions)` | Fill, line, marker, smooth, explosion for a series or one point. |
| `filter_series(sheet, index, series, hidden)`, `filter_categories(sheet, index, hidden)` | Chart Filters. |
| `apply_quick_layout(sheet, index, n)` | Quick Layout; `chartlayout::layout_count(type)` says how many (4 to 12). |
| `set_chart_style(sheet, index, n)` | Chart style 1 to 48. |

## The numbers a drawing needs (`plot`)

`plot::plot(&Workbook, sheet, &Chart, PlotSize) -> Plot` reads the series from the cells now and returns:
`series` (values, X, sizes, labels, stacking `base` and `top`, trendline fits with coefficients, R squared and
equation text, error bar amounts), `axes` (`scale::AxisScale` min, max, major, minor, log; ticks with their
text; categories; where the crossing axis sits) and, for `chartEx` charts, `extended` (histogram bins,
Pareto rows with cumulative share, box statistics, waterfall bars, funnel items, treemap and sunburst
leaves). `plot::scale::value_scale`, `plot::trend::fit`, `plot::stats::{histogram, pareto, box_stats,
waterfall, error_amounts}` are usable on their own. `chartdata::{resolve, parse_refs, shape_of, layout_series,
recommend}` read formulas and lay out selections.

## Drawing (`components::spreadsheet::chart_paint`)

- `draw(painter, area, &Plot, &Chart, &Colours, zoom)`: the whole chart (title, legend, axes, every type).
  `chart.rs::show` already calls it for each chart; `show` answers the chart pressed as before.
- `geometry::{bar_slot, along, band_centre, column, bar, slices, on_circle, treemap}`: placing bars, points,
  slices and treemap cells with no window; unit tested.
- `fill_of(&sp_pr, &accents)`, `shade`, `lighten`, `Colours::series(i)`: colours from a chart's formatting.

Pointer handling to wire: dragging a chart should call `move_chart`, a corner drag the same with a changed
`to`; the right click menu should offer the Session methods above; `chart.rs::rect_of` gives the rectangle.

## Files written

`xlsx/chart_read.rs` reads a `chart` part, `xlsx/chart_write.rs` writes one, `xlsx/chartex.rs` reads and
writes `chartEx`, `xlsx/chart_drawing.rs` reads anchors and rebuilds a drawing, `xlsx/write/charts.rs` is the
writer's chart half. Excel tools are in `tools/excel-oracle/charts-*.ps1`.
