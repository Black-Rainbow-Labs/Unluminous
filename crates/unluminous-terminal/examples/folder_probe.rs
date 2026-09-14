//! Where a real shell says it is, against where the operating system says its process is.
//!
//! `cargo run -p unluminous-terminal --example folder_probe [shell] [args...]`
//!
//! `task-1945` reads the folder off the shell process itself, which is right for `cmd.exe`, `bash` and `zsh`
//! and is no answer at all for PowerShell: `Set-Location` moves PowerShell's own location and never the
//! process's current directory. `task-1950` reads the sequence a shell reports its folder with instead. The
//! two answers are the two columns this prints, and the whole of the ticket is which of them moved.
//!
//! It types a command into a real pseudoterminal and reads both answers before and after, so what it reports
//! is what the window would record if it closed at that moment.
//!
//! | environment | what it changes |
//! |---|---|
//! | nothing | `PROBE_INTO` defaults to this machine's temporary folder |
//! | `PROBE_INTO=<folder>` | where the command moves to |
//! | `PROBE_COMMAND=<text>` | the whole command, for a shell whose `cd` is spelt some other way |
//!
//! To measure the shell integration as well, name the script on the command line the way Unluminous starts it:
//!
//! ```text
//! cargo run -p unluminous-terminal --example folder_probe -- pwsh.exe -NoExit -File <the script>
//! ```

use std::time::{Duration, Instant};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let shell = arguments.next();
    let args: Vec<String> = arguments.collect();
    let into = std::env::var("PROBE_INTO").unwrap_or_else(|_| {
        std::env::temp_dir().to_string_lossy().trim_end_matches(['\\', '/']).to_owned()
    });
    let command = std::env::var("PROBE_COMMAND").unwrap_or_else(|_| format!("cd \"{into}\""));
    let settings = unluminous_terminal::SessionSettings {
        shell: shell.clone(),
        args: args.clone(),
        working_directory: Some(std::env::current_dir().expect("a folder to start in")),
        ..Default::default()
    };
    let size = unluminous_terminal::Size { rows: 24, columns: 100, cell_width: 8, cell_height: 16 };
    let waker: unluminous_terminal::Waker = std::sync::Arc::new(|| {});
    let mut session = match unluminous_terminal::Session::spawn(&settings, size, waker) {
        Ok(session) => session,
        Err(problem) => {
            println!("no shell: {problem}");
            return;
        }
    };
    println!("--- {} {}", shell.as_deref().unwrap_or("<default>"), args.join(" "));
    println!("started in : {}", std::env::current_dir().unwrap_or_default().display());
    println!("moving to  : {into}");

    // Long enough for a shell to have printed its first prompt, which is when a reporting one first reports.
    settle(&mut session, Duration::from_millis(2500));
    report(&session, "at the first prompt");

    session.send(format!("{command}\r").into_bytes());
    settle(&mut session, Duration::from_millis(2500));
    report(&session, "after the command");

    println!();
    println!("--- the screen ---");
    println!("{}", session.snapshot().text());
    session.kill();
}

/// Pump the session for `how_long`, which is how a window would read it.
fn settle(session: &mut unluminous_terminal::Session, how_long: Duration) {
    let until = Instant::now() + how_long;
    while Instant::now() < until {
        session.pump();
        std::thread::sleep(Duration::from_millis(20));
    }
    session.pump();
}

fn report(session: &unluminous_terminal::Session, when: &str) {
    let show = |folder: Option<std::path::PathBuf>| match folder {
        Some(path) => path.display().to_string(),
        None => "<nothing>".to_owned(),
    };
    println!();
    println!("{when}");
    println!("  the shell reported : {}", show(session.reported_folder()));
    println!("  the process is in  : {}", show(session.process_folder()));
    println!("  what is written    : {}", show(session.folder()));
}
