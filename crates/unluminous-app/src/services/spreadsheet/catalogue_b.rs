//! The levels of the command tree for the clipboard and fill, page layout and print, PivotTables and slicers,
//! and the file formats (`task-2360`, the `wiringb` lane). Every entry names the words of a verb in
//! [`super::verbs_b`], so pressing it and an agent's call are the same call.

use unluminous_sheet::pages::paper::PAPERS;
use unluminous_sheet::pages::Orientation;

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

fn on_off(on: bool) -> &'static str {
    if on {
        "off"
    } else {
        "on"
    }
}

// ------------------------------------------------------------------------------------ paste and fill

/// The kinds Paste Special offers, as a label and the word `paste-special` reads.
pub const PASTE_KINDS: [(&str, &str); 12] = [
    ("All", "all"),
    ("Formulas", "formulas"),
    ("Values", "values"),
    ("Formats", "formats"),
    ("Comments and notes", "comments"),
    ("Validation", "validation"),
    ("All using source theme", "source-theme"),
    ("All except borders", "all-except-borders"),
    ("Column widths", "widths"),
    ("Formulas and number formats", "formulas-and-number-formats"),
    ("Values and number formats", "values-and-number-formats"),
    ("All, merging conditional formats", "merge-conditional"),
];

/// The Paste Special box as a form, with every option the popup has.
pub fn paste_special_form() -> Item {
    form(
        "Paste special...",
        "paste-special",
        vec![
            Field::choice("Paste", "kind", &PASTE_KINDS, "all"),
            Field::choice(
                "Operation",
                "operation",
                &[
                    ("None", "none"),
                    ("Add", "add"),
                    ("Subtract", "subtract"),
                    ("Multiply", "multiply"),
                    ("Divide", "divide"),
                ],
                "none",
            ),
            Field::check("Skip blanks", "skip-blanks", false),
            Field::check("Transpose", "transpose", false),
        ],
    )
}

/// The Paste special level: the form, each kind on its own, the operations, Paste Link and Insert Copied.
pub fn paste_special_menu() -> Item {
    let mut items = vec![
        paste_special_form(),
        run("Open the popup (Ctrl+Alt+V)", "Ctrl+Alt+V", "paste-special"),
    ];
    items.push(Item::Line);
    for (label, word) in PASTE_KINDS {
        items.push(run(label, "", &format!("paste-special {word}")));
    }
    items.push(run("Transposed", "", "paste-special transpose"));
    items.push(Item::Line);
    for (label, word) in
        [("Add", "add"), ("Subtract", "subtract"), ("Multiply", "multiply"), ("Divide", "divide")]
    {
        items.push(run(
            &format!("Values, {}", label.to_lowercase()),
            "",
            &format!("paste-special all {word}"),
        ));
    }
    items.push(Item::Line);
    items.push(run("Paste link", "", "paste-link"));
    items.push(run("Insert copied cells, shift down", "", "insert-copied down"));
    items.push(run("Insert copied cells, shift right", "", "insert-copied right"));
    items.push(run("Insert cut cells, shift down", "", "insert-cut down"));
    items.push(run("Insert cut cells, shift right", "", "insert-cut right"));
    sub("Paste special", items)
}

/// The ways of pasting the Paste Options button offers once the cells are in.
pub fn paste_options_items() -> Vec<Item> {
    let entry = |label: &str, how: &str| run(label, "", &format!("paste-options {how}"));
    vec![
        entry("Paste", "all"),
        entry("Keep source formatting", "source-theme"),
        entry("No borders", "all-except-borders"),
        entry("Keep source column widths", "widths"),
        entry("Transpose", "transpose"),
        entry("Merge conditional formatting", "merge-conditional"),
        Item::Line,
        entry("Formulas", "formulas"),
        entry("Formulas and number formatting", "formulas-and-number-formats"),
        entry("Values", "values"),
        entry("Values and number formatting", "values-and-number-formats"),
        entry("Formatting", "formats"),
        entry("Paste link", "link"),
    ]
}

/// The ways of filling the Auto Fill Options button offers once the cells are filled.
pub fn fill_options_items() -> Vec<Item> {
    let entry = |label: &str, how: &str| run(label, "", &format!("fill-options {how}"));
    vec![
        entry("Copy cells", "copy"),
        entry("Fill series", "series"),
        entry("Fill formatting only", "formats"),
        entry("Fill without formatting", "values"),
        Item::Line,
        entry("Fill days", "days"),
        entry("Fill weekdays", "weekdays"),
        entry("Fill months", "months"),
        entry("Fill years", "years"),
        Item::Line,
        entry("Linear trend", "linear"),
        entry("Growth trend", "growth"),
    ]
}

