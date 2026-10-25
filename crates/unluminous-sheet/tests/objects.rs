//! Pictures, shapes and sparklines: reading what Excel wrote, writing it back unchanged, every editing
//! operation, and Excel opening what Unluminous writes and reading back what was set.
//!
//! The fixtures in `fixtures/objects/` were made by Excel (`15-shapes.xlsx`, `16-sparklines.xlsx`,
//! `spark-default.xlsx`). The Excel tests start Excel through `tools/excel-oracle/objects-inspect.ps1` and
//! pass with a printed line when Excel is not installed; `UNLUMINOUS_REQUIRE_EXCEL=1` makes that a failure.

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::{Alignment, Distribution, Session, Shift, SparklinePoints, TextFormat, ZOrder};
use unluminous_sheet::model::Workbook;
use unluminous_sheet::objects::geometry::{EMU_PER_POINT, Rect};
use unluminous_sheet::objects::make::NewShape;
use unluminous_sheet::objects::sparkline::{AxisBound, EmptyCells, SparklineKind};
use unluminous_sheet::objects::*;
use unluminous_sheet::styles::Color;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/objects").join(name)).unwrap()
}

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn rows(first: u32, count: u32) -> Range {
    Range::new(CellRef::new(first, 0), CellRef::new(first + count - 1, 16_383))
}

fn pt(points: i64) -> i64 {
    points * EMU_PER_POINT
}

fn session(name: &str) -> Session {
    Session::new(unluminous_sheet::xlsx::read(&fixture(name)).unwrap(), 46318.5)
}

fn reread(session: &Session) -> Workbook {
    unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&session.book).unwrap()).unwrap()
}

fn blank() -> Session {
    let mut s = Session::new(Workbook::new(), 46318.5);
    for r in 0..3u32 {
        for (c, v) in [[5.0, 7.0, 9.0, 6.0], [1.0, -2.0, 3.0, -4.0], [2.0, 2.0, 2.0, 2.0]][r as usize].iter().enumerate() {
            s.set_value(0, CellRef::new(r, c as u32), unluminous_sheet::Value::Number(*v)).unwrap();
        }
    }
    s
}

// ------------------------------------------------------------------------------------- reading and writing

#[test]
fn the_objects_excel_wrote_are_read_with_their_kinds_and_places() {
    let s = session("15-shapes.xlsx");
    let kinds: Vec<_> = s.book.sheets[0].drawing.objects.iter().map(|o| (o.name.as_str(), o.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            ("Pic1", ObjectKind::Picture),
            ("Rect1", ObjectKind::Shape),
            ("Text1", ObjectKind::TextBox),
            ("Button1", ObjectKind::FormControl),
            ("Check1", ObjectKind::FormControl),
            ("Line1", ObjectKind::Connector),
        ]
    );
    let sheet = &s.book.sheets[0];
    let rect = sheet.drawing.objects[1].bounds(sheet);
    assert_eq!((rect.x, rect.y, rect.w, rect.h), (pt(150), pt(20), pt(100), pt(50)));
    let text = sheet.drawing.objects[1].shape.as_ref().unwrap().text.as_ref().unwrap();
    assert_eq!(text.plain_text(), "rect");
    assert!(sheet.drawing.objects[3].fallback.is_none(), "the button has no stored picture");
}

#[test]
fn a_drawing_nobody_changed_is_written_as_the_bytes_it_was() {
    let s = session("15-shapes.xlsx");
    let original = unluminous_sheet::xlsx::read(&fixture("15-shapes.xlsx")).unwrap();
    let back = reread(&s);
    for part in ["xl/drawings/drawing1.xml", "xl/media/image1.png", "xl/drawings/vmlDrawing1.vml"] {
        assert_eq!(original.kept.get(part), back.kept.get(part), "{part}");
    }
}

