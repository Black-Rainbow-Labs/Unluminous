//! Turning a terminal's screen into the bytes that would draw it, and back.
//!
//! **What this is for.** `task-1908` asks that a terminal come back showing what was on it — *"one with `ls`
//! command executed … I want both exactly restored so I see … the contents of `ls`"*. A process cannot come
//! back, for the reason `tasks/task-1908-restoring-what-was-open-tdd.md` §1.1 sets out with tmux's and
//! iTerm2's own documentation: a program outlives its editor only if the editor was never its parent. What
//! can come back is the screen.
//!
//! **Bytes rather than text, and that is the whole design.** `Session::written_text` already answers with what
//! a terminal *says*, and it is the wrong thing to save: it takes each cell's character and nothing else, so
//! `total 48` would come back with no bold, no green directory names and the cursor nowhere in particular. A
//! restored terminal that lost its colours is one a person can see is not the one they left. So what is written
//! down is what a terminal is written to with in the first place — an escape sequence stream, exactly as a
//! program would have sent it — and putting it back is feeding a fresh session the same bytes.
//!
//! **Only the normal buffer.** A full-screen program draws into the *alternate* screen, and xterm's own
//! reference says that buffer is cleared when it is entered and abandoned when it is left, so there is nothing
//! there to save even in principle. A session sitting in the alternate buffer has its **normal** buffer written
//! down, which is the shell's — the prompt somebody typed the program's name at. §1.2 of the design has the
//! quotation and what follows from it.
//!
//! **The sequences used are the ones every terminal has had since the 1970s**, and deliberately no more than
//! that: `SGR` for the colours and the attributes, `CUP` for the cursor, and a plain newline between rows.
//! Nothing here needs to be understood by anything but Unluminous's own emulator, but keeping to the common
//! subset means a stream written by one version is read by the next.

use crate::palette::Rgb;
use crate::screen::{Screen, ScreenCell};

/// How many bytes of a terminal's screen are kept.
///
/// **A bound rather than everything**, because a build log is megabytes and a project's own folder is not a
/// place to put megabytes without saying so. It is generous enough for a screen and a few pages behind it,
/// which is what somebody reading their last command wants back.
pub const REPLAY_LIMIT: usize = 256 * 1024;

/// How many rows of a terminal's scrollback and screen are kept.
///
/// **The number `task-1912` is about.** What was written down before it was the *visible grid*, so a node
/// fourteen rows tall kept fourteen rows and the report — *"if I've ran 5 commands, all 5 and their results
/// should be shown"* — was answered with the last two of them.
///
/// A thousand is a deliberate multiple of what the surveyed tools keep: VS Code's
/// `terminal.integrated.persistentSessionScrollback` is **100 lines** by default. Five commands with real
/// output is more than a hundred lines more often than not, and a project folder can hold a quarter of a
/// megabyte without anybody minding — which is [`REPLAY_LIMIT`], and whichever of the two binds first is the
/// one that applies.
pub const REPLAY_ROWS: usize = 1000;

/// The bytes that would draw `screen`, from an empty terminal.
///
/// The stream is: for each row, the cells in runs of one style, then a newline; then the cursor put back where
/// it was. Trailing blank rows are dropped, because a screen is mostly empty and writing eighty spaces a row
/// for twenty rows of nothing is twenty rows of nothing in the file as well.
pub fn bytes_of(screen: &Screen) -> Vec<u8> {
    let mut out = Vec::new();
    // Reset first, so a stream is read the same way whatever the terminal was doing before it.
    out.extend_from_slice(b"\x1b[0m");
    let last = last_row_with_anything(screen);
    for row in 0..=last {
        write_a_row(screen, row, &mut out);
        // No newline after the last row: a terminal that has printed `n` lines has its cursor on line `n`, and
        // a trailing newline would scroll the whole screen up by one.
        if row < last {
            out.extend_from_slice(b"\r\n");
        }
    }
    // The attributes are put back to nothing, so a restored screen does not colour whatever is typed next.
    out.extend_from_slice(b"\x1b[0m");
    // **And a newline, so the live shell's own prompt starts on a line of its own.**
    //
    // What is replayed is *history*: the last thing on it is very often a prompt, and the shell that starts
    // afterwards prints its own. Without this the two would land on the same line and read as one corrupted
    // prompt. With it the restored text reads as what it is — what was there before — and the prompt below it is
    // the live one. That is what macOS Terminal does with restored scrollback, and `task-1908` §1 records why
    // this is the honest shape: the screen comes back, the process does not.
    //
    // The cursor is deliberately **not** put back for the same reason. A restored screen is not a screen
    // somebody is typing into, and moving the caret up into the history would leave the live shell writing its
    // prompt over the text that was just replayed.
    out.extend_from_slice(b"\r\n");
    out
}

