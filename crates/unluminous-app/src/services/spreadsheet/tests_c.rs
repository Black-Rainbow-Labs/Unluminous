//! Pictures, shapes, text boxes, grouping, sparklines and the object menus driven the way an agent drives
//! them, each checked by reading the workbook back.

use serde_json::{json, Value};

use super::*;

fn opened(name: &str) -> (Spreadsheet, PathBuf) {
    let folder =
        std::env::temp_dir().join(format!("unluminous-sheet-c-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let mut sheet = Spreadsheet::new();
    sheet.open(&Context { project: Some(folder.clone()), ..Context::default() }).unwrap();
    go(&mut sheet, "new c.xlsx");
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

/// A small PNG written into the project folder.
fn write_png(folder: &Path, name: &str) {
    let image = image::RgbaImage::from_pixel(40, 20, image::Rgba([200, 30, 30, 255]));
    image.save(folder.join(name)).unwrap();
}

fn objects(sheet: &mut Spreadsheet) -> Vec<Value> {
    go(sheet, "objects").value.as_array().unwrap().clone()
}

/// The ids of the objects, bottom first.
fn ids(sheet: &mut Spreadsheet) -> Vec<u64> {
    objects(sheet).iter().map(|o| o["id"].as_u64().unwrap()).collect()
}

fn group(sheet: &mut Spreadsheet, n: usize) -> Value {
    go(sheet, "sparklines").value[n].clone()
}

#[test]
fn a_picture_from_a_file_is_inserted_chosen_moved_sized_turned_and_deleted() {
    let (mut sheet, folder) = opened("picture");
    write_png(&folder, "red.png");
    go(&mut sheet, "picture insert red.png B2");
    let all = objects(&mut sheet);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["kind"], "Picture");
    assert_eq!(all[0]["chosen"], true, "an inserted picture is the chosen object");
    let left = all[0]["left"].as_f64().unwrap();
    go(&mut sheet, "object move 10 20");
    let moved = objects(&mut sheet);
    assert!((moved[0]["left"].as_f64().unwrap() - left - 10.0).abs() < 0.1);
    go(&mut sheet, "object frame 100 50 80 40");
    let framed = objects(&mut sheet);
    assert_eq!(
        (framed[0]["left"].clone(), framed[0]["top"].clone(), framed[0]["width"].clone()),
        (json!(100.0), json!(50.0), json!(80.0))
    );
    go(&mut sheet, "object resize 40 20 keep");
    go(&mut sheet, "object rotate 90");
    go(&mut sheet, "object alt A red box");
    let turned = objects(&mut sheet);
    assert_eq!(turned[0]["rotation"], json!(90.0));
    assert_eq!(turned[0]["alt"], "A red box");
    go(&mut sheet, "object crop 10 0 10 0");
    assert_eq!(objects(&mut sheet)[0]["crop"]["left"], json!(10.0));
    go(&mut sheet, "object reset");
    go(&mut sheet, "object delete");
    assert!(objects(&mut sheet).is_empty());
    go(&mut sheet, "undo");
    assert_eq!(objects(&mut sheet).len(), 1, "a delete is one undo step");
}

#[test]
fn a_picture_chosen_in_the_file_picker_is_inserted_at_the_active_cell() {
    let (mut sheet, folder) = opened("pick");
    write_png(&folder, "pick.png");
    verbs_c::choose_picture_file(Some(folder.join("pick.png")));
    go(&mut sheet, "picture pick");
    assert_eq!(objects(&mut sheet).len(), 1);
    go(&mut sheet, "picture pick");
    assert_eq!(objects(&mut sheet).len(), 1, "cancelling the picker adds nothing");
    let problem = refused(&mut sheet, "picture insert missing.png");
    assert!(problem.contains("could not be read"), "{problem}");
}

#[test]
fn a_picture_can_be_placed_in_a_cell() {
    let (mut sheet, folder) = opened("incell");
    write_png(&folder, "cell.png");
    go(&mut sheet, "picture insert cell.png D4");
    go(&mut sheet, "picture in-cell D4");
    let book = sheet.current_book().unwrap();
    assert!(book.session.cell_image(0, unluminous_sheet::coord::CellRef::new(3, 3)).is_some());
}

#[test]
fn every_shape_kind_is_inserted_formatted_and_given_text() {
    let (mut sheet, _folder) = opened("shapes");
    for kind in ["rectangle", "rounded", "ellipse", "line", "arrow", "textbox"] {
        go(&mut sheet, &format!("shape {kind} 20 20 100 50"));
    }
    let all = objects(&mut sheet);
    assert_eq!(all.len(), 6);
    assert_eq!(all[5]["kind"], "Text box");
    let first = ids(&mut sheet)[0];
    go(&mut sheet, &format!("object select {first}"));
    go(&mut sheet, "object fill #FFC000 30");
    go(&mut sheet, "object outline #0000FF width 3 dash dash");
    go(&mut sheet, "object text Hello~there");
    go(&mut sheet, "object text-format size 18 bold on color #FF0000 align centre valign middle");
    go(&mut sheet, "object transparency 40");
    go(&mut sheet, "object name Headline");
    assert_eq!(objects(&mut sheet)[0]["text"], "Hello there");
    assert_eq!(objects(&mut sheet)[0]["name"], "Headline");
    let problem = refused(&mut sheet, "object fill notacolour");
    assert!(problem.contains("#RRGGBB"), "{problem}");
}

#[test]
fn drawing_a_shape_by_dragging_sets_the_mode_the_grid_reads() {
    let (mut sheet, _folder) = opened("draw");
    go(&mut sheet, "shape draw ellipse");
    let book = sheet.current_book().unwrap();
    assert_eq!(book.view.objects.drawing.as_deref(), Some("ellipse"));
    go(&mut sheet, "shape draw off");
    let book = sheet.current_book().unwrap();
    assert_eq!(book.view.objects.drawing, None);
    refused(&mut sheet, "shape draw hexagon");
}

#[test]
fn objects_are_arranged_aligned_grouped_and_ungrouped() {
    let (mut sheet, _folder) = opened("arrange");
    go(&mut sheet, "shape rectangle 10 10 50 50");
    go(&mut sheet, "shape rectangle 200 90 50 50");
    go(&mut sheet, "shape rectangle 400 30 50 50");
    let before = ids(&mut sheet);
    go(&mut sheet, &format!("object order {} front", before[0]));
    assert_eq!(*ids(&mut sheet).last().unwrap(), before[0]);
    go(&mut sheet, "object select all");
    go(&mut sheet, "object align top");
    let tops: Vec<f64> = objects(&mut sheet).iter().map(|o| o["top"].as_f64().unwrap()).collect();
    assert!(tops.iter().all(|t| (t - tops[0]).abs() < 0.1), "{tops:?}");
    go(&mut sheet, "object distribute horizontal");
    go(&mut sheet, "object group");
    let grouped = objects(&mut sheet);
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0]["members"], json!(3));
    go(&mut sheet, "object ungroup");
    assert_eq!(objects(&mut sheet).len(), 3);
    go(&mut sheet, "object placement fixed");
    assert_eq!(objects(&mut sheet)[0]["placement"], "fixed");
}

