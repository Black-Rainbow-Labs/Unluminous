//! The levels of the command tree for pictures, shapes, text boxes, the chosen object and sparklines
//! (`task-2360`, the `wiringc` lane). Every entry names the words of a verb in [`super::verbs_c`] or
//! [`super::verbs_c_spark`], so pressing it and an agent's call are the same call.

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

fn choice(label: &str, key: &str, options: &[(&str, &str)], first: &str) -> Field {
    Field::choice(label, key, options, first)
}

/// The shapes the Insert level offers: label, the word `shape` takes.
const SHAPES: [(&str, &str); 5] = [
    ("Rectangle", "rectangle"),
    ("Rounded rectangle", "rounded"),
    ("Oval", "ellipse"),
    ("Line", "line"),
    ("Arrow", "arrow"),
];

/// The Insert level's pictures, shapes, text box and sparklines.
pub fn insert_objects() -> Item {
    sub(
        "Illustrations and sparklines",
        vec![
            sub(
                "Picture",
                vec![
                    run("From a file...", "", "picture pick"),
                    run("From the clipboard", "", "picture paste"),
                    ask("From a path...", "Path of a picture file", "picture insert"),
                ],
            ),
            sub(
                "Shapes",
                SHAPES
                    .iter()
                    .flat_map(|(label, word)| {
                        [
                            run(
                                &format!("Draw a {}", label.to_lowercase()),
                                "",
                                &format!("shape draw {word}"),
                            ),
                            run(
                                &format!("{label} at the active cell"),
                                "",
                                &format!("shape {word}"),
                            ),
                        ]
                    })
                    .collect(),
            ),
            run("Text box", "", "shape draw textbox"),
            sub("Sparklines", sparkline_inserts()),
        ],
    )
}

/// One form for each sparkline type: its data range and the cells it goes in.
fn sparkline_inserts() -> Vec<Item> {
    let fields = |kind: &str| {
        vec![
            choice(
                "Type",
                "type",
                &[("Line", "line"), ("Column", "column"), ("Win/Loss", "winloss")],
                kind,
            ),
            Field::argument("Data range", ""),
            Field::argument("Location range", ""),
        ]
    };
    vec![
        form("Line...", "sparkline insert", fields("line")),
        form("Column...", "sparkline insert", fields("column")),
        form("Win/Loss...", "sparkline insert", fields("winloss")),
    ]
}

/// The Sparkline level for the sparkline in the active cell: every option the group has.
pub fn sparkline_level(book: &Book) -> Item {
    let cell = book.view.cursor;
    let s = &book.session.book.sheets[book.view.sheet];
    let Some((g, _)) = book.session.sparkline_at(book.view.sheet, cell) else {
        return sub(
            "Sparkline",
            vec![run("Insert sparklines in the selection...", "", "sparkline insert line")],
        );
    };
    let group = &s.sparklines[g];
    let flag = |label: &str, word: &str, on: bool| {
        check(label, &format!("sparkline points {word} {}", if on { "off" } else { "on" }), on)
    };
    let part = |label: &str, p: &str| colors(label, &format!("sparkline color {p}"), None);
    sub(
        "Sparkline",
        vec![
            sub(
                "Type",
                vec![
                    run("Line", "", "sparkline kind line"),
                    run("Column", "", "sparkline kind column"),
                    run("Win/Loss", "", "sparkline kind winloss"),
                ],
            ),
            sub(
                "Show",
                vec![
                    flag("High point", "high", group.high),
                    flag("Low point", "low", group.low),
                    flag("Negative points", "negative", group.negative),
                    flag("First point", "first", group.first),
                    flag("Last point", "last", group.last),
                    flag("Markers", "markers", group.markers),
                ],
            ),
            sub(
                "Colors",
                vec![
                    part("Sparkline color", "series"),
                    part("Marker color: all", "markers"),
                    part("Marker color: negative points", "negative"),
                    part("Marker color: high point", "high"),
                    part("Marker color: low point", "low"),
                    part("Marker color: first point", "first"),
                    part("Marker color: last point", "last"),
                    part("Axis color", "axis"),
                ],
            ),
            ask("Weight in points...", "Weight, 0 to 10 points", "sparkline weight"),
            axis_level(),
            sub(
                "Empty cells",
                vec![
                    run("Show as gaps", "", "sparkline empty gap"),
                    run("Show as zero", "", "sparkline empty zero"),
                    run("Connect data points with a line", "", "sparkline empty span"),
                ],
            ),
            check(
                "Show data in hidden rows and columns",
                &format!("sparkline hidden {}", if group.hidden_cells { "off" } else { "on" }),
                group.hidden_cells,
            ),
            ask("Date axis range...", "Range holding the dates, or off", "sparkline dates"),
            run("Show the horizontal axis", "", "sparkline haxis on"),
            run("Hide the horizontal axis", "", "sparkline haxis off"),
            run("Plot data right to left", "", "sparkline haxis on rtl"),
            ask("Edit the data...", "Data range", "sparkline data"),
            run("Group the selection", "", "sparkline group"),
            run("Ungroup", "", "sparkline ungroup"),
            run("Clear the selected sparklines", "", "sparkline clear"),
            run("Clear the group", "", "sparkline clear-group"),
        ],
    )
}

