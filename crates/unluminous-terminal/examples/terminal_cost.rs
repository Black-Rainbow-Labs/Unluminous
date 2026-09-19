//! What the terminal costs on the two paths a person waits on.
//!
//! `cargo run --release -p unluminous-terminal --example terminal_cost`
//!
//! Two questions, both raised by `task-1984` and both measured here rather than reasoned about:
//!
//! - **`Session::snapshot`**, which the painter calls once a frame for every terminal that is
//!   showing. It builds a whole `Screen` — one `ScreenCell` per cell of the grid — and `ScreenCell`
//!   holds a `Vec<char>` for combining marks, so the grid is cloned cell by cell rather than copied.
//!   P9 asks whether that is worth changing, and the answer is a number.
//!
//! ## What P9 measured, and why nothing was changed
//!
//! Measured on this machine, a release build:
//!
//! | grid | `snapshot` | of which the empty grid |
//! |---|---:|---:|
//! | 24 by 80, which is a default terminal | 0.024 ms | 0.005 ms |
//! | 60 by 200, which is a large one | 0.162 ms | 0.111 ms |
//! | 120 by 400, which is a whole large screen | 1.127 ms | 0.652 ms |
//!
//! So the review is right about the mechanism: **about two thirds of a snapshot is the blank grid**,
//! which is `vec![ScreenCell::blank(..); rows * columns]` — one `clone` per cell rather than one
//! `memcpy`, because a type holding a `Vec` is not `Copy`. And nearly every one of those cells is
//! then written over by the loop below it.
//!
//! It was left alone, and that is a decision rather than an omission. At the size a person really
//! uses, 0.162 ms a frame is **under the 0.25 ms bar `task-1813` set** for a phase being worth
//! optimising. And the only way to make `ScreenCell` `Copy` is to hold its combining marks in a fixed
//! array, which means a cap — and a cap is a change to what the terminal *shows*, for text with more
//! marks on one character than the cap allows. `task-1984` says in as many words not to change a
//! product default, and a display cap is close enough to one that the trade is not worth 0.1 ms.
//!
//! What would make it free without a cap is reusing the previous `Screen`'s allocation rather than
//! returning a fresh one — `snapshot_into(&mut Screen)` — which is a change to every caller and is
//! the thing to do if a grid ever reaches the size where the third row of that table matters.
//! - **`replay::bytes_within`**, which runs when a window closes, once per terminal, and is the one
//!   path here that a frame cannot hide. P6 found it re-encoding the whole screen once per row it
//!   dropped.

use std::time::Instant;

use unluminous_terminal::replay;
use unluminous_terminal::{Screen, Session, Size};

/// Run `body` `runs` times and give back the mean in milliseconds.
fn timed(runs: usize, mut body: impl FnMut()) -> f64 {
    let began = Instant::now();
    for _ in 0..runs {
        body();
    }
    began.elapsed().as_secs_f64() * 1000.0 / runs as f64
}

/// A session with a screenful of coloured text on it, which is what a real one has.
fn filled(rows: usize, columns: usize) -> Session {
    let mut session = Session::detached(Size::new(rows, columns));
    for row in 0..rows {
        let colour = 31 + (row % 7);
        let text = format!(
            "\x1b[{colour}m{:width$}\r\n",
            format!("row {row} of the terminal, with a command and its output on it"),
            width = columns.saturating_sub(1)
        );
        session.feed(text.as_bytes());
    }
    session
}

fn main() {
    for (rows, columns) in [(24, 80), (60, 200), (120, 400)] {
        let session = filled(rows, columns);
        let cells = rows * columns;
        let ms = timed(200, || {
            std::hint::black_box(session.snapshot());
        });
        println!(
            "snapshot {rows}x{columns} ({cells} cells): {ms:8.3} ms  ({:.0} a second)",
            1000.0 / ms
        );

        // The grid on its own, which is `vec![ScreenCell::blank(..); rows * columns]` -- one `clone`
        // per cell rather than one `memcpy`, because `ScreenCell` holds a `Vec<char>` for combining
        // marks and is therefore not `Copy`. This is the share of the reading above that making the
        // cell `Copy` could take away, and it is what `task-1984` P9 asks about.
        let palette = unluminous_terminal::palette::Palette::default();
        let ms = timed(200, || {
            std::hint::black_box(Screen::empty(
                rows,
                columns,
                palette.foreground,
                palette.background,
            ));
        });
        println!("  the empty grid alone:                  {ms:8.3} ms");

        let screen = session.snapshot();
        let whole = replay::bytes_of(&screen).len();
        for share in [1, 2, 4] {
            let limit = whole / share;
            let ms = timed(50, || {
                std::hint::black_box(replay::bytes_within(&screen, limit));
            });
            println!("  bytes_within at 1/{share} of {whole} bytes: {ms:8.3} ms");
        }
    }
}
