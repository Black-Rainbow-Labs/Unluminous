//! What a real shell reports as its foreground program, measured rather than assumed.
//!
//! `cargo run -p unluminous-terminal --example foreground_check`
//!
//! `Session::foreground` is what makes a terminal node on the Base of Infinite Space come back running what it
//! was running — `task-1907` reports that a node with `claude` typed into its shell came back as a bare shell,
//! because the node records the command it was *given* and nothing asked the terminal itself. The mechanism is
//! `tcgetpgrp` on the pseudoterminal master plus the platform's name for a process id, and it cannot be a unit
//! test: a detached session has no pseudoterminal to ask, and when a real shell has reached its prompt is not
//! something a test can know.
//!
//! So this is the fourth layer, as `terminal_capture` is. What it printed when it was written:
//!
//! ```text
//! at the prompt: Some("bash")
//! while sleeping: Some("sleep")
//! after interrupt: Some("bash")
//! ```
//!
//! Those three lines are the whole of what the feature needs to be true: a node at a prompt is a shell, a node
//! running a program is that program, and a program that ends gives the terminal back.

fn main() {
    let settings = unluminous_terminal::SessionSettings {
        shell: Some("bash".to_owned()),
        // `--norc` so the answer does not depend on whatever this machine's profile starts, and `-i` because a
        // shell with no terminal control has no foreground group to report.
        args: vec!["--norc".to_owned(), "-i".to_owned()],
        ..Default::default()
    };
    let size = unluminous_terminal::Size::new(24, 80);
    let waker: unluminous_terminal::Waker = std::sync::Arc::new(|| {});
    let mut session =
        unluminous_terminal::Session::spawn(&settings, size, waker).expect("a shell started");

    // Waits rather than polls, because what is being measured is what the answer *is* once things have
    // settled, not how quickly it settles. `pump` first, because whether the program has ended is only known
    // once the events it sent have been read.
    settle(&mut session, 900);
    println!("at the prompt: {:?}", session.foreground());

    session.send(b"sleep 20\n".to_vec());
    settle(&mut session, 1200);
    println!("while sleeping: {:?}", session.foreground());

    // The interrupt byte, which is what `Ctrl+C` sends.
    session.send(vec![0x03]);
    settle(&mut session, 600);
    println!("after interrupt: {:?}", session.foreground());
}

fn settle(session: &mut unluminous_terminal::Session, milliseconds: u64) {
    std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    session.pump();
}
