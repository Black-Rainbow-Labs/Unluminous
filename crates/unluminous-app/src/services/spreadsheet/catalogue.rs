//! Every command a person can reach in a spreadsheet tab, as one tree.
//!
//! The workbook menu under `...`, the Format panel's levels, the right click menu and the search in the
//! workbook menu are all drawn from this tree, so there is one list of what a tab can do. Each entry names
//! the `plugins run spreadsheet` words it runs, so pressing an entry and an agent sending those words are the
//! same call into [`super::commands::run`]. A test walks the tree and runs every entry.

use super::display::SWATCHES;
use super::view::Field;
use super::Book;

/// One entry.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Run these words.
    Run {
        label: String,
        keys: &'static str,
        words: Vec<String>,
        checked: bool,
    },
    /// Ask for a value in a small box first, then run the words with what was typed after them.
    Ask {
        label: String,
        title: String,
        field: String,
        words: Vec<String>,
        second: Option<String>,
        initial: String,
    },
    /// Ask for several fields in a box, then run the words with what they say.
    Form {
        label: String,
        title: String,
        words: Vec<String>,
        fields: Vec<Field>,
    },
    /// A level of its own.
    Sub {
        label: String,
        items: Vec<Item>,
    },
    /// A grid of colours, each running the words with its `#RRGGBB` after them.
    Colors {
        label: String,
        words: Vec<String>,
        none: Option<String>,
    },
    Line,
}

impl Item {
    pub fn label(&self) -> &str {
        match self {
            Item::Run { label, .. }
            | Item::Ask { label, .. }
            | Item::Form { label, .. }
            | Item::Sub { label, .. }
            | Item::Colors { label, .. } => label,
            Item::Line => "",
        }
    }
}

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

fn ask2(label: &str, field: &str, second: &str, text: &str) -> Item {
    Item::Ask {
        label: label.to_owned(),
        title: label.trim_end_matches("...").to_owned(),
        field: field.to_owned(),
        words: words(text),
        second: Some(second.to_owned()),
        initial: String::new(),
    }
}

/// A box of several fields. The title is the label without its trailing dots.
pub fn form(label: &str, text: &str, fields: Vec<Field>) -> Item {
    Item::Form {
        label: label.to_owned(),
        title: label.trim_end_matches("...").to_owned(),
        words: words(text),
        fields,
    }
}

fn sub(label: &str, items: Vec<Item>) -> Item {
    Item::Sub { label: label.to_owned(), items }
}

fn colors(label: &str, text: &str, none: Option<&str>) -> Item {
    Item::Colors { label: label.to_owned(), words: words(text), none: none.map(str::to_owned) }
}

/// The colours a swatch grid offers.
pub fn swatches() -> &'static [&'static str] {
    &SWATCHES
}

/// The Number level.
fn number() -> Item {
    sub(
        "Number",
        vec![
            run("General", "", "format general"),
            run("Number  1,234.50", "Ctrl+Shift+1", "format number #,##0.00"),
            run("Whole number  1,235", "", "format number #,##0"),
            run("Currency  $1,234.50", "Ctrl+Shift+4", "format currency"),
            run("Accounting  $ 1,234.50", "", "format accounting"),
            run("Percent  12%", "Ctrl+Shift+5", "format percent"),
            run("Percent  12.34%", "", "format number 0.00%"),
            run("Scientific  1.23E+03", "Ctrl+Shift+6", "format scientific"),
            run("Fraction  1 1/4", "", "format fraction"),
            run("Date  10/23/2026", "", "format date"),
            run("Long date  Friday, October 23, 2026", "", "format longdate"),
            run("Date  23-Oct-26", "Ctrl+Shift+3", "format number d-mmm-yy"),
            run("Time  1:30:00 PM", "", "format time"),
            run("Time  13:30", "", "format number h:mm"),
            run("Text", "", "format text"),
            Item::Line,
            run("More decimal places", "", "decimals more"),
            run("Fewer decimal places", "", "decimals fewer"),
            ask("Custom format...", "Format code", "format number"),
        ],
    )
}

