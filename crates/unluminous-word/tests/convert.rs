//! Files only Microsoft Word reads, opened through `tools/word-oracle/convert.ps1` (task-2359, TDD 5.6).
//!
//! The fixtures are `paragraphs.docx` saved by Word as `.doc`, `.rtf`, `.odt`, `.html` and `.pdf`, and
//! `convert.word.json` is how many paragraphs Word reads in each. Each is converted and the `.docx` Word
//! made is read by Unluminous, which must find the same paragraphs. Without Word the test prints a line and
//! passes, and `UNLUMINOUS_REQUIRE_WORD=1` makes that a failure.

use std::path::Path;

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

#[test]
fn files_only_word_reads_open_with_words_paragraphs() {
    if !word_is_installed() {
        return;
    }
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/convert");
    let counts: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(fixtures.join("convert.word.json"))
            .unwrap()
            .trim_start_matches('\u{feff}'),
    )
    .unwrap();
    let folder = std::env::temp_dir().join("unluminous-word-convert");
    std::fs::create_dir_all(&folder).unwrap();
    for extension in ["doc", "rtf", "odt", "html", "pdf"] {
        let target = folder.join(format!("{extension}.docx"));
        let _ = std::fs::remove_file(&target);
        unluminous_word::convert::convert(
            &fixtures.join(format!("convert.{extension}")),
            &target,
            "docx",
        )
        .unwrap_or_else(|problem| panic!("{extension}: {problem}"));
        let document = unluminous_word::read::open(&std::fs::read(&target).unwrap()).unwrap();
        let paragraphs = unluminous_word::position::paragraphs(&document.body).len();
        assert_eq!(paragraphs as u64, counts[extension].as_u64().unwrap(), "{extension}");
    }
}

#[test]
fn a_document_saved_as_rtf_and_odt_through_word_comes_back() {
    if !word_is_installed() {
        return;
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/paragraphs.docx");
    let folder = std::env::temp_dir().join("unluminous-word-convert");
    std::fs::create_dir_all(&folder).unwrap();
    for format in ["rtf", "odt"] {
        let saved = folder.join(format!("saved.{format}"));
        let back = folder.join(format!("back-{format}.docx"));
        unluminous_word::convert::convert(&source, &saved, format).unwrap();
        unluminous_word::convert::convert(&saved, &back, "docx").unwrap();
        let document = unluminous_word::read::open(&std::fs::read(&back).unwrap()).unwrap();
        assert_eq!(unluminous_word::position::paragraphs(&document.body).len(), 22, "{format}");
    }
}