/// The bytes that would draw `screen`, dropping rows from the top until they fit in `limit`.
///
/// **Rows rather than bytes, and that is a correctness rule rather than tidiness.** The stream is mostly escape
/// sequences, so a byte cut lands inside one sooner or later — and half of `\x1b[0;38;2;232;235;241m` is not a
/// shorter sequence, it is text the emulator prints. Dropping whole rows keeps every stream a valid stream.
///
/// **From the top**, because what somebody wants back is the end: the last command and its output. A screen that
/// will not fit at all answers with its last row, which is the prompt.
pub fn bytes_within(screen: &Screen, limit: usize) -> Vec<u8> {
    let whole = bytes_of(screen);
    if whole.len() <= limit {
        return whole;
    }
    // Walk the first row off at a time. A screen is at most `SCROLLBACK` rows and this happens once, when a
    // window closes, so a loop is the honest shape — a binary search over row counts would be the same answer
    // reached less legibly.
    for first in 1..screen.rows {
        let bytes = bytes_of(&without_the_first_rows(screen, first));
        if bytes.len() <= limit {
            return bytes;
        }
    }
    // Even one row is too long, which takes a row of several thousand wide characters. The last row alone, cut
    // to the limit on a **character** boundary so what is written is at least valid text.
    let last = bytes_of(&without_the_first_rows(screen, screen.rows.saturating_sub(1)));
    match last.len() <= limit {
        true => last,
        false => Vec::new(),
    }
}

/// A copy of `screen` with its first `count` rows dropped and blank rows added at the bottom.
///
/// The rows keep their order and their styles; what changes is which of them there are. The cursor comes with
/// them where it can, and is dropped when it was in a row that has gone — `bytes_of` ends with a newline rather
/// than a cursor move, so nothing depends on it.
fn without_the_first_rows(screen: &Screen, count: usize) -> Screen {
    let count = count.min(screen.rows);
    // **The background for both**, so a blank cell is one `is_plain_blank` recognises and `bytes_of` trims. The
    // foreground of a blank is never drawn — there is nothing in it — and taking the first cell's colour would
    // make the default depend on which row happened to be first, which is arbitrary even where it does not show.
    let mut out = Screen::empty(screen.rows, screen.columns, screen.background, screen.background);
    out.title = screen.title.clone();
    for row in count..screen.rows {
        for column in 0..screen.columns {
            if let (Some(from), Some(to)) = (
                screen.cell(row, column).cloned(),
                out.cells.get_mut((row - count) * screen.columns + column),
            ) {
                *to = from;
            }
        }
    }
    out.cursor = screen.cursor.as_ref().and_then(|cursor| {
        (cursor.row >= count).then(|| crate::screen::Cursor { row: cursor.row - count, ..*cursor })
    });
    out
}

/// The last row that has anything in it, so trailing blank rows are not written down.
///
/// Answers zero for a screen with nothing on it at all, which writes one empty row — a terminal has to have a
/// cursor somewhere, and a stream that wrote nothing would leave a restored session at the top left, which is
/// where an empty terminal's cursor is anyway.
fn last_row_with_anything(screen: &Screen) -> usize {
    (0..screen.rows)
        .rev()
        .find(|row| {
            (0..screen.columns).any(|column| {
                screen.cell(*row, column).is_some_and(|cell| !is_plain_blank(cell, screen))
            })
        })
        .unwrap_or(0)
}

/// Whether a cell is an ordinary empty one — nothing in it and nothing done to it.
///
/// A space with a **coloured background** is not blank: that is how a program draws a bar or a selection, and
/// dropping it would lose the thing somebody is looking at.
fn is_plain_blank(cell: &ScreenCell, screen: &Screen) -> bool {
    (cell.character == ' ' || cell.character == '\0')
        && cell.marks.is_empty()
        && cell.background == screen.background
        && !cell.underline
        && !cell.strikethrough
}