fn font() -> Item {
    sub(
        "Font",
        vec![
            run("Bold", "Ctrl+B", "format toggle bold"),
            run("Italic", "Ctrl+I", "format toggle italic"),
            run("Underline", "Ctrl+U", "format toggle underline"),
            run("Double underline", "", "format double-underline"),
            run("Strikethrough", "Ctrl+5", "format toggle strike"),
            Item::Line,
            run("Bigger", "", "format bigger"),
            run("Smaller", "", "format smaller"),
            sub(
                "Size",
                [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48]
                    .iter()
                    .map(|n| run(&n.to_string(), "", &format!("format size {n}")))
                    .collect(),
            ),
            sub(
                "Typeface",
                [
                    "Aptos Narrow",
                    "Aptos",
                    "Calibri",
                    "Arial",
                    "Segoe UI",
                    "Verdana",
                    "Georgia",
                    "Times New Roman",
                    "Cambria",
                    "Consolas",
                    "Courier New",
                ]
                .iter()
                .map(|f| Item::Run {
                    label: (*f).to_owned(),
                    keys: "",
                    words: vec!["format".to_owned(), "font".to_owned(), (*f).to_owned()],
                    checked: false,
                })
                .collect(),
            ),
            colors("Text color", "format color", Some("auto")),
        ],
    )
}

fn fill() -> Item {
    sub(
        "Fill",
        vec![
            run("No fill", "", "format fill none"),
            colors("Colors", "format fill", None),
            ask("Custom color...", "#RRGGBB", "format fill"),
        ],
    )
}

fn borders() -> Item {
    sub(
        "Borders",
        vec![
            run("All borders", "", "border all thin"),
            run("Outline", "Ctrl+Shift+7", "border outline thin"),
            run("Inside", "", "border inside thin"),
            run("Top", "", "border top thin"),
            run("Bottom", "", "border bottom thin"),
            run("Left", "", "border left thin"),
            run("Right", "", "border right thin"),
            run("Thick outline", "", "border thick"),
            run("Double bottom", "", "border bottom double"),
            run("Dashed outline", "", "border outline dashed"),
            run("Dotted outline", "", "border outline dotted"),
            run("Medium outline", "", "border outline medium"),
            run("No borders", "", "border none"),
            colors("Outline in color", "border outline thin", None),
        ],
    )
}

fn alignment() -> Item {
    sub(
        "Alignment",
        vec![
            run("Left", "", "format align left"),
            run("Center", "", "format align center"),
            run("Right", "", "format align right"),
            run("General", "", "format align general"),
            run("Center across selection", "", "format align center-across"),
            Item::Line,
            run("Top", "", "format valign top"),
            run("Middle", "", "format valign middle"),
            run("Bottom", "", "format valign bottom"),
            Item::Line,
            run("Wrap text", "", "format wrap"),
            run("Don't wrap", "", "format nowrap"),
            run("Shrink to fit", "", "format shrink"),
            run("Indent more", "", "indent more"),
            run("Indent less", "", "indent less"),
            run("Rotate up", "", "format rotate 90"),
            run("Rotate down", "", "format rotate 180"),
            run("No rotation", "", "format rotate 0"),
            Item::Line,
            run("Merge and center", "", "merge center"),
            run("Merge", "", "merge"),
            run("Unmerge", "", "unmerge"),
        ],
    )
}

