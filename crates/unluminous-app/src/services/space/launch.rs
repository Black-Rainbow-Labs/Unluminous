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

/// Whether this command is an agent Unluminous can hand a conversation id to and get it back.
///
/// **The id is one Unluminous chooses and gives, not one it reads back**, which is
/// `services::agent_tasks`' own answer to the same problem and the reason it works there: a first run is
/// `claude --session-id <uuid>` and a later one is `claude --resume <uuid>`, so the id is one Claude *answers
/// to* rather than one Unluminous hopes to parse out of somebody else's stream.
///
/// **Codex is not on the list, and `agent::why_it_cannot_resume` is why**: it names its own sessions, so an
/// id here would be a marker rather than something it answers to, and a restored Codex node begins a new
/// conversation. A shell is not on it either — a `zsh` handed a `--session-id` is a shell that refuses to
/// start, which is the failure this question exists to avoid. `task-1906`.
pub fn takes_a_session(command: &str) -> bool {
    let Some(program) = words(command).first().cloned() else { return false };
    let named = std::path::Path::new(&program)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    named == "claude"
}

/// The two arguments that decide which conversation an agent is on.
///
/// Named here rather than spelled in the two places that read them, because a command carrying one already
/// must not be given a second — see `session_for`.
const SESSION_ARGUMENTS: [&str; 2] = ["--session-id", "--resume"];

/// Whether the command a person typed already says which conversation to be on.
///
/// A node's command is somebody's own words, so `claude --resume abc` is a perfectly ordinary thing to find
/// there — and appending `--session-id <fresh>` to it hands Claude two conflicting instructions about which
/// conversation this is. Whatever it then does, the node has recorded an id that is not the one in use, which
/// is the failure the whole of `task-1906`'s session half exists to prevent.
pub fn already_names_a_session(command: &str) -> bool {
    words(command).iter().any(|word| SESSION_ARGUMENTS.contains(&word.as_str()))
}

/// What a terminal node's command becomes, and which conversation id the node should write down.
///
/// **One function rather than a condition in each of two places.** The command line is built where a command
/// line is built and the id is written down where the canvas is changed, and those are deliberately different
/// functions — but *which* id was put on the command line is one fact, and asking it twice is how the two
/// came apart: a node with no session, started with `resume` true, was given a fresh `--session-id` and then
/// recorded nothing, because the recording asked `!resume` instead of asking what had been sent. Claude then
/// answered to an id the node had already forgotten, so the next restart resumed nothing.
///
/// `held` is what the node has recorded and `fresh` is the id this run would use if it needs one. The answer
/// is the words to append and the id to write down, and they cannot disagree because they are one value.
pub fn session_for(
    command: &str,
    held: &str,
    resume: bool,
    fresh: &str,
) -> (String, Option<String>) {
    if command.trim().is_empty() || !takes_a_session(command) {
        return (String::new(), None);
    }
    // **A command that already says which conversation it is on is left exactly as it is.** Two lifecycle
    // arguments are worse than none: the node would record an id that is not the one Claude used.
    if already_names_a_session(command) {
        return (String::new(), None);
    }
    match (held.trim(), resume) {
        // One recorded and a resume asked for: that conversation, and it stays recorded.
        (recorded, true) if !recorded.is_empty() => {
            (format!(" --resume {recorded}"), Some(recorded.to_owned()))
        }
        // Everything else is a conversation beginning: nothing recorded at all, or `Restart`, which means a
        // fresh one on purpose. Either way the id given is the id written down.
        _ => (format!(" --session-id {fresh}"), Some(fresh.to_owned())),
    }
}