#[test]
fn clicking_an_object_chooses_it_and_adding_keeps_the_others() {
    let (mut sheet, _folder) = opened("choose");
    go(&mut sheet, "shape rectangle 10 10 50 50");
    go(&mut sheet, "shape ellipse 100 10 50 50");
    go(&mut sheet, "object select none");
    assert!(objects(&mut sheet).iter().all(|o| o["chosen"] == false));
    let both = ids(&mut sheet);
    go(&mut sheet, &format!("object select {}", both[0]));
    go(&mut sheet, &format!("object select add {}", both[1]));
    let chosen = go(&mut sheet, "objects selected").value;
    assert_eq!(chosen.as_array().unwrap().len(), 2);
    let problem = refused(&mut sheet, "object select 99");
    assert!(problem.contains("no object"), "{problem}");
    go(&mut sheet, "object select none");
    let problem = refused(&mut sheet, "object delete");
    assert!(problem.contains("Choose an object"), "{problem}");
}

#[test]
fn the_object_levels_are_in_the_menus_and_name_verbs_the_provider_has() {
    let (mut sheet, _folder) = opened("levels");
    go(&mut sheet, "shape rectangle 10 10 50 50");
    let book = sheet.current_book().unwrap();
    let panel = catalogue::panel_root(book);
    let labels: Vec<&str> = panel.iter().map(|i| i.label()).collect();
    assert!(labels.contains(&"Object") && labels.contains(&"Sparkline"), "{labels:?}");
    let menu = catalogue::cell_menu(book);
    assert!(!menu.is_empty());
    book.view.objects.menu = Some(egui::Pos2::ZERO);
    let object_menu = catalogue::cell_menu(book);
    assert!(object_menu.iter().any(|i| i.label() == "Group"));
    let everything = catalogue::everything(book);
    let insert = everything.iter().find(|i| i.label() == "Insert").unwrap();
    let text = format!("{insert:?}");
    assert!(text.contains("Sparklines") && text.contains("Text box") && text.contains("Picture"));
}

