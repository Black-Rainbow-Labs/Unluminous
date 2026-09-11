//! What a terminal node's command really starts.
//!
//! `task-1904` asks, in as many words, that *"we definitely need to verify claude and codex work"* in
//! a node. Driving a real window found that one of the two did not, and the reason is the one this
//! repository has already written down twice.
//!
//! **npm installs three files for `codex` on Windows** — `codex`, `codex.cmd` and `codex.ps1` — and
//! only the middle one is a program the operating system will start. A spawn by the bare name finds
//! either nothing or the extension-less shell script meant for Git Bash, and `CreateProcess` answers
//! *"The system cannot find the file specified"* or *"%1 is not a valid Win32 application"*. `claude`
//! is a real `.exe` on this machine, which is why it worked and codex did not: a check that had only
//! tried one of them would have reported that both were fine.
//!
//! So two things happen here, and neither is a nicety.
//!
//! **The name is resolved before it is spawned**, through `unluminous_chat::provider::program`, which
//! tries the `PATHEXT` extensions first and the extension-less file not at all — the rule
//! `task-1767` wrote after measuring exactly this against the real `codex`. Reusing it rather than
//! writing a second walk of `PATH` is the point: two answers to "where is this program" is one too
//! many.
//!
//! **A batch file is started through `cmd.exe`**, because `CreateProcessW` needs an executable image
//! and a `.cmd` is not one. That is what a shell does and what every terminal on Windows does, and it
//! is the last step between finding `codex.cmd` and running it.
//!
//! The decision is a pure function over a resolved path, so it is a unit test with no process behind
//! it — which is the only way to test the Windows half of it on a machine that is not Windows.

use std::path::Path;

/// The program and arguments a node's command becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// The program to start. `None` means the machine's own shell, which is what an empty command is.
    pub program: Option<String>,
    pub args: Vec<String>,
}

impl Launch {
    /// The machine's own shell, which is what a node with no command of its own runs.
    pub fn the_shell(shell: Option<String>) -> Launch {
        Launch { program: shell, args: Vec::new() }
    }
}

/// Whether this file has to be handed to `cmd.exe` rather than started directly.
///
/// Compared without case, because the extension came from `PATHEXT` and its case is the environment's
/// rather than the file's — the same comparison `login_shell` makes for the same reason.
pub fn needs_a_command_processor(found: &Path) -> bool {
    if !cfg!(windows) {
        return false;
    }
    found
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| extension == "cmd" || extension == "bat")
}

/// What to start, given the file a name resolved to and the words after it.
pub fn of(found: &Path, rest: &[String]) -> Launch {
    let named = found.display().to_string();
    match needs_a_command_processor(found) {
        true => {
            let mut args = vec!["/c".to_owned(), named];
            args.extend(rest.iter().cloned());
            Launch { program: Some("cmd.exe".to_owned()), args }
        }
        false => Launch { program: Some(named), args: rest.to_vec() },
    }
}

/// The words of a command, split the way a shell splits a double quoted word.
///
/// `run_configurations::split_command`'s own splitting, so a node's command and a run configuration's
/// are read the same way: nothing expands, nothing globs, and a backslash is a backslash unless it is
/// in front of a quote.
pub fn words(command: &str) -> Vec<String> {
    crate::services::run_configurations::split_command(command)
}

/// What a node's command starts, or a sentence saying what was looked for.
///
/// An empty command is the machine's own shell, which is what a node opens with.
pub fn resolve(command: &str, shell: Option<String>) -> Result<Launch, String> {
    let mut words = words(command);
    if words.is_empty() {
        return Ok(Launch::the_shell(shell));
    }
    let named = words.remove(0);
    let Some(found) = unluminous_chat::provider::program(&named) else {
        return Err(format!(
            "{named} is not installed, or is in a folder that is not on the PATH Unluminous searched."
        ));
    };
    Ok(of(&found, &words))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn an_empty_command_is_the_machines_own_shell() {
        let launch = resolve("   ", Some("pwsh.exe".to_owned())).expect("nothing to look up");
        assert_eq!(launch, Launch { program: Some("pwsh.exe".to_owned()), args: Vec::new() });
        let none = resolve("", None).expect("nothing to look up");
        assert_eq!(none.program, None, "and with no setting it is whatever the machine says");
    }

    #[test]
    fn a_program_is_started_by_the_file_that_was_found() {
        let found = PathBuf::from("C:/Users/jason/.local/bin/claude.exe");
        let launch = of(&found, &["--resume".to_owned(), "6f1c".to_owned()]);
        assert_eq!(launch.program.as_deref(), Some("C:/Users/jason/.local/bin/claude.exe"));
        assert_eq!(launch.args, vec!["--resume".to_owned(), "6f1c".to_owned()]);
    }

    #[test]
    #[cfg(windows)]
    fn a_batch_file_is_started_through_the_command_processor() {
        // The measured case: npm installs `codex.cmd`, and `CreateProcessW` needs an executable
        // image. Without this a node running `codex` refused with "The system cannot find the file
        // specified" and a perfectly working agent looked broken.
        let found = PathBuf::from("C:/nvm4w/nodejs/codex.cmd");
        let launch = of(&found, &["--help".to_owned()]);
        assert_eq!(launch.program.as_deref(), Some("cmd.exe"));
        assert_eq!(
            launch.args,
            vec!["/c".to_owned(), "C:/nvm4w/nodejs/codex.cmd".to_owned(), "--help".to_owned()]
        );
        assert!(needs_a_command_processor(&PathBuf::from("run.BAT")), "case does not matter");
        assert!(!needs_a_command_processor(&PathBuf::from("claude.exe")));
        assert!(!needs_a_command_processor(&PathBuf::from("codex")), "and nor does no extension");
    }

    #[test]
    fn a_command_is_split_the_way_a_run_configuration_is() {
        assert_eq!(words("claude --resume 6f1c"), vec!["claude", "--resume", "6f1c"]);
        assert_eq!(words("\"C:/Program Files/x/y.exe\" go"), vec!["C:/Program Files/x/y.exe", "go"]);
        assert!(words("   ").is_empty());
    }

    #[test]
    fn a_program_nobody_has_installed_is_refused_by_name() {
        let refusal = resolve("nothing-like-this-is-installed", None).expect_err("it is not there");
        assert!(refusal.contains("nothing-like-this-is-installed"), "{refusal}");
        assert!(refusal.contains("PATH"), "and says where it looked: {refusal}");
    }
}