/// One row, as runs of cells that share a style.
fn write_a_row(screen: &Screen, row: usize, out: &mut Vec<u8>) {
    // Trailing blanks within a row are dropped for the same reason trailing rows are, and measured the same
    // way — a coloured space is not a blank.
    let last = (0..screen.columns)
        .rev()
        .find(|column| screen.cell(row, *column).is_some_and(|cell| !is_plain_blank(cell, screen)))
        .map(|column| column as i64)
        .unwrap_or(-1);
    let mut style: Option<Style> = None;
    for column in 0..=last.max(-1) {
        if column < 0 {
            break;
        }
        let Some(cell) = screen.cell(row, column as usize) else { break };
        // **The second half of a wide character is skipped**, because the character before it already took two
        // columns and writing anything here would push the row along by one.
        if cell.spacer {
            continue;
        }
        let wanted = Style::of(cell, screen);
        if style.as_ref() != Some(&wanted) {
            out.extend_from_slice(wanted.sequence().as_bytes());
            style = Some(wanted);
        }
        let character = match cell.character {
            '\0' => ' ',
            other => other,
        };
        let mut buffer = [0_u8; 4];
        out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
        for mark in &cell.marks {
            out.extend_from_slice(mark.encode_utf8(&mut buffer).as_bytes());
        }
    }
}

/// Everything about a cell that an `SGR` sequence carries.
///
/// A value of its own so that a run of cells sharing a style is written as one sequence rather than one per
/// cell, which is what keeps a screenful of ordinary text close to a screenful of bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Style {
    foreground: Rgb,
    background: Rgb,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    hidden: bool,
}

impl Style {
    fn of(cell: &ScreenCell, _screen: &Screen) -> Self {
        Self {
            foreground: cell.foreground,
            background: cell.background,
            bold: cell.bold,
            italic: cell.italic,
            underline: cell.underline,
            strikethrough: cell.strikethrough,
            hidden: cell.hidden,
        }
    }