/// What a node's command starts, or a sentence saying what was looked for.
///
/// An empty command is the machine's own shell, which is what a node opens with.
/// **Looked for on the shell profile's `PATH`**, which is what `services::login_shell` reads. A node
/// running `claude` is a node running a program installed under the home folder, and an Unluminous started
/// from the Dock has `PATH=/usr/bin:/bin:/usr/sbin:/sbin` — so without this it could not be found at all,
/// which is the first of the two failures `login_shell`'s own module comment measured. `task-1905`.
pub fn resolve(command: &str, shell: Option<String>) -> Result<Launch, String> {
    let mut words = words(command);
    if words.is_empty() {
        return Ok(Launch::the_shell(shell));
    }
    let named = words.remove(0);
    let environment =
        unluminous_chat::Environment::from(crate::services::login_shell::for_a_child());
    let Some(found) = unluminous_chat::provider::program(&named, &environment) else {
        return Err(format!(
            "{named} is not installed, or is in a folder that is not on the PATH Unluminous searched."
        ));
    };
    Ok(of(&found, &words))
}

#[cfg(test)]
mod tests {
    /// What is put on the command line is what is written down, in every combination.
    ///
    /// The pair has to agree, and it used to be asked as two separate questions: the command line asked what
    /// the node had recorded, and the recording asked whether this was a resume. Those are different
    /// questions, and the case where they disagree is a real one — `Resume session` on a node that has never
    /// run passes `resume` true with an empty session, which sent a fresh `--session-id` and wrote nothing
    /// down. `task-1906`.
    #[test]
    fn what_the_command_line_says_about_the_conversation_is_what_the_node_records() {
        use super::session_for;

        // A first run: given an id, and that id is what is recorded.
        let (words, recorded) = session_for("claude", "", false, "fresh-one");
        assert_eq!(words, " --session-id fresh-one");
        assert_eq!(recorded.as_deref(), Some("fresh-one"));

        // A restore: resumed onto what it was on, and it stays on it.
        let (words, recorded) = session_for("claude", "was-on-this", true, "fresh-one");
        assert_eq!(words, " --resume was-on-this");
        assert_eq!(recorded.as_deref(), Some("was-on-this"));

        // `Restart`, which means a conversation beginning on purpose: a fresh id, and it replaces the old one.
        let (words, recorded) = session_for("claude", "was-on-this", false, "fresh-one");
        assert_eq!(words, " --session-id fresh-one");
        assert_eq!(recorded.as_deref(), Some("fresh-one"));

        // **The case that was wrong.** `Resume session` on a node that has never run: there is nothing to
        // resume, so it is given one — and the id it was given has to be the id it records, or Claude answers
        // to an id the node has forgotten.
        let (words, recorded) = session_for("claude", "", true, "fresh-one");
        assert_eq!(words, " --session-id fresh-one");
        assert_eq!(
            recorded.as_deref(),
            Some("fresh-one"),
            "the node was given an id and would have written down nothing",
        );

        // A shell and Codex are handed nothing at all and record nothing, whatever is asked.
        for command in ["zsh", "codex", "", "  "] {
            for resume in [true, false] {
                let (words, recorded) = session_for(command, "was-on-this", resume, "fresh-one");
                assert!(words.is_empty(), "{command:?} was handed {words:?}");
                assert_eq!(recorded, None, "{command:?} recorded {recorded:?}");
            }
        }
    }

    /// A command that already says which conversation it is on is left exactly as it is.
    ///
    /// A node's command is somebody's own words, so `claude --resume abc` is an ordinary thing to find there.
    /// Appending a second lifecycle argument hands Claude two conflicting instructions, and whichever it
    /// obeys, the node has recorded an id that is not the one in use. `task-1906`.
    #[test]
    fn a_command_that_already_names_a_conversation_is_left_alone() {
        use super::{already_names_a_session, session_for};

        assert!(already_names_a_session("claude --resume abc"));
        assert!(already_names_a_session("claude --session-id abc"));
        assert!(!already_names_a_session("claude"));
        assert!(!already_names_a_session("claude --print"));

        for command in ["claude --resume abc", "claude --session-id abc"] {
            for held in ["", "was-on-this"] {
                for resume in [true, false] {
                    let (words, recorded) = session_for(command, held, resume, "fresh-one");
                    assert!(
                        words.is_empty(),
                        "{command:?} would have been started as {command}{words}",
                    );
                    assert_eq!(
                        recorded, None,
                        "{command:?} would have recorded {recorded:?}, which is not what it is on",
                    );
                }
            }
        }
    }

