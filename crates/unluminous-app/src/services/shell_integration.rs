//! Asking PowerShell to report the folder it is in, which is the one way it can be asked.
//!
//! `task-1945` reads the folder off the shell process, which answers for `cmd.exe`, `bash` and `zsh` and does
//! not answer for PowerShell: `Set-Location` moves PowerShell's own location and never the process's current
//! directory. Measured again on `task-1950` with `examples/folder_probe`, through a real pseudoconsole — a
//! `pwsh` at `C:\Users\jason\AppData\Local\Temp` reports a process still sitting in `C:\jason\dev\unluminous`.
//!
//! [`unluminous_terminal::reported`] reads the sequence a shell reports its folder with, and that half is
//! always on because it costs nothing and changes nothing. This is the other half: making a shell that reports
//! nothing report something, which means adding to the prompt the person already has.
//!
//! **So it is a setting, and it is off.** `terminal.shell_integration`. Adding to somebody's prompt is a
//! change to their shell rather than to this editor, which is the reason `task-1945` stopped here and left it
//! as a question; what turns the question into a tick box is that the change is small, visible, and theirs to
//! make. Off, nothing is written and no shell is started any differently from the way it was before.
//!
//! **Their own prompt is kept and called, never replaced.** The script Unluminous runs is `-File`, which
//! PowerShell processes *after* the profile — measured, against a temporary profile that set a prompt of its
//! own — so what it wraps is whatever their profile installed. A prompt that fails is caught and the sequence
//! is still added, because the one thing this must never do is leave somebody without a prompt.
//!
//! **And it is not a prompt parser.** Nothing reads what the prompt says. What is read is a sequence the shell
//! wrote on purpose to be read, which is the whole of the difference.

use std::path::{Path, PathBuf};

use unluminous_terminal::SessionSettings;

/// What the script is called where Unluminous keeps its own things.
pub const FILE: &str = "shell-integration.ps1";

/// The script Unluminous runs after the person's own profile.
///
/// `OSC 9;9` rather than `OSC 7`: it is Windows Terminal's own, it is what Microsoft's published PowerShell
/// snippet writes, and it carries a path rather than a URL — so there is no percent encoding to get wrong on
/// a path with a space in it, and no host to decide about. The reader takes either.
pub const SCRIPT: &str = r#"# Unluminous shell integration for PowerShell.
#
# PowerShell's Set-Location moves PowerShell's own location and never the process's current directory, so a
# terminal tab that is closed and reopened has no other way to come back where the person was. This adds the
# sequence Windows Terminal reads for exactly that, ESC ] 9 ; 9 ; <path> ST, to the end of whatever prompt is
# already here.
#
# The prompt that is already here is kept and called rather than replaced. Unluminous runs this with -File,
# which PowerShell processes after the profile, so "already here" is whatever the profile set up.
#
# Written by Unluminous because terminal.shell_integration is on. Turn that off and this stops being run.
# Nothing else reads this file, and nothing outside the session it is run in is changed by it.

