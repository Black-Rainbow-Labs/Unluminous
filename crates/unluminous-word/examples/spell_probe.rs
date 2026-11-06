//! Says which of the words given on the command line the English speller accepts, one a line, and the
//! first suggestions for each word it does not.
//!
//! `cargo run -p unluminous-word --example spell_probe -- <words>`, or `--file <path>` for a file of words,
//! one a line. `--rejected` prints only the words not accepted, with no suggestions, which is how a long
//! list is measured. `UNLUMINOUS_SPELL_AFF` and `UNLUMINOUS_SPELL_DIC` name another Hunspell dictionary to
//! measure in place of the one that ships, and `UNLUMINOUS_SPELL_EXTRA` a file of further words, one a line,
//! added to it.

use unluminous_word::spell::Speller;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let rejected_only = args.iter().any(|a| a == "--rejected");
    args.retain(|a| a != "--rejected");
    let words: Vec<String> = if args.first().map(String::as_str) == Some("--file") {
        std::fs::read_to_string(&args[1])
            .unwrap()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect()
    } else {
        args
    };
    let speller =
        match (std::env::var("UNLUMINOUS_SPELL_AFF"), std::env::var("UNLUMINOUS_SPELL_DIC")) {
            (Ok(aff), Ok(dic)) => {
                let mut words = std::fs::read_to_string(dic).unwrap();
                if let Ok(extra) = std::env::var("UNLUMINOUS_SPELL_EXTRA") {
                    if !words.ends_with('\n') {
                        words.push('\n');
                    }
                    words.push_str(&std::fs::read_to_string(extra).unwrap());
                }
                Speller::from_hunspell(&std::fs::read_to_string(aff).unwrap(), &words).unwrap()
            }
            _ => Speller::english().unwrap(),
        };
    let mut wrong = 0;
    for word in &words {
        let correct = speller.is_correct(word);
        if !correct {
            wrong += 1;
        }
        match (rejected_only, correct) {
            (true, false) => println!("{word}"),
            (true, true) => {}
            (false, true) => println!("ok   {word}"),
            (false, false) => println!("BAD  {word}  -> {:?}", speller.suggestions(word, 3)),
        }
    }
    eprintln!("{wrong} of {} not accepted", words.len());
}