#[test]
fn a_changed_drawing_keeps_every_object_and_what_was_not_changed() {
    let mut s = session("15-shapes.xlsx");
    s.move_object(0, 4, pt(10), pt(5)).unwrap();
    let back = reread(&s);
    let sheet = &back.sheets[0];
    assert_eq!(sheet.drawing.objects.len(), 6);
    let moved = sheet.drawing.objects[1].bounds(sheet);
    assert_eq!((moved.x, moved.y, moved.w, moved.h), (pt(160), pt(25), pt(100), pt(50)));
    // The form controls inside mc:AlternateContent are written back as they were.
    assert_eq!(sheet.drawing.objects[3].kind, ObjectKind::FormControl);
    assert_eq!(sheet.drawing.objects[3].name, "Button1");
    let line = &sheet.drawing.objects[5];
    assert_eq!(line.shape.as_ref().unwrap().preset, "line");
    let original = unluminous_sheet::xlsx::read(&fixture("15-shapes.xlsx")).unwrap();
    assert_eq!(original.kept.get("xl/media/image1.png"), back.kept.get("xl/media/image1.png"));
}

#[test]
fn undo_puts_the_drawing_back() {
    let mut s = session("15-shapes.xlsx");
    s.delete_object(0, 4).unwrap();
    assert_eq!(s.book.sheets[0].drawing.objects.len(), 5);
    s.undo();
    assert_eq!(s.book.sheets[0].drawing.objects.len(), 6);
}

// --------------------------------------------------------------------------------------------- pictures

#[test]
fn a_picture_is_inserted_at_its_natural_size_and_read_back() {
    let mut s = blank();
    let png = fixture("probe-picture.png");
    let id = s.insert_picture(0, &png, at("C3")).unwrap();
    let back = reread(&s);
    let sheet = &back.sheets[0];
    let o = sheet.drawing.find(id).unwrap();
    assert_eq!(o.kind, ObjectKind::Picture);
    assert_eq!(o.anchor.from.0, at("C3"));
    let part = &o.picture.as_ref().unwrap().part;
    assert_eq!(back.kept.get(part).unwrap().as_slice(), png.as_slice());
    assert!(o.lock_aspect);
}

#[test]
fn something_that_is_not_a_picture_is_refused() {
    let mut s = blank();
    assert!(s.insert_picture(0, b"not a picture", at("A1")).is_err());
    assert!(s.book.sheets[0].drawing.objects.is_empty());
}

#[test]
fn a_picture_is_cropped_resized_and_reset() {
    let mut s = blank();
    let id = s.insert_picture(0, &fixture("probe-picture.png"), at("B2")).unwrap();
    let full = s.book.sheets[0].drawing.find(id).unwrap().bounds(&s.book.sheets[0]);
    s.crop_picture(0, id, Crop { left: 10_000, top: 0, right: 10_000, bottom: 20_000 }).unwrap();
    let sheet = &s.book.sheets[0];
    let cropped = sheet.drawing.find(id).unwrap().bounds(sheet);
    assert_eq!(cropped.w, (full.w as f64 * 0.8).round() as i64);
    assert_eq!(cropped.h, (full.h as f64 * 0.8).round() as i64);
    assert_eq!(cropped.x - full.x, (full.w as f64 * 0.1).round() as i64);
    s.resize_object(0, id, cropped.w * 2, 1, true).unwrap();
    let sheet = &s.book.sheets[0];
    assert_eq!(sheet.drawing.find(id).unwrap().bounds(sheet).h, cropped.h * 2);
    s.reset_picture(0, id, true).unwrap();
    let sheet = &s.book.sheets[0];
    let reset = sheet.drawing.find(id).unwrap();
    assert!(reset.picture.as_ref().unwrap().crop.is_none());
    assert_eq!((reset.bounds(sheet).w, reset.bounds(sheet).h), (full.w, full.h));
    let back = reread(&s);
    assert!(back.sheets[0].drawing.find(id).unwrap().picture.as_ref().unwrap().crop.is_none());
}

#[test]
fn deleting_the_only_user_of_an_image_drops_the_image_from_the_file() {
    let mut s = blank();
    let id = s.insert_picture(0, &fixture("probe-picture.png"), at("B2")).unwrap();
    s.delete_object(0, id).unwrap();
    let back = reread(&s);
    assert!(back.kept.parts.iter().all(|(p, _)| !p.starts_with("xl/media/")));
}

// ----------------------------------------------------------------------------------------------- shapes

fn five_shapes(s: &mut Session) -> Vec<ObjectId> {
    let kinds = [NewShape::Rectangle, NewShape::RoundedRectangle, NewShape::Ellipse, NewShape::Line, NewShape::Arrow];
    kinds
        .iter()
        .enumerate()
        .map(|(i, k)| s.insert_shape(0, *k, Rect::new(pt(100 + 120 * i as i64), pt(60), pt(80), pt(40))).unwrap())
        .collect()
}