/// Fill: down, right, up, left, the Series form, Justify, Across Worksheets and Flash Fill.
pub fn fill(book: &Book) -> Item {
    let others: Vec<(String, String)> = book
        .session
        .book
        .sheets
        .iter()
        .map(|s| (s.name.clone(), s.name.replace(' ', "_")))
        .collect();
    let first_other = others.get(1).or(others.first()).map(|(_, w)| w.clone()).unwrap_or_default();
    sub(
        "Fill",
        vec![
            run("Down", "Ctrl+D", "fill down"),
            run("Right", "Ctrl+R", "fill right"),
            run("Up", "", "fill up"),
            run("Left", "", "fill left"),
            form(
                "Series...",
                "series",
                vec![
                    Field::choice(
                        "Series in",
                        "in",
                        &[("Columns", "columns"), ("Rows", "rows")],
                        "columns",
                    ),
                    Field::choice(
                        "Type",
                        "type",
                        &[
                            ("Linear", "linear"),
                            ("Growth", "growth"),
                            ("Date", "date"),
                            ("AutoFill", "autofill"),
                        ],
                        "linear",
                    ),
                    Field::choice(
                        "Date unit",
                        "unit",
                        &[
                            ("(not a date)", ""),
                            ("Day", "day"),
                            ("Weekday", "weekday"),
                            ("Month", "month"),
                            ("Year", "year"),
                        ],
                        "",
                    ),
                    Field::text("Step value", "step", "1"),
                    Field::text("Stop value", "stop", ""),
                    Field::check("Trend", "trend", false),
                ],
            ),
            run("Justify", "", "justify"),
            form(
                "Across worksheets...",
                "fill-across",
                vec![
                    Field::bare("Sheets", &first_other),
                    Field::choice(
                        "Fill",
                        "what",
                        &[("All", "all"), ("Contents", "contents"), ("Formats", "formats")],
                        "all",
                    ),
                ],
            ),
            run("Flash Fill", "Ctrl+E", "flash-fill"),
        ],
    )
}

// ---------------------------------------------------------------------------------------- page layout