    /// The `SGR` sequence that sets this style, from any other.
    ///
    /// **Reset first and then set everything**, rather than working out the difference from the style before
    /// it. A difference is smaller and is a second place for the two ends to disagree about what is currently
    /// set; a reset is one rule and the cost is a few bytes a run.
    ///
    /// The colours are written in the 24-bit form, because a `Screen` holds resolved red, green and blue — the
    /// palette has already been applied by the time anything here sees a cell, so there is no index left to
    /// write and no way to invent one that would mean the same thing under a different theme.
    fn sequence(&self) -> String {
        let mut parts = vec!["0".to_owned()];
        if self.bold {
            parts.push("1".to_owned());
        }
        if self.italic {
            parts.push("3".to_owned());
        }
        if self.underline {
            parts.push("4".to_owned());
        }
        if self.hidden {
            parts.push("8".to_owned());
        }
        if self.strikethrough {
            parts.push("9".to_owned());
        }
        let foreground = self.foreground;
        parts.push(format!("38;2;{};{};{}", foreground.r, foreground.g, foreground.b));
        let background = self.background;
        parts.push(format!("48;2;{};{};{}", background.r, background.g, background.b));
        format!("\x1b[{}m", parts.join(";"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Session, Size};

    fn fed(bytes: &[u8]) -> Session {
        let mut session = Session::detached(Size::new(6, 20));
        session.feed(bytes);
        session
    }

    /// What a terminal is told to write down holds its scrollback, not only what is on its screen.
    ///
    /// **`task-1912`'s first fault against the restore.** `screen_to_replay` answered with `snapshot`, which is
    /// the *visible grid*, so a node fourteen rows tall wrote fourteen rows down — measured at 457 bytes for
    /// five commands, four of which were already above the fold. Forty rows into a six row terminal here, and
    /// the first of them has to be in the stream.
    #[test]
    fn what_is_written_down_holds_the_scrollback_and_not_only_the_screen() {
        let mut session = Session::detached(Size::new(6, 20));
        for line in 0..40 {
            session.feed(format!("line{line}\r\n").as_bytes());
        }
        let only_the_screen = bytes_of(&session.snapshot());
        assert!(
            !String::from_utf8_lossy(&only_the_screen).contains("line0\r"),
            "the screen alone cannot hold the first line, which is what made this a fault"
        );
        let with_the_history = bytes_of(&session.screen_and_history(REPLAY_ROWS));
        let text = String::from_utf8_lossy(&with_the_history);
        assert!(text.contains("line0"), "the first line is in the stream");
        assert!(text.contains("line39"), "and so is the last");
    }

    /// The report's own arithmetic: five commands and their output, through two sessions.
    ///
    /// *"if I've ran 5 commands, all 5 and their results should be shown when I reopen."* A terminal this
    /// shape shows two of them, so every one of the five is a test of the scrollback rather than of the screen.
    #[test]
    fn five_commands_and_their_output_all_come_back() {
        let mut session = Session::detached(Size::new(6, 40));
        for command in ["one", "two", "three", "four", "five"] {
            session.feed(format!("$ echo {command}\r\n{command}\r\n").as_bytes());
        }
        let written = bytes_within(&session.screen_and_history(REPLAY_ROWS), REPLAY_LIMIT);

        // A fresh terminal, given what was written down, with room to be read back in.
        let mut back = Session::detached(Size::new(40, 40));
        back.feed(&written);
        let text = back.written_text(None);
        for command in ["one", "two", "three", "four", "five"] {
            assert!(text.contains(&format!("$ echo {command}")), "{command} came back: {text:?}");
        }
    }

    /// The colours of a row that has scrolled off are the colours it had.
    ///
    /// The scrollback is read out of the grid directly rather than through `renderable_content`, which walks
    /// only the visible rows, so this is what says the second reading resolves a cell the way the first one
    /// does.
    #[test]
    fn a_row_in_the_scrollback_keeps_its_colours() {
        let mut session = Session::detached(Size::new(4, 20));
        session.feed(b"\x1b[31mred line\x1b[0m\r\n");
        for line in 0..8 {
            session.feed(format!("plain{line}\r\n").as_bytes());
        }
        let screen = session.screen_and_history(REPLAY_ROWS);
        let coloured = (0..screen.rows)
            .find(|row| {
                (0..screen.columns)
                    .filter_map(|column| screen.cell(*row, column))
                    .map(|cell| cell.character)
                    .collect::<String>()
                    .starts_with("red")
            })
            .expect("the red line is in the scrollback");
        let cell = screen.cell(coloured, 0).expect("its first cell");
        let plain = Session::detached(Size::new(4, 20)).snapshot();
        let ordinary = plain.cell(0, 0).expect("an ordinary cell").foreground;
        assert_ne!(
            cell.foreground, ordinary,
            "a scrolled-off row kept the colour it was written in"
        );
    }

    /// A screen written down and replayed is the same screen.
    ///
    /// **Compared cell for cell**, which is what makes this a test of the fidelity rather than of the text: a
    /// comparison of what the two screens *say* would pass on `written_text`, which is the thing this exists to
    /// improve on.
    #[test]
    fn a_screen_written_down_and_replayed_is_the_same_screen() {
        let was = fed(b"total 48\r\nsrc  tests  Cargo.toml").snapshot();
        let now = fed(&bytes_of(&was)).snapshot();
        for row in 0..was.rows {
            for column in 0..was.columns {
                assert_eq!(
                    now.cell(row, column).map(|cell| cell.character),
                    was.cell(row, column).map(|cell| cell.character),
                    "row {row} column {column}"
                );
            }
        }
    }

    /// And the colours come with it, which plain text loses, with the cursor left below what was restored.
    #[test]
    fn a_replay_keeps_the_colours_and_leaves_the_cursor_below_them() {
        // Red on default, then bold green, then plain — and the cursor left in the middle of a line.
        let was = fed(b"\x1b[31mred\x1b[0m \x1b[1;32mgreen\x1b[0m plain").snapshot();
        let now = fed(&bytes_of(&was)).snapshot();

        let red = was.cell(0, 0).expect("a cell");
        assert_eq!(now.cell(0, 0).expect("a cell").foreground, red.foreground, "the red survived");
        let green = was.cell(0, 4).expect("a cell");
        assert_eq!(now.cell(0, 4).expect("a cell").foreground, green.foreground, "and the green");
        assert!(green.bold, "the fixture really is bold");
        assert!(now.cell(0, 4).expect("a cell").bold, "and the bold survived");

        // **The cursor is left below the restored text rather than put back into it.** A replayed screen is
        // history, and the live shell prints its prompt next — a caret up inside the history would have that
        // prompt written over the text that was just replayed. See `bytes_of`.
        let cursor = now.cursor.as_ref().expect("a cursor");
        let was_at = was.cursor.as_ref().expect("a cursor");
        assert!(
            cursor.row > was_at.row,
            "the cursor is on the line after the restored text: {} then {}",
            was_at.row,
            cursor.row
        );
        assert_eq!(cursor.column, 0, "and at the start of it");
    }

    /// A coloured space is not a blank, so a bar a program drew is not thrown away.
    #[test]
    fn a_space_with_a_background_is_kept_rather_than_trimmed() {
        // A line of spaces on a blue background, which is how a status bar is drawn.
        let was = fed(b"\x1b[44m          \x1b[0m").snapshot();
        let now = fed(&bytes_of(&was)).snapshot();
        assert_eq!(
            now.cell(0, 5).expect("a cell").background,
            was.cell(0, 5).expect("a cell").background,
            "the bar is still there"
        );
    }

    /// A program drawing its own screen has nothing to write down, which is `task-1908` §1.2's rule.
    ///
    /// The alternate buffer is cleared when it is entered and abandoned when it is left, so there is nothing
    /// there to save. Answering with the shell's buffer behind it would be worse than answering with nothing: a
    /// screen belonging to a program that is no longer running comes back looking alive.
    #[test]
    fn only_the_normal_buffer_is_written_down() {
        // `1049` is what a full-screen program sends, and it is what `claude` sends.
        let mut session = fed(b"$ claude
");
        assert!(session.screen_to_replay().is_some(), "a shell at a prompt has a screen to save");
        session.feed(b"\x1b[?1049h");
        assert!(session.on_alternate_screen(), "the fixture really is on the alternate screen");
        assert_eq!(session.screen_to_replay(), None, "and there is nothing to save while it is");
        // Back to the normal buffer, and there is again.
        session.feed(b"\x1b[?1049l");
        assert!(session.screen_to_replay().is_some());
    }

    /// A row filled to its last column does not push the screen down by one.
    ///
    /// **The case a terminal's auto-wrap makes dangerous.** Writing the last column of a row leaves most
    /// emulators with a pending wrap, and the `\r\n` this writes between rows would then act twice — once for the
    /// wrap and once for the newline — so every row after a full one would be a row lower than it was. A screen
    /// of eighty-column output would come back double spaced.
    #[test]
    fn a_row_filled_to_its_last_column_does_not_push_the_screen_down() {
        // Twenty columns, so twenty characters exactly fills row zero.
        let filled = "x".repeat(20);
        let was = fed(format!("{filled}\r\nsecond row").as_bytes()).snapshot();
        assert_eq!(
            was.cell(1, 0).expect("a cell").character,
            's',
            "the fixture really has two rows"
        );

        let now = fed(&bytes_of(&was)).snapshot();
        assert_eq!(now.cell(0, 19).expect("a cell").character, 'x', "the full row came back whole");
        assert_eq!(
            now.cell(1, 0).expect("a cell").character,
            's',
            "and the row after it is still the row after it"
        );
    }

    /// A wide character and the blank cell behind it come back as one character, not two.
    #[test]
    fn a_wide_character_comes_back_as_one_character() {
        // A CJK character takes two columns: the cell holds it and the next is its spacer.
        let was = fed("a\u{4f60}b".as_bytes()).snapshot();
        assert!(was.cell(0, 2).expect("a cell").spacer, "the fixture really has a spacer");
        let now = fed(&bytes_of(&was)).snapshot();
        assert_eq!(now.cell(0, 0).expect("a cell").character, 'a');
        assert_eq!(now.cell(0, 1).expect("a cell").character, '\u{4f60}', "the wide one");
        assert!(now.cell(0, 2).expect("a cell").spacer, "still with its spacer");
        assert_eq!(
            now.cell(0, 3).expect("a cell").character,
            'b',
            "and what followed it did not shift"
        );
    }

    /// A bounded stream is still a valid stream, which a byte cut would not be.
    ///
    /// **The bug this is the test for was in the first version of `screen_to_replay`**, which took the last
    /// `REPLAY_LIMIT` bytes. The stream is mostly `\x1b[0;38;2;232;235;241;48;2;26;31;38m`, so a byte cut lands
    /// inside a sequence and the tail — `35;241m` — is *printed* rather than obeyed. Bounding by rows is what
    /// keeps every stream valid, and this asserts the property rather than the mechanism: whatever comes back,
    /// replaying it produces the rows it says and no stray text.
    #[test]
    fn a_bounded_stream_is_still_a_valid_stream() {
        // Six rows of coloured text, then a limit far too small for all of it.
        let mut session = Session::detached(Size::new(6, 20));
        for row in 0..6 {
            session.feed(format!("\x1b[3{}mrow {row} coloured\r\n", row + 1).as_bytes());
        }
        let screen = session.snapshot();
        let whole = bytes_of(&screen);
        assert!(
            whole.len() > 200,
            "the fixture is big enough to be worth bounding: {}",
            whole.len()
        );

        // A limit that no more than about half of it fits in, so rows really are dropped.
        let limit = whole.len() / 2;
        let bounded = bytes_within(&screen, limit);
        assert!(bounded.len() <= limit, "it really is bounded: {} of {limit}", bounded.len());

        // **The property**: nothing that comes back is a fragment of an escape sequence. A fragment would be
        // printed, so the replayed screen would hold digits and semicolons that were never in the original.
        let now = fed(&bounded).snapshot();
        let text = now.text();
        assert!(!text.contains(";2;"), "no half-written sequence was printed: {text:?}");
        assert!(!text.contains("38;"), "nor any other part of one: {text:?}");
        // And the end is what was kept, because the end is what somebody wants back.
        assert!(text.contains("row 5"), "the last row survived: {text:?}");
    }

    /// Dropping rows does not recolour the rows that are left.
    ///
    /// `without_the_first_rows` builds its replacement with `Screen::empty`, which needs a foreground for the
    /// blank cells — and the first cell's colour is an arbitrary thing to use for that. What has to be true is
    /// that the cells actually *copied* keep their own colours, and that the blanks below them are blanks: the
    /// default only shows through where nothing was copied, and `bytes_of` trims those rows anyway.
    #[test]
    fn dropping_rows_does_not_recolour_what_is_left() {
        let mut session = Session::detached(Size::new(6, 20));
        // A first row in an unusual colour, then rows in another, so a default taken from the first cell would
        // be visible if it leaked.
        session
            .feed(b"\x1b[35mmagenta first row\r\n\x1b[32mgreen second\r\n\x1b[32mgreen third\r\n");
        let screen = session.snapshot();
        let magenta = screen.cell(0, 0).expect("a cell").foreground;
        let green = screen.cell(1, 0).expect("a cell").foreground;
        assert_ne!(magenta, green, "the fixture really has two colours");

        // Drop the magenta row and the greens must still be green.
        let bounded = bytes_within(&screen, bytes_of(&screen).len() - 20);
        let now = fed(&bounded).snapshot();
        assert!(now.text().contains("green second"), "the rows that were kept came back");
        assert!(!now.text().contains("magenta"), "and the one that was dropped did not");
        assert_eq!(
            now.cell(0, 0).expect("a cell").foreground,
            green,
            "a kept row keeps its own colour rather than the dropped row's"
        );
    }

    /// A bound is never exceeded, at any limit, including limits nothing can fit in.
    ///
    /// Every path out of `bytes_within` compares against `limit` before returning, and the last one answers with
    /// nothing rather than with a row that does not fit — a stream over the bound would be a file over the bound,
    /// which is the thing `REPLAY_LIMIT` exists to prevent.
    #[test]
    fn a_bound_is_never_exceeded_whatever_it_is() {
        let mut session = Session::detached(Size::new(6, 20));
        for row in 0..6 {
            session.feed(format!("\x1b[3{}mrow {row} of coloured text\r\n", row + 1).as_bytes());
        }
        let screen = session.snapshot();
        // Every limit from nothing at all to more than the whole stream.
        let whole = bytes_of(&screen).len();
        for limit in [0, 1, 10, 50, 120, 200, whole / 2, whole - 1, whole, whole + 100] {
            let bounded = bytes_within(&screen, limit);
            assert!(
                bounded.len() <= limit,
                "a limit of {limit} answered with {} bytes",
                bounded.len()
            );
        }
        // And a limit that fits everything really does answer with everything.
        assert_eq!(bytes_within(&screen, whole).len(), whole);
    }

    /// Trailing blank rows are not written down, so an almost-empty screen is a small stream.
    #[test]
    fn an_almost_empty_screen_is_a_small_stream() {
        let one_line = fed(b"$ ").snapshot();
        let bytes = bytes_of(&one_line);
        assert!(bytes.len() < 200, "one line of prompt is a short stream: {} bytes", bytes.len());
        // And it still comes back.
        let now = fed(&bytes).snapshot();
        assert_eq!(now.cell(0, 0).expect("a cell").character, '$');
    }
}