#[test]
fn every_kind_of_shape_is_made_and_read_back() {
    let mut s = blank();
    let ids = five_shapes(&mut s);
    let tb = s.insert_shape(0, NewShape::TextBox, Rect::new(pt(10), pt(150), pt(120), pt(30))).unwrap();
    s.set_shape_text(0, tb, "Hello\nthere").unwrap();
    let back = reread(&s);
    let sheet = &back.sheets[0];
    let presets: Vec<_> = ids.iter().map(|id| sheet.drawing.find(*id).unwrap().shape.as_ref().unwrap().preset.clone()).collect();
    assert_eq!(presets, ["rect", "roundRect", "ellipse", "line", "straightConnector1"]);
    let t = sheet.drawing.find(tb).unwrap();
    assert_eq!(t.kind, ObjectKind::TextBox);
    assert_eq!(t.shape.as_ref().unwrap().text.as_ref().unwrap().plain_text(), "Hello\nthere");
}

#[test]
fn fill_line_and_text_are_set_and_survive_a_save() {
    let mut s = blank();
    let id = s.insert_shape(0, NewShape::Rectangle, Rect::new(pt(100), pt(100), pt(100), pt(50))).unwrap();
    s.set_shape_fill(0, id, Fill::Solid(ShapeColor::rgb(0, 128, 255))).unwrap();
    s.set_shape_transparency(0, id, 40.0).unwrap();
    s.set_shape_line(
        0,
        id,
        Line { stroke: Stroke::Solid(ShapeColor::rgb(255, 0, 0)), width: Some(pt(3)), dash: Dash::Dash, head: LineEnd::None, tail: LineEnd::None },
    )
    .unwrap();
    s.set_shape_text(0, id, "Box").unwrap();
    s.format_shape_text(
        0,
        id,
        TextFormat { size: Some(18.0), bold: Some(true), italic: Some(true), color: Some(ShapeColor::rgb(0, 0, 0)), align: Some(HAlign::Right), valign: Some(VAlign::Bottom), ..TextFormat::default() },
    )
    .unwrap();
    let back = reread(&s);
    let shape = back.sheets[0].drawing.find(id).unwrap().shape.clone().unwrap();
    assert_eq!(shape.fill, Fill::Solid(ShapeColor::rgb(0, 128, 255).with_transparency(40.0)));
    assert_eq!((shape.line.width, shape.line.dash), (Some(pt(3)), Dash::Dash));
    let text = shape.text.unwrap();
    let run = &text.paragraphs[0].runs[0];
    assert_eq!((run.size, run.bold, run.italic), (Some(18.0), true, true));
    assert_eq!((text.paragraphs[0].align, text.valign), (HAlign::Right, VAlign::Bottom));
}

#[test]
fn a_change_to_one_property_leaves_the_rest_of_an_excel_shape_alone() {
    let mut s = session("15-shapes.xlsx");
    s.set_shape_fill(0, 4, Fill::Solid(ShapeColor::rgb(1, 2, 3))).unwrap();
    let back = reread(&s);
    let rect = back.sheets[0].drawing.find(4).unwrap();
    let xml = rect.tree.to_xml();
    assert!(xml.contains("<xdr:style>"), "{xml}");
    assert!(xml.contains("creationId"), "{xml}");
    assert!(xml.contains("<a:t>rect</a:t>"), "{xml}");
}

