//! Starting another Unluminous.
//!
//! `tasks/improvements.md` asks for several Unluminous windows at once, each with its own project, the way
//! The reference editor does it. Each one is its own process rather than a second window inside this process.
//!
//! That is a decision worth recording. A second window in the same process would share the document, the
//! file tree, the settings in memory and the terminal sessions, so every one of those would have to learn
//! which window it belonged to. A second process shares nothing: it reads the same settings file, opens
//! its own project, and if it stops it takes nothing with it. Unluminous already takes the folder to open as
//! its first argument, which is all a second process needs, and the reference editor works the same way.

use std::path::Path;
use std::process::Command;

/// The command that starts another Unluminous on `folder`.
///
/// Split out from running it so that the arguments can be checked by a test without starting a window.
pub fn command_for(program: &Path, folder: &Path) -> Command {
    let mut command = Command::new(program);
    command.arg(folder);
    command
}

/// Start another Unluminous on `folder`.
///
/// The new process is not waited for and its output is left where this one's goes. Failing to start it is
/// reported and otherwise ignored, because the window that asked is still working.
pub fn open_window(folder: &Path) -> Option<u32> {
    let program = match std::env::current_exe() {
        Ok(program) => program,
        Err(problem) => {
            eprintln!(
                "Unluminous could not find its own program to start another window: {problem}"
            );
            return None;
        }
    };
    match command_for(&program, folder).spawn() {
        // **The process id, which restoring a session needs.** `task-1912`: the window that restores a session
        // writes the whole session down itself, and a row is the window that has that project open — so the
        // windows it starts have to be identified as it starts them, which is the one moment anything knows.
        Ok(child) => Some(child.id()),
        Err(problem) => {
            eprintln!("Unluminous could not start another window: {problem}");
            None
        }
    }
}

/// The command that opens the platform's file manager with `path` selected.
///
/// Split out from running it for the same reason [`command_for`] is: a test can check what would be
/// run without a file manager window appearing on the machine running the tests.
///
/// **The path is written the way this platform writes one before it is handed over**, which is
/// `paths::plain` and `paths::native` — the verbatim prefix off, and one separator throughout.
/// `task-2009`: *"When I right click an html file that's in a nested dir, and select show in
/// explorer, it opens explorer but doesn't open the correct dir, nor highlight the file in the
/// correct dir."*
///
/// That was measured rather than guessed at. `Path::join` does not normalise, so a project opened
/// with a forward slash anywhere in its path gives every row under it a path with both separators in
/// it — and Windows Explorer answers one of those by opening the **Desktop**:
///
/// ```text
/// explorer /select,C:\jason\dev\unluminous\_agent_output/site/nested/deep/page.html
///     -> file:///C:/Users/jason/Desktop
/// explorer /select,C:/jason/dev/unluminous/_agent_output/site/nested/deep/page.html
///     -> file:///C:/Users/jason/Desktop
/// explorer /select,C:\jason\dev\unluminous\_agent_output\site\nested\deep\page.html
///     -> file:///C:/jason/dev/unluminous/_agent_output/site/nested/deep   (the file selected)
/// ```
///
/// Nothing inside Unluminous notices a mixed path, because `Path`'s own `Eq` compares components and
/// the file system accepts both — it is only ever noticed by **another program**, which is exactly
/// what `unluminous_terminal::paths` exists for and what `task-1794` measured at a debug adapter.
pub fn reveal_command(path: &Path) -> Command {
    let path = unluminous_terminal::paths::native(&unluminous_terminal::paths::plain(path));
    reveal_command_for(path.as_path())
}

/// Windows Explorer, told to open a folder and select one thing in it.
///
/// **The quotation marks go round the path, not round the whole argument**, and that is the second
/// half of the report. `/select,` and the path have to be one argument with no space after the
/// comma — but `Command::arg` escapes what it is given, so a path with a space in it arrives with the
/// quotation mark in front of the switch, and Explorer answers that by opening **Documents**.
/// Measured against the real Explorer:
///
/// ```text
/// "/select,C:\a space here\page.html"   -> file:///C:/Users/jason/Documents
/// /select,"C:\a space here\page.html"   -> the right folder, with the file selected
/// ```
///
/// `raw_arg` is what puts an argument on the command line as written. Quoting is Windows' own escape
/// and a Windows path cannot hold a quotation mark, so there is nothing for it to swallow.
#[cfg(windows)]
fn reveal_command_for(path: &Path) -> Command {
    use std::os::windows::process::CommandExt as _;
    let mut command = Command::new("explorer");
    command.raw_arg(format!("/select,\"{}\"", path.display()));
    command
}

/// The Finder, told to reveal a file.
#[cfg(target_os = "macos")]
fn reveal_command_for(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    command
}

/// Every desktop on Linux has its own file manager, and `xdg-open` on a folder is the closest thing
/// to a common answer. It opens the folder rather than selecting the file in it.
#[cfg(not(any(windows, target_os = "macos")))]
fn reveal_command_for(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path.parent().unwrap_or(path));
    command
}

