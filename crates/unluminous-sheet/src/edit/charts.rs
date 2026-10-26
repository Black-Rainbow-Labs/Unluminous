//! Making and changing charts: every method is one undo step, refuses what Excel refuses with the
//! workbook unchanged, and marks a chart read from a file as changed so it is written from the model.
//!
//! The window and the `chart` verbs call these; none of them draws anything. See
//! `crates/unluminous-sheet/docs/charts-api.md` for the list.

use super::{Guard, Session};
use crate::chartbuild;
use crate::chartdata::{self, PlotBy};
use crate::chartspec::*;
use crate::coord::{CellRef, Range};
use crate::model::{Chart, ChartSeries, KeptSheet};
use crate::xlsx::chart_drawing::{CT_CHART_SHEET, REL_CHART_SHEET};

/// A change to a fill colour: `None` leaves it, `Some(None)` removes it.
pub type FillChange = Option<Option<(u8, u8, u8)>>;
/// A change to a line: its sRGB colour and width in points.
pub type LineChange = Option<Option<((u8, u8, u8), f64)>>;

/// Which axis of a chart an option is for. A scatter or bubble chart's category axis is its X axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisSelector {
    /// The horizontal axis of a column chart, the vertical one of a bar chart, or X.
    Category,
    /// The axis of numbers, or Y.
    Value,
    SecondaryCategory,
    SecondaryValue,
}

/// A change to an axis: `None` leaves a setting alone, and a nested `None` puts it back to automatic.
#[derive(Debug, Clone, Default)]
pub struct AxisOptions {
    pub minimum: Option<Option<f64>>,
    pub maximum: Option<Option<f64>>,
    pub major_unit: Option<Option<f64>>,
    pub minor_unit: Option<Option<f64>>,
    pub log_base: Option<Option<f64>>,
    pub reverse: Option<bool>,
    pub crosses: Option<Crosses>,
    pub number_format: Option<Option<(String, bool)>>,
    pub major_tick: Option<TickMark>,
    pub minor_tick: Option<TickMark>,
    pub tick_labels: Option<TickLabels>,
    pub visible: Option<bool>,
    pub major_gridlines: Option<bool>,
    pub minor_gridlines: Option<bool>,
    pub title: Option<Option<String>>,
    pub between: Option<bool>,
    pub label_skip: Option<Option<u32>>,
    pub mark_skip: Option<Option<u32>>,
    pub base_unit: Option<Option<TimeUnit>>,
}

/// A change to data labels.
#[derive(Debug, Clone, Default)]
pub struct LabelOptions {
    pub show_value: Option<bool>,
    pub show_category: Option<bool>,
    pub show_series: Option<bool>,
    pub show_percent: Option<bool>,
    pub show_bubble_size: Option<bool>,
    pub show_legend_key: Option<bool>,
    pub position: Option<Option<LabelPosition>>,
    pub separator: Option<Option<String>>,
    pub number_format: Option<Option<(String, bool)>>,
}

/// A change to a chart group's options.
#[derive(Debug, Clone, Default)]
pub struct GroupOptions {
    pub gap_width: Option<u32>,
    pub overlap: Option<i32>,
    pub first_slice_angle: Option<u32>,
    pub hole_size: Option<u32>,
    pub vary_colors: Option<bool>,
    pub second_plot_size: Option<u32>,
    pub split_by: Option<SplitBy>,
    pub split_at: Option<f64>,
    pub bubble_scale: Option<u32>,
    pub drop_lines: Option<bool>,
    pub high_low_lines: Option<bool>,
    pub up_down_bars: Option<bool>,
    pub series_lines: Option<bool>,
}

/// A change to how a series, or one of its points, looks.
#[derive(Debug, Clone, Default)]
pub struct SeriesOptions {
    /// An sRGB fill colour; `Some(None)` removes the fill.
    pub fill: FillChange,
    /// An sRGB line colour and its width in points; `Some(None)` removes the line.
    pub line: LineChange,
    pub marker_symbol: Option<String>,
    pub marker_size: Option<u32>,
    pub smooth: Option<bool>,
    pub explosion: Option<u32>,
    pub invert_if_negative: Option<bool>,
}

/// What a series reads: `Some(None)` takes a part away.
#[derive(Debug, Clone, Default)]
pub struct SeriesSource {
    pub name: Option<Option<String>>,
    pub categories: Option<Option<String>>,
    pub values: Option<String>,
    pub sizes: Option<Option<String>>,
}

/// A part of a chart the Format pane can format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartElement {
    ChartArea,
    PlotArea,
    Title,
    Legend,
    Axis(AxisSelector),
    AxisTitle(AxisSelector),
    /// The data labels of a series, or of every series.
    DataLabels(Option<usize>),
}

/// Fill, border and text of an element: `None` leaves a setting alone and a nested `None` removes it.
#[derive(Debug, Clone, Default)]
pub struct ElementFormat {
    pub fill: FillChange,
    pub line: LineChange,
    pub font_color: Option<(u8, u8, u8)>,
    pub font_size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

/// A `txPr` with the text settings given, over the ones the old one had.
fn merge_tx_pr(old: &Option<String>, f: &ElementFormat) -> Option<String> {
    let old_text = old.as_deref().unwrap_or("");
    let attr = |name: &str| -> Option<String> {
        let at = old_text.find("<a:defRPr")?;
        let rest = &old_text[at..old_text[at..].find('>').map_or(old_text.len(), |e| at + e)];
        let key = format!(" {name}=\"");
        let from = rest.find(&key)? + key.len();
        Some(rest[from..rest[from..].find('"')? + from].to_owned())
    };
    let size = f.font_size.map(|s| ((s * 100.0).round() as i64).to_string()).or_else(|| attr("sz"));
    let bold = f.bold.map(|b| u8::from(b).to_string()).or_else(|| attr("b"));
    let italic = f.italic.map(|b| u8::from(b).to_string()).or_else(|| attr("i"));
    let mut attrs = String::new();
    for (name, value) in [("sz", size), ("b", bold), ("i", italic)] {
        if let Some(v) = value {
            attrs.push_str(&format!(" {name}=\"{v}\""));
        }
    }
    let colour = match f.font_color {
        Some(c) => srgb_fill(Some(c)),
        None => old_text
            .find("<a:solidFill>")
            .and_then(|a| old_text[a..].find("</a:solidFill>").map(|e| old_text[a..a + e + 14].to_owned()))
            .unwrap_or_default(),
    };
    Some(format!(
        "<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr{attrs}>{colour}</a:defRPr></a:pPr><a:endParaRPr lang=\"en-US\"/></a:p></c:txPr>"
    ))
}

fn wants_text(f: &ElementFormat) -> bool {
    f.font_color.is_some() || f.font_size.is_some() || f.bold.is_some() || f.italic.is_some()
}

/// How big and where a new chart is, in cells.
const NEW_ROWS: u32 = 15;
const NEW_COLUMNS: u32 = 8;

fn srgb_fill(color: Option<(u8, u8, u8)>) -> String {
    match color {
        Some((r, g, b)) => format!("<a:solidFill><a:srgbClr val=\"{r:02X}{g:02X}{b:02X}\"/></a:solidFill>"),
        None => "<a:noFill/>".to_owned(),
    }
}

/// An `spPr` with the fill and line it was given, over what it had.
fn merge_sp_pr(old: &Option<String>, fill: FillChange, line: LineChange) -> Option<String> {
    if fill.is_none() && line.is_none() {
        return old.clone();
    }
    let kept = |tag: &str| -> Option<String> {
        let xml = old.as_deref()?;
        let open = xml.find(&format!("<{tag}"))?;
        let close = xml[open..].find(&format!("</{tag}>")).map(|i| open + i + tag.len() + 3)?;
        Some(xml[open..close].to_owned())
    };
    let fill_xml = match fill {
        Some(f) => srgb_fill(f),
        None => {
            let old = old.as_deref().unwrap_or("");
            ["<a:noFill/>", "<a:solidFill>", "<a:gradFill", "<a:pattFill", "<a:blipFill"]
                .iter()
                .find(|t| old.contains(**t))
                .map(|t| {
                    if *t == "<a:noFill/>" {
                        t.to_string()
                    } else {
                        let end = format!("</{}>", &t[1..]);
                        old.find(t).and_then(|s| old[s..].find(&end).map(|e| old[s..s + e + end.len()].to_owned())).unwrap_or_default()
                    }
                })
                .unwrap_or_default()
        }
    };
    let line_xml = match line {
        Some(Some((color, width))) => format!("<a:ln w=\"{}\" cap=\"rnd\">{}<a:round/></a:ln>", (width * 12700.0).round() as i64, srgb_fill(Some(color))),
        Some(None) => "<a:ln><a:noFill/></a:ln>".to_owned(),
        None => kept("a:ln").unwrap_or_default(),
    };
    Some(format!("<c:spPr>{fill_xml}{line_xml}</c:spPr>"))
}

impl Session {
    /// A chart lives on a worksheet or on a chart sheet; either is a sheet that exists.
    fn check_object_sheet(&self, sheet: usize) -> Result<(), String> {
        if sheet < self.book.sheets.len() {
            Ok(())
        } else {
            Err("There is no such sheet.".to_owned())
        }
    }

