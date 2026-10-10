//! What a person chose from the completion list before, so it comes first among equals the next time.
//! `task-2231` §6.3, weigher 5, after the reference editor's `StatisticsManager`.
//!
//! A choice is counted under the language, the kind of place it was made in, and the first letter of
//! what had been typed: choosing `draw_frame` after `d` in an expression in Rust makes it come first the
//! next time `d` is typed in an expression in Rust, and changes nothing in a type position or in
//! TypeScript. The count only decides between rows the earlier weighers left equal, so a choice can
//! never lift a subsequence match above a prefix match.
//!
//! The counts are per person and kept in `completion-stats.txt` in the person's own settings folder,
//! one `count<TAB>last<TAB>language<TAB>place<TAB>letter<TAB>name` a line. They are written once the
//! window has settled after a choice ([`crate::app::WINDOW_SETTLE`]) and when the window closes, never
//! on the keystroke itself. A window with no settings folder, which is every window a test builds,
//! counts in memory and writes nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use unluminous_core::place::Place;

/// The file the counts are kept in, inside the person's settings folder.
const FILE: &str = "completion-stats.txt";

/// How many choices are remembered. Past this the least recently chosen are forgotten.
const LIMIT: usize = 5_000;

/// The most a count grows to, so one name chosen ten thousand times is not worth more than one chosen
/// a hundred times when the two are next to each other.
const MOST: u32 = 100;

/// Every choice remembered, and whether any of them still has to be written down.
#[derive(Debug, Default)]
pub struct CompletionStats {
    counts: HashMap<String, (u32, u64)>,
    /// When the last unwritten choice was made, in the window's own seconds.
    changed_at: Option<f64>,
    /// How many choices have been made, which orders them by recency.
    clock: u64,
    folder: Option<PathBuf>,
}

/// The key a choice is counted under.
///
/// @param language - the language's name
/// @param place - the kind of place
/// @param stem - what had been typed
/// @param name - the row chosen
fn key(language: &str, place: Place, stem: &str, name: &str) -> String {
    let letter: String =
        stem.chars().next().map(|c| c.to_lowercase().collect()).unwrap_or_default();
    format!("{language}\t{}\t{letter}\t{name}", place.name())
}

impl CompletionStats {
    /// The counts kept in a settings folder, or none when there is no file yet.
    ///
    /// @param folder - the person's settings folder
    pub fn read_from(folder: &Path) -> CompletionStats {
        let mut stats =
            CompletionStats { folder: Some(folder.to_path_buf()), ..Default::default() };
        let Ok(text) = std::fs::read_to_string(folder.join(FILE)) else { return stats };
        for line in text.lines() {
            let mut parts = line.splitn(3, '\t');
            let (Some(count), Some(last), Some(rest)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let (Ok(count), Ok(last)) = (count.parse::<u32>(), last.parse::<u64>()) else {
                continue;
            };
            stats.clock = stats.clock.max(last);
            stats.counts.insert(rest.to_owned(), (count.min(MOST), last));
        }
        stats
    }

    /// How many times a row was chosen before for a question like this one.
    ///
    /// @param language - the language's name
    /// @param place - the kind of place
    /// @param stem - what has been typed
    /// @param name - the row
    pub fn chosen(&self, language: &str, place: Place, stem: &str, name: &str) -> u32 {
        if self.counts.is_empty() {
            return 0;
        }
        self.counts.get(&key(language, place, stem, name)).map_or(0, |(count, _)| *count)
    }

    /// Counts a choice.
    ///
    /// @param language - the language's name
    /// @param place - the kind of place
    /// @param stem - what had been typed
    /// @param name - the row chosen
    /// @param now - the window's time, in seconds
    pub fn record(&mut self, language: &str, place: Place, stem: &str, name: &str, now: f64) {
        self.clock += 1;
        let entry = self.counts.entry(key(language, place, stem, name)).or_insert((0, 0));
        entry.0 = (entry.0 + 1).min(MOST);
        entry.1 = self.clock;
        self.changed_at = Some(now);
        if self.counts.len() > LIMIT {
            self.forget_the_oldest();
        }
    }

    /// Forgets the least recently chosen tenth, so the file stays small.
    fn forget_the_oldest(&mut self) {
        let mut lasts: Vec<u64> = self.counts.values().map(|(_, last)| *last).collect();
        lasts.sort_unstable();
        let cut = lasts[lasts.len() / 10];
        self.counts.retain(|_, (_, last)| *last > cut);
    }

    /// Writes the counts down when a choice has gone unwritten for `settle` seconds, or at once when
    /// `settle` is zero, which is what closing the window asks.
    ///
    /// @param now - the window's time, in seconds
    /// @param settle - how long to wait after the last choice
    pub fn write_when_settled(&mut self, now: f64, settle: f64) {
        let Some(changed) = self.changed_at else { return };
        if now - changed < settle {
            return;
        }
        self.changed_at = None;
        let Some(folder) = &self.folder else { return };
        let mut lines: Vec<(&String, &(u32, u64))> = self.counts.iter().collect();
        lines.sort_by_key(|(_, (_, last))| std::cmp::Reverse(*last));
        let text: String =
            lines.iter().map(|(key, (count, last))| format!("{count}\t{last}\t{key}\n")).collect();
        if let Err(problem) =
            crate::services::store::write_atomically(&folder.join(FILE), text.as_bytes())
        {
            eprintln!("unluminous: could not write the completion statistics: {problem}");
        }
    }

    /// True when a choice has not been written down yet.
    pub fn is_unwritten(&self) -> bool {
        self.changed_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_is_counted_under_its_language_its_place_and_its_first_letter() {
        let mut stats = CompletionStats::default();
        stats.record("Rust", Place::Expression, "dr", "draw_frame", 1.0);
        stats.record("Rust", Place::Expression, "d", "draw_frame", 2.0);
        assert_eq!(stats.chosen("Rust", Place::Expression, "dra", "draw_frame"), 2);
        assert_eq!(stats.chosen("Rust", Place::Type, "dra", "draw_frame"), 0, "another place");
        assert_eq!(stats.chosen("TypeScript", Place::Expression, "d", "draw_frame"), 0);
        assert_eq!(stats.chosen("Rust", Place::Expression, "f", "draw_frame"), 0, "another letter");
    }

    #[test]
    fn the_counts_are_written_once_settled_and_read_back() {
        let folder = std::env::temp_dir().join(format!("unluminous-stats-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let mut stats = CompletionStats::read_from(&folder);
        stats.record("Rust", Place::Statement, "l", "layout", 10.0);
        stats.write_when_settled(10.1, 0.35);
        assert!(stats.is_unwritten(), "not before the window has settled");
        stats.write_when_settled(10.5, 0.35);
        assert!(!stats.is_unwritten());
        let again = CompletionStats::read_from(&folder);
        assert_eq!(again.chosen("Rust", Place::Statement, "la", "layout"), 1);
        std::fs::remove_dir_all(&folder).ok();
    }
}