/// Excel's built in cell styles, each a format made from the words of `format`.
fn styles() -> Item {
    sub(
        "Styles",
        vec![
            run("Normal", "", "clear formats"),
            run("Good", "", "format fill #C6EFCE color #006100"),
            run("Bad", "", "format fill #FFC7CE color #9C0006"),
            run("Neutral", "", "format fill #FFEB9C color #9C5700"),
            run("Input", "", "format fill #FFCC99 color #3F3F76"),
            run("Output", "", "format fill #F2F2F2 color #3F3F3F bold"),
            run("Calculation", "", "format fill #F2F2F2 color #FA7D00 bold"),
            run("Note", "", "format fill #FFFFCC"),
            run("Warning text", "", "format color #FF0000"),
            run("Explanatory text", "", "format italic color #7F7F7F"),
            Item::Line,
            run("Title", "", "format size 18 color #44546A"),
            run("Heading 1", "", "format size 15 bold color #44546A"),
            run("Heading 2", "", "format size 13 bold color #44546A"),
            run("Heading 3", "", "format size 11 bold color #44546A"),
            run("Total", "", "format bold"),
            Item::Line,
            run("Copy format", "", "copy"),
            run("Paste format", "", "paste formats"),
        ],
    )
}

fn cell_size() -> Item {
    sub(
        "Cell size",
        vec![
            ask("Column width...", "Width in characters", "width"),
            ask("Row height...", "Height in points", "height"),
            run("AutoFit columns", "", "width auto"),
            run("AutoFit rows", "", "height auto"),
            run("Default column width", "", "width default"),
            run("Default row height", "", "height default"),
            Item::Line,
            run("Hide rows", "Ctrl+9", "hide rows"),
            run("Hide columns", "Ctrl+0", "hide columns"),
            run("Unhide rows", "Ctrl+Shift+9", "unhide rows"),
            run("Unhide columns", "Ctrl+Shift+0", "unhide columns"),
        ],
    )
}

fn rules() -> Item {
    sub(
        "Rules",
        vec![
            ask("Greater than...", "Value", "conditional greater"),
            ask("Less than...", "Value", "conditional less"),
            ask2("Between...", "From", "To", "conditional between"),
            ask("Equal to...", "Value", "conditional equal"),
            ask("Text that contains...", "Text", "conditional contains"),
            run("Duplicate values", "", "conditional duplicates"),
            run("Unique values", "", "conditional unique"),
            run("Top 10", "", "conditional top 10"),
            run("Bottom 10", "", "conditional bottom 10"),
            run("Top 10%", "", "conditional top 10%"),
            run("Above average", "", "conditional above"),
            run("Below average", "", "conditional below"),
            run("Blanks", "", "conditional blanks"),
            run("Errors", "", "conditional errors"),
            ask("Formula...", "Formula, true for the cells to mark", "conditional formula"),
            sub(
                "Color scale",
                vec![
                    run("Red, yellow, green", "", "conditional scale #F8696B #FFEB84 #63BE7B"),
                    run("Green, yellow, red", "", "conditional scale #63BE7B #FFEB84 #F8696B"),
                    run("White to blue", "", "conditional scale #FFFFFF #5A8AC6"),
                    run("White to green", "", "conditional scale #FFFFFF #63BE7B"),
                    run("White to red", "", "conditional scale #FFFFFF #F8696B"),
                ],
            ),
            sub(
                "Data bars",
                vec![
                    run("Blue", "", "conditional bar #638EC6"),
                    run("Green", "", "conditional bar #63C384"),
                    run("Red", "", "conditional bar #FF555A"),
                    run("Orange", "", "conditional bar #FFB628"),
                    run("Purple", "", "conditional bar #D6007B"),
                ],
            ),
            Item::Line,
            run("Clear rules", "", "conditional clear"),
        ],
    )
}

fn validation() -> Item {
    sub(
        "Allowed values",
        vec![
            ask("List...", "Choices, separated by commas, or =A1:A9", "validate list"),
            ask2("Whole numbers between...", "From", "To", "validate whole between"),
            ask2("Decimals between...", "From", "To", "validate decimal between"),
            ask2("Dates between...", "From", "To", "validate date between"),
            ask("Text no longer than...", "Characters", "validate length <="),
            ask("Custom formula...", "Formula, true for an allowed value", "validate custom"),
            run("Allow anything", "", "validate off"),
        ],
    )
}