    /// Run a change to one chart: tried on a copy first, so a refusal leaves the workbook as it was, then
    /// kept as one undo step. The chart is marked as changed so it is written from the model.
    fn change_chart<R>(
        &mut self,
        label: &str,
        sheet: usize,
        index: usize,
        change: impl FnOnce(&mut Chart) -> Result<R, String>,
    ) -> Result<R, String> {
        self.check_object_sheet(sheet)?;
        self.allowed(sheet, Guard::Objects)?;
        let Some(chart) = self.book.sheets[sheet].charts.get(index) else {
            return Err(format!("there is no chart {} on this sheet", index + 1));
        };
        let mut trial = chart.clone();
        let answer = change(&mut trial)?;
        trial.spec.dirty = true;
        trial.sync_kind();
        self.remember_whole(label);
        self.book.sheets[sheet].charts[index] = trial;
        self.modified = true;
        Ok(answer)
    }

    /// Put a chart on a sheet. A chart given with no groups gets the groups and axes of its coarse kind.
    pub fn add_chart(&mut self, sheet: usize, mut chart: Chart) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        complete_new_chart(&mut chart);
        self.remember_whole("Chart");
        self.book.sheets[sheet].charts.push(chart);
        self.modified = true;
        Ok(())
    }

    /// Take a chart off a sheet. A chart read from a file leaves its parts behind in the file's package, so
    /// they are not written again.
    pub fn delete_chart(&mut self, sheet: usize, index: usize) -> Result<(), String> {
        self.check_object_sheet(sheet)?;
        self.allowed(sheet, Guard::Objects)?;
        let Some(chart) = self.book.sheets[sheet].charts.get(index) else {
            return Err(format!("there is no chart {} on this sheet", index + 1));
        };
        let part = (!chart.new && !chart.part.is_empty()).then(|| chart.part.clone());
        self.remember_whole("Delete chart");
        let s = &mut self.book.sheets[sheet];
        s.charts.remove(index);
        s.chart_removed.extend(part);
        self.modified = true;
        Ok(())
    }

