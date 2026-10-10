//! The identifier tokens and definitions of every file in a corpus, for the completion evaluation's
//! position generator (`tools/completion-eval/gen-positions.mjs`, `task-2231` §8.1).
//!
//! It reads each file with the same tokeniser and the same definition reading the code index uses, so
//! a position the generator chooses is never inside a comment or a string by the index's own account,
//! and the language of a file is decided by the same manifests. It is an example rather than a command
//! because nothing but the harness asks this question: a command would be a row in the catalogue, a
//! section in `commands.md` and a tool in every agent's preamble for a measurement nobody else runs.
//!
//! Every offset is a byte of the file's text **after `\r\n` has been turned into `\n`**, which is the
//! text an editor's document holds and the coordinate `positions.json` is written in.
//!
//! ```sh
//! cargo run --release -p unluminous-cli --example completion_tokens -- <corpus root> <language> > tokens.jsonl
//! ```
//!
//! One JSON object a line, one line a file:
//! `{"path":"src/a.rs","words":[[start,end,"k"]],"quiet":[[start,end]],"definitions":[...]}`, where the
//! word kind is `k` for a keyword, `b` a builtin, `f` a word followed by a bracket, `t` a capitalised
//! word and `w` any other word.

use std::path::Path;

use atrius_index::lang::symbols::FileSymbols;
use atrius_index::lang::syntax::{self, Token};
use serde_json::{json, Value};

/// Prints one line for every file of the language under a root.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (Some(root), Some(language)) = (arguments.first(), arguments.get(1)) else {
        eprintln!(
            "usage: completion_tokens <corpus root> <language id, such as rust or typescript>"
        );
        std::process::exit(2);
    };
    let root = Path::new(root);
    for found in atrius_index::files::walk(root) {
        let Some(found_language) = atrius_index::grammars::for_path(&found.rel) else { continue };
        if found_language.id != *language || found.size > 1_048_576 {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(&found.rel)) else { continue };
        let Ok(text) = String::from_utf8(bytes) else { continue };
        let text = text.replace("\r\n", "\n");
        println!("{}", read_one(&found.rel, &text, &found_language.grammar));
    }
}

/// The tokens and definitions of one file as one JSON line.
///
/// @param rel - the file's path relative to the root
/// @param text - its text with `\n` line breaks
/// @param grammar - its language
fn read_one(rel: &str, text: &str, grammar: &syntax::Grammar) -> Value {
    let mut words: Vec<Value> = Vec::new();
    let mut quiet: Vec<Value> = Vec::new();
    syntax::scan(text, grammar, |range, token| {
        let kind = match token {
            Token::Keyword => "k",
            Token::Builtin => "b",
            Token::Function => "f",
            Token::Type => "t",
            Token::Text => "w",
            Token::Comment | Token::String => {
                quiet.push(json!([range.start, range.end]));
                return;
            }
            Token::Number | Token::Operator => return,
        };
        let word = &text[range.clone()];
        let starts_a_word = word.chars().next().is_some_and(|c| grammar.is_word_character(c, true));
        if starts_a_word {
            words.push(json!([range.start, range.end, kind]));
        }
    });
    let symbols = FileSymbols::read(text, grammar);
    let definitions: Vec<Value> = symbols
        .definitions()
        .iter()
        .map(|d| {
            json!({
                "start": d.name_range.start,
                "end": d.name_range.end,
                "kind": d.kind.name(),
                "exported": d.exported,
            })
        })
        .collect();
    let outline = atrius_index::outline::read(rel, text, atrius_index::outline::CHUNK_BUDGET);
    let blocks: Vec<Value> = outline
        .definitions
        .iter()
        .map(|d| json!({"name": d.name, "kind": d.kind, "line": d.line, "end": d.end, "depth": d.depth}))
        .collect();
    json!({"path": rel, "words": words, "quiet": quiet, "definitions": definitions, "blocks": blocks})
}
