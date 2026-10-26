//! The chart verbs about series and what hangs on them (`task-2360`, the `chartwire` lane): `chart-series`
//! (Select Data, Change Series Chart Type, secondary axis, filters and the look of a series or a point),
//! `chart-labels`, `chart-trendline`, `chart-errors` and `chart-group`. A series is named by its number from
//! 1 in the order the chart lists them.

use unluminous_sheet::chartspec::ChartType;
use unluminous_sheet::chartspec::{
    ErrorAmount, ErrorBars, ErrorDirection, ErrorSides, ErrorSource, GroupKind, LabelPosition,
    SplitBy, TrendKind, Trendline,
};
use unluminous_sheet::edit::charts::{GroupOptions, LabelOptions, SeriesOptions, SeriesSource};

use super::verbs_a::Out;
use super::verbs_charts::{chart_on, fill_word, line_word, name_source, reference, Words};
use super::{Book, Spreadsheet};

/// Run a verb this file owns, or answer `None` for one it does not.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "chart-series" => chart_on(sheet, args, series),
        "chart-labels" => chart_on(sheet, args, labels),
        "chart-trendline" => chart_on(sheet, args, trendline),
        "chart-errors" => chart_on(sheet, args, errors),
        "chart-group" => chart_on(sheet, args, group),
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------------ the series

/// `chart-series <n> add|edit|remove|move|type|secondary|hide|format ...`.
fn series(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let sub = w.need("add, edit, remove, move, type, secondary, hide or format")?;
    let shown = index + 1;
    match sub {
        "add" => {
            let source = source_of(book, &mut w, true)?;
            let made = book.session.add_series(at, index, source)?;
            Ok(format!("Added series {} to chart {shown}", made + 1))
        }
        "edit" => {
            let s = w.place("a series number")?;
            let source = source_of(book, &mut w, false)?;
            book.session.edit_series(at, index, s, source)?;
            Ok(format!("Edited series {} of chart {shown}", s + 1))
        }
        "remove" => {
            let s = w.place("a series number")?;
            book.session.remove_series(at, index, s)?;
            Ok(format!("Removed series {} from chart {shown}", s + 1))
        }
        "move" => {
            let s = w.place("a series number")?;
            let later = match w.need("earlier or later")? {
                "later" => true,
                "earlier" => false,
                other => return Err(format!("`{other}` is not earlier or later")),
            };
            book.session.move_series(at, index, s, later)?;
            Ok(format!("Moved series {} of chart {shown}", s + 1))
        }
        "type" => series_type(book, at, index, &mut w),
        "secondary" => {
            let s = w.place("a series number")?;
            let on = w.onoff("on or off")?;
            book.session.set_series_secondary(at, index, s, on)?;
            Ok(format!("Series {} of chart {shown} is on the secondary axis: {on}", s + 1))
        }
        "hide" => {
            let s = w.place("a series number")?;
            let on = if w.done() { true } else { w.onoff("on or off")? };
            book.session.filter_series(at, index, s, on)?;
            Ok(format!("Series {} of chart {shown} hidden: {on}", s + 1))
        }
        "format" => series_format(book, at, index, &mut w),
        other => Err(format!("chart-series takes add, edit, remove, move, type, secondary, hide or format, not `{other}`")),
    }
}

/// `type <s> <type>`: one series drawn as another type, which makes a combination chart.
fn series_type(book: &mut Book, at: usize, index: usize, w: &mut Words) -> Result<String, String> {
    let s = w.place("a series number")?;
    let word = w.need("a chart type")?;
    let ty = ChartType::parse(word)
        .ok_or_else(|| format!("`{word}` is not a chart type; `chart-types` lists them"))?;
    book.session.change_series_type(at, index, s, ty)?;
    Ok(format!("Series {} of chart {} is now {}", s + 1, index + 1, ty.name()))
}

/// What a series reads, from `values <range>`, `name <cell or words>`, `categories <range>` and
/// `sizes <range>`. For a new series the first bare word is its values. `none` takes a part away.
fn source_of(book: &Book, w: &mut Words, adding: bool) -> Result<SeriesSource, String> {
    let mut source = SeriesSource::default();
    if adding {
        let first = w.need("the range of the values")?;
        source.values = Some(reference(book, first)?);
    }
    while let Some(key) = w.take() {
        let word = w.need("a range after the key")?;
        let optional = |text: &str, make: &dyn Fn(&str) -> Result<String, String>| {
            if text == "none" {
                Ok(None)
            } else {
                make(text).map(Some)
            }
        };
        match key {
            "values" => source.values = Some(reference(book, word)?),
            "name" => source.name = Some(optional(word, &|t| name_source(book, t))?),
            "categories" => source.categories = Some(optional(word, &|t| reference(book, t))?),
            "sizes" => source.sizes = Some(optional(word, &|t| reference(book, t))?),
            other => return Err(format!("`{other}` is not values, name, categories or sizes")),
        }
    }
    Ok(source)
}