/// The Page Layout level: margins, orientation, size, print area, breaks, print titles, scaling, header and
/// footer, sheet options, page order, and the print preview, Print and Export to PDF.
pub fn page_layout(book: &Book) -> Item {
    let sheet = &book.session.book.sheets[book.view.sheet];
    let page = &sheet.page;
    let landscape = page.orientation_or_default() == Orientation::Landscape;
    let papers: Vec<(String, String)> =
        PAPERS.iter().map(|p| (p.name.to_owned(), p.code.to_string())).collect();
    sub(
        "Page Layout",
        vec![
            sub("Margins", margins()),
            sub(
                "Orientation",
                vec![
                    check("Portrait", "page-setup orientation portrait", !landscape),
                    check("Landscape", "page-setup orientation landscape", landscape),
                ],
            ),
            sub("Size", paper_sizes(&papers, page.paper_size)),
            sub(
                "Print area",
                vec![
                    run("Set print area", "", "print-area set"),
                    run("Add to print area", "", "print-area add"),
                    run("Clear print area", "", "print-area clear"),
                ],
            ),
            sub(
                "Breaks",
                vec![
                    run("Insert page break", "", "page-break insert"),
                    run("Remove page break", "", "page-break remove"),
                    run("Reset all page breaks", "", "page-break reset"),
                ],
            ),
            sub(
                "Print titles",
                vec![
                    form(
                        "Rows to repeat at top...",
                        "print-titles rows",
                        vec![Field::bare("Rows", "1:1")],
                    ),
                    form(
                        "Columns to repeat at left...",
                        "print-titles columns",
                        vec![Field::bare("Columns", "A:A")],
                    ),
                    run("Clear print titles", "", "print-titles clear"),
                ],
            ),
            sub(
                "Scale to fit",
                vec![
                    form(
                        "Fit to pages...",
                        "page-setup fit",
                        vec![Field::bare("Pages wide", "1"), Field::bare("Pages tall", "1")],
                    ),
                    ask("Scale...", "Percent of normal size, 10 to 400", "page-setup scale"),
                    run("Fit all columns on one page", "", "page-setup fit 1 0"),
                    run("Fit all rows on one page", "", "page-setup fit 0 1"),
                ],
            ),
            sub("Header and footer", header_and_footer(book)),
            sub(
                "Sheet options",
                vec![
                    check(
                        "Print gridlines",
                        &format!("page-setup gridlines {}", on_off(page.options.grid_lines)),
                        page.options.grid_lines,
                    ),
                    check(
                        "Print headings",
                        &format!("page-setup headings {}", on_off(page.options.headings)),
                        page.options.headings,
                    ),
                    check(
                        "Center horizontally",
                        &format!(
                            "page-setup center {}",
                            if page.options.horizontal_centered { "off" } else { "horizontal" }
                        ),
                        page.options.horizontal_centered,
                    ),
                    check(
                        "Center vertically",
                        &format!(
                            "page-setup center {}",
                            if page.options.vertical_centered { "off" } else { "vertical" }
                        ),
                        page.options.vertical_centered,
                    ),
                    sub(
                        "Comments",
                        vec![
                            run("None", "", "page-setup comments none"),
                            run("At the end of the sheet", "", "page-setup comments end"),
                            run("As displayed on the sheet", "", "page-setup comments displayed"),
                        ],
                    ),
                    sub(
                        "Cell errors as",
                        vec![
                            run("Displayed", "", "page-setup errors displayed"),
                            run("Blank", "", "page-setup errors blank"),
                            run("Dashes", "", "page-setup errors dashes"),
                            run("#N/A", "", "page-setup errors na"),
                        ],
                    ),
                    check(
                        "Black and white",
                        &format!(
                            "page-setup quality black-and-white {}",
                            on_off(page.black_and_white)
                        ),
                        page.black_and_white,
                    ),
                    check(
                        "Draft quality",
                        &format!("page-setup quality draft {}", on_off(page.draft)),
                        page.draft,
                    ),
                ],
            ),
            sub(
                "Page order",
                vec![
                    run("Down, then over", "", "page-setup order down"),
                    run("Over, then down", "", "page-setup order over"),
                ],
            ),
            ask("First page number...", "Number, or auto", "page-setup first-page"),
            Item::Line,
            run("Print preview", "Ctrl+F2", "print-preview"),
            run("Print", "Ctrl+P", "print"),
            ask("Export to PDF...", "File name ending .pdf", "export pdf"),
        ],
    )
}

fn margins() -> Vec<Item> {
    vec![
        run("Normal", "", "page-setup margins normal"),
        run("Narrow", "", "page-setup margins narrow"),
        run("Wide", "", "page-setup margins wide"),
        form(
            "Custom margins...",
            "page-setup margins",
            vec![
                Field::bare("Left", "0.7"),
                Field::bare("Right", "0.7"),
                Field::bare("Top", "0.75"),
                Field::bare("Bottom", "0.75"),
                Field::bare("Header", "0.3"),
                Field::bare("Footer", "0.3"),
            ],
        ),
    ]
}

fn paper_sizes(papers: &[(String, String)], current: u32) -> Vec<Item> {
    let mut items = Vec::new();
    for name in ["Letter", "Legal", "Tabloid", "A3", "A4", "A5"] {
        if let Some((label, code)) = papers.iter().find(|(l, _)| l == name) {
            items.push(check(
                label,
                &format!("page-setup paper {code}"),
                code == &current.to_string(),
            ));
        }
    }
    items.push(form(
        "More paper sizes...",
        "page-setup paper",
        vec![Field::choices("Paper size", "size", papers.to_vec(), &current.to_string())],
    ));
    items
}

fn header_and_footer(book: &Book) -> Vec<Item> {
    let page = &book.session.book.sheets[book.view.sheet].page;
    let part = |label: &str, verb: &str| {
        form(
            label,
            &format!("page-setup {verb}"),
            vec![
                Field::choice(
                    "Section",
                    "section",
                    &[("Left", "left"), ("Center", "center"), ("Right", "right")],
                    "center",
                ),
                Field::bare("Text", ""),
            ],
        )
    };
    let hf = &page.header_footer;
    vec![
        part("Header...", "header"),
        part("Footer...", "footer"),
        part("First page header...", "header first"),
        part("First page footer...", "footer first"),
        part("Even page header...", "header even"),
        part("Even page footer...", "footer even"),
        check(
            "Different first page",
            &format!("page-setup different-first {}", on_off(hf.different_first)),
            hf.different_first,
        ),
        check(
            "Different odd and even pages",
            &format!("page-setup different-odd-even {}", on_off(hf.different_odd_even)),
            hf.different_odd_even,
        ),
        check(
            "Scale with document",
            &format!("page-setup scale-with-document {}", on_off(hf.scale_with_doc)),
            hf.scale_with_doc,
        ),
        check(
            "Align with page margins",
            &format!("page-setup align-with-margins {}", on_off(hf.align_with_margins)),
            hf.align_with_margins,
        ),
    ]
}

