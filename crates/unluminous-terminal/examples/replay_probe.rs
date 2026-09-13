//! What a terminal node really comes back showing, measured rather than assumed.
//!
//! `cargo run -p unluminous-terminal --example replay_probe [shell] [args...]`
//!
//! `task-1912` reports that a terminal node comes back empty though a screen was written down for it and read
//! back. Driving the real window showed the replay being *accepted* and the screen empty three frames later,
//! so the first question was what wipes it: the shell's own startup, or the pseudoconsole repainting the
//! buffer it believes it owns. The answer is the pseudoconsole, and this is how that was measured and how it
//! is measured again on a machine that disagrees.
//!
//! It reads the **screen** rather than `written_text`, which is the distinction the first version of this got
//! wrong: `written_text` reads the scrollback as well, so a screen that has been cleared *into* history still
//! answers with the marker and looks fine.
//!
//! | environment | what it measures |
//! |---|---|
//! | nothing | a screen drawn in before the program has written: it is erased |
//! | `PROBE_LATE=<ms>` | one drawn after the console host's own repaint: it stays, until something happens |
//! | `PROBE_COMMAND=1` | and what one keystroke then does to it |
//! | `PROBE_RESIZE=1` | and what a resize does to it |
//! | `PROBE_SHIM=<unluminous-cli>` | the real mechanism: the screen printed *inside* the console, which is what ships |

use std::time::{Duration, Instant};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let shell = arguments.next();
    let args: Vec<String> = arguments.collect();
    let mut settings = unluminous_terminal::SessionSettings {
        shell: shell.clone(),
        args: args.clone(),
        ..Default::default()
    };
    // **The shim, in a real pseudoconsole.** `PROBE_SHIM=<program>` writes a screen down and starts the
    // session the way a restored node really starts — so what this prints is what a node comes back showing,
    // measured rather than photographed. The command line is written out here rather than borrowed, because
    // `unluminous-cli` owns that shape and this crate is below it.
    if let Ok(shim) = std::env::var("PROBE_SHIM") {
        let file = std::env::temp_dir().join("unluminous-probe-screen.bytes");
        std::fs::write(&file, b"\x1b[0mRESTORED-LINE-ONE\r\nRESTORED-LINE-TWO\r\n")
            .expect("write a screen down");
        let real =
            settings.shell.clone().unwrap_or_else(unluminous_terminal::session::default_shell);
        settings.name = Some(real.clone());
        settings.args =
            vec!["--replay-screen".to_owned(), file.display().to_string(), "--".to_owned(), real];
        settings.args.extend(args.clone());
        settings.shell = Some(shim);
    }
    let size = unluminous_terminal::Size { rows: 14, columns: 46, cell_width: 8, cell_height: 16 };
    let waker: unluminous_terminal::Waker = std::sync::Arc::new(|| {});
    let started = Instant::now();
    let mut session = match unluminous_terminal::Session::spawn(&settings, size, waker) {
        Ok(session) => session,
        Err(problem) => {
            println!("no shell: {problem}");
            return;
        }
    };
    println!("--- {} {}", shell.as_deref().unwrap_or("<default>"), args.join(" "));

    let marker = b"\x1b[0mRESTORED-LINE-ONE\r\nRESTORED-LINE-TWO\r\n".to_vec();
    // **A late draw**, which is the question the early one raises: if what wipes the screen is the
    // pseudoconsole's one repaint at startup, a draw made after it has happened would survive, and the fix
    // would be to wait rather than to hurry.
    let late: u64 =
        std::env::var("PROBE_LATE").ok().and_then(|value| value.parse().ok()).unwrap_or(0);
    for _ in 0..late / 50 {
        std::thread::sleep(Duration::from_millis(50));
        session.pump();
    }

    if std::env::var_os("PROBE_SHIM").is_some() {
        // Nothing is put back by hand: the whole point is that the program inside the console prints it.
        watch(&mut session, true);
        session.kill();
        return;
    }
    session.draw_over_the_terminal(&marker);
    println!("drew over the terminal ({:?})", started.elapsed());
    watch(&mut session, false);
    session.kill();
}

/// Watch the screen for four seconds, doing the two things that make a console host redraw.
fn watch(session: &mut unluminous_terminal::Session, print_each: bool) {
    let mut lost_at = None;
    for step in 1..=16 {
        std::thread::sleep(Duration::from_millis(250));
        session.pump();
        if step == 6 && std::env::var_os("PROBE_COMMAND").is_some() {
            session.send(b"echo AFTERWARDS\x0d".to_vec());
            println!("  sent a command");
        }
        if step == 6 && std::env::var_os("PROBE_RESIZE").is_some() {
            session.resize(unluminous_terminal::Size {
                rows: 16,
                columns: 60,
                cell_width: 8,
                cell_height: 16,
            });
            println!("  resized");
        }
        let on_screen = screen_text(session).contains("RESTORED-LINE-ONE");
        if !on_screen && lost_at.is_none() {
            let in_history = session.written_text(Some(60)).contains("RESTORED-LINE-ONE");
            lost_at = Some(step * 250);
            println!("  gone from the screen after {} ms (still in history: {in_history})", step * 250);
        }
        if print_each && step % 8 == 0 {
            println!("  {} ms screen:", step * 250);
            for line in screen_text(session).lines().filter(|line| !line.trim().is_empty()) {
                println!("  | {line}");
            }
        }
    }
    if lost_at.is_none() {
        println!("  the restored screen was still there after four seconds");
    }
    println!("  screen at the end:");
    for line in screen_text(session).lines().filter(|line| !line.trim().is_empty()) {
        println!("  | {line}");
    }
}

fn screen_text(session: &unluminous_terminal::Session) -> String {
    let screen = session.snapshot();
    (0..screen.rows)
        .map(|row| {
            let line: String = (0..screen.columns)
                .filter_map(|column| screen.cell(row, column).map(|cell| cell.character))
                .collect();
            format!("{}\n", line.trim_end())
        })
        .collect()
}