fn data(book: &Book) -> Item {
    let filtered = book.session.book.sheets[book.view.sheet].filter.is_some();
    sub(
        "Data",
        vec![
            run("Sort A to Z", "", "sort asc"),
            run("Sort Z to A", "", "sort desc"),
            check("Filter", "filter", filtered),
            run("Remove duplicates", "", "remove-duplicates"),
            sub(
                "Split text into columns",
                vec![
                    run("At commas", "", "split ,"),
                    run("At tabs", "", "split tab"),
                    run("At semicolons", "", "split ;"),
                    run("At spaces", "", "split space"),
                    ask("At...", "Separator", "split"),
                ],
            ),
            run("Make a table", "Ctrl+T", "table"),
            validation(),
            Item::Line,
            ask("Name these cells...", "Name", "name"),
            run("Recalculate", "F9", "recalc"),
        ],
    )
}

fn view(book: &Book) -> Item {
    let s = &book.session.book.sheets[book.view.sheet];
    let v = &s.view;
    let kind = |k: unluminous_sheet::model::ViewKind| v.kind == k;
    sub(
        "View",
        vec![
            check("Normal", "view normal", kind(unluminous_sheet::model::ViewKind::Normal)),
            check(
                "Page break preview",
                "view page-break",
                kind(unluminous_sheet::model::ViewKind::PageBreakPreview),
            ),
            check(
                "Page layout",
                "view layout",
                kind(unluminous_sheet::model::ViewKind::PageLayout),
            ),
            Item::Line,
            run("Freeze panes here", "", "freeze"),
            run("Freeze top row", "", "freeze rows 1"),
            run("Freeze first column", "", "freeze columns 1"),
            run("Unfreeze panes", "", "freeze off"),
            check(
                "Split",
                if v.split.is_some() { "split off" } else { "split window" },
                v.split.is_some(),
            ),
            Item::Line,
            check(
                "Gridlines",
                if s.show_grid { "gridlines off" } else { "gridlines on" },
                s.show_grid,
            ),
            check("Headings", "headings toggle", v.show_headings),
            check("Show formulas", "show-formulas toggle", v.show_formulas),
            check("Zero values", "zeros toggle", v.show_zeros),
            check("Right to left", "right-to-left toggle", v.right_to_left),
            sub(
                "Status bar",
                super::config::AGGREGATES
                    .iter()
                    .map(|name| {
                        let on = book.view.aggregates.iter().any(|a| a == name);
                        check(&status_label(name), &format!("status-bar toggle {name}"), on)
                    })
                    .collect(),
            ),
            sub(
                "Calculation",
                vec![
                    check("Automatic", "calc-mode automatic", book.session.book.calc.automatic),
                    check("Manual", "calc-mode manual", !book.session.book.calc.automatic),
                    run("Calculate now", "F9", "calculate all"),
                    run("Calculate sheet", "Shift+F9", "calculate sheet"),
                ],
            ),
            sub(
                "Zoom",
                [50, 75, 90, 100, 125, 150, 200]
                    .iter()
                    .map(|z| {
                        check(
                            &format!("{z}%"),
                            &format!("zoom {z}"),
                            (book.view.zoom * 100.0).round() as i32 == *z,
                        )
                    })
                    .collect(),
            ),
        ],
    )
}

/// The name of a status bar figure as Excel's menu calls it.
fn status_label(name: &str) -> String {
    match name {
        "numbers" => "Numerical Count".to_owned(),
        other => capitalised(other),
    }
}