/// `format <s> [point <p>] [fill #|none] [line #|none [width]] [marker <symbol>] [marker-size <n>]
/// [smooth on|off] [explosion <n>]`.
fn series_format(
    book: &mut Book,
    at: usize,
    index: usize,
    w: &mut Words,
) -> Result<String, String> {
    let s = w.place("a series number")?;
    let (mut point, mut options) = (None, SeriesOptions::default());
    while let Some(key) = w.take() {
        match key {
            "point" => point = Some(w.place("a point number")? as u32),
            "fill" => options.fill = Some(fill_word(w.need("a colour or none")?)?),
            "line" => options.line = Some(line_word(w, 1.5)?),
            "marker" => options.marker_symbol = Some(w.need("a marker symbol")?.to_owned()),
            "marker-size" => options.marker_size = Some(w.whole("a marker size")?),
            "smooth" => options.smooth = Some(w.onoff("on or off")?),
            "explosion" => options.explosion = Some(w.whole("an explosion percentage")?),
            other => return Err(format!("`{other}` is not a series format")),
        }
    }
    book.session.set_series_format(at, index, s, point, options)?;
    Ok(format!("Formatted series {} of chart {}", s + 1, index + 1))
}

// ------------------------------------------------------------------------------------------ the labels

/// The position a word names; `auto` puts it back to Excel's choice.
fn label_position(word: &str) -> Result<Option<LabelPosition>, String> {
    Ok(Some(match word {
        "auto" => return Ok(None),
        "center" => LabelPosition::Center,
        "inside-end" => LabelPosition::InsideEnd,
        "inside-base" => LabelPosition::InsideBase,
        "outside-end" => LabelPosition::OutsideEnd,
        "left" => LabelPosition::Left,
        "right" => LabelPosition::Right,
        "above" => LabelPosition::Above,
        "below" => LabelPosition::Below,
        "best-fit" => LabelPosition::BestFit,
        other => return Err(format!("`{other}` is not a label position")),
    }))
}

/// The separator a word names, or the word itself.
fn separator(word: &str) -> Option<String> {
    Some(match word {
        "auto" => return None,
        "comma" => ", ".to_owned(),
        "semicolon" => "; ".to_owned(),
        "period" => ". ".to_owned(),
        "newline" => "\n".to_owned(),
        "space" => " ".to_owned(),
        other => other.to_owned(),
    })
}

/// `chart-labels <n> [series <s>] on|off [value] [category] [series-name] [percent] [bubble-size] [key]
/// [position <p>] [separator <s>] [format <code> [linked]]`. Named parts are shown and the rest hidden.
fn labels(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let mut which = None;
    if w.peek() == Some("series") {
        w.take();
        which = Some(w.place("a series number")?);
    }
    let show = w.onoff("on or off")?;
    let mut options = LabelOptions::default();
    let mut named = false;
    while let Some(key) = w.take() {
        named |= label_option(&mut options, key, &mut w)?;
    }
    if named {
        fill_unnamed(&mut options);
    }
    book.session.set_data_labels(at, index, which, show, options)?;
    Ok(format!("Data labels of chart {} set", index + 1))
}

/// Read one label option; answers whether it was one of the parts a label shows.
fn label_option(options: &mut LabelOptions, key: &str, w: &mut Words) -> Result<bool, String> {
    match key {
        "value" => options.show_value = Some(true),
        "category" => options.show_category = Some(true),
        "series-name" => options.show_series = Some(true),
        "percent" => options.show_percent = Some(true),
        "bubble-size" => options.show_bubble_size = Some(true),
        "key" => options.show_legend_key = Some(true),
        "position" => {
            options.position = Some(label_position(w.need("a position")?)?);
            return Ok(false);
        }
        "separator" => {
            options.separator = Some(separator(w.need("a separator")?));
            return Ok(false);
        }
        "format" => {
            let code = w.need("a number format")?;
            let linked = w.peek() == Some("linked");
            if linked {
                w.take();
            }
            options.number_format = Some((code != "auto").then(|| (code.to_owned(), linked)));
            return Ok(false);
        }
        other => return Err(format!("`{other}` is not a data label option")),
    }
    Ok(true)
}