if (-not $global:__UnluminousReportsItsFolder) {
    $global:__UnluminousReportsItsFolder = $true
    $global:__UnluminousInnerPrompt = $function:prompt

    function global:prompt {
        $text = ''
        try {
            if ($global:__UnluminousInnerPrompt) {
                $text = -join @(& $global:__UnluminousInnerPrompt)
            }
        } catch {
            $text = ''
        }
        if ($text -eq '') {
            $text = "PS $($ExecutionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) "
        }
        $here = $ExecutionContext.SessionState.Path.CurrentLocation
        if ($here -and $here.Provider.Name -eq 'FileSystem') {
            $text = $text + "$([char]27)]9;9;`"$($here.ProviderPath)`"$([char]27)\"
        }
        $text
    }
}
"#;

/// Whether `program` is a PowerShell, whatever it is spelt as.
///
/// `pwsh` and `powershell` are two programs rather than two names for one — they read different profiles —
/// and both take the same switches. A path is compared by its last part, because `terminal.shell` may name
/// one and `default_shell` answers with a bare name.
pub fn is_powershell(program: &str) -> bool {
    let last = Path::new(program).file_name().map(|name| name.to_string_lossy().into_owned());
    let last = last.unwrap_or_else(|| program.to_owned());
    let stem = last.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&last);
    stem.eq_ignore_ascii_case("pwsh") || stem.eq_ignore_ascii_case("powershell")
}

/// Write the script into `folder` and answer where it is, or `None` when it could not be written.
///
/// Rewritten every time rather than written once, so a script left behind by an older Unluminous is this
/// Unluminous's script. It is the one thing here that touches the disk and it happens when a terminal opens.
pub fn write_the_script(folder: &Path) -> Option<PathBuf> {
    let file = folder.join(FILE);
    if std::fs::read_to_string(&file).is_ok_and(|there| there == SCRIPT) {
        return Some(file);
    }
    std::fs::create_dir_all(folder).ok()?;
    std::fs::write(&file, SCRIPT).ok()?;
    Some(file)
}

/// Start this shell under the script, when it is a shell the script is for and there is nothing else it was
/// asked to do.
///
/// **Only a PowerShell, and only with no arguments of its own.** A terminal tab and a plain terminal node
/// start a shell with nothing else to do, which is the case this exists for. Anything with arguments was
/// asked to run a particular thing — a node running `claude`, a run configuration — and a `-NoExit -File`
/// pushed in front of that would be changing what somebody asked to run, which is not what a setting about
/// a prompt may do.
///
/// Answers whether it applied, so a caller can say so and a test can assert on it.
pub fn apply(settings: &mut SessionSettings, script: &Path) -> bool {
    if !settings.args.is_empty() {
        return false;
    }
    let shell = settings.shell.clone().unwrap_or_else(unluminous_terminal::session::default_shell);
    if !is_powershell(&shell) {
        return false;
    }
    // `-NoExit` so the shell stays interactive once the script has run, and `-File` rather than `-Command`
    // because a script with quotes, braces and `$` in it has to survive a command line, a ConPTY and the C
    // runtime's own escaping on the way to the shell. The profile is not suppressed: this adds to it.
    settings.shell = Some(shell);
    settings.args = vec!["-NoExit".to_owned(), "-File".to_owned(), script.display().to_string()];
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(shell: &str) -> SessionSettings {
        SessionSettings { shell: Some(shell.to_owned()), ..Default::default() }
    }

    #[test]
    fn both_powershells_are_powershell_however_they_are_spelt() {
        assert!(is_powershell("pwsh.exe"));
        assert!(is_powershell("pwsh"));
        assert!(is_powershell("powershell.exe"));
        assert!(is_powershell("PowerShell.EXE"));
        assert!(is_powershell(r"C:\Program Files\PowerShell\7\pwsh.exe"));
        assert!(is_powershell("/usr/bin/pwsh"));
    }

    #[test]
    fn nothing_else_is_a_powershell() {
        assert!(!is_powershell("cmd.exe"));
        assert!(!is_powershell("bash"));
        assert!(!is_powershell("/bin/zsh"));
        assert!(!is_powershell("claude"));
        // Not a PowerShell, and the one name most likely to be mistaken for one.
        assert!(!is_powershell("powershell-ise.exe"));
    }

    #[test]
    fn a_powershell_with_nothing_else_to_do_is_started_under_the_script() {
        let script =
            PathBuf::from(r"C:\Users\jason\AppData\Roaming\Unluminous\shell-integration.ps1");
        let mut asked = settings("pwsh.exe");
        assert!(apply(&mut asked, &script));
        assert_eq!(
            asked.shell.as_deref(),
            Some("pwsh.exe"),
            "the tab is still named after the shell"
        );
        assert_eq!(
            asked.args,
            vec![
                "-NoExit".to_owned(),
                "-File".to_owned(),
                r"C:\Users\jason\AppData\Roaming\Unluminous\shell-integration.ps1".to_owned(),
            ]
        );
    }

    #[test]
    fn a_shell_that_was_asked_to_run_something_is_left_exactly_as_it_was() {
        // A terminal node running `claude`, a run configuration, `pwsh -Command …` a person typed into the
        // shell setting: each was asked to run a particular thing, and a prompt is not worth changing it for.
        let script = PathBuf::from("x.ps1");
        let mut asked = settings("pwsh.exe");
        asked.args = vec!["-Command".to_owned(), "Get-ChildItem".to_owned()];
        assert!(!apply(&mut asked, &script));
        assert_eq!(asked.args, vec!["-Command".to_owned(), "Get-ChildItem".to_owned()]);

        let mut other = settings("claude");
        assert!(!apply(&mut other, &script));
        assert!(other.args.is_empty());
    }

    #[test]
    fn a_shell_that_is_not_a_powershell_is_left_exactly_as_it_was() {
        let script = PathBuf::from("x.ps1");
        let mut asked = settings("cmd.exe");
        assert!(!apply(&mut asked, &script));
        assert!(asked.args.is_empty());
        assert_eq!(asked.shell.as_deref(), Some("cmd.exe"));
    }

    #[test]
    fn the_script_is_written_once_and_then_left_alone() {
        let folder = std::env::temp_dir().join("unluminous-shell-integration-test");
        let _ = std::fs::remove_dir_all(&folder);
        let file = write_the_script(&folder).expect("write the script");
        assert_eq!(std::fs::read_to_string(&file).expect("read it back"), SCRIPT);
        let written = std::fs::metadata(&file).expect("ask about it").modified().expect("a time");
        assert_eq!(write_the_script(&folder).as_deref(), Some(file.as_path()));
        let again = std::fs::metadata(&file).expect("ask again").modified().expect("a time");
        assert_eq!(
            written, again,
            "an unchanged script is not written over on every terminal opened"
        );
    }

    /// The whole feature, against a real PowerShell in a real pseudoconsole.
    ///
    /// **The one test that could have found any of this**, and the reason it earns its two seconds: every
    /// other test here is about one end or the other, and what `task-1950` is about is the two ends meeting.
    /// A script that wrote the sequence into a prompt PowerShell never calls, a shell started with switches
    /// PowerShell refuses, a reader that misses a sequence split across two reads — each of those leaves every
    /// other test in this file green and the feature doing nothing at all.
    ///
    /// It asserts the difference as well as the answer: the process is still in the folder it was started in,
    /// which is the fault the ticket reports, and what is written down is where the person really is.
    #[test]
    #[cfg(windows)]
    fn a_powershell_asked_where_it_is_comes_back_where_somebody_moved_it_to() {
        let Some(shell) = a_powershell_on_this_machine() else {
            return;
        };
        let folder = std::env::temp_dir().join("unluminous-shell-integration-live");
        std::fs::create_dir_all(&folder).expect("make somewhere to move to");
        let started_in = std::env::current_dir().expect("a folder to start in");
        let script =
            write_the_script(&std::env::temp_dir().join("unluminous-shell-integration-script"))
                .expect("write the script");

        let mut settings = SessionSettings {
            shell: Some(shell),
            working_directory: Some(started_in.clone()),
            ..Default::default()
        };
        assert!(apply(&mut settings, &script), "a plain PowerShell is what this is for");
        let waker: unluminous_terminal::Waker = std::sync::Arc::new(|| {});
        let mut session = unluminous_terminal::Session::spawn(
            &settings,
            unluminous_terminal::Size::new(12, 100),
            waker,
        )
        .expect("start a shell");

        let moved = folder.display().to_string();
        session.send(format!("Set-Location \"{moved}\"\r").into_bytes());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            session.pump();
            if session.reported_folder().as_deref() == Some(folder.as_path()) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "PowerShell never reported {moved}; the screen was:\n{}",
                session.snapshot().text()
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        // The fault the ticket reports, still true and now no longer the answer.
        let process = session.process_folder();
        assert_ne!(
            process.as_deref(),
            Some(folder.as_path()),
            "Set-Location moved the process's own current directory, which it has never done here"
        );
        assert_eq!(
            session.folder().as_deref(),
            Some(folder.as_path()),
            "the shell's answer is the one kept"
        );
        session.kill();
    }

    /// A PowerShell to drive, or `None` on a machine with neither — where there is nothing to measure.
    #[cfg(windows)]
    fn a_powershell_on_this_machine() -> Option<String> {
        let shell = unluminous_terminal::session::default_shell();
        is_powershell(&shell).then_some(shell)
    }

    /// The one thing about the script that a reader of this file cannot check by eye, and the thing that would
    /// silently do nothing if it were wrong.
    #[test]
    fn the_script_writes_the_sequence_the_reader_reads() {
        assert!(SCRIPT.contains("]9;9;"), "the script does not write OSC 9;9");
        assert!(SCRIPT.contains("[char]27"), "the script does not write an escape character");
        assert!(
            SCRIPT.contains("__UnluminousInnerPrompt"),
            "the prompt that was there is not kept"
        );
        // Read by `unluminous_terminal::reported`, which is the other end of it. The path is a real folder so
        // the reading is the whole path a live shell's report takes.
        let here = std::env::temp_dir();
        let reported = unluminous_terminal::reported::Reported::new();
        let mut scanner = unluminous_terminal::reported::Scanner::new(reported.clone());
        scanner.read(format!("PS> \x1b]9;9;\"{}\"\x1b\\", here.display()).as_bytes());
        assert_eq!(reported.folder(), Some(here));
    }
}