/// Show `path` in the platform's file manager.
///
/// The name of the entry says `Explorer` on Windows and `Finder` on macOS, because those are what
/// the thing is called there.
pub fn reveal(path: &Path) -> bool {
    match reveal_command(path).spawn() {
        Ok(_) => true,
        Err(problem) => {
            eprintln!(
                "Unluminous could not show {} in the file manager: {problem}",
                path.display()
            );
            false
        }
    }
}

/// What the entry that does it is called on this platform.
pub fn file_manager_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in Explorer"
    } else {
        "Open Containing Folder"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn revealing_a_file_asks_the_platform_for_its_own_file_manager() {
        let command = reveal_command(Path::new("/tmp/notes/one.md"));
        let program = command.get_program().to_string_lossy().to_string();
        let arguments: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().to_string()).collect();
        if cfg!(target_os = "windows") {
            assert_eq!(program, "explorer");
            // One argument, with no space after the comma: `explorer` will not select the file if
            // the switch and the path are separate arguments. Written with this platform's own
            // separator, and with the quotation marks round the **path** rather than round the whole
            // argument — see [`reveal_command_for`] for what the other spelling answers with.
            assert_eq!(arguments, vec![r#"/select,"\tmp\notes\one.md""#.to_owned()]);
        } else if cfg!(target_os = "macos") {
            assert_eq!(program, "open");
            assert_eq!(arguments, vec!["-R".to_owned(), "/tmp/notes/one.md".to_owned()]);
        } else {
            assert_eq!(program, "xdg-open");
            assert_eq!(arguments, vec!["/tmp/notes".to_owned()]);
        }
    }

    /// `task-2009`: a path with both separators in it, which is what every row under a project
    /// opened with a forward slash anywhere in its path is, made Windows Explorer open the Desktop.
    /// Measured against the real Explorer; the three commands and their answers are in
    /// [`reveal_command`]'s own note.
    #[test]
    fn a_path_written_with_both_separators_is_handed_over_with_one() {
        let mixed = if cfg!(windows) {
            PathBuf::from(r"C:\project\nested").join("deep/page.html")
        } else {
            PathBuf::from("/project/nested").join("deep/page.html")
        };
        let command = reveal_command(&mixed);
        let arguments: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().to_string()).collect();
        let handed = arguments.last().expect("a path was handed over").clone();
        if cfg!(windows) {
            assert!(handed.ends_with(r#"C:\project\nested\deep\page.html""#), "{handed}");
        } else {
            assert!(handed.ends_with("/project/nested/deep/page.html"), "{handed}");
        }
    }

    /// And a verbatim path — what `std::fs::canonicalize` answers with on Windows — loses its
    /// prefix, for the reason `unluminous_terminal::paths::plain` exists at all.
    #[test]
    fn a_verbatim_path_is_handed_over_plain() {
        if !cfg!(windows) {
            return;
        }
        let command = reveal_command(Path::new(r"\\?\C:\project\page.html"));
        let arguments: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().to_string()).collect();
        assert_eq!(arguments, vec![r#"/select,"C:\project\page.html""#.to_owned()]);
    }

    /// `task-2009`: a path with a space in it, which is what `Command::arg`'s own escaping breaks.
    ///
    /// Measured against the real Explorer, which answered the escaped form by opening `Documents`.
    /// See [`reveal_command_for`].
    #[test]
    fn a_path_with_a_space_in_it_is_quoted_round_the_path_alone() {
        if !cfg!(windows) {
            return;
        }
        let command = reveal_command(Path::new(r"C:\a space here\page.html"));
        let arguments: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().to_string()).collect();
        assert_eq!(arguments, vec![r#"/select,"C:\a space here\page.html""#.to_owned()]);
    }

    #[test]
    fn the_command_runs_unluminous_again_with_the_folder_after_it() {
        let program = PathBuf::from("/somewhere/unluminous");
        let folder = PathBuf::from("/projects/book");
        let command = command_for(&program, &folder);
        assert_eq!(command.get_program(), program.as_os_str());
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(arguments, vec![folder.as_os_str()], "the folder is the only argument");
    }

    /// A small program this machine is certain to have, that takes a path after it and stops straight
    /// away. `/bin/echo` is a Unix path and there is nothing at it on Windows, so the test asked the
    /// operating system to start a program that was not there and read the refusal as the plumbing being
    /// broken. `where` is the nearest thing Windows ships: it is in the folder `SystemRoot` names on every
    /// installation, and whatever it is handed it prints a line and exits.
    fn harmless_program() -> PathBuf {
        if cfg!(target_os = "windows") {
            let system_root =
                std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_owned());
            PathBuf::from(system_root).join("System32").join("where.exe")
        } else {
            PathBuf::from("/bin/echo")
        }
    }

    /// Starting a real second window is what the `New Window` entry does, and this checks the plumbing
    /// under it without opening a window: `true` is only returned once a process has actually started.
    #[test]
    fn a_second_process_can_be_started() {
        // `std::env::current_exe` inside a test is the test binary, so this would run the tests again.
        // The command is built for a program that exists and does nothing instead.
        let program = harmless_program();
        let folder = std::env::temp_dir();
        let child = command_for(&program, &folder).spawn();
        assert!(child.is_ok(), "spawning a second process should work on this machine");
        // Wait for it, so the test leaves nothing behind for the operating system to collect.
        if let Ok(mut child) = child {
            child.wait().ok();
        }
    }
}
