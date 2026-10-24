//! Documents Unluminous writes, opened by Microsoft Word.
//!
//! Each test writes a `.docx` with Unluminous, asks Word for its report through
//! `tools/word-oracle/inspect.ps1`, and checks that Word opened it without repairing it and reads in it what
//! Unluminous put there. Word has to be installed: without it each test prints a line and passes, and
//! `UNLUMINOUS_REQUIRE_WORD=1` makes a missing Word a failure instead.

use std::path::{Path, PathBuf};

use serde_json::Value;
use unluminous_word::builtin::ListKind;
use unluminous_word::position::{Pos, Selection};
use unluminous_word::session::Session;

fn word_is_installed() -> bool {
    let found = [
        "C:/Program Files/Microsoft Office/root/Office16/WINWORD.EXE",
        "C:/Program Files (x86)/Microsoft Office/root/Office16/WINWORD.EXE",
    ]
    .iter()
    .any(|p| Path::new(p).is_file());
    if !found {
        assert!(
            std::env::var("UNLUMINOUS_REQUIRE_WORD").is_err(),
            "UNLUMINOUS_REQUIRE_WORD is set and Word is not installed"
        );
        println!("Word is not installed, so this test checked nothing");
    }
    found
}

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join("unluminous-word-live");
    std::fs::create_dir_all(&folder).unwrap();
    folder.join(name)
}

/// Word's report on a file.
fn inspect(path: &Path) -> Value {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/word-oracle/inspect.ps1");
    let out = path.with_extension("word.json");
    let _ = std::fs::remove_file(&out);
    let status = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(&script)
        .arg("-Path")
        .arg(path)
        .arg("-Out")
        .arg(&out)
        .status()
        .expect("pwsh runs");
    assert!(status.success(), "inspect.ps1 failed on {}", path.display());
    serde_json::from_str(std::fs::read_to_string(&out).unwrap().trim_start_matches('\u{feff}'))
        .unwrap()
}

fn texts(report: &Value) -> Vec<String> {
    report["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            p["text"]
                .as_str()
                .unwrap_or_default()
                .replace('\u{7}', "")
                .trim_end_matches('\r')
                .to_owned()
        })
        .collect()
}

/// A tiny PNG, 4 by 2 pixels, written by hand so the test needs no image library.
fn png() -> Vec<u8> {
    use std::io::Write;
    let (w, h) = (4u32, 2u32);
    let mut raw = Vec::new();
    for y in 0..h {
        raw.push(0u8);
        for x in 0..w {
            raw.extend([(x * 60) as u8, (y * 120) as u8, 200u8]);
        }
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&raw).unwrap();
    let data = z.finish().unwrap();
    let chunk = |kind: &[u8], body: &[u8]| {
        let mut out = (body.len() as u32).to_be_bytes().to_vec();
        out.extend(kind);
        out.extend(body);
        let mut crc_input = kind.to_vec();
        crc_input.extend(body);
        out.extend(crc32fast::hash(&crc_input).to_be_bytes());
        out
    };
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IDAT", &data));
    png.extend(chunk(b"IEND", &[]));
    png
}

/// Select the text `words` in the body.
fn select(session: &mut Session, words: &str) {
    let found = session.find(words, true, false);
    session.selection =
        found.first().cloned().unwrap_or_else(|| panic!("{words} is in the document"));
}

fn caret_at_end(session: &mut Session) {
    let all = unluminous_word::position::paragraphs(&session.document.body);
    let (path, p) = all.last().unwrap();
    session.selection = Selection::caret(Pos::new(path.clone(), p.len()));
}