    /// Insert a chart of a type over a range of cells (`Insert`, `Chart`). The series are laid out as Excel
    /// lays out a selection: names from a text first row, categories from a text or date first column,
    /// series in the direction with more points. Returns the new chart's index.
    pub fn insert_chart(
        &mut self,
        sheet: usize,
        range: Range,
        ty: ChartType,
        title: Option<String>,
    ) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        if ty == ChartType::Map {
            return Err("A map chart looks its regions up on Bing Maps, which a chart made here cannot do.".to_owned());
        }
        let shape = chartdata::shape_of(&self.book, sheet, range);
        let xy = matches!(
            ty,
            ChartType::ScatterMarkers
                | ChartType::ScatterSmooth
                | ChartType::ScatterSmoothNoMarkers
                | ChartType::ScatterLines
                | ChartType::ScatterLinesNoMarkers
                | ChartType::Bubble
                | ChartType::Bubble3D
        );
        let series = if xy {
            let bubble = matches!(ty, ChartType::Bubble | ChartType::Bubble3D);
            chartdata::layout_xy(&self.book, sheet, range, shape.header_row, bubble)
        } else {
            chartdata::layout_series(&self.book, sheet, range, shape)
        };
        if series.is_empty() || range.rows() == 0 {
            return Err("The data range is not valid.".to_owned());
        }
        self.new_chart_from(sheet, range, ty, title, series, shape.dates)
    }

    /// Insert a chart from series the caller laid out.
    fn new_chart_from(
        &mut self,
        sheet: usize,
        range: Range,
        ty: ChartType,
        title: Option<String>,
        series: Vec<ChartSeries>,
        dates: bool,
    ) -> Result<usize, String> {
        let mut chart = Chart {
            from: (CellRef::new(range.first.row, range.last.col + 2), 0, 0),
            to: (CellRef::new(range.first.row + NEW_ROWS, range.last.col + 2 + NEW_COLUMNS), 0, 0),
            title,
            series,
            new: true,
            ..Chart::default()
        };
        chart.spec.style = Some(2);
        chartbuild::apply_type(&mut chart, ty);
        chart.spec.rounded_corners = Some(false);
        chart.spec.title_deleted = false;
        chart.spec.title_auto = chart.title.is_none();
        if chart.series.len() > 1 || matches!(ty, ChartType::Pie | ChartType::Doughnut | ChartType::PieExploded | ChartType::Pie3D | ChartType::Pie3DExploded | ChartType::DoughnutExploded | ChartType::PieOfPie | ChartType::BarOfPie) {
            chart.spec.legend = Some(Legend { position: legend_default(ty), ..Legend::default() });
        }
        if dates {
            use_date_axis(&mut chart);
        }
        chart.spec.plot_visible_only = Some(true);
        chart.spec.blanks = Some(Blanks::Gap);
        self.remember_whole("Chart");
        self.book.sheets[sheet].charts.push(chart);
        self.modified = true;
        Ok(self.book.sheets[sheet].charts.len() - 1)
    }

    /// The charts Excel recommends for a range, best first (`Insert`, `Recommended Charts`).
    pub fn recommended_charts(&self, sheet: usize, range: Range) -> Result<Vec<ChartType>, String> {
        self.check_sheet(sheet)?;
        Ok(chartdata::recommend(&self.book, sheet, range))
    }

    /// Change the type of the whole chart (`Change Chart Type`). Series keep their sources and formatting.
    pub fn change_chart_type(&mut self, sheet: usize, index: usize, ty: ChartType) -> Result<(), String> {
        self.change_chart("Change chart type", sheet, index, |chart| {
            if ty == ChartType::Combo {
                return Err("Choose the type of each series to make a combination chart.".to_owned());
            }
            chartbuild::apply_type(chart, ty);
            Ok(())
        })
    }

    /// Change the type of one series, making a combination chart (`Change Series Chart Type`).
    pub fn change_series_type(&mut self, sheet: usize, index: usize, series: usize, ty: ChartType) -> Result<(), String> {
        self.change_chart("Change series chart type", sheet, index, |chart| {
            let secondary = series_is_secondary(chart, series)?;
            retype_series(chart, series, ty, secondary)
        })
    }

    /// Plot a series on the secondary axis, or back on the primary one.
    pub fn set_series_secondary(&mut self, sheet: usize, index: usize, series: usize, secondary: bool) -> Result<(), String> {
        self.change_chart("Secondary axis", sheet, index, |chart| {
            series_is_secondary(chart, series)?;
            let ty = chart.spec.groups[chart.series[series].group].chart_type();
            retype_series(chart, series, ty, secondary)
        })
    }

    /// Add a series (`Select Data`, `Add`). The formulas are references such as `Sheet1!$B$2:$B$9`.
    pub fn add_series(&mut self, sheet: usize, index: usize, source: SeriesSource) -> Result<usize, String> {
        let current = sheet;
        let checks = self.series_source_check(current, &source);
        checks?;
        self.change_chart("Add series", sheet, index, |chart| {
            let values = source.values.clone().ok_or("A series needs values.")?;
            let mut s = ChartSeries::new(source.name.clone().flatten(), source.categories.clone().flatten(), values);
            s.sizes = source.sizes.clone().flatten();
            s.group = 0;
            chartbuild::style_series(chart.chart_type(), &mut s);
            s.fmt.idx = None;
            s.fmt.order = None;
            chart.series.push(s);
            renumber(chart);
            if let Some(ex) = &mut chart.spec.ex {
                let data = chart.series.len() - 1;
                let mut template = ex.series.first().cloned().unwrap_or_default();
                template.data = data;
                template.format_index = ex.series.len() as u32;
                template.unique_id.clear();
                ex.series.push(template);
            }
            Ok(chart.series.len() - 1)
        })
    }

    /// Change what a series reads (`Select Data`, `Edit`).
    pub fn edit_series(&mut self, sheet: usize, index: usize, series: usize, source: SeriesSource) -> Result<(), String> {
        self.series_source_check(sheet, &source)?;
        self.change_chart("Edit series", sheet, index, |chart| {
            let s = chart.series.get_mut(series).ok_or("There is no such series.")?;
            if let Some(name) = source.name {
                s.name = name;
            }
            if let Some(cats) = source.categories {
                s.categories = cats;
            }
            if let Some(values) = source.values {
                s.values = values;
            }
            if let Some(sizes) = source.sizes {
                s.sizes = sizes;
            }
            Ok(())
        })
    }

    /// Refuse a formula that is not a reference, as Excel does in the Edit Series box.
    fn series_source_check(&self, sheet: usize, source: &SeriesSource) -> Result<(), String> {
        let bad = |f: &Option<String>, name: &str| match f {
            Some(f) if !f.starts_with('"') && !chartdata::is_reference(&self.book, sheet, f) && !f.starts_with('{') => {
                Err(format!("The reference you entered for {name} is not valid."))
            }
            _ => Ok(()),
        };
        bad(&source.values, "the series values")?;
        bad(&source.categories.clone().flatten(), "the axis labels")?;
        bad(&source.name.clone().flatten().filter(|n| !n.starts_with('"')), "the series name")?;
        bad(&source.sizes.clone().flatten(), "the bubble sizes")
    }

    /// Take a series out of the chart (`Select Data`, `Remove`). A chart with no series is refused.
    pub fn remove_series(&mut self, sheet: usize, index: usize, series: usize) -> Result<(), String> {
        self.change_chart("Remove series", sheet, index, |chart| {
            if series >= chart.series.len() {
                return Err("There is no such series.".to_owned());
            }
            if chart.series.len() == 1 {
                return Err("A chart needs at least one series.".to_owned());
            }
            chart.series.remove(series);
            if let Some(ex) = &mut chart.spec.ex {
                ex.series.retain(|e| e.data != series);
                for e in &mut ex.series {
                    if e.data > series {
                        e.data -= 1;
                    }
                }
            }
            drop_empty_groups(chart);
            renumber(chart);
            Ok(())
        })
    }

    /// Move a series one place earlier or later in the plot order.
    pub fn move_series(&mut self, sheet: usize, index: usize, series: usize, later: bool) -> Result<(), String> {
        self.change_chart("Move series", sheet, index, |chart| {
            let to = if later { series + 1 } else { series.wrapping_sub(1) };
            if series >= chart.series.len() || to >= chart.series.len() {
                return Err("The series cannot move that way.".to_owned());
            }
            chart.series.swap(series, to);
            renumber(chart);
            Ok(())
        })
    }

    /// Make the series of a chart from a range of cells (`Select Data`, chart data range), laid out as Excel
    /// lays out a selection, or the other way round when `plot_by` says so.
    pub fn set_chart_source(&mut self, sheet: usize, index: usize, range: Range, plot_by: Option<PlotBy>) -> Result<(), String> {
        let mut shape = chartdata::shape_of(&self.book, sheet, range);
        if let Some(by) = plot_by {
            shape.plot_by = by;
        }
        let series = chartdata::layout_series(&self.book, sheet, range, shape);
        if series.is_empty() {
            return Err("The data range is not valid.".to_owned());
        }
        self.change_chart("Chart data range", sheet, index, |chart| {
            let ty = chart.chart_type();
            let old = std::mem::take(&mut chart.series);
            chart.series = series;
            for (i, s) in chart.series.iter_mut().enumerate() {
                if let Some(o) = old.get(i) {
                    s.fmt = o.fmt.clone();
                    s.group = o.group;
                } else {
                    chartbuild::style_series(ty, s);
                }
            }
            drop_empty_groups(chart);
            renumber(chart);
            Ok(())
        })
    }

    /// Swap the rows and columns of the data (`Select Data`, `Switch Row/Column`).
    pub fn switch_row_column(&mut self, sheet: usize, index: usize) -> Result<(), String> {
        let chart = self.book.sheets.get(sheet).and_then(|s| s.charts.get(index)).ok_or("there is no such chart")?;
        let (range, by) = source_block(&self.book, sheet, chart).ok_or("The chart's data is not a block of cells.")?;
        let flipped = if by == PlotBy::Columns { PlotBy::Rows } else { PlotBy::Columns };
        let mut shape = chartdata::shape_of(&self.book, sheet, range);
        shape.plot_by = flipped;
        shape.header_row = true;
        shape.label_column = true;
        let sheet_of = chart_sheet_of(&self.book, sheet, chart);
        let series = chartdata::layout_series(&self.book, sheet_of, range, shape);
        if series.is_empty() {
            return Err("The data cannot be switched.".to_owned());
        }
        self.change_chart("Switch row/column", sheet, index, |chart| {
            let ty = chart.chart_type();
            chart.series = series;
            for s in chart.series.iter_mut() {
                chartbuild::style_series(ty, s);
            }
            drop_empty_groups(chart);
            renumber(chart);
            Ok(())
        })
    }

    /// How hidden and empty cells are treated (`Select Data`, `Hidden and Empty Cells`).
    pub fn set_hidden_and_empty_cells(&mut self, sheet: usize, index: usize, show_hidden: bool, blanks: Blanks) -> Result<(), String> {
        self.change_chart("Hidden and empty cells", sheet, index, |chart| {
            chart.spec.plot_visible_only = Some(!show_hidden);
            chart.spec.blanks = Some(blanks);
            Ok(())
        })
    }

    /// Move a chart or change its size, as the two cells its corners are in.
    pub fn move_chart(&mut self, sheet: usize, index: usize, from: (CellRef, i64, i64), to: (CellRef, i64, i64)) -> Result<(), String> {
        self.change_chart("Move chart", sheet, index, |chart| {
            chart.from = from;
            chart.to = to;
            chart.spec.moved = true;
            Ok(())
        })
    }

    /// Name a chart, and describe it for people who cannot see it.
    pub fn set_chart_name(&mut self, sheet: usize, index: usize, name: Option<String>, alt_text: Option<Option<String>>) -> Result<(), String> {
        self.change_chart("Chart name", sheet, index, |chart| {
            if let Some(name) = name {
                chart.spec.name = Some(name).filter(|n| !n.is_empty());
            }
            if let Some(alt) = alt_text {
                chart.spec.alt_text = alt;
            }
            chart.spec.moved = true;
            Ok(())
        })
    }

    /// Move a chart to another sheet (`Move Chart`, `Object in`). It keeps its place in cells.
    pub fn move_chart_to_sheet(&mut self, sheet: usize, index: usize, target: usize) -> Result<usize, String> {
        self.check_sheet(target)?;
        self.allowed(target, Guard::Forbidden)?;
        let mut chart = take_chart(self, sheet, index)?;
        let from_sheet = sheet;
        let name = self.book.sheets[sheet].name.clone();
        qualify_formulas(&mut chart, &name);
        chart.new = true;
        self.remember_whole("Move chart");
        let removed = self.book.sheets[from_sheet].charts.remove(index);
        if !removed.new && !removed.part.is_empty() {
            self.book.sheets[from_sheet].chart_removed.push(removed.part);
        }
        self.book.sheets[target].charts.push(chart);
        self.modified = true;
        Ok(self.book.sheets[target].charts.len() - 1)
    }

    /// Move a chart onto a new chart sheet (`Move Chart`, `New sheet`, or F11). Returns the sheet's index.
    pub fn move_chart_to_new_sheet(&mut self, sheet: usize, index: usize, name: Option<&str>) -> Result<usize, String> {
        let mut chart = take_chart(self, sheet, index)?;
        let source = self.book.sheets[sheet].name.clone();
        qualify_formulas(&mut chart, &source);
        chart.new = true;
        chart.spec.anchor_edit_as = None;
        chart.from = (CellRef::new(0, 0), 0, 0);
        chart.to = (CellRef::new(0, 0), 0, 0);
        let at = self.add_sheet(name, None)?;
        self.remember_whole("Move chart");
        let removed = self.book.sheets[sheet].charts.remove(index);
        if !removed.new && !removed.part.is_empty() {
            self.book.sheets[sheet].chart_removed.push(removed.part);
        }
        let s = &mut self.book.sheets[at];
        s.charts.push(chart);
        s.kept_part = Some(KeptSheet { part: String::new(), kind: REL_CHART_SHEET.to_owned(), content_type: CT_CHART_SHEET.to_owned() });
        self.modified = true;
        Ok(at)
    }

    /// Make a chart sheet holding a new chart of the default type from a range (F11).
    pub fn insert_chart_sheet(&mut self, sheet: usize, range: Range, ty: ChartType) -> Result<usize, String> {
        let index = self.insert_chart(sheet, range, ty, None)?;
        self.move_chart_to_new_sheet(sheet, index, None)
    }

    /// Set, change or remove the chart title (`Chart Title`). `None` removes it.
    pub fn set_chart_title(&mut self, sheet: usize, index: usize, title: Option<String>, overlay: Option<bool>) -> Result<(), String> {
        self.change_chart("Chart title", sheet, index, |chart| {
            chart.spec.title_deleted = title.is_none();
            chart.spec.title_auto = false;
            chart.title = title;
            if let Some(o) = overlay {
                chart.spec.title.overlay = o;
            }
            Ok(())
        })
    }

    /// Change an axis: its bounds, units, scale, number format, tick marks, labels, gridlines and title.
    pub fn set_axis_options(&mut self, sheet: usize, index: usize, axis: AxisSelector, options: AxisOptions) -> Result<(), String> {
        self.change_chart("Format axis", sheet, index, |chart| {
            let id = axis_id(chart, axis).ok_or("The chart has no such axis.")?;
            let reversed = options.reverse;
            let a = chart.spec.axis_mut(id).ok_or("The chart has no such axis.")?;
            let was = a.reverse;
            apply_axis_options(a, options)?;
            if reversed.is_some_and(|r| r != was) {
                flip_crossing_axes(chart, id);
            }
            Ok(())
        })
    }

    /// Show or hide the legend, and place it (`Legend`).
    pub fn set_legend(&mut self, sheet: usize, index: usize, position: Option<LegendPosition>, overlay: bool) -> Result<(), String> {
        self.change_chart("Legend", sheet, index, |chart| {
            chart.spec.legend = position.map(|p| {
                let old = chart.spec.legend.take().unwrap_or_default();
                Legend { position: p, overlay, ..old }
            });
            Ok(())
        })
    }

    /// Show or hide the data table under the chart (`Data Table`).
    pub fn set_data_table(&mut self, sheet: usize, index: usize, table: Option<DataTable>) -> Result<(), String> {
        self.change_chart("Data table", sheet, index, |chart| {
            if table.is_some() && chart.spec.groups.iter().any(|g| !g.kind.has_axes() || matches!(g.kind, GroupKind::Scatter | GroupKind::Bubble)) {
                return Err("A data table is not available for this type of chart.".to_owned());
            }
            chart.spec.data_table = table;
            Ok(())
        })
    }

    /// Turn data labels on or off for one series, or for all of them, and say what they show.
    pub fn set_data_labels(&mut self, sheet: usize, index: usize, series: Option<usize>, show: bool, options: LabelOptions) -> Result<(), String> {
        self.change_chart("Data labels", sheet, index, |chart| {
            let range: Vec<usize> = match series {
                Some(s) if s < chart.series.len() => vec![s],
                Some(_) => return Err("There is no such series.".to_owned()),
                None => (0..chart.series.len()).collect(),
            };
            let ty = chart.chart_type();
            for i in range {
                let labels = &mut chart.series[i].fmt.labels;
                if !show {
                    *labels = None;
                    continue;
                }
                let l = labels.get_or_insert_with(|| default_labels(ty));
                l.delete = false;
                apply_label_options(&mut l.fields, &options);
            }
            Ok(())
        })
    }

    /// Add a trendline to a series. Returns its number among the series' trendlines.
    pub fn add_trendline(&mut self, sheet: usize, index: usize, series: usize, trendline: Trendline) -> Result<usize, String> {
        self.change_chart("Add trendline", sheet, index, |chart| {
            check_trendline(chart, series, &trendline)?;
            let s = &mut chart.series[series];
            s.fmt.trendlines.push(trendline);
            Ok(s.fmt.trendlines.len() - 1)
        })
    }

    /// Change a trendline of a series.
    pub fn edit_trendline(&mut self, sheet: usize, index: usize, series: usize, which: usize, trendline: Trendline) -> Result<(), String> {
        self.change_chart("Format trendline", sheet, index, |chart| {
            check_trendline(chart, series, &trendline)?;
            let slot = chart.series[series].fmt.trendlines.get_mut(which).ok_or("There is no such trendline.")?;
            *slot = trendline;
            Ok(())
        })
    }

    /// Remove a trendline of a series.
    pub fn remove_trendline(&mut self, sheet: usize, index: usize, series: usize, which: usize) -> Result<(), String> {
        self.change_chart("Remove trendline", sheet, index, |chart| {
            let s = chart.series.get_mut(series).ok_or("There is no such series.")?;
            if which >= s.fmt.trendlines.len() {
                return Err("There is no such trendline.".to_owned());
            }
            s.fmt.trendlines.remove(which);
            Ok(())
        })
    }

    /// Put error bars on a series, replacing those in the same direction.
    pub fn set_error_bars(&mut self, sheet: usize, index: usize, series: usize, bars: ErrorBars) -> Result<(), String> {
        self.change_chart("Error bars", sheet, index, |chart| {
            let kind = chart.spec.groups.get(chart.series.get(series).ok_or("There is no such series.")?.group).map(|g| g.kind);
            if matches!(kind, Some(GroupKind::Pie | GroupKind::Doughnut | GroupKind::OfPie | GroupKind::Radar | GroupKind::Surface)) {
                return Err("Error bars are not available for this type of chart.".to_owned());
            }
            if bars.amount == ErrorAmount::Custom && bars.plus.is_none() && bars.minus.is_none() {
                return Err("Custom error bars need values.".to_owned());
            }
            let list = &mut chart.series[series].fmt.error_bars;
            list.retain(|e| e.direction != bars.direction);
            list.push(bars);
            Ok(())
        })
    }

    /// Take the error bars of one direction off a series.
    pub fn remove_error_bars(&mut self, sheet: usize, index: usize, series: usize, direction: Option<ErrorDirection>) -> Result<(), String> {
        self.change_chart("Remove error bars", sheet, index, |chart| {
            let s = chart.series.get_mut(series).ok_or("There is no such series.")?;
            s.fmt.error_bars.retain(|e| e.direction != direction);
            Ok(())
        })
    }

    /// Change how the series of one chart group are drawn: gap, overlap, hole, angle, lines.
    pub fn set_group_options(&mut self, sheet: usize, index: usize, group: usize, options: GroupOptions) -> Result<(), String> {
        self.change_chart("Format data series", sheet, index, |chart| {
            let g = chart.spec.groups.get_mut(group).ok_or("There is no such chart group.")?;
            apply_group_options(g, &options);
            Ok(())
        })
    }

    /// Change how a series, or one of its points, looks.
    pub fn set_series_format(&mut self, sheet: usize, index: usize, series: usize, point: Option<u32>, options: SeriesOptions) -> Result<(), String> {
        self.change_chart("Format data series", sheet, index, |chart| {
            let s = chart.series.get_mut(series).ok_or("There is no such series.")?;
            apply_series_options(s, point, &options);
            Ok(())
        })
    }

    /// Format an element of the chart: fill, border and text (the Format pane).
    pub fn set_element_format(&mut self, sheet: usize, index: usize, element: ChartElement, format: ElementFormat) -> Result<(), String> {
        self.change_chart("Format chart element", sheet, index, |chart| {
            let text = wants_text(&format);
            match element {
                ChartElement::ChartArea => {
                    let raw = &mut chart.spec.raw;
                    raw.sp_pr = merge_sp_pr(&raw.sp_pr, format.fill, format.line);
                    if text {
                        raw.tx_pr = merge_tx_pr(&raw.tx_pr, &format);
                    }
                }
                ChartElement::PlotArea => {
                    let raw = &mut chart.spec.raw;
                    raw.plot_sp_pr = merge_sp_pr(&raw.plot_sp_pr, format.fill, format.line);
                }
                ChartElement::Title => {
                    let t = &mut chart.spec.title;
                    t.sp_pr = merge_sp_pr(&t.sp_pr, format.fill, format.line);
                    if text {
                        t.tx_pr = merge_tx_pr(&t.tx_pr, &format);
                    }
                }
                ChartElement::Legend => {
                    let l = chart.spec.legend.as_mut().ok_or("The chart has no legend.")?;
                    l.sp_pr = merge_sp_pr(&l.sp_pr, format.fill, format.line);
                    if text {
                        l.tx_pr = merge_tx_pr(&l.tx_pr, &format);
                    }
                }
                ChartElement::Axis(which) | ChartElement::AxisTitle(which) => {
                    let id = axis_id(chart, which).ok_or("The chart has no such axis.")?;
                    let a = chart.spec.axis_mut(id).ok_or("The chart has no such axis.")?;
                    if matches!(element, ChartElement::AxisTitle(_)) {
                        let t = a.title.as_mut().ok_or("The axis has no title.")?;
                        t.format.sp_pr = merge_sp_pr(&t.format.sp_pr, format.fill, format.line);
                        if text {
                            t.format.tx_pr = merge_tx_pr(&t.format.tx_pr, &format);
                        }
                    } else {
                        if format.fill.is_some() || format.line.is_some() {
                            a.sp_pr = merge_sp_pr(&a.sp_pr, format.fill, format.line);
                        }
                        if text {
                            a.tx_pr = merge_tx_pr(&a.tx_pr, &format);
                        }
                    }
                }
                ChartElement::DataLabels(series) => {
                    let range: Vec<usize> = match series {
                        Some(s) if s < chart.series.len() => vec![s],
                        Some(_) => return Err("There is no such series.".to_owned()),
                        None => (0..chart.series.len()).collect(),
                    };
                    for i in range {
                        let l = chart.series[i].fmt.labels.as_mut().ok_or("The series has no data labels.")?;
                        l.fields.sp_pr = merge_sp_pr(&l.fields.sp_pr, format.fill, format.line);
                        if text {
                            l.fields.tx_pr = merge_tx_pr(&l.fields.tx_pr, &format);
                        }
                    }
                }
            }
            Ok(())
        })
    }

    /// Hide a series from the chart without removing it (`Chart Filters`).
    pub fn filter_series(&mut self, sheet: usize, index: usize, series: usize, hidden: bool) -> Result<(), String> {
        self.change_chart("Chart filters", sheet, index, |chart| {
            if hidden && chart.series.iter().filter(|s| !s.fmt.filtered).count() <= 1 && !chart.series.get(series).ok_or("There is no such series.")?.fmt.filtered {
                return Err("A chart needs at least one series.".to_owned());
            }
            chart.series.get_mut(series).ok_or("There is no such series.")?.fmt.filtered = hidden;
            Ok(())
        })
    }

    /// Hide categories of the chart by position (`Chart Filters`).
    pub fn filter_categories(&mut self, sheet: usize, index: usize, hidden: Vec<u32>) -> Result<(), String> {
        self.change_chart("Chart filters", sheet, index, |chart| {
            for s in &mut chart.series {
                s.fmt.filtered_categories = hidden.clone();
            }
            Ok(())
        })
    }

    /// Apply one of the 14 quick layouts Excel offers for the chart's type (`Quick Layout`).
    pub fn apply_quick_layout(&mut self, sheet: usize, index: usize, layout: u32) -> Result<(), String> {
        self.change_chart("Quick layout", sheet, index, |chart| crate::chartlayout::apply(chart, layout))
    }

    /// Choose the chart style, a number from 1 to 48 (`Chart Styles`).
    pub fn set_chart_style(&mut self, sheet: usize, index: usize, style: u32) -> Result<(), String> {
        self.change_chart("Chart style", sheet, index, |chart| {
            if !(1..=48).contains(&style) {
                return Err("Chart styles are numbered 1 to 48.".to_owned());
            }
            chart.spec.style = Some(style);
            Ok(())
        })
    }
}

