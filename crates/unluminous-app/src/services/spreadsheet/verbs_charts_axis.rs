//! The chart verbs about axes and the look of chart elements (`task-2360`, the `chartwire` lane):
//! `chart-axis` (Format Axis: bounds, units, log scale, reverse order, where the other axis crosses, number
//! format, tick marks, labels, gridlines, title) and `chart-format` (fill, border and text of the chart
//! area, plot area, title, legend, an axis, an axis title or data labels).

use unluminous_sheet::chartspec::{Crosses, TickLabels, TickMark, TimeUnit};
use unluminous_sheet::edit::charts::{AxisOptions, AxisSelector, ChartElement, ElementFormat};

use super::verbs_a::Out;
use super::verbs_charts::{chart_on, colour, fill_word, line_word, Words};
use super::{Book, Spreadsheet};

/// Run a verb this file owns, or answer `None` for one it does not.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "chart-axis" => chart_on(sheet, args, axis),
        "chart-format" => chart_on(sheet, args, format),
        _ => return None,
    })
}

/// The words that start an axis option, so a title running over several words stops at the next one.
const AXIS_KEYS: [&str; 19] = [
    "min",
    "max",
    "major",
    "minor",
    "log",
    "reverse",
    "crosses",
    "format",
    "major-tick",
    "minor-tick",
    "labels",
    "visible",
    "major-gridlines",
    "minor-gridlines",
    "title",
    "between",
    "label-skip",
    "mark-skip",
    "unit",
];

/// The axis a word names.
fn selector(word: &str) -> Result<AxisSelector, String> {
    match word {
        "category" | "x" => Ok(AxisSelector::Category),
        "value" | "y" => Ok(AxisSelector::Value),
        "secondary-category" => Ok(AxisSelector::SecondaryCategory),
        "secondary-value" => Ok(AxisSelector::SecondaryValue),
        other => {
            Err(format!("`{other}` is not category, value, secondary-category or secondary-value"))
        }
    }
}

/// A number, or `auto` which puts the setting back to Excel's choice.
fn automatic(w: &mut Words, what: &str) -> Result<Option<f64>, String> {
    if w.peek() == Some("auto") {
        w.take();
        return Ok(None);
    }
    w.number(what).map(Some)
}

/// A tick mark style.
fn tick(word: &str) -> Result<TickMark, String> {
    match word {
        "none" => Ok(TickMark::None),
        "inside" => Ok(TickMark::Inside),
        "outside" => Ok(TickMark::Outside),
        "cross" => Ok(TickMark::Cross),
        other => Err(format!("`{other}` is not none, inside, outside or cross")),
    }
}

/// Where the axis labels sit.
fn tick_labels(word: &str) -> Result<TickLabels, String> {
    match word {
        "next" => Ok(TickLabels::NextTo),
        "low" => Ok(TickLabels::Low),
        "high" => Ok(TickLabels::High),
        "none" => Ok(TickLabels::Hidden),
        other => Err(format!("`{other}` is not next, low, high or none")),
    }
}

/// Where the other axis crosses this one.
fn crosses(w: &mut Words) -> Result<Crosses, String> {
    match w.need("auto, min, max or a value")? {
        "auto" => Ok(Crosses::Automatic),
        "min" => Ok(Crosses::Minimum),
        "max" => Ok(Crosses::Maximum),
        number => number
            .parse()
            .map(Crosses::At)
            .map_err(|_| format!("`{number}` is not auto, min, max or a value")),
    }
}

/// A unit of time for a date axis.
fn time_unit(word: &str) -> Result<Option<TimeUnit>, String> {
    match word {
        "auto" => Ok(None),
        "days" => Ok(Some(TimeUnit::Days)),
        "months" => Ok(Some(TimeUnit::Months)),
        "years" => Ok(Some(TimeUnit::Years)),
        other => Err(format!("`{other}` is not days, months, years or auto")),
    }
}

/// `chart-axis <n> category|value|secondary-category|secondary-value <option> <value> ...` where an option is
/// `min`, `max`, `major`, `minor` (a number or `auto`), `log <base>|off`, `reverse on|off`,
/// `crosses auto|min|max|<value>`, `format <code>|auto [linked]`, `major-tick` and `minor-tick`
/// `none|inside|outside|cross`, `labels next|low|high|none`, `visible on|off`,
/// `major-gridlines on|off`, `minor-gridlines on|off`, `title <words>|off`, `between on|off`,
/// `label-skip` and `mark-skip` (a number or `auto`) and `unit days|months|years|auto`.
fn axis(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let which = selector(w.need("the axis")?)?;
    let mut options = AxisOptions::default();
    let mut any = false;
    while let Some(key) = w.take() {
        axis_option(&mut options, key, &mut w)?;
        any = true;
    }
    if !any {
        return Err("chart-axis needs an option such as min 0 max 100".to_owned());
    }
    book.session.set_axis_options(at, index, which, options)?;
    Ok(format!("Axis of chart {} set", index + 1))
}