#[test]
fn a_document_using_every_feature_opens_in_word_as_written() {
    if !word_is_installed() {
        return;
    }
    let mut s = Session::new(unluminous_word::new::blank());
    s.author = "Unluminous Test".into();
    s.type_text("Annual Review\nIntroduction\nThis document is written by Unluminous and read by Word.\nalpha\nbeta\ngamma\none\ntwo\nThe end of the body.");
    select(&mut s, "Annual Review");
    s.apply_style("Title").unwrap();
    select(&mut s, "Introduction");
    s.apply_style("Heading1").unwrap();
    select(&mut s, "written");
    s.toggle("w:b");
    select(&mut s, "Unluminous");
    s.toggle("w:i");
    s.format(|rpr| unluminous_word::props::set_val(rpr, "w:color", Some("C00000")));
    select(&mut s, "Word");
    s.format(|rpr| unluminous_word::props::set_val(rpr, "w:highlight", Some("yellow")));
    s.format(|rpr| unluminous_word::props::set_val(rpr, "w:u", Some("double")));
    select(&mut s, "This document");
    s.format_paragraphs(|ppr| {
        unluminous_word::props::set_val(ppr, "w:jc", Some("both"));
        ppr.child_or_insert("w:ind").set_attr("w:firstLine", "360");
        ppr.child_or_insert("w:spacing").set_attr("w:after", "240");
    });
    // A bullet list, then a numbered one.
    let alpha = s.find("alpha", true, true)[0].anchor.clone();
    let gamma = s.find("gamma", true, true)[0].head.clone();
    s.select(alpha, gamma);
    s.toggle_list(ListKind::Bullet);
    let one = s.find("one", true, true)[0].anchor.clone();
    let two = s.find("two", true, true)[0].head.clone();
    s.select(one, two);
    s.toggle_list(ListKind::Number);
    // A table.
    select(&mut s, "The end of the body.");
    s.selection = Selection::caret(s.selection.anchor.clone());
    s.insert_table(3, 3);
    s.type_text("Name");
    s.table_op("merge-right").unwrap();
    // A picture, a footnote, a comment, a link and a bookmark.
    select(&mut s, "read by Word");
    s.selection = Selection::caret(s.selection.head.clone());
    unluminous_word::media::insert_picture(&mut s, &png(), "png", 40.0, 20.0, "swatch.png")
        .unwrap();
    s.insert_note(false, "A footnote from Unluminous.");
    select(&mut s, "Introduction");
    let comment = s.add_comment("Is this heading right?");
    s.reply_to_comment(comment, "It is.").unwrap();
    s.resolve_comment(comment, true).unwrap();
    select(&mut s, "body");
    s.insert_hyperlink("https://example.com/", None);
    select(&mut s, "Annual");
    s.insert_bookmark("Top");
    // Headers, footers, a section break into landscape, and a table of contents.
    s.set_header_text(false, "default", "Unluminous header");
    s.set_header_text(true, "default", "Page {PAGE} of {NUMPAGES}");
    caret_at_end(&mut s);
    s.insert_section_break("nextPage");
    s.type_text("A landscape section.");
    s.set_orientation(true);
    select(&mut s, "Introduction");
    s.selection = Selection::caret(s.selection.anchor.clone());
    let pages = unluminous_word::layout::lay_out(
        &s.document,
        &unluminous_core::FixedMetrics::default(),
        Default::default(),
    );
    s.insert_toc(3, &pages);
    // Tracked changes last.
    s.set_tracking(true);
    select(&mut s, "landscape");
    s.type_text("wide");
    unluminous_word::new::set_property(&mut s.document, "dc:title", "Annual Review");

    let path = scratch("every-feature.docx");
    std::fs::write(&path, unluminous_word::write::save(&s.document).unwrap()).unwrap();
    let report = inspect(&path);
    assert_eq!(report["opened"], true, "Word did not open it: {}", report["error"]);
    let texts = texts(&report);
    let has = |text: &str| texts.iter().any(|t| t.contains(text));
    for expected in [
        "Annual Review",
        "Introduction",
        "alpha",
        "gamma",
        "The end of the",
        // "wide" is the tracked insertion and "landscape" the tracked deletion, which Word's range
        // text still contains.
        "A widelandscape section.",
    ] {
        assert!(has(expected), "Word does not see {expected:?} in {texts:?}");
    }
    let paragraphs = report["paragraphs"].as_array().unwrap();
    let style_of = |text: &str| {
        paragraphs
            .iter()
            .find(|p| p["text"].as_str() == Some(text))
            .map(|p| p["style"].as_str().unwrap_or("").to_owned())
    };
    assert_eq!(style_of("Annual Review").as_deref(), Some("Title"));
    assert_eq!(style_of("Introduction").as_deref(), Some("Heading 1"));
    let lists: Vec<&str> = paragraphs
        .iter()
        .filter_map(|p| p["listString"].as_str())
        .filter(|l| !l.is_empty())
        .collect();
    assert!(lists.len() >= 5, "Word sees the two lists: {lists:?}");
    assert!(lists.contains(&"1.") && lists.contains(&"2."), "the numbered list: {lists:?}");
    assert_eq!(report["tables"].as_array().unwrap().len(), 1);
    assert_eq!(report["footnotes"].as_array().unwrap().len(), 1);
    assert!(report["footnotes"][0].as_str().unwrap_or("").contains("A footnote from Unluminous."));
    let comments = report["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2, "a comment and its reply");
    assert_eq!(comments[0]["author"], "Unluminous Test");
    assert!(comments.iter().any(|c| c["done"] == true), "the resolved comment is done in Word");
    assert!(report["hyperlinks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["address"] == "https://example.com/"));
    assert!(report["bookmarks"].as_array().unwrap().iter().any(|b| b == "Top"));
    assert_eq!(report["inlineShapes"].as_array().unwrap().len(), 1, "the picture");
    assert_eq!(report["tablesOfContents"], 1);
    assert!(
        report["revisions"].as_array().unwrap().len() >= 2,
        "the tracked insertion and deletion"
    );
    let sections = report["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[1]["orientation"], 1, "the second section is landscape");
    assert_eq!(sections[0]["headers"]["primary"], "Unluminous header");
    assert!(sections[0]["footers"]["primary"].as_str().unwrap_or("").starts_with("Page "));
    assert_eq!(report["properties"]["title"], "Annual Review");
}

#[test]
fn every_fixture_saved_by_unluminous_opens_in_word_unrepaired() {
    if !word_is_installed() {
        return;
    }
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "docx"))
        .collect();
    files.sort();
    for file in files {
        let name = file.file_stem().unwrap().to_string_lossy().into_owned();
        let original = inspect_copy(&file, &name);
        let mut session =
            Session::new(unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap());
        // An edit at the start of the body, so the main part is written from the model.
        session.type_text("Edited by Unluminous. ");
        let path = scratch(&format!("{name}-edited.docx"));
        std::fs::write(&path, unluminous_word::write::save(&session.document).unwrap()).unwrap();
        let report = inspect(&path);
        assert_eq!(report["opened"], true, "{name}: Word did not open it: {}", report["error"]);
        assert_eq!(
            report["paragraphs"].as_array().unwrap().len(),
            original["paragraphs"].as_array().unwrap().len(),
            "{name}: the paragraph count changed"
        );
        assert_eq!(
            report["tables"].as_array().unwrap().len(),
            original["tables"].as_array().unwrap().len(),
            "{name}: the tables"
        );
        assert_eq!(report["footnotes"], original["footnotes"], "{name}: the footnotes");
        assert_eq!(
            report["comments"].as_array().unwrap().len(),
            original["comments"].as_array().unwrap().len(),
            "{name}: the comments"
        );
        assert_eq!(
            report["inlineShapes"].as_array().unwrap().len(),
            original["inlineShapes"].as_array().unwrap().len(),
            "{name}: the pictures"
        );
        assert_eq!(
            report["sections"].as_array().unwrap().len(),
            original["sections"].as_array().unwrap().len(),
            "{name}: the sections"
        );
        let first = texts(&report).into_iter().find(|t| !t.is_empty()).unwrap_or_default();
        assert!(
            first.starts_with("Edited by Unluminous."),
            "{name}: the edit is where it was typed: {first:?}"
        );
    }
}

/// Word's committed report for a fixture.
fn inspect_copy(file: &Path, name: &str) -> Value {
    let report = file.with_extension("word.json");
    let _ = name;
    serde_json::from_str(std::fs::read_to_string(report).unwrap().trim_start_matches('\u{feff}'))
        .unwrap()
}
