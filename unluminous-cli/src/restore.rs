//! Printing a terminal's remembered screen *inside* its own console, and then becoming the shell.
//!
//! **Why a program rather than a write into the emulator.** `task-1908` put a remembered screen back by feeding
//! the bytes to the emulator before the shell had written anything, and `task-1912` measured what that does on
//! Windows: the console host clears the screen the first time the program writes and thereafter repaints the
//! cells it believes it owns, so the restored screen is pushed into the scrollback and erased from view. Put
//! back *later* it survives until the next keystroke and then comes back visibly corrupt, because the console
//! host redraws only the cells it knows about. `examples/replay_probe` is that measurement, and §2 of
//! `tasks/task-1912-a-session-and-a-terminal-tdd.md` is the table: `pwsh`, `cmd.exe` and `powershell.exe` all
//! clear it, two of them started with no banner and no profile, which is what says it is the console host
//! rather than any one shell. Microsoft's own `CreatePseudoConsole` documentation names the flag that would
//! avoid it — `PSEUDOCONSOLE_INHERIT_CURSOR`, *"attempt to inherit the cursor position of the parent
//! console"* — and `alacritty_terminal` passes `0`, which is *"a standard pseudoconsole creation"*.
//!
//! So the screen is restored the way every other piece of text in a terminal arrives: something inside the
//! console prints it. Then the console host holds it, its repaints keep it, a resize reflows it, and the rows
//! that scroll off reach Unluminous's own scrollback exactly as a real command's output does.
//!
//! **Both ends live here** — the command line [`command_line`] builds and the function [`run`] that carries it
//! out — because a protocol split across two crates is a protocol with two chances to disagree.
//!
//! **One mechanism on both platforms**, though only Windows needs it. Two would mean the tests on one platform
//! proved nothing about the other, which is the hole `task-1908` fell into: it was built and verified on
//! macOS, where a shell writes into a terminal that keeps what it was given.

use std::path::{Path, PathBuf};

/// The switch the shim is asked for by. Public because `unluminous-app`'s argument reader matches on it, and
/// a second spelling in a second crate is how the two ends stop agreeing.
pub const SWITCH: &str = "--replay-screen";

/// Everything after this is the program to become, and is not read as a switch.
pub const THEN: &str = "--";

/// A screen to print before a shell starts, and the program that prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restore {
    /// The file of bytes to print. Deleted once it has been printed — see [`run`].
    pub file: PathBuf,
    /// The program that prints the file and then becomes the shell, which is `unluminous-cli`.
    ///
    /// **The console program rather than the window's**, and the reason is measured. `alacritty_terminal`
    /// creates a pseudoconsole's child with `STARTF_USESTDHANDLES` and every handle left null — its own
    /// comment says that is so the child inherits nothing from the editor — and Windows fills a console
    /// subsystem program's standard handles in from the console it is attached to, while a *windows*
    /// subsystem program's stay null. So `unluminous.exe` as the shim printed nothing at all, and the shell it
    /// started inherited the same null handles and printed nothing either: a node that came back perfectly
    /// blank. Measured with `examples/replay_probe`, `PROBE_SHIM=…`.
    pub shim: PathBuf,
}

/// The command line that prints `file` and then becomes `program` with `args`.
///
/// Answers the program to start and the arguments to start it with, so that `Session::spawn` can hand them
/// straight to the pseudoterminal. Written as a function of its own so the shape can be asserted with no
/// process anywhere near it.
pub fn command_line(restore: &Restore, program: &str, args: &[String]) -> (PathBuf, Vec<String>) {
    let mut out = vec![
        SWITCH.to_owned(),
        restore.file.display().to_string(),
        THEN.to_owned(),
        program.to_owned(),
    ];
    out.extend(args.iter().cloned());
    (restore.shim.clone(), out)
}

/// Whether this restore can be carried out at all.
///
/// **A shim that is not there is not fatal.** A terminal that will not open is worse than a terminal that opens
/// with nothing restored, so a missing file or a missing program means the shell is started plainly.
pub fn is_worth_trying(restore: &Restore) -> bool {
    restore.file.is_file() && restore.shim.is_file()
}

/// Read the shim's own command line: the file to print, the program to become, and its arguments.
///
/// `None` when this is not a shim invocation at all, which is every ordinary start of Unluminous. A malformed
/// one — the switch with no file, or no program after the separator — also answers `None`, so a mistyped
/// command line opens a window rather than half-starting a shell.
pub fn asked_for(words: &[String]) -> Option<(PathBuf, String, Vec<String>)> {
    // **The first word, not anywhere on the line** (`task-1984` L12). Unluminous builds this command
    // line itself and always puts the switch first, so nothing that works stops working -- and a
    // project or a file whose name happens to be `--replay-screen` no longer turns an ordinary start
    // into a shim that prints a file and becomes a shell.
    let switch = match words.first() {
        Some(first) if first == SWITCH => 0,
        _ => return None,
    };
    let file = words.get(switch + 1)?;
    let then = words.iter().skip(switch + 2).position(|word| word == THEN)? + switch + 2;
    let program = words.get(then + 1)?;
    Some((PathBuf::from(file), program.clone(), words[then + 2..].to_vec()))
}

