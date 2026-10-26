//! The levels of the command tree for data tools, outlines, rules, what if, protection, styles, themes and
//! review. Every entry names the words of a verb in [`super::verbs_a`], so pressing it and an agent's call
//! are the same call.

use super::catalogue::{form, Item};
use super::view::Field;
use super::Book;

fn words(text: &str) -> Vec<String> {
    text.split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

fn run(label: &str, keys: &'static str, text: &str) -> Item {
    Item::Run { label: label.to_owned(), keys, words: words(text), checked: false }
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

fn colors(label: &str, text: &str) -> Item {
    Item::Colors { label: label.to_owned(), words: words(text), none: None }
}

/// The selection as `A1:B2`.
fn selected(book: &Book) -> String {
    book.view.range().a1()
}

fn choice(label: &str, key: &str, options: &[(&str, &str)], first: &str) -> Field {
    Field::choice(label, key, options, first)
}

/// Sort and Filter: the Sort popup as a level, and every kind of filter.
pub fn sort_and_filter(book: &Book) -> Item {
    let draft = &book.view.audit.sort;
    let mut sort = vec![
        run("Open the Sort levels", "", "sort-draft show"),
        run("Add level", "", "sort-draft add"),
        run("Reset levels", "", "sort-draft reset"),
        run("Sort by the levels", "", "sort-draft apply"),
        sub(
            "Options",
            vec![
                run("My data has headers", "", "sort-draft options header"),
                run("No headers", "", "sort-draft options noheader"),
                run("Guess the header", "", "sort-draft options guess"),
                run("Case sensitive", "", "sort-draft options case"),
                run("Sort left to right", "", "sort-draft options left-to-right"),
                run("Sort top to bottom", "", "sort-draft options top-to-bottom"),
            ],
        ),
    ];
    for (i, level) in draft.levels.iter().enumerate() {
        let n = i + 1;
        let name = format!(
            "Level {n}: {} {} {}",
            unluminous_sheet::coord::column_name(level.key),
            level.on,
            if level.descending { "Z to A" } else { "A to Z" }
        );
        sort.push(sub(
            &name,
            vec![
                form(
                    "Change...",
                    &format!("sort-draft set {n}"),
                    vec![
                        Field::text(
                            "Column",
                            "column",
                            &unluminous_sheet::coord::column_name(level.key),
                        ),
                        choice(
                            "Sort on",
                            "on",
                            &[
                                ("Cell values", "values"),
                                ("Cell color", "cell-color"),
                                ("Font color", "font-color"),
                                ("Cell icon", "icon"),
                            ],
                            &level.on,
                        ),
                        Field::text("Color or icon", "with", &level.with),
                        choice("Order", "order", &[("A to Z", "asc"), ("Z to A", "desc")], "asc"),
                    ],
                ),
                run("Copy level", "", &format!("sort-draft copy {n}")),
                run("Move up", "", &format!("sort-draft up {n}")),
                run("Move down", "", &format!("sort-draft down {n}")),
                run("Delete level", "", &format!("sort-draft delete {n}")),
            ],
        ));
    }
    sub(
        "Sort and Filter",
        vec![
            run("Sort A to Z", "", "sort asc"),
            run("Sort Z to A", "", "sort desc"),
            sub("Sort by levels", sort),
            run("Filter", "Ctrl+Shift+L", "filter"),
            run("Reapply", "Ctrl+Alt+L", "filter-reapply"),
            run("Clear", "", "filter-clear"),
            sub("Filter a column", filter_column_items()),
            form(
                "Advanced filter...",
                "advanced-filter",
                vec![
                    Field::bare("List range", &selected(book)),
                    Field::text("Criteria range", "criteria", ""),
                    Field::text("Copy to", "copy-to", ""),
                    Field::check("Unique records only", "unique", false),
                ],
            ),
            run("Show every row again", "", "advanced-filter show-all"),
        ],
    )
}

/// The filter kinds of the filter buttons, each asking for the column and its value.
fn filter_column_items() -> Vec<Item> {
    let text = |label: &str, kind: &str| {
        form(
            label,
            "filter-column",
            vec![
                Field::bare("Column", "A"),
                Field::choice("Kind", "kind", &[(label.trim_end_matches("..."), kind)], kind),
                Field::bare("Text", ""),
            ],
        )
    };
    vec![
        text("Equals...", "equals"),
        text("Does not equal...", "not-equals"),
        text("Begins with...", "begins"),
        text("Ends with...", "ends"),
        text("Contains...", "contains"),
        text("Does not contain...", "not-contains"),
        form(
            "Number filter...",
            "filter-column",
            vec![
                Field::bare("Column", "A"),
                Field::choice("Kind", "kind", &[("Number", "number")], "number"),
                Field::bare("Comparison and value, such as > 10", ""),
            ],
        ),
        form(
            "Between...",
            "filter-column",
            vec![
                Field::bare("Column", "A"),
                Field::choice("Kind", "kind", &[("Between", "between")], "between"),
                Field::bare("From", ""),
                Field::bare("To", ""),
            ],
        ),
        form(
            "Top or bottom...",
            "filter-column",
            vec![
                Field::bare("Column", "A"),
                Field::choice("Kind", "kind", &[("Top", "top"), ("Bottom", "bottom")], "top"),
                Field::bare("How many", "10"),
                Field::check("Percent", "percent", false),
            ],
        ),
        form(
            "Above or below average...",
            "filter-column",
            vec![
                Field::bare("Column", "A"),
                Field::choice(
                    "Kind",
                    "kind",
                    &[("Above average", "above-average"), ("Below average", "below-average")],
                    "above-average",
                ),
            ],
        ),
        form(
            "Dates...",
            "filter-column",
            vec![
                Field::bare("Column", "A"),
                Field::choice("Kind", "kind", &[("Dates", "dates")], "dates"),
                Field::bare("Year, 2026-03, 2026-03-05, or today, thisWeek, Q1, M3", ""),
            ],
        ),
        form(
            "Color...",
            "filter-column",
            vec![
                Field::bare("Column", "A"),
                Field::choice("Kind", "kind", &[("Color", "color")], "color"),
                Field::choice(
                    "Of",
                    "of",
                    &[("Cell color", "cell"), ("Font color", "font")],
                    "cell",
                ),
                Field::bare("Color, #RRGGBB or none", ""),
            ],
        ),
        form(
            "Icon...",
            "filter-column",
            vec![
                Field::bare("Column", "A"),
                Field::choice("Kind", "kind", &[("Icon", "icon")], "icon"),
                Field::bare("Icon, such as 3Arrows/1 or none", ""),
            ],
        ),
        ask("Show every row of a column...", "Column", "filter-column"),
    ]
}

/// Data tools: duplicates, Text to Columns, Consolidate, What If and tables.
pub fn data_tools(book: &Book) -> Item {
    sub(
        "Data tools",
        vec![
            form(
                "Remove duplicates...",
                "remove-duplicates",
                vec![
                    Field::bare("Range", &selected(book)),
                    Field::text("Columns, such as A,C", "columns", ""),
                    Field::choice(
                        "Header",
                        "h",
                        &[("My data has headers", "header"), ("No headers", "noheader")],
                        "header",
                    ),
                ],
            ),
            form(
                "Text to columns (delimited)...",
                "text-to-columns",
                vec![
                    Field::bare("Range", &selected(book)),
                    Field::choice("Mode", "m", &[("Delimited", "delimited")], "delimited"),
                    Field::check("Comma", "comma", true),
                    Field::check("Tab", "tab", false),
                    Field::check("Semicolon", "semicolon", false),
                    Field::check("Space", "space", false),
                    Field::text("Other", "other", ""),
                    Field::check("Treat consecutive delimiters as one", "consecutive", false),
                    Field::text("Text qualifier", "qualifier", "\""),
                    Field::text("Column formats, such as text,date:mdy,skip", "formats", ""),
                    Field::text("Destination", "to", ""),
                ],
            ),
            form(
                "Text to columns (fixed width)...",
                "text-to-columns",
                vec![
                    Field::bare("Range", &selected(book)),
                    Field::choice("Mode", "m", &[("Fixed width", "fixed")], "fixed"),
                    Field::bare("Break positions, such as 3,8,12", ""),
                    Field::text("Column formats", "formats", ""),
                ],
            ),
            form(
                "Consolidate...",
                "consolidate",
                vec![
                    Field::bare("Destination", &book.view.cursor.a1()),
                    Field::choice(
                        "Function",
                        "f",
                        &[
                            ("Sum", "sum"),
                            ("Count", "count"),
                            ("Average", "average"),
                            ("Max", "max"),
                            ("Min", "min"),
                            ("Product", "product"),
                            ("Count numbers", "count-numbers"),
                            ("StdDev", "stddev"),
                            ("StdDevp", "stddevp"),
                            ("Var", "var"),
                            ("Varp", "varp"),
                        ],
                        "sum",
                    ),
                    Field::bare("Source ranges", &selected(book)),
                    Field::check("Top row", "top-row", false),
                    Field::check("Left column", "left-column", false),
                    Field::check("Create links to source data", "links", false),
                ],
            ),
            sub("What if", what_if(book)),
            sub("Table", table_items()),
        ],
    )
}

fn what_if(book: &Book) -> Vec<Item> {
    vec![
        form(
            "Goal seek...",
            "goal-seek",
            vec![
                Field::bare("Set cell", &book.view.cursor.a1()),
                Field::bare("To value", "0"),
                Field::bare("By changing cell", ""),
            ],
        ),
        form(
            "Data table...",
            "data-table",
            vec![
                Field::bare("Range with its headers", &selected(book)),
                Field::text("Row input cell", "row", ""),
                Field::text("Column input cell", "column", ""),
            ],
        ),
        ask("Remove data table at...", "Cell in the table", "data-table remove"),
        sub("Scenario Manager", scenario_items(book)),
    ]
}

fn scenario_items(book: &Book) -> Vec<Item> {
    let index = book.view.sheet;
    let mut items = vec![
        form(
            "Add scenario...",
            "scenario add",
            vec![
                Field::bare("Name", ""),
                Field::bare("Changing cells", &selected(book)),
                Field::text("Values, separated by commas", "values", ""),
            ],
        ),
        ask("Summary report for result cells...", "Result cells", "scenario summary"),
        Item::Line,
    ];
    for s in book.session.scenarios(index).list {
        items.push(sub(
            &s.name,
            vec![
                run("Show", "", &format!("scenario show {}", s.name.replace(' ', "~"))),
                run("Delete", "", &format!("scenario delete {}", s.name.replace(' ', "~"))),
            ],
        ));
    }
    items
}

fn table_items() -> Vec<Item> {
    vec![
        ask("Resize to...", "New range", "table resize"),
        run("Totals row on", "", "table totals on"),
        run("Totals row off", "", "table totals off"),
        run("Header row on", "", "table header on"),
        run("Header row off", "", "table header off"),
        ask("Rename table...", "New name", "table rename"),
        run("Convert to range", "", "table to-range"),
        run("Add column", "", "table column add"),
        run("Add row", "", "table row add"),
        run("Calculated columns fill automatically", "", "table auto-fill on"),
    ]
}

/// Group and outline, with the level buttons and Subtotal.
pub fn outline(book: &Book) -> Item {
    sub(
        "Group and Outline",
        vec![
            run("Group", "Shift+Alt+Right", "group"),
            run("Ungroup", "Shift+Alt+Left", "ungroup"),
            run("Auto outline", "", "outline auto"),
            run("Clear outline", "", "outline clear"),
            run("Show outline symbols", "Ctrl+8", "outline symbols on"),
            run("Hide outline symbols", "", "outline symbols off"),
            run("Summary rows below detail", "", "outline summary rows below"),
            run("Summary rows above detail", "", "outline summary rows above"),
            run("Summary columns right of detail", "", "outline summary columns right"),
            run("Summary columns left of detail", "", "outline summary columns left"),
            sub(
                "Show level",
                (1..=8)
                    .map(|n| run(&format!("Level {n}"), "", &format!("outline level {n} {n}")))
                    .collect(),
            ),
            form(
                "Subtotal...",
                "subtotal",
                vec![
                    Field::bare("List with its header row", &selected(book)),
                    Field::text("At each change in", "at", "A"),
                    Field::choice(
                        "Use function",
                        "function",
                        &[
                            ("Sum", "sum"),
                            ("Count", "count"),
                            ("Average", "average"),
                            ("Max", "max"),
                            ("Min", "min"),
                            ("Product", "product"),
                        ],
                        "sum",
                    ),
                    Field::text("Add subtotal to, such as B,C", "add", "B"),
                    Field::check("Page break between groups", "page-breaks", false),
                    Field::check("Add to the current subtotals", "nest", false),
                    Field::check("Summary above data", "summary-above", false),
                ],
            ),
            run("Remove subtotals", "", "subtotal remove"),
        ],
    )
}

/// Manage Rules: every rule, with its own changes, and the new kinds of rule.
pub fn manage_rules(book: &Book) -> Item {
    let mut items = vec![
        form(
            "Icon set...",
            "rules add",
            vec![
                Field::bare("Range", &selected(book)),
                Field::choice("Kind", "k", &[("Icon set", "icons")], "icons"),
                Field::bare("Set, such as 3Arrows, 4Rating, 5Quarters", "3Arrows"),
                Field::check("Reverse icon order", "reverse", false),
                Field::check("Show icon only", "icon-only", false),
            ],
        ),
        form(
            "Data bar...",
            "rules add",
            vec![
                Field::bare("Range", &selected(book)),
                Field::choice("Kind", "k", &[("Data bar", "bar")], "bar"),
                Field::bare("Color", "#638EC6"),
                Field::text("Border color", "border", ""),
                Field::text("Negative color", "negative", ""),
                Field::text("Axis, automatic, middle or none", "axis", ""),
                Field::check("Solid fill", "solid", false),
                Field::check("Right to left", "rtl", false),
                Field::check("Show bar only", "no-value", false),
            ],
        ),
        form(
            "Date occurring...",
            "rules add",
            vec![
                Field::bare("Range", &selected(book)),
                Field::choice("Kind", "k", &[("Date occurring", "date")], "date"),
                Field::bare("Period, such as lastWeek, thisMonth, today", "today"),
            ],
        ),
        form(
            "Above or below standard deviation...",
            "rules add",
            vec![
                Field::bare("Range", &selected(book)),
                Field::choice("Kind", "k", &[("Standard deviation", "stddev")], "stddev"),
                Field::bare("Deviations, 1 to 3", "1"),
                Field::choice("Side", "s", &[("Above", "above"), ("Below", "below")], "above"),
            ],
        ),
        Item::Line,
    ];
    for e in book.session.conditional_rules(book.view.sheet, None) {
        let ranges: Vec<String> = e.ranges.iter().map(|r| r.a1()).collect();
        let p = e.priority;
        items.push(sub(
            &format!("{p}. {} on {}", e.rule.kind, ranges.join(",")),
            vec![
                run("Move up", "", &format!("rules move {p} up")),
                run("Move down", "", &format!("rules move {p} down")),
                run("Move to top", "", &format!("rules move {p} first")),
                run("Move to bottom", "", &format!("rules move {p} last")),
                ask("Applies to...", "Ranges", &format!("rules applies {p}")),
                run("Stop if true", "", &format!("rules stop {p} on")),
                run("Do not stop", "", &format!("rules stop {p} off")),
                run("Delete rule", "", &format!("rules delete {p}")),
            ],
        ));
    }
    sub("Manage rules", items)
}

/// Protection: sheet, workbook and edit ranges.
pub fn protection() -> Item {
    let fields = vec![
        Field::bare("Sheet", "sheet"),
        Field::text("Password", "password", ""),
        Field::text("Allow, such as format-cells,sort", "allow", ""),
    ];
    sub(
        "Protection",
        vec![
            form("Protect sheet...", "protect", fields),
            form(
                "Protect workbook...",
                "protect",
                vec![
                    Field::bare("Workbook", "book"),
                    Field::text("Password", "password", ""),
                    Field::check("Structure", "structure", true),
                    Field::check("Windows", "windows", false),
                ],
            ),
            form("Unprotect sheet...", "unprotect", vec![Field::text("Password", "password", "")]),
            form(
                "Unprotect workbook...",
                "unprotect",
                vec![Field::bare("Workbook", "book"), Field::text("Password", "password", "")],
            ),
            form(
                "Allow edit ranges...",
                "edit-range add",
                vec![
                    Field::bare("Title", "Range1"),
                    Field::bare("Ranges", ""),
                    Field::text("Password", "password", ""),
                ],
            ),
            run("List edit ranges", "", "edit-range list"),
        ],
    )
}

/// The 47 built in cell styles, new and modified styles, themes, colours and fonts.
pub fn styles_and_themes(book: &Book) -> Item {
    let gallery: Vec<Item> = book
        .session
        .style_gallery()
        .iter()
        .filter(|s| !s.hidden)
        .map(|s| run(&s.name, "", &format!("style apply {}", s.name.replace(' ', "~"))))
        .collect();
    sub(
        "Cell styles and themes",
        vec![
            sub("Cell styles", gallery),
            ask("New cell style from the active cell...", "Style name", "style new"),
            ask("Modify style...", "Name, then changes such as fill #FFFF00 bold", "style modify"),
            ask("Delete style...", "Style name", "style delete"),
            ask("Merge styles from a workbook...", "Workbook file", "style merge"),
            sub(
                "Themes",
                unluminous_sheet::theme::BUILT_IN
                    .iter()
                    .map(|t| run(t.name, "", &format!("theme {}", t.name.replace(' ', "~"))))
                    .collect(),
            ),
            sub(
                "Theme colors",
                unluminous_sheet::theme::color_scheme_names()
                    .iter()
                    .map(|n| run(n, "", &format!("theme colors {}", n.replace(' ', "~"))))
                    .collect(),
            ),
            sub(
                "Theme fonts",
                unluminous_sheet::theme::font_scheme_names()
                    .iter()
                    .map(|n| run(n, "", &format!("theme fonts {}", n.replace(' ', "~"))))
                    .collect(),
            ),
            colors("Accent 1 color", "theme colors set accent1"),
        ],
    )
}

/// Comments, notes, find and replace, Go To Special.
pub fn review(book: &Book) -> Item {
    let at = book.view.cursor.a1();
    sub(
        "Review",
        vec![
            ask("New comment...", "Comment", "comment new"),
            ask("Reply...", "Reply", "comment reply"),
            run("Resolve thread", "", "comment resolve"),
            run("Reopen thread", "", "comment reopen"),
            run("Delete thread", "", "comment delete"),
            run("Comments pane", "", "comment pane on"),
            run("List comments", "", "comment list"),
            Item::Line,
            run("Show note", "Shift+F2", &format!("note {at} show")),
            run("Hide note", "", &format!("note {at} hide")),
            run("Show all notes", "", "note show all"),
            run("Hide all notes", "", "note hide all"),
            run("Next note", "", "note next"),
            run("Previous note", "", "note previous"),
            run("Convert notes to comments", "", "note convert"),
            Item::Line,
            sub("Go to special", special_items()),
            run("Workbook statistics", "", "statistics"),
        ],
    )
}

fn special_items() -> Vec<Item> {
    [
        ("Notes", "notes"),
        ("Comments", "comments"),
        ("Constants", "constants"),
        ("Formulas", "formulas"),
        ("Blanks", "blanks"),
        ("Current region", "region"),
        ("Current array", "array"),
        ("Row differences", "row-differences"),
        ("Column differences", "column-differences"),
        ("Precedents", "precedents"),
        ("Precedents, all levels", "precedents all"),
        ("Dependents", "dependents"),
        ("Dependents, all levels", "dependents all"),
        ("Last cell", "last"),
        ("Visible cells only", "visible"),
        ("Conditional formats", "conditional"),
        ("Same conditional formats", "conditional same"),
        ("Data validation", "validation"),
        ("Same data validation", "validation same"),
        ("Objects", "objects"),
    ]
    .iter()
    .map(|(label, w)| run(label, "", &format!("goto-special {w}")))
    .collect()
}

/// Formula auditing: trace arrows, error checking, Evaluate Formula and the Watch Window.
pub fn auditing(book: &Book) -> Vec<Item> {
    let at = book.view.cursor.a1();
    vec![
        run("Trace precedents", "", "trace precedents"),
        run("Trace precedents, all levels", "", "trace precedents all"),
        run("Trace dependents", "", "trace dependents"),
        run("Trace dependents, all levels", "", "trace dependents all"),
        run("Trace error", "", "trace error"),
        run("Remove arrows", "", "trace clear"),
        run("Error checking", "", "error-check"),
        run("Error checking, every rule", "", "error-check all"),
        run("Reset ignored errors", "", "error-check reset"),
        run("Evaluate formula", "", &format!("evaluate {at}")),
        run("Evaluate: evaluate", "", "evaluate evaluate"),
        run("Evaluate: step in", "", "evaluate in"),
        run("Evaluate: step out", "", "evaluate out"),
        run("Evaluate: restart", "", "evaluate restart"),
        run("Evaluate: close", "", "evaluate close"),
        run("Watch this cell", "", &format!("watch add {at}")),
        run("Stop watching this cell", "", &format!("watch remove {at}")),
        run("Watch window", "", "watch show"),
        run("Hide watch window", "", "watch hide"),
    ]
}