    /// Which commands take a conversation id, and which deliberately do not.
    ///
    /// `task-1906`: a `zsh` handed a `--session-id` is a shell that refuses to start, and Codex names its own
    /// sessions — `agent::why_it_cannot_resume` is the sentence a person reads about that.
    #[test]
    fn only_an_agent_that_answers_to_an_id_is_given_one() {
        use super::takes_a_session;
        assert!(takes_a_session("claude"));
        assert!(takes_a_session("claude --dangerously-skip-permissions"), "with its own arguments");
        assert!(takes_a_session("/Users/somebody/.local/bin/claude"), "named by its full path");
        assert!(takes_a_session("CLAUDE"), "however it is spelled");
        // The ones that must not be given one.
        assert!(!takes_a_session("codex"), "Codex names its own sessions");
        assert!(!takes_a_session("zsh"), "a shell has no conversation");
        assert!(!takes_a_session("pwsh -NoLogo"));
        assert!(!takes_a_session(""), "and a node with no command runs the machine's own shell");
        assert!(!takes_a_session("   "));
    }

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
        assert_eq!(
            words("\"C:/Program Files/x/y.exe\" go"),
            vec!["C:/Program Files/x/y.exe", "go"]
        );
        assert!(words("   ").is_empty());
    }

    #[test]
    fn a_program_nobody_has_installed_is_refused_by_name() {
        let refusal = resolve("nothing-like-this-is-installed", None).expect_err("it is not there");
        assert!(refusal.contains("nothing-like-this-is-installed"), "{refusal}");
        assert!(refusal.contains("PATH"), "and says where it looked: {refusal}");
    }
}

/// The command line that starts `program` again, continuing its conversation where it has one.
///
/// **`--continue` rather than `--resume <id>`, and the difference is the whole of why this is honest.**
/// `--resume <id>` names a conversation Unluminous chose and *gave* to Claude, which is what
/// [`session_for`] does for a node whose **command** is an agent. It cannot work here: this is for a node
/// whose terminal is a shell somebody typed `claude` into, so by the time there is anything to record the
/// process is already running without an id, and there is no id to resume.
///
/// `--continue` is Claude's own answer to *"continue the most recent conversation in this directory"* — a
/// question it answers from its own records rather than one Unluminous answers from a file it wrote. It is
/// weaker in two specific ways, and both are told rather than hidden: two nodes in one folder come back on
/// the same conversation, and a conversation continued in a terminal elsewhere since is the one that comes
/// back. It is still the difference between `task-1907`'s *"no claude code"* and a node that comes back
/// where it was.
///
/// **Codex gets no flag**, and `agent::why_it_cannot_resume` is the existing reason: it names its own
/// sessions, so anything Unluminous passed would be a marker rather than something it answers to. Starting
/// it again begins a new conversation, which is what the row on the node says it does.
pub fn continues_a_conversation(program: &str) -> String {
    let named = program.trim();
    match takes_a_session(named) {
        true => format!("{named} --continue"),
        false => named.to_owned(),
    }
}

#[cfg(test)]
mod continuing_tests {
    use super::*;

    /// An agent typed into a shell is continued rather than resumed.
    ///
    /// `task-1907`: the conversation cannot be resumed by id, because the id has to be given to Claude when it
    /// starts and by the time somebody has typed `claude` it is already running without one. `--continue` is
    /// Claude's own question about its own records.
    #[test]
    fn the_offer_is_a_continue_when_there_is_no_recorded_conversation() {
        assert_eq!(continues_a_conversation("claude"), "claude --continue");
        assert_eq!(continues_a_conversation("  claude  "), "claude --continue");
    }