// ----------------------------------------------------------------------------------------- pivot tables

/// The PivotTable level: Insert, the Fields pane, refresh, source, layout, totals, value and field settings,
/// sorting, grouping, calculated fields, slicers and timelines.
pub fn pivot_table(book: &Book) -> Item {
    let mut items = vec![
        run("Insert PivotTable", "", "pivot create"),
        check("PivotTable Fields pane", "pivot pane", book.view.lane_b.pivot_pane),
        run("Refresh all", "Ctrl+Alt+F5", "pivot refresh-all"),
    ];
    let pivots = &book.session.book.pivots;
    let t =
        pivots.at(book.view.sheet, book.view.cursor).or((pivots.tables.len() == 1).then_some(0));
    if let Some(t) = t {
        items.push(Item::Line);
        items.extend(table_items(book, t));
    }
    sub("PivotTable", items)
}

fn table_items(book: &Book, t: usize) -> Vec<Item> {
    let table = &book.session.book.pivots.tables[t];
    let at = format!("in {}", table.name.replace(' ', "_"));
    let on = |text: &str| run_in(text, &at);
    let mut items = vec![
        on("Refresh|pivot refresh"),
        form(
            "Change data source...",
            &format!("pivot source {at}"),
            vec![Field::bare("Source", "")],
        ),
        sub(
            "Report layout",
            vec![
                on("Compact form|pivot layout compact"),
                on("Outline form|pivot layout outline"),
                on("Tabular form|pivot layout tabular"),
            ],
        ),
        sub(
            "Grand totals",
            vec![
                on("Off for rows and columns|pivot totals none"),
                on("On for rows and columns|pivot totals both"),
                on("On for rows only|pivot totals rows"),
                on("On for columns only|pivot totals columns"),
            ],
        ),
        form(
            "Move PivotTable...",
            &format!("pivot move-table {at}"),
            vec![Field::bare("To", "H1")],
        ),
        on("Delete PivotTable|pivot delete"),
    ];
    items.push(sub("Value field settings", value_items(book, t, &at)));
    items.push(sub("Field settings", field_items(book, t, &at)));
    items.push(sub("Filters, sort and groups", sort_items(book, t, &at)));
    items.push(form(
        "Calculated field...",
        &format!("pivot calculated-field {at}"),
        vec![Field::bare("Name", "Field1"), Field::bare("Formula", "=0")],
    ));
    items.push(sub("Slicers and timelines", slicer_items(book, t)));
    items
}

/// An entry written as `Label|words`, with the table's `in <name>` added after the words.
fn run_in(spec: &str, at: &str) -> Item {
    let (label, text) = spec.split_once('|').unwrap_or((spec, ""));
    run(label, "", &format!("{text} {at}"))
}

fn value_items(book: &Book, t: usize, at: &str) -> Vec<Item> {
    let table = &book.session.book.pivots.tables[t];
    (0..table.data.len()).map(|i| value_form(book, t, i, at)).collect()
}

/// The Value Field Settings box of the value field at a place in the table, as a form.
pub fn value_form(book: &Book, t: usize, i: usize, at: &str) -> Item {
    let d = &book.session.book.pivots.tables[t].data[i];
    let summaries: Vec<(&str, &str)> = vec![
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
    ];
    let shows: Vec<(&str, &str)> =
        super::verbs_b_pivot::SHOW_AS.iter().map(|(word, _)| (*word, *word)).collect();
    form(
        &format!("{}...", d.name),
        &format!("pivot value {} {at}", i + 1),
        vec![
            Field::text("Custom name", "name", &d.name),
            Field::choice(
                "Summarize values by",
                "summarize",
                &summaries,
                super::verbs_b_pivot::func_word(d.func),
            ),
            Field::choice(
                "Show values as",
                "show",
                &shows,
                super::verbs_b_pivot::show_as_name(d.show),
            ),
            Field::text("Number format", "numfmt", d.num_fmt.as_deref().unwrap_or("")),
        ],
    )
}