/// Where the legend goes by default for a type: Excel's is on the right, and at the bottom for
/// charts made with several series on the Office 2013 styles.
fn legend_default(_ty: ChartType) -> LegendPosition {
    LegendPosition::Bottom
}

/// Give a chart made with only a coarse kind the groups and axes of that kind.
fn complete_new_chart(chart: &mut Chart) {
    if !chart.spec.groups.is_empty() || chart.spec.ex.is_some() {
        return;
    }
    let ty = match chart.kind {
        crate::model::ChartKind::Column => ChartType::ColumnClustered,
        crate::model::ChartKind::Bar => ChartType::BarClustered,
        crate::model::ChartKind::Line => ChartType::LineMarkers,
        crate::model::ChartKind::Area => ChartType::Area,
        crate::model::ChartKind::Pie => ChartType::Pie,
        crate::model::ChartKind::Doughnut => ChartType::Doughnut,
        crate::model::ChartKind::Scatter => ChartType::ScatterMarkers,
        crate::model::ChartKind::Other(_) => ChartType::ColumnClustered,
    };
    chartbuild::apply_type(chart, ty);
    chart.spec.style = Some(2);
    chart.spec.rounded_corners = Some(false);
    chart.spec.title_deleted = false;
    chart.spec.title_auto = chart.title.is_none();
    if chart.series.len() > 1 || matches!(ty, ChartType::Pie | ChartType::Doughnut) {
        chart.spec.legend = Some(Legend { position: LegendPosition::Bottom, ..Legend::default() });
    }
    chart.spec.dirty = false;
    chart.spec.plot_visible_only = Some(true);
    chart.spec.blanks = Some(Blanks::Gap);
}