/// Read one axis option and the words that belong to it.
fn axis_option(options: &mut AxisOptions, key: &str, w: &mut Words) -> Result<(), String> {
    match key {
        "min" => options.minimum = Some(automatic(w, "a minimum")?),
        "max" => options.maximum = Some(automatic(w, "a maximum")?),
        "major" => options.major_unit = Some(automatic(w, "a major unit")?),
        "minor" => options.minor_unit = Some(automatic(w, "a minor unit")?),
        "log" => {
            options.log_base = Some(if w.peek() == Some("off") {
                w.take();
                None
            } else {
                Some(w.number("a base")?)
            })
        }
        "reverse" => options.reverse = Some(w.onoff("on or off")?),
        "crosses" => options.crosses = Some(crosses(w)?),
        "format" => {
            let code = w.need("a number format")?;
            let linked = w.peek() == Some("linked");
            if linked {
                w.take();
            }
            options.number_format = Some((code != "auto").then(|| (code.to_owned(), linked)));
        }
        "major-tick" => options.major_tick = Some(tick(w.need("a tick mark")?)?),
        "minor-tick" => options.minor_tick = Some(tick(w.need("a tick mark")?)?),
        "labels" => options.tick_labels = Some(tick_labels(w.need("where the labels go")?)?),
        "visible" => options.visible = Some(w.onoff("on or off")?),
        "major-gridlines" => options.major_gridlines = Some(w.onoff("on or off")?),
        "minor-gridlines" => options.minor_gridlines = Some(w.onoff("on or off")?),
        "title" => {
            let text = w.text(&AXIS_KEYS);
            options.title = Some(Some(text).filter(|t| t != "off" && !t.is_empty()));
        }
        "between" => options.between = Some(w.onoff("on or off")?),
        "label-skip" => options.label_skip = Some(automatic(w, "a number")?.map(|n| n as u32)),
        "mark-skip" => options.mark_skip = Some(automatic(w, "a number")?.map(|n| n as u32)),
        "unit" => options.base_unit = Some(time_unit(w.need("a unit of time")?)?),
        other => return Err(format!("`{other}` is not an axis option")),
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ elements

/// The element a word names, with the word after it when it needs one.
fn element(w: &mut Words) -> Result<ChartElement, String> {
    match w.need("an element")? {
        "area" => Ok(ChartElement::ChartArea),
        "plot" => Ok(ChartElement::PlotArea),
        "title" => Ok(ChartElement::Title),
        "legend" => Ok(ChartElement::Legend),
        "axis" => Ok(ChartElement::Axis(selector(w.need("the axis")?)?)),
        "axis-title" => Ok(ChartElement::AxisTitle(selector(w.need("the axis")?)?)),
        "labels" => match w.need("a series number or all")? {
            "all" => Ok(ChartElement::DataLabels(None)),
            number => number
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .map(|n| ChartElement::DataLabels(Some(n)))
                .ok_or_else(|| format!("`{number}` is not a series number or all")),
        },
        other => {
            Err(format!("`{other}` is not area, plot, title, legend, axis, axis-title or labels"))
        }
    }
}

/// `chart-format <n> <element> [fill #|none] [line #|none [width]] [color #] [size <points>]
/// [bold on|off] [italic on|off]` where the element is `area`, `plot`, `title`, `legend`,
/// `axis <axis>`, `axis-title <axis>` or `labels <series>|all`.
fn format(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let which = element(&mut w)?;
    let mut f = ElementFormat::default();
    while let Some(key) = w.take() {
        match key {
            "fill" => f.fill = Some(fill_word(w.need("a colour or none")?)?),
            "line" => f.line = Some(line_word(&mut w, 1.0)?),
            "color" | "colour" | "font-color" => f.font_color = Some(colour(w.need("a colour")?)?),
            "size" => f.font_size = Some(w.number("a size in points")?),
            "bold" => f.bold = Some(w.onoff("on or off")?),
            "italic" => f.italic = Some(w.onoff("on or off")?),
            other => return Err(format!("`{other}` is not a chart format option")),
        }
    }
    book.session.set_element_format(at, index, which, f)?;
    Ok(format!("Formatted chart {}", index + 1))
}