fn field_items(book: &Book, t: usize, at: &str) -> Vec<Item> {
    let table = &book.session.book.pivots.tables[t];
    let cache = &book.session.book.pivots.caches[table.cache];
    (0..cache.fields.len())
        .map(|f| {
            let name = table.caption(cache, f);
            form(
                &format!("{name}..."),
                &format!("pivot field #{} {at}", f + 1),
                vec![
                    Field::choice(
                        "Layout",
                        "form",
                        &[("Compact", "compact"), ("Outline", "outline"), ("Tabular", "tabular")],
                        "compact",
                    ),
                    Field::text("Custom name", "name", ""),
                    Field::text("Subtotals", "subtotals", "automatic"),
                ],
            )
        })
        .collect()
}

fn sort_items(book: &Book, t: usize, at: &str) -> Vec<Item> {
    let table = &book.session.book.pivots.tables[t];
    let cache = &book.session.book.pivots.caches[table.cache];
    let mut items = Vec::new();
    for f in 0..cache.fields.len() {
        let name = table.caption(cache, f);
        items.push(sub(
            &name,
            vec![
                run("Sort A to Z", "", &format!("pivot sort #{} ascending {at}", f + 1)),
                run("Sort Z to A", "", &format!("pivot sort #{} descending {at}", f + 1)),
                run("Show every item", "", &format!("pivot items #{} all {at}", f + 1)),
                run("Clear filter", "", &format!("pivot filter #{} clear {at}", f + 1)),
                form(
                    "Label filter...",
                    &format!("pivot filter #{} {at}", f + 1),
                    vec![
                        Field::choice(
                            "Show items",
                            "kind",
                            &[
                                ("Equals", "equals"),
                                ("Does not equal", "not-equals"),
                                ("Begins with", "begins-with"),
                                ("Ends with", "ends-with"),
                                ("Contains", "contains"),
                                ("Does not contain", "not-contains"),
                            ],
                            "equals",
                        ),
                        Field::bare("Value", ""),
                    ],
                ),
                run(
                    "Group dates by months and years",
                    "",
                    &format!("pivot group #{} dates months,years {at}", f + 1),
                ),
                run("Ungroup", "", &format!("pivot ungroup #{} {at}", f + 1)),
            ],
        ));
    }
    items
}

fn slicer_items(book: &Book, t: usize) -> Vec<Item> {
    let table = &book.session.book.pivots.tables[t];
    let cache = &book.session.book.pivots.caches[table.cache];
    let at = format!("in {}", table.name.replace(' ', "_"));
    let mut items: Vec<Item> = (0..cache.fields.len())
        .map(|f| {
            run(
                &format!("Slicer: {}", table.caption(cache, f)),
                "",
                &format!("slicer add #{} {at}", f + 1),
            )
        })
        .collect();
    for f in (0..cache.fields.len()).filter(|f| cache.is_date_field(*f)) {
        items.push(run(
            &format!("Timeline: {}", table.caption(cache, f)),
            "",
            &format!("timeline add #{} {at}", f + 1),
        ));
    }
    for s in &book.session.book.slicers.slicers {
        items.push(run(
            &format!("Show every item of {}", s.name),
            "",
            &format!("slicer select {} all", s.name.replace(' ', "_")),
        ));
    }
    items
}

// ------------------------------------------------------------------------------------------------ files

/// Save As in every format the version writes, and the copies Export makes.
pub fn save_as(book: &Book) -> Vec<Item> {
    let mut items = vec![
        ask("Save as...", "File name; the extension chooses the format", "save-as"),
        ask("Save as CSV (UTF-8)...", "File name ending .csv", "save-as"),
        form(
            "Save as CSV (comma delimited)...",
            "save-as",
            vec![Field::bare("File name", "book.csv"), Field::bare("Encoding", "plain")],
        ),
        ask("Save as Unicode text...", "File name ending .txt", "save-as"),
        ask("Save as OpenDocument...", "File name ending .ods", "save-as"),
        ask("Save as web page...", "File name ending .html", "save-as"),
        ask("Export to PDF...", "File name ending .pdf", "export pdf"),
    ];
    if !book.kind.can_save() {
        items.insert(0, run("Save as .xlsx", "", "save-as-xlsx"));
    }
    items
}