/// Print `file` to standard output and then become `program`.
///
/// **The file is taken away by whoever used it**, which is `task-1908`'s rule kept and moved to the place that
/// now knows: a canvas that failed to come back must not replay a week-old screen for ever. It is deleted after
/// it has been written and before the shell starts, so a shell that then fails to start costs the screen and
/// not a repeat of it.
///
/// **On Unix the shim replaces itself.** `exec` means the process that printed *is* the shell, so nothing is
/// added to the process tree, `foreground` reads what it always read, and the exit code is the shell's by
/// construction. On Windows there is no `exec`, so the shell is started as a child and waited for; §5 of the
/// design says what that costs and where it is answered.
///
/// Answers the exit code the process should end with.
pub fn run(file: &Path, program: &str, args: &[String]) -> i32 {
    use std::io::Write;
    if let Ok(bytes) = std::fs::read(file) {
        let mut out = std::io::stdout();
        let _ = out.write_all(&bytes);
        let _ = out.flush();
    }
    let _ = std::fs::remove_file(file);
    become_the_program(program, args)
}

#[cfg(unix)]
fn become_the_program(program: &str, args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    // `exec` only returns when it failed, and then there is nothing to be but an error.
    let problem = std::process::Command::new(program).args(args).exec();
    eprintln!("Unluminous could not start {program}: {problem}");
    127
}

#[cfg(not(unix))]
fn become_the_program(program: &str, args: &[String]) -> i32 {
    match std::process::Command::new(program).args(args).spawn() {
        // Waited for rather than left to run, because the pseudoconsole's child is this process: alacritty
        // watches *its* handle for the exit, so a shim that returned would be read as the shell ending and
        // would take the pseudoterminal with it.
        Ok(mut child) => match child.wait() {
            Ok(status) => status.code().unwrap_or(0),
            Err(_) => 0,
        },
        Err(problem) => {
            eprintln!("Unluminous could not start {program}: {problem}");
            127
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn the_command_line_a_restored_terminal_is_started_with() {
        let restore = Restore {
            file: PathBuf::from("/p/.unluminous/terminals/2.bytes"),
            shim: PathBuf::from("/apps/unluminous"),
        };
        let (program, args) =
            command_line(&restore, "pwsh.exe", &["-NoLogo".to_owned(), "-i".to_owned()]);
        assert_eq!(program, PathBuf::from("/apps/unluminous"));
        assert_eq!(
            args,
            vec![
                "--replay-screen",
                "/p/.unluminous/terminals/2.bytes",
                "--",
                "pwsh.exe",
                "-NoLogo",
                "-i"
            ]
        );
    }

    /// The other end of the same protocol, which is what makes the two halves one test rather than two
    /// spellings that agree today.
    #[test]
    fn what_the_shim_reads_is_what_the_command_line_wrote() {
        let restore =
            Restore { file: PathBuf::from("/p/2.bytes"), shim: PathBuf::from("/apps/unluminous") };
        let (_, args) = command_line(&restore, "zsh", &["-i".to_owned()]);
        let (file, program, rest) = asked_for(&args).expect("the shim's own command line");
        assert_eq!(file, PathBuf::from("/p/2.bytes"));
        assert_eq!(program, "zsh");
        assert_eq!(rest, vec!["-i"]);
    }

    #[test]
    fn an_ordinary_start_is_not_a_shim_invocation() {
        assert_eq!(asked_for(&words("/some/project")), None);
        assert_eq!(asked_for(&words("--version")), None);
    }

    /// A half-written command line opens a window rather than starting half a shell.
    #[test]
    fn a_malformed_shim_command_line_is_not_one() {
        assert_eq!(asked_for(&words("--replay-screen")), None);
        assert_eq!(asked_for(&words("--replay-screen /p/2.bytes")), None);
        assert_eq!(asked_for(&words("--replay-screen /p/2.bytes --")), None);
    }

    #[test]
    fn a_restore_whose_file_has_gone_is_not_worth_trying() {
        let folder = std::env::temp_dir().join("unluminous-restore-worth-trying");
        let _ = std::fs::create_dir_all(&folder);
        let file = folder.join("2.bytes");
        let _ = std::fs::write(&file, b"hello");
        let here = std::env::current_exe().expect("this test's own program");
        assert!(is_worth_trying(&Restore { file: file.clone(), shim: here.clone() }));
        assert!(!is_worth_trying(&Restore {
            file: folder.join("never-written.bytes"),
            shim: here,
        }));
        assert!(!is_worth_trying(&Restore { file, shim: folder.join("no-such-program") }));
    }
}