#[test]
fn objects_are_stacked_aligned_and_distributed() {
    let mut s = blank();
    let ids = five_shapes(&mut s);
    s.reorder_object(0, ids[0], ZOrder::ToFront).unwrap();
    assert_eq!(s.book.sheets[0].drawing.objects.last().unwrap().id, ids[0]);
    s.reorder_object(0, ids[0], ZOrder::Backward).unwrap();
    let order: Vec<_> = s.book.sheets[0].drawing.objects.iter().map(|o| o.id).collect();
    assert_eq!(order, [ids[1], ids[2], ids[3], ids[0], ids[4]]);
    s.reorder_object(0, ids[4], ZOrder::ToBack).unwrap();
    s.reorder_object(0, ids[4], ZOrder::Forward).unwrap();
    assert_eq!(s.book.sheets[0].drawing.objects[1].id, ids[4]);
    let three = [ids[0], ids[1], ids[2]];
    s.align_objects(0, &three, Alignment::Top).unwrap();
    s.align_objects(0, &three, Alignment::Left).unwrap();
    let sheet = &s.book.sheets[0];
    let lefts: Vec<_> = three.iter().map(|i| sheet.drawing.find(*i).unwrap().bounds(sheet).x).collect();
    assert!(lefts.iter().all(|x| *x == lefts[0]));
    s.move_object(0, ids[1], pt(100), pt(0)).unwrap();
    s.move_object(0, ids[2], pt(300), pt(0)).unwrap();
    s.distribute_objects(0, &three, Distribution::Horizontally).unwrap();
    let sheet = &s.book.sheets[0];
    let b: Vec<_> = three.iter().map(|i| sheet.drawing.find(*i).unwrap().bounds(sheet)).collect();
    let gaps = [b[1].x - b[0].right(), b[2].x - b[1].right()];
    assert!((gaps[0] - gaps[1]).abs() <= 2, "{gaps:?}");
}

#[test]
fn objects_are_grouped_and_ungrouped_where_they_were() {
    let mut s = blank();
    let ids = five_shapes(&mut s);
    let before: Vec<Rect> = ids[..3].iter().map(|i| s.book.sheets[0].drawing.find(*i).unwrap().bounds(&s.book.sheets[0])).collect();
    let g = s.group_objects(0, &ids[..3]).unwrap();
    assert_eq!(s.book.sheets[0].drawing.objects.len(), 3);
    let back = reread(&s);
    let group = back.sheets[0].drawing.find(g).unwrap();
    assert_eq!((group.kind, group.children.len()), (ObjectKind::Group, 3));
    s.move_object(0, g, pt(20), pt(10)).unwrap();
    let freed = s.ungroup_object(0, g).unwrap();
    assert_eq!(freed, ids[..3]);
    let sheet = &s.book.sheets[0];
    for (i, id) in freed.iter().enumerate() {
        let now = sheet.drawing.find(*id).unwrap().bounds(sheet);
        assert_eq!((now.x - before[i].x, now.y - before[i].y), (pt(20), pt(10)));
        assert_eq!((now.w, now.h), (before[i].w, before[i].h));
    }
}

#[test]
fn rotation_alt_text_names_and_placement_are_kept() {
    let mut s = blank();
    let id = s.insert_shape(0, NewShape::Ellipse, Rect::new(pt(100), pt(100), pt(100), pt(50))).unwrap();
    s.rotate_object(0, id, 30.0).unwrap();
    s.set_alt_text(0, id, "A circle").unwrap();
    s.set_object_name(0, id, "Moon").unwrap();
    s.set_placement(0, id, Placement::Fixed).unwrap();
    let back = reread(&s);
    let o = back.sheets[0].drawing.find(id).unwrap();
    assert_eq!((o.rotation, o.descr.as_str(), o.name.as_str(), o.anchor.placement), (30.0, "A circle", "Moon", Placement::Fixed));
    let frame = o.frame_rect(&back.sheets[0]);
    assert_eq!((frame.w, frame.h), (pt(100), pt(50)));
    assert!(s.set_object_name(0, id, "").is_err());
}

#[test]
fn an_object_follows_its_cells_when_rows_are_inserted() {
    let mut s = blank();
    let id = s.insert_shape(0, NewShape::Rectangle, Rect::new(pt(100), pt(100), pt(100), pt(50))).unwrap();
    let before = s.book.sheets[0].drawing.find(id).unwrap().anchor.from.0.row;
    s.insert(0, rows(0, 2), Shift::WholeRows).unwrap();
    let sheet = &s.book.sheets[0];
    assert_eq!(sheet.drawing.find(id).unwrap().anchor.from.0.row, before + 2);
}

// ---------------------------------------------------------------------------------------------- sparklines