/// Turn a chart's category axis into a date axis, which Excel does for categories that are dates.
fn use_date_axis(chart: &mut Chart) {
    for axis in chart.spec.axes.iter_mut().filter(|a| a.kind == AxisKind::Category && !a.deleted) {
        axis.kind = AxisKind::Date;
        axis.auto = Some(true);
        axis.base_unit = Some(TimeUnit::Days);
        axis.label_align = None;
        axis.no_multi_level = None;
        axis.num_fmt = Some(("m/d/yyyy".to_owned(), true));
    }
}

/// Types whose data labels have no position setting: Excel gives them none.
fn labels_have_no_position(ty: ChartType) -> bool {
    use ChartType as T;
    ty.is_extended()
        || matches!(
            ty,
            T::Doughnut
                | T::DoughnutExploded
                | T::ColumnStacked
                | T::ColumnStacked100
                | T::BarStacked
                | T::BarStacked100
                | T::AreaStacked
                | T::AreaStacked100
                | T::Area
                | T::Radar
                | T::RadarMarkers
                | T::RadarFilled
                | T::Column3DClustered
                | T::Column3DStacked
                | T::Column3DStacked100
                | T::Column3D
                | T::Bar3DClustered
                | T::Bar3DStacked
                | T::Bar3DStacked100
                | T::Line3D
                | T::Area3D
                | T::Area3DStacked
                | T::Area3DStacked100
                | T::Pie3D
                | T::Pie3DExploded
        )
}