/// The vertical axis options of a sparkline group.
fn axis_level() -> Item {
    let bounds = |which: &str| {
        sub(
            &format!("Vertical axis {which}"),
            vec![
                run("Automatic for each sparkline", "", &format!("sparkline axis {which} auto")),
                run("Same for all sparklines", "", &format!("sparkline axis {which} same")),
                ask("Custom value...", "Value", &format!("sparkline axis {which}")),
            ],
        )
    };
    sub("Axis", vec![bounds("min"), bounds("max")])
}

/// The Object level for the chosen objects: fill, outline, text, size and properties, order and grouping.
pub fn object_level(book: &Book) -> Item {
    let chosen = &book.view.objects.selected;
    if chosen.is_empty() {
        return sub("Object", vec![run("Choose an object by clicking it", "", "objects selected")]);
    }
    let s = &book.session.book.sheets[book.view.sheet];
    let picture = chosen.iter().any(|id| s.drawing.find(*id).is_some_and(|o| o.picture.is_some()));
    let mut items = vec![
        colors("Shape fill", "object fill", Some("none")),
        sub("Shape outline", outline_items()),
        form("Text...", "object text", vec![Field::argument("Text", "")]),
        sub("Text format", text_items()),
        form(
            "Size and position...",
            "object frame",
            vec![
                Field::argument("Left in points", ""),
                Field::argument("Top in points", ""),
                Field::argument("Width in points", ""),
                Field::argument("Height in points", ""),
            ],
        ),
        sub("Properties", property_items()),
        ask("Alt text...", "Alternative text", "object alt"),
        ask("Name...", "Name", "object name"),
        ask("Rotate by degrees...", "Degrees clockwise", "object rotate"),
        run("Flip horizontally", "", "object flip h"),
        run("Flip vertically", "", "object flip v"),
        sub("Arrange", arrange_items()),
        sub("Align", align_items()),
        run("Group", "", "object group"),
        run("Ungroup", "", "object ungroup"),
        run("Delete", "Delete", "object delete"),
    ];
    if picture {
        items.insert(
            2,
            sub(
                "Picture",
                vec![
                    form(
                        "Crop...",
                        "object crop",
                        vec![
                            Field::argument("Left, percent", "0"),
                            Field::argument("Top, percent", "0"),
                            Field::argument("Right, percent", "0"),
                            Field::argument("Bottom, percent", "0"),
                        ],
                    ),
                    run("Reset picture", "", "object reset"),
                    run("Reset picture and size", "", "object reset size"),
                    run("Place in cell", "", "picture in-cell"),
                ],
            ),
        );
    }
    sub("Object", items)
}

fn outline_items() -> Vec<Item> {
    vec![
        colors("Color", "object outline", Some("none")),
        ask("Weight in points...", "Weight in points", "object outline width"),
        run("Solid", "", "object outline dash solid"),
        run("Dashed", "", "object outline dash dash"),
        run("Dotted", "", "object outline dash dot"),
        run("Arrow at the end", "", "object outline tail arrow"),
        run("No arrow at the end", "", "object outline tail none"),
    ]
}

fn text_items() -> Vec<Item> {
    vec![
        ask("Size...", "Size in points", "object text-format size"),
        run("Bold", "", "object text-format bold on"),
        run("Not bold", "", "object text-format bold off"),
        run("Italic", "", "object text-format italic on"),
        run("Underline", "", "object text-format underline on"),
        colors("Color", "object text-format color", None),
        ask("Font...", "Font name", "object text-format font"),
        run("Align left", "", "object text-format align left"),
        run("Centre", "", "object text-format align centre"),
        run("Align right", "", "object text-format align right"),
        run("Top", "", "object text-format valign top"),
        run("Middle", "", "object text-format valign middle"),
        run("Bottom", "", "object text-format valign bottom"),
    ]
}

fn property_items() -> Vec<Item> {
    vec![
        run("Move and size with cells", "", "object placement move-size"),
        run("Move but do not size with cells", "", "object placement move"),
        run("Do not move or size with cells", "", "object placement fixed"),
        run("Lock aspect ratio", "", "object lock-aspect on"),
        run("Free aspect ratio", "", "object lock-aspect off"),
        ask("Transparency percent...", "Transparency, 0 to 100", "object transparency"),
    ]
}

fn arrange_items() -> Vec<Item> {
    vec![
        run("Bring to front", "", "object order front"),
        run("Bring forward", "", "object order forward"),
        run("Send backward", "", "object order backward"),
        run("Send to back", "", "object order back"),
    ]
}

fn align_items() -> Vec<Item> {
    let mut items: Vec<Item> = ["left", "centre", "right", "top", "middle", "bottom"]
        .iter()
        .map(|w| run(&format!("Align {w}"), "", &format!("object align {w}")))
        .collect();
    items.push(run("Distribute horizontally", "", "object distribute horizontal"));
    items.push(run("Distribute vertically", "", "object distribute vertical"));
    items
}

/// The right click menu of the chosen objects.
pub fn object_menu(book: &Book) -> Vec<Item> {
    let mut items = vec![
        run("Cut", "Ctrl+X", "cut"),
        run("Copy", "Ctrl+C", "copy"),
        Item::Line,
        sub("Arrange", arrange_items()),
        sub("Align", align_items()),
        run("Group", "", "object group"),
        run("Ungroup", "", "object ungroup"),
        Item::Line,
        ask("Edit alt text...", "Alternative text", "object alt"),
        run("Format object...", "Ctrl+1", "panel"),
        run("Delete", "Delete", "object delete"),
    ];
    if let Item::Sub { items: more, .. } = object_level(book) {
        items.extend(more.into_iter().filter(|i| matches!(i.label(), "Picture")));
    }
    items
}