#[test]
fn sparkline_groups_are_read_with_every_option() {
    let s = session("16-sparklines.xlsx");
    let groups = &s.book.sheets[0].sparklines;
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].kind, SparklineKind::WinLoss);
    assert_eq!((groups[2].min, groups[2].min_value), (AxisBound::Automatic, Some(-5.0)));
    assert!(groups[2].markers);
    assert_eq!(groups[2].colors.series, Some(Color::Rgb(0xFFFF0000)));
    assert_eq!(groups[2].sparklines.len(), 4);
    assert_eq!(groups[2].sparklines[1].data, "Spark!B3:E3");
}

#[test]
fn sparklines_round_trip_and_leave_the_rest_of_the_sheet_alone() {
    let s = session("16-sparklines.xlsx");
    let back = reread(&s);
    assert_eq!(back.sheets[0].sparklines, s.book.sheets[0].sparklines);
}

#[test]
fn a_structural_edit_and_a_rename_move_sparkline_references() {
    let mut s = session("16-sparklines.xlsx");
    s.insert(0, rows(0, 3), Shift::WholeRows).unwrap();
    let g = &s.book.sheets[0].sparklines[2];
    assert_eq!(g.sparklines[0].location, at("F5"));
    assert_eq!(g.sparklines[0].data, "Spark!B5:E5");
    s.rename_sheet(0, "Renamed").unwrap();
    assert_eq!(s.book.sheets[0].sparklines[0].sparklines[0].data, "Renamed!B5:E5");
    let back = reread(&s);
    assert_eq!(back.sheets[0].sparklines[0].sparklines[0].data, "Renamed!B5:E5");
    s.delete(0, rows(4, 1), Shift::WholeRows).unwrap();
    assert_eq!(s.book.sheets[0].sparklines[2].sparklines.len(), 3, "the sparkline in the deleted row goes");
}

#[test]
fn sparklines_are_inserted_edited_grouped_and_cleared() {
    let mut s = blank();
    let g = s.insert_sparklines(0, SparklineKind::Line, 0, Range::parse("A1:D3").unwrap(), Range::parse("F1:F3").unwrap()).unwrap();
    assert_eq!(s.book.sheets[0].sparklines[g].sparklines[1].data, "Sheet1!A2:D2");
    assert!(s.insert_sparklines(0, SparklineKind::Column, 0, Range::parse("A1:D3").unwrap(), Range::parse("F1:F3").unwrap()).is_err());
    let w = s.insert_sparklines(0, SparklineKind::WinLoss, 0, Range::parse("A1:D3").unwrap(), Range::parse("G1:G3").unwrap()).unwrap();
    assert!(s.book.sheets[0].sparklines[w].negative);
    s.set_sparkline_points(0, g, SparklinePoints { markers: true, high: true, low: true, first: true, last: true, negative: true }).unwrap();
    s.set_sparkline_axis(0, g, AxisBound::Custom(-10.0), AxisBound::SameForAll).unwrap();
    s.set_sparkline_hidden_and_empty(0, g, EmptyCells::Span, true).unwrap();
    s.set_sparkline_weight(0, g, 2.25).unwrap();
    s.set_sparkline_horizontal_axis(0, g, true, true).unwrap();
    s.set_sparkline_kind(0, g, SparklineKind::Column).unwrap();
    let back = reread(&s);
    assert_eq!(back.sheets[0].sparklines, s.book.sheets[0].sparklines);
    let merged = s.group_sparklines(0, &[at("F1"), at("G1")]).unwrap();
    assert_eq!(s.book.sheets[0].sparklines[merged].sparklines.len(), 4);
    let alone = s.ungroup_sparkline(0, at("G1")).unwrap();
    assert_eq!(s.book.sheets[0].sparklines[alone].sparklines.len(), 1);
    assert_eq!(s.clear_sparklines(0, Range::parse("F1:F2").unwrap()).unwrap(), 2);
    s.clear_sparkline_group(0, at("G1")).unwrap();
    assert_eq!(s.book.sheets[0].sparklines.iter().map(|g| g.sparklines.len()).sum::<usize>(), 3);
}

#[test]
fn a_sparkline_gives_a_drawing_its_values_and_scale() {
    let mut s = blank();
    let line = s.insert_sparklines(0, SparklineKind::Line, 0, Range::parse("A1:D3").unwrap(), Range::parse("F1:F3").unwrap()).unwrap();
    let d = unluminous_sheet::objects::sparkline::sparkline_data(&s.book, 0, line, 1).unwrap();
    assert_eq!(d.values, vec![Some(1.0), Some(-2.0), Some(3.0), Some(-4.0)]);
    assert_eq!((d.axis_min, d.axis_max), (-4.0, 3.0));
    assert_eq!((d.high, d.low, d.first, d.last), (Some(2), Some(3), Some(0), Some(3)));
    assert_eq!(d.negatives, vec![1, 3]);
}