    /// A shell and Codex get no flag, for two different reasons that both end the same way.
    ///
    /// A shell has no conversation at all. Codex names its own sessions, so anything passed here would be a
    /// marker rather than something it answers to — `agent::why_it_cannot_resume`'s own limitation.
    #[test]
    fn a_program_with_no_conversation_is_started_as_it_is() {
        assert_eq!(continues_a_conversation("zsh"), "zsh");
        assert_eq!(continues_a_conversation("bash"), "bash");
        assert_eq!(continues_a_conversation("codex"), "codex");
        assert_eq!(continues_a_conversation("vim"), "vim");
    }
}

/// Whether this program is a shell rather than something somebody ran in one.
///
/// **What it is for is not classifying shells, it is deciding what may be overwritten.** A restored terminal
/// node starts a shell, so the first reading of what it is running after a project opens answers `zsh` — and
/// before `task-1907` that replaced the program the file held before anything could offer it. Measured on the
/// released build: `space.conf` held `running = sleep` before a restart and `running = zsh` a second after it,
/// and the offer then said *"Started zsh again"*. So a shell is not recorded over a program.
///
/// The list is the shells a machine actually starts a terminal with — `Settings::shell()` names the first two
/// on each platform — and a name that is not on it is treated as a program, which is the safe direction: a
/// program wrongly kept is an offer nobody has to take, and a program wrongly discarded is the report.
pub fn is_a_shell(program: &str) -> bool {
    const SHELLS: [&str; 10] =
        ["zsh", "bash", "sh", "fish", "dash", "ksh", "tcsh", "csh", "powershell", "pwsh"];
    // **Both separators, rather than `Path::file_stem`**, which splits only the running platform's — so a
    // `C:\Windows\System32\cmd.exe` read out of a `space.conf` written on Windows came back whole on macOS and
    // was not recognised. `is_the_node_runtime` splits both for the same reason: a canvas written on one
    // platform is read on the other.
    let last = program.trim().rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = last.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(last);
    let named = stem.to_ascii_lowercase();
    // `cmd.exe` and `login`, which macOS starts a terminal through, are shells for this purpose too.
    SHELLS.contains(&named.as_str()) || named == "cmd" || named == "login"
}

#[cfg(test)]
mod shell_tests {
    use super::*;

    /// The shells a machine starts a terminal with are recognised, whatever path they arrive by.
    ///
    /// What this decides is not a classification for its own sake — it is what may be overwritten. A restored
    /// node starts a shell, so the first reading after a project opens must not replace the program the file
    /// held. `task-1907` found that by driving the released build: `running = sleep` before a restart became
    /// `running = zsh` a second after it, and the offer then said *"Started zsh again"*.
    #[test]
    fn the_shells_a_terminal_starts_with_are_recognised() {
        for shell in ["zsh", "bash", "sh", "fish", "pwsh", "powershell", "cmd", "login"] {
            assert!(is_a_shell(shell), "{shell} is a shell");
        }
        // By a full path and with an extension, which is how a machine names them.
        assert!(is_a_shell("/bin/zsh"));
        assert!(is_a_shell("/usr/local/bin/fish"));
        assert!(is_a_shell("C:\\Windows\\System32\\cmd.exe"));
        assert!(is_a_shell("  /bin/bash  "));
    }

    /// And a program somebody ran in a shell is not one.
    ///
    /// A name that is not on the list is treated as a program, which is the safe direction: a program wrongly
    /// kept is an offer nobody has to take, and a program wrongly discarded is the report.
    #[test]
    fn a_program_somebody_ran_is_not_a_shell() {
        for program in ["claude", "codex", "vim", "sleep", "node", "cargo", "python3", ""] {
            assert!(!is_a_shell(program), "{program} is not a shell");
        }
    }
}