fn sheet(book: &Book) -> Item {
    let hidden: Vec<Item> = book
        .session
        .book
        .sheets
        .iter()
        .filter(|s| s.state != unluminous_sheet::model::Visibility::Visible)
        .map(|s| Item::Run {
            label: s.name.clone(),
            keys: "",
            words: vec!["sheet".to_owned(), "unhide".to_owned(), s.name.clone()],
            checked: false,
        })
        .collect();
    let mut items = vec![
        run("Add a sheet", "Shift+F11", "sheet add"),
        ask("Rename...", "Name", "sheet rename"),
        run("Duplicate", "", "sheet duplicate"),
        run("Delete", "", "sheet delete"),
        run("Hide", "", "sheet hide"),
    ];
    if !hidden.is_empty() {
        items.push(sub("Unhide", hidden));
    }
    let at = book.view.sheet + 1;
    let count = book.session.book.sheets.len();
    items.push(run("Move left", "", &format!("sheet move {} {}", at, at.saturating_sub(1).max(1))));
    items.push(run("Move right", "", &format!("sheet move {} {}", at, (at + 1).min(count))));
    items.push(colors("Tab color", "sheet color", Some("none")));
    sub("Sheet", items)
}

fn insert() -> Item {
    sub(
        "Insert",
        vec![
            run("Cells...", "Ctrl+Shift+=", "insert ask"),
            run("Rows above", "", "insert rows"),
            run("Columns left", "", "insert columns"),
            run("Cells, shifting down", "", "insert down"),
            run("Cells, shifting right", "", "insert right"),
            run("Sheet", "Shift+F11", "sheet add"),
            Item::Line,
            insert_function(),
            run("AutoSum", "Alt+=", "autosum"),
            ask("Note...", "Note", "note"),
            ask2("Link...", "Address, or a place such as Sheet2!A1", "Text to show", "link"),
            run("Table", "Ctrl+T", "table"),
            sub(
                "Chart",
                ["column", "bar", "line", "area", "pie", "doughnut", "scatter"]
                    .iter()
                    .map(|k| run(&capitalised(k), "", &format!("chart {k}")))
                    .collect(),
            ),
        ],
    )
}

/// Insert Function: every function, by category, each opening its Function Arguments box.
pub fn insert_function() -> Item {
    let mut categories: Vec<&'static str> = Vec::new();
    for f in unluminous_sheet::functions::all() {
        if !categories.contains(&f.category) {
            categories.push(f.category);
        }
    }
    categories.sort_unstable();
    let levels = categories
        .iter()
        .map(|category| {
            let items = unluminous_sheet::functions::all()
                .iter()
                .filter(|f| f.category == *category)
                .map(|f| run(f.name, "", &format!("insert-function {}", f.name)))
                .collect();
            sub(category, items)
        })
        .collect();
    sub("Function", levels)
}

fn delete() -> Item {
    sub(
        "Delete",
        vec![
            run("Cells...", "Ctrl+-", "delete ask"),
            run("Rows", "", "delete rows"),
            run("Columns", "", "delete columns"),
            run("Cells, shifting up", "", "delete up"),
            run("Cells, shifting left", "", "delete left"),
            Item::Line,
            run("Contents", "Delete", "clear contents"),
            run("Formats", "", "clear formats"),
            run("Everything", "", "clear all"),
            run("Note", "", "note clear"),
            run("Link", "", "link clear"),
        ],
    )
}