// ------------------------------------------------------------------------------------------------ Excel

fn excel_is_installed() -> bool {
    cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

/// Ask Excel what it sees in some files, one JSON object each.
fn inspect(paths: &[&Path]) -> Vec<serde_json::Value> {
    let script = repo_root().join("tools/excel-oracle/objects-inspect.ps1");
    let list = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join(",");
    let output = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(&script)
        .arg("-Paths")
        .arg(list)
        .output()
        .expect("pwsh runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().trim_start_matches('\u{FEFF}'))
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}")))
        .collect()
}

fn excel_or_skip(what: &str) -> bool {
    if excel_is_installed() {
        return true;
    }
    assert!(std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(), "UNLUMINOUS_REQUIRE_EXCEL is set and Excel is not installed");
    println!("Excel is not installed here, so {what} was not checked");
    false
}

fn temp_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("unluminous-objects-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn near(a: &serde_json::Value, expected_points: f64) -> bool {
    (a.as_f64().unwrap_or(f64::NAN) - expected_points).abs() < 0.06
}

#[test]
fn excel_opens_the_feature_workbooks_unchanged_and_after_each_kind_of_edit() {
    if !excel_or_skip("the shapes and sparklines workbooks") {
        return;
    }
    let folder = temp_folder("edits");
    let mut written = Vec::new();
    for name in ["15-shapes.xlsx", "16-sparklines.xlsx"] {
        let s = session(name);
        let path = folder.join(format!("untouched-{name}"));
        std::fs::write(&path, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
        written.push(path);
    }
    let mut s = session("15-shapes.xlsx");
    s.move_object(0, 4, pt(10), pt(5)).unwrap();
    s.set_shape_fill(0, 4, Fill::Solid(ShapeColor::rgb(255, 0, 0))).unwrap();
    s.reorder_object(0, 3, ZOrder::ToFront).unwrap();
    s.set_alt_text(0, 6, "Words").unwrap();
    let g = s.insert_shape(0, NewShape::Ellipse, Rect::new(pt(300), pt(300), pt(50), pt(50))).unwrap();
    s.group_objects(0, &[4, g]).unwrap();
    let path = folder.join("edited-15-shapes.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    written.push(path);
    let mut s = session("16-sparklines.xlsx");
    s.insert(0, rows(0, 2), Shift::WholeRows).unwrap();
    s.rename_sheet(0, "Moved").unwrap();
    let path = folder.join("edited-16-sparklines.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    written.push(path);
    let refs: Vec<&Path> = written.iter().map(|p| p.as_path()).collect();
    let results = inspect(&refs);
    assert_eq!(results.len(), 4);
    for r in &results {
        assert_eq!(r["repaired"], false, "{}: {}", r["file"], r["problem"]);
    }
    // Untouched files show what the originals show.
    let originals = inspect(&[&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/objects/15-shapes.xlsx"), &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/objects/16-sparklines.xlsx")]);
    for (a, b) in originals.iter().zip(&results) {
        assert_eq!(a["sheets"], b["sheets"], "{}", a["file"]);
    }
    let edited = &results[2]["sheets"][0]["shapes"];
    let by_name = |n: &str| edited.as_array().unwrap().iter().find(|s| s["name"] == n).cloned();
    assert_eq!(by_name("Line1").unwrap()["alt"], "Words");
    let count = edited.as_array().unwrap().len() as i64;
    assert_eq!(by_name("Pic1").unwrap()["z"], count - 1, "the picture was brought to the front, below the group made after it");
    let group = edited.as_array().unwrap().iter().find(|s| s["type"] == 6).expect("a group");
    assert!(group["items"].as_str().unwrap().contains("Rect1"), "{group}");
    let moved = results[3]["sheets"][0]["sparklines"].as_array().unwrap().iter().find(|g| g["type"] == 1).unwrap();
    assert_eq!(moved["each"], "F4=B4:E4;F5=B5:E5;F6=B6:E6;F7=B7:E7");
    assert_eq!(results[3]["sheets"][0]["name"], "Moved");
    for p in written {
        let _ = std::fs::remove_file(p);
    }
    let _ = std::fs::remove_dir(&folder);
}

fn find<'a>(shapes: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    shapes.as_array().unwrap().iter().find(|s| s["name"] == name).unwrap_or_else(|| panic!("no shape {name} in {shapes}"))
}

#[test]
fn excel_reads_back_everything_a_workbook_made_from_nothing_says() {
    if !excel_or_skip("a workbook made from nothing") {
        return;
    }
    let mut s = blank();
    let pic = s.insert_picture(0, &fixture("probe-picture.png"), at("B8")).unwrap();
    s.set_alt_text(0, pic, "A probe").unwrap();
    s.crop_picture(0, pic, Crop { left: 10_000, top: 5_000, right: 0, bottom: 20_000 }).unwrap();
    let rect = s.insert_shape(0, NewShape::Rectangle, Rect::new(pt(300), pt(40), pt(100), pt(50))).unwrap();
    s.set_shape_fill(0, rect, Fill::Solid(ShapeColor::rgb(0, 128, 255))).unwrap();
    s.set_shape_transparency(0, rect, 40.0).unwrap();
    s.set_shape_line(0, rect, Line { stroke: Stroke::Solid(ShapeColor::rgb(255, 0, 0)), width: Some(pt(3)), dash: Dash::Dash, ..Line::default() }).unwrap();
    s.set_shape_text(0, rect, "Box").unwrap();
    s.format_shape_text(0, rect, TextFormat { size: Some(18.0), bold: Some(true), italic: Some(true), align: Some(HAlign::Right), valign: Some(VAlign::Bottom), ..TextFormat::default() }).unwrap();
    let round = s.insert_shape(0, NewShape::RoundedRectangle, Rect::new(pt(420), pt(40), pt(80), pt(50))).unwrap();
    let oval = s.insert_shape(0, NewShape::Ellipse, Rect::new(pt(300), pt(120), pt(90), pt(60))).unwrap();
    s.rotate_object(0, oval, 30.0).unwrap();
    let line = s.insert_shape(0, NewShape::Line, Rect::new(pt(420), pt(120), pt(100), pt(40))).unwrap();
    let arrow = s.insert_shape(0, NewShape::Arrow, Rect::new(pt(420), pt(180), pt(100), pt(0))).unwrap();
    let text = s.insert_shape(0, NewShape::TextBox, Rect::new(pt(300), pt(200), pt(120), pt(30))).unwrap();
    s.set_shape_text(0, text, "Notes").unwrap();
    s.set_placement(0, text, Placement::Fixed).unwrap();
    s.reorder_object(0, rect, ZOrder::ToFront).unwrap();
    let g = s.group_objects(0, &[round, line]).unwrap();
    s.set_object_name(0, g, "Pair").unwrap();
    let a = s.insert_sparklines(0, SparklineKind::Line, 0, Range::parse("A1:D3").unwrap(), Range::parse("F1:F3").unwrap()).unwrap();
    s.set_sparkline_points(0, a, SparklinePoints { markers: true, high: true, low: true, first: false, last: true, negative: true }).unwrap();
    s.set_sparkline_axis(0, a, AxisBound::Custom(-10.0), AxisBound::SameForAll).unwrap();
    s.set_sparkline_weight(0, a, 2.25).unwrap();
    s.set_sparkline_hidden_and_empty(0, a, EmptyCells::Span, true).unwrap();
    s.set_sparkline_colors(0, a, unluminous_sheet::objects::sparkline::SparklineColors { series: Some(Color::Rgb(0xFF00_8000)), ..Default::default() }).unwrap();
    s.insert_sparklines(0, SparklineKind::Column, 0, Range::parse("A1:D3").unwrap(), Range::parse("G1:G3").unwrap()).unwrap();
    s.insert_sparklines(0, SparklineKind::WinLoss, 0, Range::parse("A1:D3").unwrap(), Range::parse("H1:H3").unwrap()).unwrap();
    let folder = temp_folder("scratch");
    let path = folder.join("from-nothing.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    let results = inspect(&[&path]);
    let r = &results[0];
    assert_eq!(r["repaired"], false, "{}", r["problem"]);
    let shapes = &r["sheets"][0]["shapes"];
    let b = |id: ObjectId| s.book.sheets[0].drawing.find(id).unwrap().bounds(&s.book.sheets[0]);
    let p = find(shapes, &format!("Picture {}", pic - 1));
    assert_eq!(p["type"], 13);
    assert_eq!(p["alt"], "A probe");
    assert!(near(&p["left"], b(pic).x as f64 / 12700.0) && near(&p["width"], b(pic).w as f64 / 12700.0), "{p}");
    assert!(p["cropLeft"].as_f64().unwrap() > 0.0 && p["cropBottom"].as_f64().unwrap() > 0.0, "{p}");
    let rc = find(shapes, &format!("Rectangle {}", rect - 1));
    assert_eq!((rc["type"].as_i64(), rc["auto"].as_i64()), (Some(1), Some(1)));
    assert!(near(&rc["left"], 300.0) && near(&rc["top"], 40.0) && near(&rc["width"], 100.0) && near(&rc["height"], 50.0), "{rc}");
    assert_eq!(rc["fillRgb"], 0xFF8000, "BGR of 0,128,255");
    assert!(near(&rc["fillTransparency"], 0.4), "{rc}");
    assert_eq!((rc["lineRgb"].as_i64(), rc["lineWeight"].as_f64()), (Some(255), Some(3.0)));
    assert_eq!(rc["lineDash"], 4, "dash");
    assert_eq!((rc["text"].as_str(), rc["fontSize"].as_f64()), (Some("Box"), Some(18.0)));
    assert_eq!((rc["bold"].as_i64(), rc["italic"].as_i64()), (Some(-1), Some(-1)));
    assert_eq!((rc["halign"].as_i64(), rc["valign"].as_i64()), (Some(3), Some(4)));
    let top = shapes.as_array().unwrap().iter().filter_map(|s| s["z"].as_i64()).max();
    assert_eq!(rc["z"].as_i64(), top, "brought to the front");
    let ov = find(shapes, &format!("Oval {}", oval - 1));
    assert_eq!(ov["auto"], 9);
    assert!(near(&ov["rotation"], 30.0), "{ov}");
    assert!(near(&ov["width"], 90.0) && near(&ov["height"], 60.0), "{ov}");
    let tb = find(shapes, &format!("TextBox {}", text - 1));
    assert_eq!((tb["type"].as_i64(), tb["text"].as_str(), tb["placement"].as_i64()), (Some(17), Some("Notes"), Some(3)));
    let ar = find(shapes, &format!("Straight Arrow Connector {}", arrow - 1));
    assert_eq!(ar["type"], 1);
    assert_eq!(ar["endArrow"], 2, "a triangle at the end");
    let grp = find(shapes, "Pair");
    assert_eq!(grp["type"], 6);
    assert_eq!(grp["items"].as_str().unwrap().split(',').count(), 2);
    let groups = r["sheets"][0]["sparklines"].as_array().unwrap();
    assert_eq!(groups.len(), 3);
    let line_group = groups.iter().find(|g| g["type"] == 1).unwrap();
    assert_eq!(line_group["location"], "F1:F3");
    assert_eq!(line_group["source"], "A1:D3");
    assert_eq!((line_group["markers"].as_bool(), line_group["high"].as_bool(), line_group["low"].as_bool()), (Some(true), Some(true), Some(true)));
    assert_eq!((line_group["first"].as_bool(), line_group["last"].as_bool(), line_group["negative"].as_bool()), (Some(false), Some(true), Some(true)));
    assert_eq!(line_group["weight"].as_f64(), Some(2.25));
    assert_eq!((line_group["minType"].as_i64(), line_group["minCustom"].as_f64(), line_group["maxType"].as_i64()), (Some(3), Some(-10.0), Some(1)));
    assert_eq!(line_group["displayHidden"], true);
    assert_eq!(line_group["displayBlanksAs"], 3, "span");
    assert_eq!(line_group["seriesColor"], 0x008000, "BGR of 0,128,0");
    assert!(groups.iter().any(|g| g["type"] == 2 && g["location"] == "G1:G3"));
    assert!(groups.iter().any(|g| g["type"] == 3 && g["location"] == "H1:H3" && g["negative"] == true));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&folder);
}