/// The default data labels Excel gives a type: the value, outside the end for columns and bars.
fn default_labels(ty: ChartType) -> DataLabels {
    use ChartType as T;
    let pie = matches!(ty, T::Pie | T::PieExploded | T::PieOfPie | T::BarOfPie);
    let position = if labels_have_no_position(ty) {
        None
    } else if matches!(ty, T::ColumnClustered | T::BarClustered) {
        Some(LabelPosition::OutsideEnd)
    } else if pie {
        Some(LabelPosition::BestFit)
    } else {
        Some(LabelPosition::Above)
    };
    DataLabels {
        fields: LabelFields { show_value: true, position, ..LabelFields::default() },
        show_leader_lines: pie.then_some(true),
        ..DataLabels::default()
    }
}

fn apply_label_options(f: &mut LabelFields, o: &LabelOptions) {
    if let Some(v) = o.show_value {
        f.show_value = v;
    }
    if let Some(v) = o.show_category {
        f.show_category = v;
    }
    if let Some(v) = o.show_series {
        f.show_series = v;
    }
    if let Some(v) = o.show_percent {
        f.show_percent = v;
    }
    if let Some(v) = o.show_bubble_size {
        f.show_bubble_size = v;
    }
    if let Some(v) = o.show_legend_key {
        f.show_legend_key = v;
    }
    if let Some(v) = o.position {
        f.position = v;
    }
    if let Some(v) = &o.separator {
        f.separator = v.clone();
    }
    if let Some(v) = &o.number_format {
        f.num_fmt = v.clone();
    }
}

fn apply_group_options(g: &mut Group, o: &GroupOptions) {
    if let Some(v) = o.gap_width {
        g.gap_width = Some(v.min(500));
    }
    if let Some(v) = o.overlap {
        g.overlap = Some(v.clamp(-100, 100));
    }
    if let Some(v) = o.first_slice_angle {
        g.first_slice_angle = Some(v.min(360));
    }
    if let Some(v) = o.hole_size {
        g.hole_size = Some(v.clamp(10, 90));
    }
    if let Some(v) = o.vary_colors {
        g.vary_colors = Some(v);
    }
    if let Some(v) = o.second_plot_size {
        g.second_size = Some(v.clamp(5, 200));
    }
    if let Some(v) = o.split_by {
        g.split_by = v;
    }
    if let Some(v) = o.split_at {
        g.split_at = Some(v);
    }
    if let Some(v) = o.bubble_scale {
        g.bubble_scale = Some(v.min(300));
    }
    if let Some(v) = o.drop_lines {
        g.drop_lines = v;
    }
    if let Some(v) = o.high_low_lines {
        g.hi_low_lines = v;
    }
    if let Some(v) = o.up_down_bars {
        g.up_down_bars = v.then_some(150);
    }
    if let Some(v) = o.series_lines {
        g.series_lines = v;
    }
}

