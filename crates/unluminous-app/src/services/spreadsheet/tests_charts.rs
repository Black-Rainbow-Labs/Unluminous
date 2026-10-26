//! The chart verbs and menus driven the way an agent drives them, each checked by reading the workbook back
//! (`task-2360`, the `chartwire` lane). The engine is checked against Excel; these check the wiring.

use serde_json::{json, Value};

use super::*;

/// A workbook with a month, three columns of figures and one chart over the lot.
fn with_chart(name: &str) -> (Spreadsheet, PathBuf) {
    let folder =
        std::env::temp_dir().join(format!("unluminous-sheet-charts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let mut sheet = Spreadsheet::new();
    sheet.open(&Context { project: Some(folder.clone()), ..Context::default() }).unwrap();
    go(&mut sheet, "new charts.xlsx");
    go(
        &mut sheet,
        "write A1 [[\"Month\",\"Sales\",\"Costs\",\"Profit\"],[\"Jan\",10,4,6],[\"Feb\",14,5,9],[\"Mar\",9,6,3],[\"Apr\",17,7,10],[\"May\",21,8,13],[\"Jun\",18,9,9]]",
    );
    go(&mut sheet, "chart column A1:D7 Results");
    (sheet, folder)
}

/// Words with a `~` standing for a space, so a value with a space in it is one word.
fn words(line: &str) -> Vec<String> {
    line.split(' ').map(|w| w.replace('~', " ")).collect()
}

fn go(sheet: &mut Spreadsheet, line: &str) -> Answer {
    let w = words(line);
    sheet.command(&w[0], &w[1..]).unwrap_or_else(|e| panic!("`{line}` failed: {e}"))
}

fn refused(sheet: &mut Spreadsheet, line: &str) -> String {
    let w = words(line);
    match sheet.command(&w[0], &w[1..]) {
        Ok(_) => panic!("`{line}` should have been refused"),
        Err(e) => e,
    }
}

/// `chart-info` of a chart.
fn info(sheet: &mut Spreadsheet, n: usize) -> Value {
    go(sheet, &format!("chart-info {n}")).value
}

/// The axis of a kind in a chart's information.
fn axis<'a>(info: &'a Value, kind: &str) -> &'a Value {
    info["axes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == kind)
        .unwrap_or_else(|| panic!("no {kind} axis"))
}

#[test]
fn every_type_excel_offers_can_be_inserted_over_the_data() {
    let (mut sheet, _folder) = with_chart("types");
    let types = go(&mut sheet, "chart-types").value;
    let mut made = 0;
    let mut failures = Vec::new();
    for entry in types.as_array().unwrap() {
        let name = entry["type"].as_str().unwrap();
        if name == "map" || name == "combo" {
            continue;
        }
        let w = words(&format!("chart {name} A1:D7"));
        match sheet.command(&w[0], &w[1..]) {
            Ok(answer) => {
                made += 1;
                let n = answer.value["chart"].as_u64().unwrap() as usize;
                assert_eq!(
                    info(&mut sheet, n)["type"],
                    name,
                    "chart {name} is read back as another type"
                );
            }
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    assert!(made >= 55, "{made} types were made; refused: {failures:?}");
    let book = sheet.current_book().unwrap();
    assert_eq!(book.session.book.sheets[0].charts.len(), made + 1);
}

#[test]
fn a_map_chart_is_refused_with_a_reason_and_a_bad_type_names_the_list() {
    let (mut sheet, _folder) = with_chart("map");
    assert!(refused(&mut sheet, "chart map A1:B7").contains("Bing"));
    assert!(refused(&mut sheet, "chart nonsense A1:B7").contains("chart-types"));
}

#[test]
fn the_old_chart_words_still_make_the_charts_they_made() {
    let (mut sheet, _folder) = with_chart("old");
    for kind in ["bar", "line", "area", "pie", "doughnut", "scatter"] {
        go(&mut sheet, &format!("chart {kind} A1:B7 A~{kind}~chart"));
    }
    let charts = go(&mut sheet, "charts").value;
    assert_eq!(charts.as_array().unwrap().len(), 7);
    assert_eq!(charts[1]["kind"], "bar");
    assert_eq!(charts[6]["kind"], "scatter");
}

#[test]
fn recommended_charts_are_read_back_as_data() {
    let (mut sheet, _folder) = with_chart("recommend");
    let answer = go(&mut sheet, "chart recommend A1:D7");
    let types = answer.value["types"].as_array().unwrap();
    assert!(!types.is_empty() && types.len() <= 5, "{types:?}");
    for t in types {
        assert!(unluminous_sheet::chartspec::ChartType::parse(t.as_str().unwrap()).is_some());
    }
}

#[test]
fn f11_makes_a_chart_sheet_that_the_other_verbs_reach() {
    let (mut sheet, _folder) = with_chart("f11");
    let answer = go(&mut sheet, "chart sheet line A1:C7");
    assert_eq!(answer.value["type"], "line");
    let book = sheet.current_book().unwrap();
    let shown = &book.session.book.sheets[book.view.sheet];
    assert!(shown.kept_part.is_some(), "the new sheet is a chart sheet");
    assert_eq!(shown.charts.len(), 1);
    go(&mut sheet, "chart-look 1 style 6");
    go(&mut sheet, "chart-title 1 On~a~sheet");
    assert_eq!(info(&mut sheet, 1)["title"], "On a sheet");
    assert_eq!(info(&mut sheet, 1)["style"], 6);
}

#[test]
fn changing_the_type_of_the_chart_and_of_one_series_makes_a_combination() {
    let (mut sheet, _folder) = with_chart("type");
    go(&mut sheet, "chart-look 1 type line-markers");
    assert_eq!(info(&mut sheet, 1)["type"], "line-markers");
    go(&mut sheet, "chart-look 1 type column");
    go(&mut sheet, "chart-series 1 type 2 line");
    assert_eq!(info(&mut sheet, 1)["type"], "combo");
    go(&mut sheet, "chart-series 1 secondary 2 on");
    assert_eq!(info(&mut sheet, 1)["series"][1]["secondary"], true);
    go(&mut sheet, "chart-series 1 secondary 2 off");
    assert_eq!(info(&mut sheet, 1)["series"][1]["secondary"], false);
    assert!(
        refused(&mut sheet, "chart-look 1 type combo").contains("Choose the type of each series")
    );
}

#[test]
fn series_are_added_edited_moved_hidden_and_removed() {
    let (mut sheet, _folder) = with_chart("series");
    assert_eq!(info(&mut sheet, 1)["series"].as_array().unwrap().len(), 3);
    go(&mut sheet, "chart-series 1 remove 3");
    go(&mut sheet, "chart-series 1 add D2:D7 name D1 categories A2:A7");
    let after = info(&mut sheet, 1);
    assert_eq!(after["series"].as_array().unwrap().len(), 3);
    assert_eq!(after["series"][2]["values"], "Sheet1!$D$2:$D$7");
    assert_eq!(after["series"][2]["name"], "Sheet1!$D$1");
    go(&mut sheet, "chart-series 1 edit 1 values C2:C7 name Takings");
    let after = info(&mut sheet, 1);
    assert_eq!(after["series"][0]["values"], "Sheet1!$C$2:$C$7");
    assert_eq!(after["series"][0]["name"], "\"Takings\"");
    go(&mut sheet, "chart-series 1 move 1 later");
    assert_eq!(info(&mut sheet, 1)["series"][1]["values"], "Sheet1!$C$2:$C$7");
    go(&mut sheet, "chart-series 1 hide 1");
    assert_eq!(info(&mut sheet, 1)["series"][0]["hidden"], true);
    go(&mut sheet, "chart-series 1 hide 1 off");
    assert_eq!(info(&mut sheet, 1)["series"][0]["hidden"], false);
    assert!(refused(&mut sheet, "chart-series 1 add Not~a~range").contains("not a range"));
    assert!(refused(&mut sheet, "chart-series 1 remove 9").contains("no such series"));
}

#[test]
fn a_series_and_one_of_its_points_are_formatted() {
    let (mut sheet, _folder) = with_chart("format");
    go(&mut sheet, "chart-series 1 format 1 fill #FF0000 line #00FF00 2 explosion 10");
    go(&mut sheet, "chart-series 1 format 1 point 2 fill #0000FF");
    let book = sheet.current_book().unwrap();
    let s = &book.session.book.sheets[0].charts[0].series[0];
    assert!(s.fmt.sp_pr.as_deref().is_some_and(|x| x.contains("FF0000") && x.contains("00FF00")));
    assert_eq!(s.fmt.points.len(), 1);
    assert_eq!(s.fmt.points[0].index, 1);
}

#[test]
fn the_data_range_the_orientation_and_the_empty_cells_are_set() {
    let (mut sheet, _folder) = with_chart("data");
    go(&mut sheet, "chart-data 1 range A1:C7");
    assert_eq!(info(&mut sheet, 1)["series"].as_array().unwrap().len(), 2);
    go(&mut sheet, "chart-data 1 switch");
    assert_eq!(info(&mut sheet, 1)["series"].as_array().unwrap().len(), 6);
    go(&mut sheet, "chart-data 1 range A1:C7 columns");
    assert_eq!(info(&mut sheet, 1)["series"].as_array().unwrap().len(), 2);
    go(&mut sheet, "chart-data 1 cells hidden zero");
    let after = info(&mut sheet, 1);
    assert_eq!(after["hiddenCellsPlotted"], true);
    assert_eq!(after["emptyCells"], "zero");
    go(&mut sheet, "chart-data 1 hide-categories 2,4");
    let book = sheet.current_book().unwrap();
    assert_eq!(book.session.book.sheets[0].charts[0].series[0].fmt.filtered_categories, vec![1, 3]);
    assert!(refused(&mut sheet, "chart-data 1 range Nowhere!A1:B2").contains("not a range"));
}

#[test]
fn a_chart_is_moved_sized_named_and_sent_to_another_sheet() {
    let (mut sheet, _folder) = with_chart("move");
    go(&mut sheet, "chart-move 1 100 50 300 200");
    let a = info(&mut sheet, 1);
    let near = |v: &Value, want: f64| (v.as_f64().unwrap() - want).abs() < 1.5;
    assert!(near(&a["left"], 100.0) && near(&a["top"], 50.0), "{a}");
    assert!(near(&a["width"], 300.0) && near(&a["height"], 200.0), "{a}");
    go(&mut sheet, "chart-move 1 220 90");
    let b = info(&mut sheet, 1);
    assert!(near(&b["left"], 220.0) && near(&b["top"], 90.0), "{b}");
    assert!(near(&b["width"], 300.0) && near(&b["height"], 200.0), "moving keeps the size: {b}");
    go(&mut sheet, "chart-name 1 name Takings~chart alt A~chart~of~takings");
    let named = info(&mut sheet, 1);
    assert_eq!(named["name"], "Takings chart");
    assert_eq!(named["alt"], "A chart of takings");
    go(&mut sheet, "sheet add");
    go(&mut sheet, "sheet show Sheet1");
    go(&mut sheet, "chart-move 1 sheet Sheet2");
    let book = sheet.current_book().unwrap();
    assert_eq!(book.session.book.sheets[0].charts.len(), 0);
    assert_eq!(book.session.book.sheets[1].charts.len(), 1);
    go(&mut sheet, "chart-move 1 new-sheet Chart~one");
    let book = sheet.current_book().unwrap();
    let last = book.session.book.sheets.last().unwrap();
    assert_eq!(last.name, "Chart one");
    assert!(last.kept_part.is_some());
}

#[test]
fn title_legend_data_table_layout_and_style_are_set_and_undone() {
    let (mut sheet, _folder) = with_chart("whole");
    go(&mut sheet, "chart-title 1 Quarterly~results");
    assert_eq!(info(&mut sheet, 1)["title"], "Quarterly results");
    go(&mut sheet, "chart-title 1 off");
    assert_eq!(info(&mut sheet, 1)["title"], "");
    go(&mut sheet, "undo");
    assert_eq!(info(&mut sheet, 1)["title"], "Quarterly results");
    go(&mut sheet, "chart-legend 1 left");
    assert_eq!(info(&mut sheet, 1)["legend"], "left");
    go(&mut sheet, "chart-legend 1 off");
    assert_eq!(info(&mut sheet, 1)["legend"], Value::Null);
    go(&mut sheet, "chart-table 1 on");
    assert_eq!(info(&mut sheet, 1)["dataTable"], true);
    go(&mut sheet, "chart-table 1 off");
    assert_eq!(info(&mut sheet, 1)["dataTable"], false);
    go(&mut sheet, "chart-look 1 style 12");
    assert_eq!(info(&mut sheet, 1)["style"], 12);
    go(&mut sheet, "chart-look 1 layout 2");
    assert!(refused(&mut sheet, "chart-look 1 style 99").contains("1 to 48"));
}

#[test]
fn axis_options_change_the_scale_the_drawing_reads() {
    let (mut sheet, _folder) = with_chart("axis");
    go(&mut sheet, "chart-axis 1 value min 0 max 40 major 10 title Money~earned");
    let a = info(&mut sheet, 1);
    let value = axis(&a, "value");
    assert_eq!(
        (value["min"].as_f64(), value["max"].as_f64(), value["major"].as_f64()),
        (Some(0.0), Some(40.0), Some(10.0))
    );
    assert_eq!(value["ticks"].as_array().unwrap().len(), 5);
    go(&mut sheet, "chart-axis 1 value reverse on");
    assert_eq!(axis(&info(&mut sheet, 1), "value")["reverse"], true);
    go(&mut sheet, "chart-axis 1 value reverse off log 10 min auto max auto major auto");
    let logged = info(&mut sheet, 1);
    assert_eq!(axis(&logged, "value")["log"], true);
    go(&mut sheet, "chart-axis 1 value log off major-gridlines off minor-gridlines on format 0.00 major-tick cross labels low");
    let book = sheet.current_book().unwrap();
    let ax = book.session.book.sheets[0].charts[0]
        .spec
        .axes
        .iter()
        .find(|a| a.kind == unluminous_sheet::chartspec::AxisKind::Value)
        .unwrap();
    assert!(ax.major_gridlines.is_none() && ax.minor_gridlines.is_some());
    assert_eq!(ax.num_fmt.as_ref().map(|f| f.0.as_str()), Some("0.00"));
    assert!(refused(&mut sheet, "chart-axis 1 value").contains("needs an option"));
    assert!(refused(&mut sheet, "chart-axis 1 depth min 1").contains("secondary-category"));
}

#[test]
fn data_labels_trendlines_and_error_bars_show_in_the_read_back() {
    let (mut sheet, _folder) = with_chart("hangers");
    go(&mut sheet, "chart-labels 1 on value position outside-end");
    assert_eq!(info(&mut sheet, 1)["series"][0]["labels"], true);
    go(&mut sheet, "chart-labels 1 series 1 off");
    assert_eq!(info(&mut sheet, 1)["series"][0]["labels"], false);
    assert_eq!(info(&mut sheet, 1)["series"][1]["labels"], true);
    go(&mut sheet, "chart-trendline 1 1 add linear equation r2");
    let t = &info(&mut sheet, 1)["series"][0]["trendlines"][0];
    assert!(t["equation"].as_str().unwrap().starts_with("y = "), "{t}");
    assert!(t["rSquared"].as_f64().is_some());
    go(&mut sheet, "chart-trendline 1 1 add poly order 3 forward 2");
    go(&mut sheet, "chart-trendline 1 1 edit 2 order 2");
    assert_eq!(info(&mut sheet, 1)["series"][0]["trendlines"].as_array().unwrap().len(), 2);
    go(&mut sheet, "chart-trendline 1 1 remove 1");
    assert_eq!(info(&mut sheet, 1)["series"][0]["trendlines"].as_array().unwrap().len(), 1);
    go(&mut sheet, "chart-errors 1 2 both percent 10");
    assert_eq!(info(&mut sheet, 1)["series"][1]["errorBars"], 1);
    go(&mut sheet, "chart-errors 1 2 custom plus-values 1,2,3,4,5,6 minus-values 1");
    go(&mut sheet, "chart-errors 1 2 remove");
    assert_eq!(info(&mut sheet, 1)["series"][1]["errorBars"], 0);
    assert!(refused(&mut sheet, "chart-trendline 1 1 add log order").contains("needs"));
    assert!(refused(&mut sheet, "chart-trendline 1 1 remove 5").contains("no such trendline"));
}

#[test]
fn group_options_and_element_formats_are_applied() {
    let (mut sheet, _folder) = with_chart("group");
    go(&mut sheet, "chart-group 1 gap 40 overlap 20 vary on");
    let info = info(&mut sheet, 1);
    assert_eq!(info["groups"][0]["gapWidth"], 40);
    assert_eq!(info["groups"][0]["overlap"], 20);
    go(&mut sheet, "chart-format 1 area fill #EEEEEE line #333333 2");
    go(&mut sheet, "chart-format 1 title size 18 bold on color #AA0000");
    go(&mut sheet, "chart-format 1 axis value size 9");
    go(&mut sheet, "chart-labels 1 on value");
    go(&mut sheet, "chart-format 1 labels all size 8");
    let book = sheet.current_book().unwrap();
    let spec = &book.session.book.sheets[0].charts[0].spec;
    assert!(spec.raw.sp_pr.as_deref().is_some_and(|x| x.contains("EEEEEE")), "{:?}", spec.raw);
}

#[test]
fn a_chart_is_found_by_its_number_and_a_bad_number_says_how() {
    let (mut sheet, _folder) = with_chart("numbers");
    assert!(refused(&mut sheet, "chart-title 0 x").contains("from 1"));
    assert!(refused(&mut sheet, "chart-title x y").contains("not a chart number"));
    assert!(refused(&mut sheet, "chart-title 5 y").contains("no chart 5"));
    assert!(refused(&mut sheet, "chart-info").contains("by its number"));
    go(&mut sheet, "chart delete 1");
    assert_eq!(go(&mut sheet, "charts").value.as_array().unwrap().len(), 0);
}

#[test]
fn chart_info_reads_back_what_is_drawn() {
    let (mut sheet, _folder) = with_chart("info");
    let i = info(&mut sheet, 1);
    assert_eq!(i["type"], "column");
    assert_eq!(i["title"], "Results");
    assert_eq!(i["series"][0]["drawn"], json!([10.0, 14.0, 9.0, 17.0, 21.0, 18.0]));
    let value = axis(&i, "value");
    assert_eq!(value["min"].as_f64(), Some(0.0));
    assert!(value["max"].as_f64().unwrap() >= 21.0);
    assert_eq!(axis(&i, "category")["categories"].as_array().unwrap().len(), 6);
}

/// Every word list the chart menus offer, found by walking the tree.
fn leaves(items: &[catalogue::Item], out: &mut Vec<Vec<String>>) {
    for item in items {
        match item {
            catalogue::Item::Run { words, .. } => out.push(words.clone()),
            catalogue::Item::Ask { words, .. } => {
                let mut w = words.clone();
                w.push("5".to_owned());
                out.push(w);
            }
            catalogue::Item::Colors { words, .. } => {
                let mut w = words.clone();
                w.push("#336699".to_owned());
                out.push(w);
            }
            catalogue::Item::Form { words, fields, title, .. } => {
                out.push(view::Prompt::form(words.clone(), title.clone(), fields.clone()).answer())
            }
            catalogue::Item::Sub { items, .. } => leaves(items, out),
            catalogue::Item::Line => {}
        }
    }
}

#[test]
fn every_entry_of_the_chart_menus_runs_a_verb_the_provider_has() {
    let (mut probe, _folder) = with_chart("menu-probe");
    let book = probe.current_book().unwrap();
    book.view.chart = Some(0);
    let mut all = Vec::new();
    leaves(
        &[catalogue::Item::Sub {
            label: "Chart".into(),
            items: super::catalogue_charts::chart_menu(book),
        }],
        &mut all,
    );
    let level = super::catalogue_charts::chart_level(book);
    leaves(&[level], &mut all);
    leaves(&[super::catalogue_charts::insert_charts()], &mut all);
    assert!(all.len() > 200, "the chart menus offer {} entries", all.len());
    let (mut worked, mut refused_for_a_reason) = (0, Vec::new());
    for (i, line) in all.iter().enumerate() {
        let (mut sheet, _f) = with_chart(&format!("menu-{i}"));
        let book = sheet.current_book().unwrap();
        book.view.chart = Some(0);
        match sheet.command(&line[0], &line[1..]) {
            Ok(_) => worked += 1,
            Err(e) => {
                assert!(!e.contains("there is no `"), "`{}` is not a command: {e}", line.join(" "));
                refused_for_a_reason.push(format!("{}: {e}", line.join(" ")));
            }
        }
    }
    assert!(
        worked * 10 >= all.len() * 8,
        "{worked} of {} worked; refused: {refused_for_a_reason:#?}",
        all.len()
    );
}

#[test]
fn a_right_click_on_a_chart_offers_the_chart_commands_and_a_cell_does_not() {
    let (mut sheet, _folder) = with_chart("rightclick");
    let book = sheet.current_book().unwrap();
    book.view.chart = None;
    let cell = catalogue::cell_menu(book);
    assert!(cell.iter().all(|i| i.label() != "Change chart type"));
    book.view.chart = Some(0);
    let menu = catalogue::cell_menu(book);
    let labels: Vec<&str> = menu.iter().map(|i| i.label()).collect();
    for want in ["Change chart type", "Select data", "Move chart", "Format chart", "Delete"] {
        assert!(labels.contains(&want), "{want} is missing from {labels:?}");
    }
    let panel = catalogue::panel_root(book);
    assert!(panel.iter().any(|i| i.label() == "Chart"));
}
