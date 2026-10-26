//! The levels of the command tree for charts (`task-2360`, the `chartwire` lane): the Insert level's chart
//! types, the Chart level of the Format panel and workbook menu for the chart chosen, and the menu a right
//! click on a chart opens. Every entry names the words of a verb in [`super::verbs_charts`] and its
//! siblings, so pressing it and an agent's call are the same call. A dialog Excel has for a chart becomes a
//! level that drills down or a small form, never a window with tabs.

use unluminous_sheet::chartspec::ChartType;
use unluminous_sheet::model::{Chart, ChartKind};

use super::catalogue::{form, Item};
use super::view::Field;
use super::Book;

fn words(text: &str) -> Vec<String> {
    text.split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

fn run(label: &str, keys: &'static str, text: &str) -> Item {
    Item::Run { label: label.to_owned(), keys, words: words(text), checked: false }
}

fn check(label: &str, text: &str, checked: bool) -> Item {
    Item::Run { label: label.to_owned(), keys: "", words: words(text), checked }
}

fn ask(label: &str, field: &str, text: &str) -> Item {
    Item::Ask {
        label: label.to_owned(),
        title: label.trim_end_matches("...").to_owned(),
        field: field.to_owned(),
        words: words(text),
        second: None,
        initial: String::new(),
    }
}

fn sub(label: &str, items: Vec<Item>) -> Item {
    Item::Sub { label: label.to_owned(), items }
}

fn colors(label: &str, text: &str, none: Option<&str>) -> Item {
    Item::Colors { label: label.to_owned(), words: words(text), none: none.map(str::to_owned) }
}

/// A type word as a label: `column-stacked-100` becomes `Column stacked 100`.
fn type_label(ty: ChartType) -> String {
    let text = ty.name().replace('-', " ");
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Every chart type, grouped by family, each running `<verb> <type>`.
fn type_families(verb: &str) -> Vec<Item> {
    let mut families: Vec<(String, Vec<Item>)> = Vec::new();
    for ty in ChartType::ALL.iter().copied().filter(|t| *t != ChartType::Map) {
        let family = ChartKind::of_type(ty).name().to_owned();
        let item = run(&type_label(ty), "", &format!("{verb} {}", ty.name()));
        match families.iter_mut().find(|(name, _)| *name == family) {
            Some((_, items)) => items.push(item),
            None => families.push((family, vec![item])),
        }
    }
    families
        .into_iter()
        .map(|(name, items)| match <[Item; 1]>::try_from(items) {
            Ok([only]) => only,
            Err(items) => sub(&type_label_of(&name), items),
        })
        .collect()
}

/// A family name with a capital letter.
fn type_label_of(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The Insert level's Chart entry: recommended charts, a chart sheet, and every type Excel offers.
pub fn insert_charts() -> Item {
    let mut items = vec![
        run("Recommended charts", "", "chart recommend"),
        run("Chart sheet", "F11", "chart sheet"),
        Item::Line,
    ];
    items.extend(type_families("chart"));
    sub("Chart", items)
}

/// The chart chosen on the sheet showing, if there is one.
fn chosen(book: &Book) -> Option<(usize, &Chart)> {
    let index = book.view.chart?;
    let chart = book.session.book.sheets[book.view.sheet].charts.get(index)?;
    Some((index + 1, chart))
}

/// The Chart level of the Format panel and the workbook menu.
pub fn chart_level(book: &Book) -> Item {
    let Some((n, chart)) = chosen(book) else {
        return sub(
            "Chart",
            vec![run("Choose a chart by clicking it", "", "charts"), insert_charts()],
        );
    };
    sub("Chart", chart_items(book, n, chart))
}

/// The menu a right click on a chart opens.
pub fn chart_menu(book: &Book) -> Vec<Item> {
    let Some((n, chart)) = chosen(book) else { return Vec::new() };
    vec![
        sub("Change chart type", type_families(&format!("chart-look {n} type"))),
        select_data(n),
        sub("Move chart", move_items(book, n)),
        Item::Line,
        sub("Title and legend", title_and_legend(n)),
        sub("Axes", axes(n, chart)),
        sub("Data labels", data_labels(n)),
        sub("Quick layout", layouts(n, chart)),
        Item::Line,
        sub("Format chart", format_items(n)),
        run("Delete", "Delete", &format!("chart delete {n}")),
    ]
}

/// Every entry of the Chart level.
fn chart_items(book: &Book, n: usize, chart: &Chart) -> Vec<Item> {
    vec![
        sub("Change chart type", type_families(&format!("chart-look {n} type"))),
        select_data(n),
        sub("Series", (0..chart.series.len()).map(|s| series_level(n, s, chart)).collect()),
        sub("Title and legend", title_and_legend(n)),
        sub("Axes", axes(n, chart)),
        sub("Data labels", data_labels(n)),
        sub("Data table", data_table(n)),
        sub("Group options", group_options(n)),
        sub("Quick layout", layouts(n, chart)),
        ask("Chart style (1 to 48)...", "Style number", &format!("chart-look {n} style")),
        sub("Format chart", format_items(n)),
        sub("Size, name and place", place_items(book, n)),
        run("Chart information", "", &format!("chart-info {n}")),
        run("Delete chart", "Delete", &format!("chart delete {n}")),
    ]
}

// ------------------------------------------------------------------------------------------ the data

/// Select Data: the range, switch row and column, series, and how hidden and empty cells are plotted.
fn select_data(n: usize) -> Item {
    sub(
        "Select data",
        vec![
            form(
                "Chart data range...",
                &format!("chart-data {n} range"),
                vec![
                    Field::argument("Range", ""),
                    Field::choice(
                        "Plot by",
                        "by",
                        &[
                            ("Automatic", ""),
                            ("Series in rows", "rows"),
                            ("Series in columns", "columns"),
                        ],
                        "",
                    ),
                ],
            ),
            run("Switch row and column", "", &format!("chart-data {n} switch")),
            form(
                "Add series...",
                &format!("chart-series {n} add"),
                vec![
                    Field::argument("Values", ""),
                    Field::text("Name", "name", ""),
                    Field::text("Axis labels", "categories", ""),
                    Field::text("Bubble sizes", "sizes", ""),
                ],
            ),
            form(
                "Hidden and empty cells...",
                &format!("chart-data {n} cells"),
                vec![
                    Field::choice(
                        "Hidden rows and columns",
                        "hidden",
                        &[("Not plotted", "visible"), ("Plotted", "hidden")],
                        "visible",
                    ),
                    Field::choice(
                        "Empty cells",
                        "empty",
                        &[("Gaps", "gap"), ("Zero", "zero"), ("Join with a line", "span")],
                        "gap",
                    ),
                ],
            ),
        ],
    )
}

/// The level for one series: order, axis, type, look, labels, trendlines and error bars.
fn series_level(n: usize, s: usize, chart: &Chart) -> Item {
    let k = s + 1;
    let hidden = chart.series[s].fmt.filtered;
    let has_labels = chart.series[s].fmt.labels.as_ref().is_some_and(|l| !l.delete);
    let label = chart.series[s].name.clone().unwrap_or_default();
    let name =
        if label.is_empty() { format!("Series {k}") } else { format!("Series {k} ({label})") };
    sub(
        &name,
        vec![
            ask(
                "Edit values...",
                "Range of the values",
                &format!("chart-series {n} edit {k} values"),
            ),
            ask("Edit name...", "Cell or words", &format!("chart-series {n} edit {k} name")),
            ask("Edit axis labels...", "Range", &format!("chart-series {n} edit {k} categories")),
            run("Move earlier", "", &format!("chart-series {n} move {k} earlier")),
            run("Move later", "", &format!("chart-series {n} move {k} later")),
            run("Remove series", "", &format!("chart-series {n} remove {k}")),
            Item::Line,
            sub("Series chart type", type_families(&format!("chart-series {n} type {k}"))),
            run("On the secondary axis", "", &format!("chart-series {n} secondary {k} on")),
            run("On the primary axis", "", &format!("chart-series {n} secondary {k} off")),
            check(
                "Hide this series",
                &format!("chart-series {n} hide {k} {}", if hidden { "off" } else { "on" }),
                hidden,
            ),
            Item::Line,
            colors("Fill", &format!("chart-series {n} format {k} fill"), Some("none")),
            colors("Line", &format!("chart-series {n} format {k} line"), Some("none")),
            check("Smooth line", &format!("chart-series {n} format {k} smooth on"), false),
            sub("Data labels", series_labels(n, k, has_labels)),
            sub("Trendline", trendlines(n, k)),
            sub("Error bars", error_bars(n, k)),
        ],
    )
}

/// Data labels of one series.
fn series_labels(n: usize, k: usize, shown: bool) -> Vec<Item> {
    vec![
        check("Show data labels", &format!("chart-labels {n} series {k} on"), shown),
        run("Hide data labels", "", &format!("chart-labels {n} series {k} off")),
        run("Show the value", "", &format!("chart-labels {n} series {k} on value")),
        run(
            "Show the category and value",
            "",
            &format!("chart-labels {n} series {k} on category value"),
        ),
        run("Show the percentage", "", &format!("chart-labels {n} series {k} on percent")),
    ]
}

/// The trendline kinds a series offers, and removing them.
fn trendlines(n: usize, k: usize) -> Vec<Item> {
    let line =
        |label: &str, kind: &str| run(label, "", &format!("chart-trendline {n} {k} add {kind}"));
    vec![
        line("Linear", "linear"),
        line("Exponential", "exp"),
        line("Logarithmic", "log"),
        line("Polynomial (order 2)", "poly order 2"),
        line("Power", "power"),
        line("Moving average (2 periods)", "average period 2"),
        line("Linear with equation and R squared", "linear equation r2"),
        Item::Line,
        run("Remove the first trendline", "", &format!("chart-trendline {n} {k} remove 1")),
    ]
}

/// Error bars of one series.
fn error_bars(n: usize, k: usize) -> Vec<Item> {
    let bars =
        |label: &str, kind: &str| run(label, "", &format!("chart-errors {n} {k} both {kind}"));
    vec![
        bars("Fixed value 1", "fixed 1"),
        bars("Percentage 5", "percent 5"),
        bars("Standard deviation", "stddev 1"),
        bars("Standard error", "stderr"),
        form(
            "Custom...",
            &format!("chart-errors {n} {k} both custom"),
            vec![
                Field::text("Plus values", "plus-values", ""),
                Field::text("Minus values", "minus-values", ""),
            ],
        ),
        Item::Line,
        run("Remove error bars", "", &format!("chart-errors {n} {k} remove")),
    ]
}

// ------------------------------------------------------------------------------------------ elements

/// The chart title and the legend.
fn title_and_legend(n: usize) -> Vec<Item> {
    let legend = |label: &str, at: &str| run(label, "", &format!("chart-legend {n} {at}"));
    vec![
        ask("Chart title...", "Words of the title", &format!("chart-title {n}")),
        run("No title", "", &format!("chart-title {n} off")),
        run("Title over the chart", "", &format!("chart-title {n} overlay on")),
        run("Title above the chart", "", &format!("chart-title {n} overlay off")),
        Item::Line,
        legend("Legend at the right", "right"),
        legend("Legend at the top", "top"),
        legend("Legend at the left", "left"),
        legend("Legend at the bottom", "bottom"),
        legend("Legend at the top right", "top-right"),
        legend("No legend", "off"),
    ]
}

/// The axes the chart has, each a level of its own.
fn axes(n: usize, chart: &Chart) -> Vec<Item> {
    if !chart.spec.groups.iter().any(|g| g.kind.has_axes()) {
        return vec![run("This chart has no axes", "", &format!("chart-info {n}"))];
    }
    let mut which = vec![("Horizontal axis", "category"), ("Vertical axis", "value")];
    if chart.spec.axes.len() > 2 {
        which.push(("Secondary horizontal axis", "secondary-category"));
        which.push(("Secondary vertical axis", "secondary-value"));
    }
    which.into_iter().map(|(label, axis)| sub(label, axis_level(n, axis))).collect()
}

/// The options of one axis.
fn axis_level(n: usize, axis: &str) -> Vec<Item> {
    let base = format!("chart-axis {n} {axis}");
    let one = |label: &str, key: &str| {
        ask(label, label.trim_end_matches("..."), &format!("{base} {key}"))
    };
    vec![
        one("Minimum...", "min"),
        one("Maximum...", "max"),
        one("Major unit...", "major"),
        one("Minor unit...", "minor"),
        run(
            "Bounds and units automatic",
            "",
            &format!("{base} min auto max auto major auto minor auto"),
        ),
        one("Logarithmic base...", "log"),
        run("Linear scale", "", &format!("{base} log off")),
        run("Values in reverse order", "", &format!("{base} reverse on")),
        run("Values in normal order", "", &format!("{base} reverse off")),
        one("Number format...", "format"),
        one("Axis title...", "title"),
        run("Show the axis", "", &format!("{base} visible on")),
        run("Hide the axis", "", &format!("{base} visible off")),
        run("Major gridlines on", "", &format!("{base} major-gridlines on")),
        run("Major gridlines off", "", &format!("{base} major-gridlines off")),
        run("Minor gridlines on", "", &format!("{base} minor-gridlines on")),
        run("Minor gridlines off", "", &format!("{base} minor-gridlines off")),
        sub("Major tick marks", ticks(&base, "major-tick")),
        sub("Minor tick marks", ticks(&base, "minor-tick")),
        sub(
            "Labels",
            ["next", "low", "high", "none"]
                .iter()
                .map(|w| run(&type_label_of(w), "", &format!("{base} labels {w}")))
                .collect(),
        ),
    ]
}

/// The tick mark styles for an axis.
fn ticks(base: &str, key: &str) -> Vec<Item> {
    ["none", "inside", "outside", "cross"]
        .iter()
        .map(|w| run(&type_label_of(w), "", &format!("{base} {key} {w}")))
        .collect()
}

/// Data labels for every series.
fn data_labels(n: usize) -> Vec<Item> {
    let at =
        |label: &str, place: &str| run(label, "", &format!("chart-labels {n} on position {place}"));
    vec![
        run("Show data labels", "", &format!("chart-labels {n} on")),
        run("No data labels", "", &format!("chart-labels {n} off")),
        run("Value only", "", &format!("chart-labels {n} on value")),
        run("Category and value", "", &format!("chart-labels {n} on category value")),
        run("Percentage", "", &format!("chart-labels {n} on percent")),
        Item::Line,
        at("Center", "center"),
        at("Inside end", "inside-end"),
        at("Inside base", "inside-base"),
        at("Outside end", "outside-end"),
        at("Best fit", "best-fit"),
        ask("Number format...", "Number format", &format!("chart-labels {n} on format")),
    ]
}

/// The data table under the chart.
fn data_table(n: usize) -> Vec<Item> {
    vec![
        run("Show the data table", "", &format!("chart-table {n} on")),
        run("Show it with legend keys only", "", &format!("chart-table {n} on keys")),
        run("Hide the data table", "", &format!("chart-table {n} off")),
    ]
}

/// The options that shape how the bars, slices or lines of a group are drawn.
fn group_options(n: usize) -> Vec<Item> {
    let one = |label: &str, key: &str| {
        ask(label, label.trim_end_matches("..."), &format!("chart-group {n} {key}"))
    };
    vec![
        one("Gap width...", "gap"),
        one("Series overlap...", "overlap"),
        one("Angle of the first slice...", "angle"),
        one("Doughnut hole size...", "hole"),
        one("Bubble scale...", "bubble-scale"),
        run("Vary colors by point", "", &format!("chart-group {n} vary on")),
        run("One color for the series", "", &format!("chart-group {n} vary off")),
        run("Drop lines", "", &format!("chart-group {n} drop-lines on")),
        run("High low lines", "", &format!("chart-group {n} high-low on")),
        run("Up down bars", "", &format!("chart-group {n} up-down on")),
        run("Series lines", "", &format!("chart-group {n} series-lines on")),
    ]
}

/// The quick layouts the chart's type offers.
fn layouts(n: usize, chart: &Chart) -> Vec<Item> {
    let count = unluminous_sheet::chartlayout::layout_count(chart.chart_type());
    (1..=count)
        .map(|k| run(&format!("Layout {k}"), "", &format!("chart-look {n} layout {k}")))
        .collect()
}

/// Fill, border and text of the parts of the chart.
fn format_items(n: usize) -> Vec<Item> {
    let part = |label: &str, element: &str| {
        sub(
            label,
            vec![
                colors("Fill", &format!("chart-format {n} {element} fill"), Some("none")),
                colors("Border", &format!("chart-format {n} {element} line"), Some("none")),
                colors("Text color", &format!("chart-format {n} {element} color"), None),
                ask("Text size...", "Size in points", &format!("chart-format {n} {element} size")),
                run("Bold", "", &format!("chart-format {n} {element} bold on")),
                run("Not bold", "", &format!("chart-format {n} {element} bold off")),
            ],
        )
    };
    vec![
        part("Chart area", "area"),
        part("Plot area", "plot"),
        part("Chart title", "title"),
        part("Legend", "legend"),
        part("Horizontal axis", "axis category"),
        part("Vertical axis", "axis value"),
        part("Data labels", "labels all"),
    ]
}

/// Name, alternative text, size and place, and moving the chart to another sheet.
fn place_items(book: &Book, n: usize) -> Vec<Item> {
    vec![
        ask("Name...", "Name", &format!("chart-name {n} name")),
        ask("Alternative text...", "Describe the chart", &format!("chart-name {n} alt")),
        form(
            "Size and position...",
            &format!("chart-move {n}"),
            vec![
                Field::argument("Left in points", ""),
                Field::argument("Top in points", ""),
                Field::argument("Width in points", ""),
                Field::argument("Height in points", ""),
            ],
        ),
        sub("Move chart", move_items(book, n)),
    ]
}

/// Moving a chart to a sheet, or to a new chart sheet.
fn move_items(book: &Book, n: usize) -> Vec<Item> {
    let mut items = vec![run("New chart sheet", "", &format!("chart-move {n} new-sheet"))];
    for sheet in &book.session.book.sheets {
        items.push(Item::Run {
            label: format!("Object in {}", sheet.name),
            keys: "",
            words: vec![
                "chart-move".to_owned(),
                n.to_string(),
                "sheet".to_owned(),
                sheet.name.clone(),
            ],
            checked: false,
        });
    }
    items
}