fn apply_series_options(s: &mut ChartSeries, point: Option<u32>, o: &SeriesOptions) {
    match point {
        Some(p) => {
            let at = s.fmt.points.iter().position(|x| x.index == p).unwrap_or_else(|| {
                s.fmt.points.push(PointFormat { index: p, ..PointFormat::default() });
                s.fmt.points.len() - 1
            });
            let pf = &mut s.fmt.points[at];
            pf.sp_pr = merge_sp_pr(&pf.sp_pr, o.fill, o.line);
            if let Some(e) = o.explosion {
                pf.explosion = Some(e);
            }
            if o.marker_symbol.is_some() || o.marker_size.is_some() {
                let m = pf.marker.get_or_insert_with(Marker::default);
                if let Some(sym) = &o.marker_symbol {
                    m.symbol = Some(sym.clone());
                }
                if let Some(size) = o.marker_size {
                    m.size = Some(size);
                }
            }
            s.fmt.points.sort_by_key(|x| x.index);
        }
        None => {
            if o.fill.is_some() || o.line.is_some() {
                s.fmt.sp_pr = merge_sp_pr(&s.fmt.sp_pr, o.fill, o.line);
            }
            if o.marker_symbol.is_some() || o.marker_size.is_some() {
                let m = s.fmt.marker.get_or_insert_with(Marker::default);
                if let Some(sym) = &o.marker_symbol {
                    m.symbol = Some(sym.clone());
                }
                if let Some(size) = o.marker_size {
                    m.size = Some(size);
                }
            }
            if let Some(v) = o.smooth {
                s.fmt.smooth = Some(v);
            }
            if let Some(e) = o.explosion {
                s.fmt.explosion = Some(e);
            }
            if let Some(v) = o.invert_if_negative {
                s.fmt.invert_if_negative = Some(v);
            }
        }
    }
}

fn axis_id(chart: &Chart, which: AxisSelector) -> Option<u32> {
    let spec = &chart.spec;
    let secondary = matches!(which, AxisSelector::SecondaryCategory | AxisSelector::SecondaryValue);
    let group = (0..spec.groups.len()).find(|&g| spec.is_secondary(g) == secondary)?;
    let g = &spec.groups[group];
    let wants_value = matches!(which, AxisSelector::Value | AxisSelector::SecondaryValue);
    let axes: Vec<&Axis> = g.axes.iter().filter_map(|id| spec.axis(*id)).collect();
    if g.kind == GroupKind::Bar && g.horizontal {
        // A bar chart's value axis runs along the bottom, but is still its value axis.
    }
    let scatter = matches!(g.kind, GroupKind::Scatter | GroupKind::Bubble);
    if scatter {
        let index = usize::from(wants_value);
        return axes.get(index).map(|a| a.id);
    }
    axes.iter().find(|a| (a.kind == AxisKind::Value) == wants_value && a.kind != AxisKind::Series).map(|a| a.id)
}

/// Reversing an axis moves the axes that cross it to the other side, as Excel does.
fn flip_crossing_axes(chart: &mut Chart, id: u32) {
    for a in chart.spec.axes.iter_mut().filter(|a| a.cross_axis == id && !a.deleted) {
        a.position = match a.position {
            AxisPosition::Left => AxisPosition::Right,
            AxisPosition::Right => AxisPosition::Left,
            AxisPosition::Top => AxisPosition::Bottom,
            AxisPosition::Bottom => AxisPosition::Top,
        };
    }
}

/// Change an axis, refusing a scale Excel refuses.
fn apply_axis_options(a: &mut Axis, o: AxisOptions) -> Result<(), String> {
    apply_axis_scale(a, &o)?;
    apply_axis_display(a, o);
    Ok(())
}

/// The bounds, units and log base of an axis.
fn apply_axis_scale(a: &mut Axis, o: &AxisOptions) -> Result<(), String> {
    let mut next = a.clone();
    if let Some(v) = o.minimum {
        next.min = v;
    }
    if let Some(v) = o.maximum {
        next.max = v;
    }
    if let (Some(min), Some(max)) = (next.min, next.max) {
        if min >= max {
            return Err("The maximum must be greater than the minimum.".to_owned());
        }
    }
    if let Some(v) = o.major_unit {
        if v.is_some_and(|u| u <= 0.0) {
            return Err("The major unit must be greater than zero.".to_owned());
        }
        next.major_unit = v;
    }
    if let Some(v) = o.minor_unit {
        if v.is_some_and(|u| u <= 0.0) {
            return Err("The minor unit must be greater than zero.".to_owned());
        }
        next.minor_unit = v;
    }
    if let Some(v) = o.log_base {
        if v.is_some_and(|b| !(2.0..=1000.0).contains(&b)) {
            return Err("The logarithmic base must be from 2 to 1000.".to_owned());
        }
        next.log_base = v;
    }
    *a = next;
    Ok(())
}

/// How an axis looks: direction, crossing, number format, ticks, labels, gridlines, title.
fn apply_axis_display(a: &mut Axis, o: AxisOptions) {
    if let Some(v) = o.reverse {
        a.reverse = v;
    }
    if let Some(v) = o.crosses {
        a.crosses = v;
    }
    if let Some(v) = o.number_format {
        a.num_fmt = v;
    }
    if let Some(v) = o.major_tick {
        a.major_tick = v;
    }
    if let Some(v) = o.minor_tick {
        a.minor_tick = v;
    }
    if let Some(v) = o.tick_labels {
        a.tick_labels = v;
    }
    if let Some(v) = o.visible {
        a.deleted = !v;
    }
    if let Some(v) = o.major_gridlines {
        a.major_gridlines = v.then(|| a.major_gridlines.take().unwrap_or_default());
    }
    if let Some(v) = o.minor_gridlines {
        a.minor_gridlines = v.then(|| a.minor_gridlines.take().unwrap_or_default());
    }
    if let Some(v) = o.title {
        a.title = v.map(|text| AxisTitle { text, format: a.title.take().map(|t| t.format).unwrap_or_default() });
    }
    if let Some(v) = o.between {
        a.between = Some(v);
    }
    if let Some(v) = o.label_skip {
        a.label_skip = v;
    }
    if let Some(v) = o.mark_skip {
        a.mark_skip = v;
    }
    if let Some(v) = o.base_unit {
        a.base_unit = v;
    }
}

fn check_trendline(chart: &Chart, series: usize, t: &Trendline) -> Result<(), String> {
    let s = chart.series.get(series).ok_or("There is no such series.")?;
    let kind = chart.spec.groups.get(s.group).map(|g| g.kind);
    if matches!(kind, Some(GroupKind::Pie | GroupKind::Doughnut | GroupKind::OfPie | GroupKind::Radar | GroupKind::Surface))
        || matches!(chart.spec.groups.get(s.group), Some(g) if g.three_d || g.grouping == Grouping::Stacked || g.grouping == Grouping::PercentStacked)
    {
        return Err("Trendlines are not available for this type of chart.".to_owned());
    }
    match t.kind {
        TrendKind::Polynomial(order) if !(2..=6).contains(&order) => {
            Err("The polynomial order must be from 2 to 6.".to_owned())
        }
        TrendKind::MovingAverage(period) if period < 2 => Err("The period must be at least 2.".to_owned()),
        _ => Ok(()),
    }
}

