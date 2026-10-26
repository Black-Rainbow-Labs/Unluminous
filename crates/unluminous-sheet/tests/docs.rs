//! The documentation states facts about the engine that a test can check, so they cannot fall behind.

/// `CLAUDE.md` names how many functions the evaluator has, and the number must be the real one.
#[test]
fn claude_md_states_the_number_of_functions() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../CLAUDE.md");
    let text = std::fs::read_to_string(path).expect("CLAUDE.md is at the root of the workspace");
    let count = unluminous_sheet::functions::all().len();
    let phrase = format!("the evaluator and its {count} functions");
    assert!(
        text.contains(&phrase),
        "CLAUDE.md should say \"{phrase}\", because functions::all() has {count} entries"
    );
}