#[test]
fn sparklines_are_inserted_listed_and_every_option_is_set() {
    let (mut sheet, _folder) = opened("spark");
    go(&mut sheet, "write A1 [[1,3,2,5],[4,1,6,2],[-1,2,-3,4]]");
    go(&mut sheet, "sparkline insert line A1:D3 F1:F3");
    let g = group(&mut sheet, 0);
    assert_eq!(g["kind"], "line");
    assert_eq!(g["sparklines"].as_array().unwrap().len(), 3);
    assert_eq!(g["sparklines"][1]["location"], "F2");
    go(&mut sheet, "sparkline kind F2 column");
    assert_eq!(group(&mut sheet, 0)["kind"], "column");
    go(&mut sheet, "sparkline points F1 high on low on markers on");
    let g = group(&mut sheet, 0);
    assert_eq!(
        (g["high"].clone(), g["low"].clone(), g["markers"].clone()),
        (json!(true), json!(true), json!(true))
    );
    go(&mut sheet, "sparkline color F1 series #FF0000");
    assert_eq!(group(&mut sheet, 0)["colors"]["series"], "#FF0000");
    go(&mut sheet, "sparkline weight F1 2.25");
    assert_eq!(group(&mut sheet, 0)["weight"], json!(2.25));
    go(&mut sheet, "sparkline axis F1 min 0 max same");
    let g = group(&mut sheet, 0);
    assert_eq!((g["min"].clone(), g["max"].clone()), (json!(0.0), json!("same")));
    go(&mut sheet, "sparkline haxis F1 on rtl");
    assert_eq!(group(&mut sheet, 0)["axis"], json!(true));
    go(&mut sheet, "sparkline empty F1 span");
    assert_eq!(group(&mut sheet, 0)["empty"], "span");
    go(&mut sheet, "sparkline hidden F1 on");
    assert_eq!(group(&mut sheet, 0)["hiddenCells"], json!(true));
    go(&mut sheet, "sparkline data F1 A1:C1");
    assert!(group(&mut sheet, 0)["sparklines"][0]["data"].as_str().unwrap().ends_with("A1:C1"));
}

#[test]
fn sparklines_are_grouped_ungrouped_and_cleared() {
    let (mut sheet, _folder) = opened("spark-group");
    go(&mut sheet, "write A1 [[1,3,2],[4,1,6]]");
    go(&mut sheet, "sparkline insert column A1:C1 E1");
    go(&mut sheet, "sparkline insert winloss A2:C2 E2");
    assert_eq!(go(&mut sheet, "sparklines").value.as_array().unwrap().len(), 2);
    go(&mut sheet, "sparkline group E1:E2");
    assert_eq!(go(&mut sheet, "sparklines").value.as_array().unwrap().len(), 1);
    go(&mut sheet, "sparkline ungroup E2");
    assert_eq!(go(&mut sheet, "sparklines").value.as_array().unwrap().len(), 2);
    go(&mut sheet, "sparkline clear E1:E2");
    assert!(go(&mut sheet, "sparklines").value.as_array().unwrap().is_empty());
    let problem = refused(&mut sheet, "sparkline kind E1 column");
    assert!(problem.contains("no sparkline"), "{problem}");
}

#[test]
fn a_sparkline_level_offers_the_group_of_the_active_cell() {
    let (mut sheet, _folder) = opened("spark-level");
    go(&mut sheet, "write A1 [[1,3,2]]");
    go(&mut sheet, "sparkline insert line A1:C1 E1");
    go(&mut sheet, "select E1");
    let book = sheet.current_book().unwrap();
    let level = catalogue::panel_root(book).into_iter().find(|i| i.label() == "Sparkline").unwrap();
    let text = format!("{level:?}");
    for part in ["High point", "Markers", "Date axis range", "Show as gaps", "Clear the group"] {
        assert!(text.contains(part), "the Sparkline level lacks {part}");
    }
}