/// Number series again, as Excel does after an addition or removal.
fn renumber(chart: &mut Chart) {
    for (i, s) in chart.series.iter_mut().enumerate() {
        s.fmt.idx = Some(i as u32);
        s.fmt.order = Some(i as u32);
    }
}

/// Take away chart groups no series is in, and the axes no group is drawn against.
fn drop_empty_groups(chart: &mut Chart) {
    let used: Vec<usize> = (0..chart.spec.groups.len()).filter(|g| chart.series.iter().any(|s| s.group == *g)).collect();
    let map: Vec<Option<usize>> = (0..chart.spec.groups.len()).map(|g| used.iter().position(|u| *u == g)).collect();
    for s in &mut chart.series {
        s.group = map[s.group].unwrap_or(0);
    }
    let mut keep = Vec::new();
    for (i, g) in std::mem::take(&mut chart.spec.groups).into_iter().enumerate() {
        if map[i].is_some() {
            keep.push(g);
        }
    }
    if keep.is_empty() {
        keep.push(chartbuild::build(ChartType::ColumnClustered).groups.remove(0));
    }
    chart.spec.groups = keep;
    let ids: Vec<u32> = chart.spec.groups.iter().flat_map(|g| g.axes.clone()).collect();
    chart.spec.axes.retain(|a| ids.contains(&a.id) || chart.spec.groups.iter().all(|g| g.axes.is_empty()));
}

fn series_is_secondary(chart: &Chart, series: usize) -> Result<bool, String> {
    let s = chart.series.get(series).ok_or("There is no such series.")?;
    if chart.spec.ex.is_some() {
        return Err("This type of chart cannot be combined with another.".to_owned());
    }
    Ok(chart.spec.is_secondary(s.group))
}

/// Draw one series as another type, on the primary or secondary axis, in a group of its own if no group
/// of that type is there already. Excel refuses types that cannot be combined.
fn retype_series(chart: &mut Chart, series: usize, ty: ChartType, secondary: bool) -> Result<(), String> {
    if ty.is_extended() || ty == ChartType::Combo || chart.spec.ex.is_some() {
        return Err("This type of chart cannot be combined with another.".to_owned());
    }
    let built = chartbuild::build(ty);
    let Some(mut target) = built.groups.into_iter().next() else { return Err("Unknown chart type.".to_owned()) };
    let other_kinds = chart.series.iter().enumerate().filter(|(i, _)| *i != series).any(|(_, s)| {
        chart.spec.groups.get(s.group).is_some_and(|g| {
            (g.three_d || target.three_d) && (g.kind != target.kind || g.three_d != target.three_d)
                || (g.horizontal != target.horizontal && g.kind == GroupKind::Bar && target.kind == GroupKind::Bar)
        })
    });
    if other_kinds || (target.three_d && chart.series.len() > 1) {
        return Err("Some chart types cannot be combined with other chart types. Select a different chart type.".to_owned());
    }
    if !target.kind.has_axes() && chart.series.len() > 1 {
        return Err("Some chart types cannot be combined with other chart types. Select a different chart type.".to_owned());
    }
    let scatter = matches!(target.kind, GroupKind::Scatter | GroupKind::Bubble);
    if secondary {
        target.axes = chartbuild::add_secondary_axes(&mut chart.spec, scatter);
    } else if chart.spec.groups.is_empty() || target.axes.is_empty() {
        target.axes = chartbuild::primary_axes();
    } else {
        target.axes = chart.spec.groups.first().map(|g| g.axes.clone()).filter(|a| !chart.spec.is_secondary(0) && !a.is_empty()).unwrap_or_else(chartbuild::primary_axes);
    }
    for id in &target.axes {
        if chart.spec.axis(*id).is_none() {
            for axis in built_axes(ty) {
                if axis.id == *id {
                    chart.spec.axes.push(axis);
                }
            }
        }
    }
    let same = |g: &Group| {
        g.kind == target.kind
            && g.three_d == target.three_d
            && g.horizontal == target.horizontal
            && g.grouping == target.grouping
            && g.scatter_style == target.scatter_style
            && g.radar_style == target.radar_style
            && g.of_pie == target.of_pie
            && g.axes == target.axes
    };
    let at = match chart.spec.groups.iter().position(same) {
        Some(at) => at,
        None => {
            chart.spec.groups.push(target);
            chart.spec.groups.len() - 1
        }
    };
    chart.series[series].group = at;
    chartbuild::style_series(ty, &mut chart.series[series]);
    drop_empty_groups(chart);
    Ok(())
}

fn built_axes(ty: ChartType) -> Vec<Axis> {
    chartbuild::build(ty).axes
}

/// The block of cells a chart's series read, and which way the series lie in it.
fn source_block(book: &crate::model::Workbook, sheet: usize, chart: &Chart) -> Option<(Range, PlotBy)> {
    let mut union: Option<(usize, Range)> = None;
    let mut add = |f: &Option<String>| -> Option<()> {
        for part in chartdata::parse_refs(book, sheet, f.as_deref()?)? {
            union = Some(match union {
                Some((s, r)) if s == part.sheet => (s, r.union(&part.range)),
                Some(_) => return None,
                None => (part.sheet, part.range),
            });
        }
        Some(())
    };
    for s in &chart.series {
        add(&s.name)?;
        add(&s.categories)?;
        add(&Some(s.values.clone()))?;
    }
    let (_, range) = union?;
    let first = chart.series.first()?;
    let values = chartdata::parse_refs(book, sheet, &first.values)?.first().copied()?;
    let by = if values.range.cols() == 1 && values.range.rows() >= 1 && (values.range.rows() > 1 || chart.series.len() == 1) {
        PlotBy::Columns
    } else {
        PlotBy::Rows
    };
    Some((range, by))
}

/// The sheet a chart's data lives on: the sheet its first series' values name.
fn chart_sheet_of(book: &crate::model::Workbook, sheet: usize, chart: &Chart) -> usize {
    chart
        .series
        .first()
        .and_then(|s| chartdata::parse_refs(book, sheet, &s.values))
        .and_then(|p| p.first().map(|r| r.sheet))
        .unwrap_or(sheet)
}

fn take_chart(session: &mut Session, sheet: usize, index: usize) -> Result<Chart, String> {
    session.check_object_sheet(sheet)?;
    session.allowed(sheet, Guard::Objects)?;
    session.book.sheets[sheet].charts.get(index).cloned().ok_or_else(|| format!("there is no chart {} on this sheet", index + 1))
}

/// Make every unqualified reference in a chart name its sheet, for a chart that moves to another sheet.
fn qualify_formulas(chart: &mut Chart, sheet_name: &str) {
    let prefix = if sheet_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        sheet_name.to_owned()
    } else {
        format!("'{}'", sheet_name.replace('\'', "''"))
    };
    let fix = |f: &mut String| {
        if !f.contains('!') && !f.starts_with('{') && !f.starts_with('"') && !f.is_empty() {
            *f = format!("{prefix}!{f}");
        }
    };
    for s in &mut chart.series {
        if let Some(n) = s.name.as_mut() {
            fix(n);
        }
        if let Some(c) = s.categories.as_mut() {
            fix(c);
        }
        fix(&mut s.values);
        if let Some(c) = s.sizes.as_mut() {
            fix(c);
        }
    }
}