fn capitalised(word: &str) -> String {
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Name Manager: every defined name with what it refers to, each opening to edit or delete it, and new
/// names made from a box or from the labels beside the selection.
pub fn names(book: &Book) -> Item {
    let wb = &book.session.book;
    let mut items = vec![
        form(
            "New name...",
            "names new",
            vec![
                Field::bare("Name", ""),
                Field::bare("Refers to", &selection_reference(book)),
                Field::choices("Scope", "scope", scopes(book), "workbook"),
            ],
        ),
        form(
            "Create from selection...",
            "names from-selection",
            vec![
                Field::check("Top row", "top", true),
                Field::check("Left column", "left", true),
                Field::check("Bottom row", "bottom", false),
                Field::check("Right column", "right", false),
            ],
        ),
        run("Use in formula", "F3", "names paste"),
        Item::Line,
    ];
    for n in wb.names.iter().filter(|n| !n.hidden && !n.name.starts_with("_xl")) {
        let scope = match n.scope {
            Some(i) => wb.sheets.get(i).map_or("?".to_owned(), |s| s.name.clone()),
            None => "workbook".to_owned(),
        };
        let label = format!("{}  ={}  ({scope})", n.name, n.formula);
        let scope_word = n.scope.map_or("workbook".to_owned(), |i| (i + 1).to_string());
        items.push(sub(
            &label,
            vec![
                Item::Ask {
                    label: "Edit...".to_owned(),
                    title: format!("Edit {}", n.name),
                    field: "Refers to".to_owned(),
                    words: vec!["names".into(), "edit".into(), n.name.clone(), scope_word.clone()],
                    second: None,
                    initial: n.formula.clone(),
                },
                Item::Run {
                    label: "Delete".to_owned(),
                    keys: "",
                    words: vec!["names".into(), "delete".into(), n.name.clone(), scope_word],
                    checked: false,
                },
            ],
        ));
    }
    sub("Names", items)
}

/// The selection written as a name refers to it: `Sheet1!$A$1:$B$4`.
fn selection_reference(book: &Book) -> String {
    let r = book.view.range();
    let a = |c: unluminous_sheet::coord::CellRef| {
        format!("${}${}", unluminous_sheet::coord::column_name(c.col), c.row + 1)
    };
    let area = if r.len() == 1 { a(r.first) } else { format!("{}:{}", a(r.first), a(r.last)) };
    let sheet = &book.session.book.sheets[book.view.sheet].name;
    format!("{}!{area}", unluminous_sheet::formula::print::sheet_name(sheet))
}

/// The scopes a name can have: the workbook, or one sheet by its number.
fn scopes(book: &Book) -> Vec<(String, String)> {
    let mut out = vec![("Workbook".to_owned(), "workbook".to_owned())];
    for (i, s) in book.session.book.sheets.iter().enumerate() {
        out.push((s.name.clone(), (i + 1).to_string()));
    }
    out
}

/// The Formulas level: functions, names, auditing and calculation, as Excel's Formulas tab groups them.
pub fn formulas(book: &Book) -> Item {
    let mut items = vec![insert_function(), run("AutoSum", "Alt+=", "autosum"), names(book)];
    items.push(sub("Formula auditing", super::catalogue_a::auditing(book)));
    sub("Formulas", items)
}

/// The Format panel's first level.
pub fn panel_root(book: &Book) -> Vec<Item> {
    vec![
        number(),
        font(),
        fill(),
        borders(),
        alignment(),
        styles(),
        cell_size(),
        rules(),
        data(book),
        super::catalogue_a::sort_and_filter(book),
        super::catalogue_a::data_tools(book),
        super::catalogue_a::outline(book),
        super::catalogue_a::manage_rules(book),
        super::catalogue_a::review(book),
        super::catalogue_a::protection(),
        super::catalogue_a::styles_and_themes(book),
        super::catalogue_b::page_layout(book),
        super::catalogue_b::pivot_table(book),
        view(book),
        sheet(book),
    ]
}

/// What each of the Format panel's first rows says about the active cell now: `Currency`, `Calibri 11,
/// Bold`, `#FFF2CC`, `Left, wrap`.
pub fn details(book: &Book) -> Vec<(&'static str, String)> {
    let s = &book.session.book;
    let sheet = &s.sheets[book.view.sheet];
    let style = super::display::style_at(sheet, book.view.cursor);
    let code = s.styles.format_code(style);
    let font = s.styles.font_of(style);
    let mut text = format!("{} {}", font.name, unluminous_sheet::value::number_to_text(font.size));
    for (on, word) in [(font.bold, "Bold"), (font.italic, "Italic"), (font.strike, "Strike")] {
        if on {
            text.push_str(", ");
            text.push_str(word);
        }
    }
    let fill =
        s.styles.fill_of(style).paint().map_or("None".to_owned(), |c| c.to_hex(&s.styles.theme));
    let border = s.styles.border_of(style);
    let sides = [
        (&border.top, "top"),
        (&border.bottom, "bottom"),
        (&border.left, "left"),
        (&border.right, "right"),
    ];
    let drawn: Vec<&str> = sides.iter().filter(|(e, _)| !e.is_none()).map(|(_, n)| *n).collect();
    let borders = match drawn.len() {
        0 => "None".to_owned(),
        4 => "All sides".to_owned(),
        _ => drawn.join(", "),
    };
    let align = s.styles.xf(style).align;
    let mut alignment = align.horizontal.clone().unwrap_or_else(|| "General".to_owned());
    if align.wrap {
        alignment.push_str(", wrap");
    }
    let rules = sheet
        .conditional
        .iter()
        .filter(|c| c.ranges.iter().any(|r| r.contains(book.view.cursor)))
        .map(|c| c.rules.len())
        .sum::<usize>();
    vec![
        ("Number", code),
        ("Font", text),
        ("Fill", fill),
        ("Borders", borders),
        ("Alignment", alignment),
        ("Rules", if rules == 0 { "None".to_owned() } else { format!("{rules} on this cell") }),
    ]
}

/// Everything, which the workbook menu under `...` holds.
pub fn everything(book: &Book) -> Vec<Item> {
    vec![
        sub(
            "File",
            vec![
                run("Save", "Ctrl+S", "save"),
                sub("Save as", super::catalogue_b::save_as(book)),
                ask("Save this sheet as CSV...", "File name ending .csv", "save-as"),
            ],
        ),
        sub(
            "Edit",
            vec![
                run("Undo", "Ctrl+Z", "undo"),
                run("Redo", "Ctrl+Y", "redo"),
                run("Repeat", "F4", "repeat"),
                Item::Line,
                run("Cut", "Ctrl+X", "cut"),
                run("Copy", "Ctrl+C", "copy"),
                run("Paste", "Ctrl+V", "paste"),
                run("Paste values", "", "paste values"),
                run("Paste formulas", "", "paste formulas"),
                run("Paste formats", "", "paste formats"),
                run("Paste transposed", "", "paste transpose"),
                super::catalogue_b::paste_special_menu(),
                Item::Line,
                super::catalogue_b::fill(book),
                run("Find...", "Ctrl+F", "find-bar"),
                run("Replace...", "Ctrl+H", "find-bar replace"),
                run("Go to...", "Ctrl+G", "goto"),
            ],
        ),
        insert(),
        delete(),
        formulas(book),
        sub(
            "Format",
            vec![
                number(),
                font(),
                fill(),
                borders(),
                alignment(),
                styles(),
                cell_size(),
                run("Show the Format panel", "Ctrl+1", "panel"),
            ],
        ),
        rules(),
        data(book),
        super::catalogue_a::sort_and_filter(book),
        super::catalogue_a::data_tools(book),
        super::catalogue_a::outline(book),
        super::catalogue_a::manage_rules(book),
        super::catalogue_a::review(book),
        super::catalogue_a::protection(),
        super::catalogue_a::styles_and_themes(book),
        super::catalogue_b::page_layout(book),
        super::catalogue_b::pivot_table(book),
        view(book),
        sheet(book),
    ]
}

/// The right click menu of the chosen cells.
pub fn cell_menu(book: &Book) -> Vec<Item> {
    let at = book.view.cursor;
    let s = &book.session.book.sheets[book.view.sheet];
    let has_note = s.notes.contains_key(&(at.row, at.col));
    let mut items = vec![
        run("Cut", "Ctrl+X", "cut"),
        run("Copy", "Ctrl+C", "copy"),
        run("Paste", "Ctrl+V", "paste"),
        super::catalogue_b::paste_special_menu(),
        Item::Line,
        insert(),
        delete(),
        run("Clear contents", "Delete", "clear contents"),
        Item::Line,
        sub("Sort", vec![run("A to Z", "", "sort asc"), run("Z to A", "", "sort desc")]),
        check("Filter", "filter", s.filter.is_some()),
        Item::Line,
        run("Format cells...", "Ctrl+1", "panel"),
    ];
    if has_note {
        let text = s.notes.get(&(at.row, at.col)).map(|n| n.text.clone()).unwrap_or_default();
        items.push(Item::Ask {
            label: "Edit note...".to_owned(),
            title: "Note".to_owned(),
            field: "Note".to_owned(),
            words: words("note"),
            second: None,
            initial: text,
        });
        items.push(run("Delete note", "", "note clear"));
    } else {
        items.push(ask("Add a note...", "Note", "note"));
    }
    items.push(ask2("Link...", "Address, or a place such as Sheet2!A1", "Text to show", "link"));
    items.push(ask("Name these cells...", "Name", "name"));
    items.push(Item::Line);
    items.push(ask("Column width...", "Width in characters", "width"));
    items.push(ask("Row height...", "Height in points", "height"));
    items.push(sub(
        "Hide",
        vec![run("Rows", "Ctrl+9", "hide rows"), run("Columns", "Ctrl+0", "hide columns")],
    ));
    items.push(sub(
        "Unhide",
        vec![run("Rows", "", "unhide rows"), run("Columns", "", "unhide columns")],
    ));
    if book.session.book.pivots.at(book.view.sheet, at).is_some() {
        items.push(Item::Line);
        items.push(super::catalogue_b::pivot_table(book));
    }
    items
}

/// The right click menu of a sheet's tab.
pub fn sheet_menu(book: &Book, index: usize) -> Vec<Item> {
    let name = book.session.book.sheets[index].name.clone();
    let at = index + 1;
    let count = book.session.book.sheets.len();
    let named =
        |verb: &str| -> Vec<String> { vec!["sheet".to_owned(), verb.to_owned(), name.clone()] };
    vec![
        Item::Ask {
            label: "Rename...".to_owned(),
            title: "Rename sheet".to_owned(),
            field: "Name".to_owned(),
            words: named("rename"),
            second: None,
            initial: name.clone(),
        },
        Item::Run {
            label: "Duplicate".to_owned(),
            keys: "",
            words: named("duplicate"),
            checked: false,
        },
        Item::Run { label: "Delete".to_owned(), keys: "", words: named("delete"), checked: false },
        Item::Run { label: "Hide".to_owned(), keys: "", words: named("hide"), checked: false },
        run("Move left", "", &format!("sheet move {} {}", at, at.saturating_sub(1).max(1))),
        run("Move right", "", &format!("sheet move {} {}", at, (at + 1).min(count))),
        Item::Colors {
            label: "Tab color".to_owned(),
            words: named("color"),
            none: Some("none".to_owned()),
        },
        run("Add a sheet", "Shift+F11", "sheet add"),
    ]
}

/// The level at a path of labels.
pub fn at_path<'a>(items: &'a [Item], path: &[String]) -> Option<&'a [Item]> {
    let mut here = items;
    for label in path {
        let found = here.iter().find_map(|item| match item {
            Item::Sub { label: l, items } if l.eq_ignore_ascii_case(label) => {
                Some(items.as_slice())
            }
            _ => None,
        })?;
        here = found;
    }
    Some(here)
}

/// Every entry whose label holds the words typed, with the path to it, for the search in the workbook menu.
pub fn search(items: &[Item], text: &str) -> Vec<(String, Item)> {
    let wanted = text.trim().to_lowercase();
    let mut out = Vec::new();
    fn walk(items: &[Item], path: &str, wanted: &str, out: &mut Vec<(String, Item)>) {
        for item in items {
            match item {
                Item::Sub { label, items } => {
                    walk(items, &format!("{path}{label} / "), wanted, out)
                }
                Item::Line => {}
                other => {
                    let full = format!("{path}{}", other.label());
                    if wanted.split(' ').all(|w| full.to_lowercase().contains(w)) {
                        out.push((full, other.clone()));
                    }
                }
            }
        }
    }
    walk(items, "", &wanted, &mut out);
    out.dedup_by(|a, b| a.1 == b.1);
    out.truncate(40);
    out
}