/// Hide the parts of a label the words did not name.
fn fill_unnamed(options: &mut LabelOptions) {
    for part in [
        &mut options.show_value,
        &mut options.show_category,
        &mut options.show_series,
        &mut options.show_percent,
        &mut options.show_bubble_size,
        &mut options.show_legend_key,
    ] {
        part.get_or_insert(false);
    }
}

// ------------------------------------------------------------------------------------------ trendlines

/// The kind of trendline a word names, with a default order or period.
fn trend_kind(word: &str, n: u32) -> Option<TrendKind> {
    Some(match word {
        "linear" => TrendKind::Linear,
        "exp" | "exponential" => TrendKind::Exponential,
        "log" | "logarithmic" => TrendKind::Logarithmic,
        "poly" | "polynomial" => TrendKind::Polynomial(n),
        "power" => TrendKind::Power,
        "average" | "moving-average" => TrendKind::MovingAverage(n),
        _ => return None,
    })
}

/// A trendline from words, over the one it starts from.
fn trend_from(mut base: Trendline, w: &mut Words) -> Result<Trendline, String> {
    let mut n = None;
    while let Some(key) = w.take() {
        match key {
            "order" | "period" => n = Some(w.whole("an order or a period")?),
            "forward" => base.forward = w.number("periods forward")?,
            "backward" => base.backward = w.number("periods backward")?,
            "intercept" => base.intercept = Some(w.number("an intercept")?),
            "no-intercept" => base.intercept = None,
            "equation" => base.show_equation = true,
            "r2" => base.show_r_squared = true,
            "name" => base.name = Some(w.text(&[])),
            word => {
                base.kind = trend_kind(word, 2)
                    .ok_or_else(|| format!("`{word}` is not a trendline option"))?
            }
        }
    }
    base.kind = match (base.kind, n) {
        (TrendKind::Polynomial(_), Some(n)) => TrendKind::Polynomial(n),
        (TrendKind::MovingAverage(_), Some(n)) => TrendKind::MovingAverage(n),
        (kind, _) => kind,
    };
    Ok(base)
}

/// `chart-trendline <n> <s> add <kind> [order <k>|period <k>] [forward <v>] [backward <v>] [intercept <v>]
/// [equation] [r2] [name <words>]`, `edit <t> <same>` and `remove <t>`.
fn trendline(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let s = w.place("a series number")?;
    let shown = format!("series {} of chart {}", s + 1, index + 1);
    match w.need("add, edit or remove")? {
        "add" => {
            let line = trend_from(Trendline::default(), &mut w)?;
            let made = book.session.add_trendline(at, index, s, line)?;
            Ok(format!("Added trendline {} to {shown}", made + 1))
        }
        "edit" => {
            let t = w.place("a trendline number")?;
            let base = book.session.book.sheets[at].charts[index]
                .series
                .get(s)
                .and_then(|x| x.fmt.trendlines.get(t))
                .cloned()
                .ok_or("There is no such trendline.")?;
            let line = trend_from(base, &mut w)?;
            book.session.edit_trendline(at, index, s, t, line)?;
            Ok(format!("Edited trendline {} of {shown}", t + 1))
        }
        "remove" => {
            let t = w.place("a trendline number")?;
            book.session.remove_trendline(at, index, s, t)?;
            Ok(format!("Removed trendline {} from {shown}", t + 1))
        }
        other => Err(format!("chart-trendline takes add, edit or remove, not `{other}`")),
    }
}

// ------------------------------------------------------------------------------------------ error bars

/// Where custom error amounts come from: numbers joined by commas, or a range.
fn error_source(book: &Book, word: &str) -> Result<ErrorSource, String> {
    let numbers: Option<Vec<f64>> = word.split(',').map(|p| p.trim().parse().ok()).collect();
    match numbers {
        Some(list) => Ok(ErrorSource::Numbers(list)),
        None => reference(book, word).map(ErrorSource::Range),
    }
}

