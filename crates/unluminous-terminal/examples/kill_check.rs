//! Whether `Session::kill` really ends the program, measured rather than assumed.
//!
//! `cargo run -p unluminous-terminal --example kill_check`
//!
//! **Why this exists.** `task-1907` duplicates the pseudoterminal master in `Session::spawn` so that
//! `Session::foreground` can ask what is running, and the Codex Sol review of that change raised the obvious
//! worry: on Unix `Reaper::kill` does nothing, so if ending a program depended on the pseudoterminal closing,
//! a second descriptor held open would stop it — and a run the person had stopped would go on running.
//!
//! Measured, it does not: `alacritty_terminal`'s own `impl Drop for Pty` sends `SIGHUP` to the child **by
//! process id** rather than relying on the descriptor closing, so a duplicate makes no difference. What this
//! printed with the duplicate held and the session deliberately kept alive, which is the run tile's own case:
//!
//! ```text
//! before kill, matching processes: 46992
//! after kill, session still held: (none)
//! ```
//!
//! The same run with the duplicate removed prints the same thing, which is what makes it evidence about the
//! mechanism rather than about one run.
fn main() {
    let settings = unluminous_terminal::SessionSettings {
        shell: Some("bash".to_owned()),
        args: vec!["--norc".to_owned(), "-c".to_owned(), "sleep 300".to_owned()],
        ..Default::default()
    };
    let size = unluminous_terminal::Size::new(24, 80);
    let waker: unluminous_terminal::Waker = std::sync::Arc::new(|| {});
    let mut session = unluminous_terminal::Session::spawn(&settings, size, waker).expect("a shell");
    std::thread::sleep(std::time::Duration::from_millis(1200));
    session.pump();
    let before = std::process::Command::new("pgrep").args(["-f", "sleep 300"]).output().expect("pgrep");
    println!("before kill, matching processes: {}", String::from_utf8_lossy(&before.stdout).trim());
    session.kill();
    // **The session is deliberately kept alive**, which is the case the run tile is: it goes on showing the
    // output of a program it stopped, so `kill` has to end the program without the session being dropped.
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let after = std::process::Command::new("pgrep").args(["-f", "sleep 300"]).output().expect("pgrep");
    let left = String::from_utf8_lossy(&after.stdout).trim().to_owned();
    println!("after kill, session still held: {}", if left.is_empty() { "(none)".into() } else { left });
    drop(session);
}