/// The direction the series' error bars default to: Y on a scatter or bubble series, none elsewhere.
fn default_direction(book: &Book, at: usize, index: usize, s: usize) -> Option<ErrorDirection> {
    let chart = &book.session.book.sheets[at].charts[index];
    let group = chart.series.get(s).and_then(|x| chart.spec.groups.get(x.group));
    group
        .filter(|g| matches!(g.kind, GroupKind::Scatter | GroupKind::Bubble))
        .map(|_| ErrorDirection::Y)
}

/// `chart-errors <n> <s> [x|y] plus|minus|both fixed|percent|stddev|stderr|custom [value] [plus-values <r>]
/// [minus-values <r>] [no-cap]`, or `chart-errors <n> <s> remove [x|y]`.
fn errors(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let s = w.place("a series number")?;
    let mut direction = default_direction(book, at, index, s);
    let mut bars = ErrorBars { direction, ..ErrorBars::default() };
    while let Some(key) = w.take() {
        match key {
            "remove" => {
                let wanted = match w.take() {
                    Some("x") => Some(ErrorDirection::X),
                    Some("y") => Some(ErrorDirection::Y),
                    _ => direction,
                };
                book.session.remove_error_bars(at, index, s, wanted)?;
                return Ok(format!("Removed error bars from series {}", s + 1));
            }
            "x" => direction = Some(ErrorDirection::X),
            "y" => direction = Some(ErrorDirection::Y),
            "plus" => bars.sides = ErrorSides::Plus,
            "minus" => bars.sides = ErrorSides::Minus,
            "both" => bars.sides = ErrorSides::Both,
            "fixed" => bars.amount = ErrorAmount::Fixed,
            "percent" => bars.amount = ErrorAmount::Percentage,
            "stddev" => bars.amount = ErrorAmount::StandardDeviation,
            "stderr" => bars.amount = ErrorAmount::StandardError,
            "custom" => bars.amount = ErrorAmount::Custom,
            "no-cap" => bars.no_cap = true,
            "plus-values" => bars.plus = Some(error_source(book, w.need("values")?)?),
            "minus-values" => bars.minus = Some(error_source(book, w.need("values")?)?),
            number => {
                bars.value = Some(
                    number.parse().map_err(|_| format!("`{number}` is not an error bar option"))?,
                )
            }
        }
    }
    bars.direction = direction;
    book.session.set_error_bars(at, index, s, bars)?;
    Ok(format!("Error bars set on series {} of chart {}", s + 1, index + 1))
}

// ------------------------------------------------------------------------------------------ the group

/// `chart-group <n> [group <g>] gap <v> overlap <v> angle <v> hole <v> vary on|off second-size <v>
/// split position|value|percent|custom split-at <v> bubble-scale <v> drop-lines on|off high-low on|off
/// up-down on|off series-lines on|off`.
fn group(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let (mut which, mut options) = (0, GroupOptions::default());
    while let Some(key) = w.take() {
        match key {
            "group" => which = w.place("a group number")?,
            "gap" => options.gap_width = Some(w.whole("a gap width")?),
            "overlap" => options.overlap = Some(w.number("an overlap")? as i32),
            "angle" => options.first_slice_angle = Some(w.whole("an angle")?),
            "hole" => options.hole_size = Some(w.whole("a hole size")?),
            "vary" => options.vary_colors = Some(w.onoff("on or off")?),
            "second-size" => options.second_plot_size = Some(w.whole("a size")?),
            "split" => options.split_by = Some(split_by(w.need("a split")?)?),
            "split-at" => options.split_at = Some(w.number("a split value")?),
            "bubble-scale" => options.bubble_scale = Some(w.whole("a bubble scale")?),
            "drop-lines" => options.drop_lines = Some(w.onoff("on or off")?),
            "high-low" => options.high_low_lines = Some(w.onoff("on or off")?),
            "up-down" => options.up_down_bars = Some(w.onoff("on or off")?),
            "series-lines" => options.series_lines = Some(w.onoff("on or off")?),
            other => return Err(format!("`{other}` is not a group option")),
        }
    }
    book.session.set_group_options(at, index, which, options)?;
    Ok(format!("Group {} of chart {} set", which + 1, index + 1))
}

/// How a pie of pie or bar of pie splits its second plot.
fn split_by(word: &str) -> Result<SplitBy, String> {
    match word {
        "auto" => Ok(SplitBy::Auto),
        "position" => Ok(SplitBy::Position),
        "value" => Ok(SplitBy::Value),
        "percent" => Ok(SplitBy::Percent),
        "custom" => Ok(SplitBy::Custom),
        other => Err(format!("`{other}` is not a way to split")),
    }
}
