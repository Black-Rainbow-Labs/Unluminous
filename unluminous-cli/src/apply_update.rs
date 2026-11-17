//! Installing a newer Unluminous once the window that downloaded it has gone, and starting it again.
//!
//! `task-2063`: *"Check for updates should provide me an option to install & restart."* An installer
//! cannot replace a program that is running, so the window cannot install itself. It downloads the
//! installer, checks its size and its SHA-256, starts this helper, and quits. The helper waits for the
//! window's process to end, runs the installer, and starts the new Unluminous. VS Code and Electron's
//! Squirrel update the same way, with a small process of their own that outlives the application.
//!
//! **It is `unluminous-cli`, and a copy of it.** The window starts the copy it put in the download folder,
//! because the installer replaces `unluminous-cli.exe` in the install folder and cannot replace a program
//! that is running. It is a console program, so the window starts it with no console window.
//!
//! **Both ends live here**, the command line [`command_line`] builds and [`run`] that carries it out, for
//! the reason `restore` gives: a protocol split across two crates has two chances to disagree.
//!
//! Everything it does is written to `apply-update.log` beside the installer, because it runs with no
//! window and no console and a failure would otherwise say nothing to anybody.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The switch the helper is asked for by. It must be the first word, as `restore::SWITCH` must.
pub const SWITCH: &str = "--apply-update";

/// What the helper is asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The installer that was downloaded and checked: Inno Setup's program on Windows, a zip holding
    /// `Unluminous.app` on macOS.
    pub installer: PathBuf,
    /// The process to wait for, which is the window that started this.
    pub wait: u32,
    /// What to start once it is installed: `unluminous.exe` on Windows, the `.app` bundle on macOS.
    pub relaunch: PathBuf,
    /// Whether Unluminous was installed for everybody on this machine, which the installer has to be
    /// told again or it installs a second copy for this person alone.
    pub all_users: bool,
    /// Where a failed install is written for the window started afterwards, normally [`failure_note`].
    /// `None` writes nothing, which is what a test asks for so it never leaves a note for the person's
    /// own Unluminous to show.
    pub note: Option<PathBuf>,
}

/// The arguments that start the helper on `plan`, in the order [`asked_for`] reads them.
pub fn command_line(plan: &Plan) -> Vec<String> {
    let mut words = vec![
        SWITCH.to_owned(),
        plan.installer.display().to_string(),
        "--wait".to_owned(),
        plan.wait.to_string(),
        "--relaunch".to_owned(),
        plan.relaunch.display().to_string(),
    ];
    if plan.all_users {
        words.push("--all-users".to_owned());
    }
    if let Some(note) = &plan.note {
        words.push("--note".to_owned());
        words.push(note.display().to_string());
    }
    words
}

/// The plan a command line asks for, or nothing when it is not a request for this helper.
pub fn asked_for(words: &[String]) -> Option<Plan> {
    if words.first().map(String::as_str) != Some(SWITCH) {
        return None;
    }
    let installer = PathBuf::from(words.get(1)?);
    let value = |name: &str| {
        let at = words.iter().position(|word| word == name)?;
        words.get(at + 1).cloned()
    };
    Some(Plan {
        installer,
        wait: value("--wait")?.parse().ok()?,
        relaunch: PathBuf::from(value("--relaunch")?),
        all_users: words.iter().any(|word| word == "--all-users"),
        note: value("--note").map(PathBuf::from),
    })
}

/// How long the window is given to close before the install goes ahead anyway. The installer asks a
/// program still holding its files to close through the Restart Manager, so going ahead is safe.
const PATIENCE: Duration = Duration::from_secs(60);

/// Wait for the window, install, start the new one, and answer the exit code.
///
/// A failed install is written to the plan's note, [`failure_note`], before the old copy is started again, because this
/// helper has no window: the Unluminous that comes back reads the note at startup and says what went
/// wrong (`task-2730`). Before that, a failed install started the old copy with nothing on the screen,
/// and the only trace was a line in `apply-update.log` in the temporary folder.
pub fn run(plan: &Plan) -> i32 {
    let folder = plan.installer.parent().map(Path::to_path_buf).unwrap_or_default();
    let log_path = folder.join("apply-update.log");
    let mut log = Log::at(&log_path);
    log.say(&format!("waiting for process {} to close", plan.wait));
    let started = Instant::now();
    while crate::instances::is_running(plan.wait) && started.elapsed() < PATIENCE {
        std::thread::sleep(Duration::from_millis(200));
    }
    log.say(&format!("waited {:.1} s", started.elapsed().as_secs_f32()));
    let installed = install(plan, &folder, &mut log);
    match &installed {
        Ok(()) => {
            log.say("installed");
            if let Some(note) = &plan.note {
                let _ = std::fs::remove_file(note);
            }
        }
        Err(problem) => {
            log.say(&format!("the install failed: {problem}"));
            if let Some(note) = &plan.note {
                let sentence = failure_sentence(version_of(&folder).as_deref(), problem, &log_path);
                if let Err(written) = std::fs::write(note, sentence) {
                    log.say(&format!("could not write {}: {written}", note.display()));
                }
            }
        }
    }
    // Started whether or not the install worked: a person who pressed Install & Restart gets an
    // Unluminous back either way, and the old one is what is there when it did not.
    match relaunch(&plan.relaunch) {
        Ok(()) => log.say(&format!("started {}", plan.relaunch.display())),
        Err(problem) => log.say(&format!("could not start {}: {problem}", plan.relaunch.display())),
    }
    match installed {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// The file a failed install is written to, for the window that is started afterwards to read.
///
/// It is in the temporary folder, which on both platforms is the person's own: the helper inherits it
/// from the window, and on macOS a window started by `open` is given the same per-user folder by
/// launchd. One file and not one per version, because only the last attempt is worth saying.
pub fn failure_note() -> PathBuf {
    std::env::temp_dir().join("unluminous-update-failed.txt")
}

/// Read and remove what [`run`] wrote to `path` after a failed install, so it is said once.
pub fn take_the_failure(path: &Path) -> Option<String> {
    let note = std::fs::read_to_string(path).ok()?;
    let _ = std::fs::remove_file(path);
    let note = note.trim();
    (!note.is_empty()).then(|| note.to_owned())
}

/// The sentence the relaunched window shows.
fn failure_sentence(version: Option<&str>, problem: &str, log: &Path) -> String {
    let what = match version {
        Some(version) => format!("Unluminous {version}"),
        None => "the update".to_owned(),
    };
    format!(
        "Could not install {what}, so this is still the version you had: {problem}. The steps are in {}.",
        log.display()
    )
}

/// The version a download folder holds, from its name, `unluminous-update-<version>`.
fn version_of(folder: &Path) -> Option<String> {
    let name = folder.file_name()?.to_str()?;
    name.strip_prefix("unluminous-update-").filter(|version| !version.is_empty()).map(str::to_owned)
}

/// Run Inno Setup with no window and no questions, and wait for it.
///
/// `/VERYSILENT` shows no progress window and `/SUPPRESSMSGBOXES` answers every question with its
/// default. `/NORESTART` never restarts the machine. `/CLOSEAPPLICATIONS` closes any other Unluminous
/// window still holding a file it has to replace, through the Restart Manager, which asks the window to
/// close rather than killing it. Inno Setup remembers the folder and the choices made at the first
/// install, so the PATH entry and the right click verbs are kept.
#[cfg(windows)]
fn install(plan: &Plan, folder: &Path, log: &mut Log) -> Result<(), String> {
    let mode = match plan.all_users {
        true => "/ALLUSERS",
        false => "/CURRENTUSER",
    };
    let setup_log = folder.join("setup.log");
    let status = std::process::Command::new(&plan.installer)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", mode])
        .arg(format!("/LOG={}", setup_log.display()))
        .status()
        .map_err(|problem| problem.to_string())?;
    log.say(&format!("the installer answered {status}"));
    match status.success() {
        true => Ok(()),
        false => Err(format!("the installer answered {status}; see {}", setup_log.display())),
    }
}

/// Unzip the new bundle and swap it for the old one. `ditto` is what keeps a bundle's signature and its
/// extended attributes whole, which `unzip` does not.
#[cfg(target_os = "macos")]
fn install(plan: &Plan, folder: &Path, log: &mut Log) -> Result<(), String> {
    let unpacked = folder.join("unpacked");
    let _ = std::fs::remove_dir_all(&unpacked);
    let status = std::process::Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(&plan.installer)
        .arg(&unpacked)
        .status()
        .map_err(|problem| problem.to_string())?;
    if !status.success() {
        return Err(format!("ditto answered {status}"));
    }
    let fresh = the_one_bundle_in(&unpacked)?;
    swap_in(&fresh, &plan.relaunch, log)
}

/// The single `.app` folder the zip unpacked, whatever it is called.
///
/// It used to be looked for under the running bundle's own name, so a copy somebody renamed, such as
/// `Unluminous - company install.app` put there by a company's software portal, found nothing in a zip
/// that holds `Unluminous.app` (`task-2730`).
pub fn the_one_bundle_in(unpacked: &Path) -> Result<PathBuf, String> {
    let mut bundles = std::fs::read_dir(unpacked)
        .map_err(|problem| format!("could not read {}: {problem}", unpacked.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir() && path.extension().is_some_and(|extension| extension == "app")
        });
    match (bundles.next(), bundles.next()) {
        (Some(bundle), None) => Ok(bundle),
        _ => Err("the zip does not hold exactly one .app bundle".to_owned()),
    }
}

/// The prefix of the name the old bundle is moved to, beside itself, while the new one takes its place.
const PARKED_PREFIX: &str = ".unluminous-previous-";

/// Put `fresh` where `target` is, and try to delete the old copy.
///
/// **The old copy is renamed inside the folder it is in, never moved to another folder** (`task-2730`).
/// On macOS a folder moved to a different parent has its own `..` entry rewritten, which needs write
/// permission on the folder being moved. A bundle a company's software portal installed is owned by
/// root with mode `drwxr-x---`, so moving it into the temporary folder failed with `EACCES`. A rename
/// inside one folder needs write permission on that folder only, and `/Applications` is `root:admin`
/// with mode `drwxrwxr-x`, so an administrator's account can rename a bundle owned by root there.
///
/// The parked name starts with a dot and has no `.app` extension, so LaunchServices does not list it as
/// a second Unluminous. A bundle owned by root cannot be deleted without administrator rights, so a
/// failed delete leaves it and says so in the log. `remove_dir_all` does not follow symbolic links, and
/// a parked symbolic link is removed as a link, so the delete cannot reach outside the folder.
pub fn swap_in(fresh: &Path, target: &Path, log: &mut Log) -> Result<(), String> {
    let parent = target.parent().ok_or("the bundle has no folder")?;
    forget_older_parked_copies(parent, log);
    let parked = parent.join(format!("{PARKED_PREFIX}{}", std::process::id()));
    let _ = remove_parked(&parked);
    std::fs::rename(target, &parked)
        .map_err(|problem| format!("could not move {} aside: {problem}", target.display()))?;
    if let Err(problem) = std::fs::rename(fresh, target) {
        // Put the old one back, so the relaunch still finds an Unluminous.
        let _ = std::fs::rename(&parked, target);
        return Err(format!("could not move the new bundle to {}: {problem}", target.display()));
    }
    log.say("swapped the bundle");
    if let Err(problem) = remove_parked(&parked) {
        log.say(&format!("left the old bundle at {}: {problem}", parked.display()));
    }
    Ok(())
}

/// Delete what an earlier update parked and could not delete, which a later attempt may be able to.
fn forget_older_parked_copies(parent: &Path, log: &mut Log) {
    let Ok(entries) = std::fs::read_dir(parent) else { return };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(PARKED_PREFIX) {
            continue;
        }
        if let Err(problem) = remove_parked(&entry.path()) {
            log.say(&format!("left an older bundle at {}: {problem}", entry.path().display()));
        }
    }
}

/// Remove a parked bundle without following a symbolic link out of its folder.
fn remove_parked(parked: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(parked) {
        Err(problem) if problem.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(problem) => Err(problem),
        Ok(found) if found.file_type().is_symlink() || found.is_file() => {
            std::fs::remove_file(parked)
        }
        Ok(_) => std::fs::remove_dir_all(parked),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn install(_plan: &Plan, _folder: &Path, _log: &mut Log) -> Result<(), String> {
    Err("there is no installer to run on this platform".to_owned())
}

/// Start the new Unluminous with no project named, from the folder it lives in.
///
/// Started that way, Unluminous opens the windows that were open in the last session, which is
/// `unluminous_app::starting_folder`'s own rule, so what comes back is what was there.
fn relaunch(target: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("/usr/bin/open");
        command.arg(target);
        command
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = std::process::Command::new(target);
    if let Some(folder) = target.parent() {
        command.current_dir(folder);
    }
    command.spawn().map(|_| ()).map_err(|problem| problem.to_string())
}

/// A log that is appended to and never fails the work it describes.
pub struct Log {
    file: Option<std::fs::File>,
}

impl Log {
    /// A log appended to `path`, or one that writes nowhere when the file cannot be opened.
    pub fn at(path: &Path) -> Self {
        Self { file: std::fs::OpenOptions::new().create(true).append(true).open(path).ok() }
    }

    /// Write one line.
    pub fn say(&mut self, line: &str) {
        if let Some(file) = self.file.as_mut() {
            let _ = writeln!(file, "{line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_is_read_back_from_the_command_line_it_builds() {
        let plan = Plan {
            installer: PathBuf::from(
                "C:/temp/unluminous-update-9.9.9/UnluminousSetup-9.9.9-x64.exe",
            ),
            wait: 4242,
            relaunch: PathBuf::from("C:/Users/me/AppData/Local/Programs/Unluminous/unluminous.exe"),
            all_users: true,
            note: Some(PathBuf::from("C:/temp/unluminous-update-failed.txt")),
        };
        assert_eq!(asked_for(&command_line(&plan)), Some(plan.clone()));
        let silent = Plan { note: None, ..plan.clone() };
        assert_eq!(asked_for(&command_line(&silent)), Some(silent));
        let per_user = Plan { all_users: false, ..plan };
        assert_eq!(asked_for(&command_line(&per_user)), Some(per_user));
    }

    #[test]
    fn only_a_line_that_starts_with_the_switch_is_a_request() {
        let words = |line: &str| line.split(' ').map(str::to_owned).collect::<Vec<String>>();
        assert_eq!(asked_for(&words("status --json")), None);
        assert_eq!(asked_for(&words("tab open --apply-update x --wait 1 --relaunch y")), None);
        assert_eq!(
            asked_for(&words("--apply-update x --relaunch y")),
            None,
            "no process to wait for"
        );
    }

    /// The whole helper against a fake installer: it waits for a process that is already gone, runs the
    /// installer, and starts the relaunch target. Windows only, where the installer is a program.
    #[cfg(windows)]
    #[test]
    fn it_installs_and_starts_the_new_one() {
        let folder = std::env::temp_dir().join(format!("unluminous-apply-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");
        // A batch file cannot be the installer, because `Command` refuses a batch file with arguments it
        // cannot escape. `cmd.exe` copied under another name is a real program that ignores Inno's
        // switches and exits 0, which is all the installer's half needs to be here.
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| "C:/Windows".to_owned());
        let cmd = PathBuf::from(system).join("System32").join("where.exe");
        let installer = folder.join("setup.exe");
        std::fs::copy(&cmd, &installer).expect("a program to stand in for the installer");
        let relaunch = folder.join("relaunched.exe");
        std::fs::copy(&cmd, &relaunch).expect("a program to stand in for Unluminous");
        let note = folder.join("failed.txt");
        let plan = Plan {
            installer,
            wait: u32::MAX - 1,
            relaunch,
            all_users: false,
            note: Some(note.clone()),
        };
        let code = run(&plan);
        let log = std::fs::read_to_string(folder.join("apply-update.log")).expect("a log");
        // A failed install leaves a sentence for the window that is started afterwards, and taking it
        // removes it, so it is said once.
        if code != 0 {
            let said = take_the_failure(&note).expect("a failed install leaves a note");
            assert!(said.starts_with("Could not install"), "{said}");
            assert!(said.contains("apply-update.log"), "{said}");
            assert!(take_the_failure(&note).is_none(), "the note is said once");
        }
        assert!(log.contains("the installer answered"), "{log}");
        assert!(log.contains("started"), "{log}");
        // `where.exe` with Inno's switches exits non-zero, which the helper reports rather than hides.
        assert!(code == 0 || log.contains("the install failed"), "{log}");
        // The stand-in that was relaunched may still be running for a moment, and a program that is
        // running cannot be deleted on Windows, so its file is asked for again until it goes.
        for name in ["setup.exe", "relaunched.exe", "apply-update.log", "setup.log", "failed.txt"] {
            for _ in 0..50 {
                let path = folder.join(name);
                if !path.exists() || std::fs::remove_file(&path).is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        let _ = std::fs::remove_dir(&folder);
    }

    /// A scratch folder of its own for one test.
    fn scratch(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("unluminous-apply-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");
        folder
    }

    #[test]
    fn the_bundle_in_the_zip_is_found_whatever_it_is_called() {
        let folder = scratch("one-bundle");
        std::fs::create_dir_all(folder.join("Unluminous.app/Contents")).expect("a bundle");
        std::fs::write(folder.join("readme.txt"), "x").expect("a file");
        assert_eq!(the_one_bundle_in(&folder), Ok(folder.join("Unluminous.app")));
        std::fs::create_dir_all(folder.join("Other.app")).expect("a second bundle");
        assert!(the_one_bundle_in(&folder).is_err(), "two bundles is not one");
        let _ = std::fs::remove_dir_all(&folder);
        let empty = scratch("no-bundle");
        assert!(the_one_bundle_in(&empty).is_err(), "no bundle is not one");
        let _ = std::fs::remove_dir_all(&empty);
    }

    /// The new bundle takes the old one's name, even a renamed one, and the old copy is parked inside
    /// the same folder and then deleted, so nothing is moved to another folder.
    #[test]
    fn the_new_bundle_replaces_a_renamed_one_and_the_old_copy_is_parked_beside_it() {
        let folder = scratch("swap");
        let applications = folder.join("Applications");
        let target = applications.join("Unluminous - company install.app");
        std::fs::create_dir_all(target.join("Contents")).expect("the old bundle");
        std::fs::write(target.join("Contents/old"), "old").expect("an old file");
        // A copy an earlier update could not delete.
        std::fs::create_dir_all(applications.join(format!("{PARKED_PREFIX}1")))
            .expect("a leftover");
        let unpacked = folder.join("unpacked");
        let fresh = unpacked.join("Unluminous.app");
        std::fs::create_dir_all(fresh.join("Contents")).expect("the new bundle");
        std::fs::write(fresh.join("Contents/new"), "new").expect("a new file");
        let mut log = Log::at(&folder.join("log.txt"));
        swap_in(&the_one_bundle_in(&unpacked).expect("one bundle"), &target, &mut log)
            .expect("the swap works");
        assert!(target.join("Contents/new").exists(), "the new bundle is in place");
        assert!(!target.join("Contents/old").exists(), "the old one is not");
        let left: Vec<String> = std::fs::read_dir(&applications)
            .expect("the folder")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            left,
            vec!["Unluminous - company install.app".to_owned()],
            "nothing parked is left"
        );
        let said = std::fs::read_to_string(folder.join("log.txt")).expect("the log");
        assert!(said.contains("swapped the bundle"), "{said}");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_swap_that_cannot_place_the_new_bundle_puts_the_old_one_back() {
        let folder = scratch("swap-back");
        let target = folder.join("Unluminous.app");
        std::fs::create_dir_all(&target).expect("the old bundle");
        std::fs::write(target.join("old"), "old").expect("an old file");
        let mut log = Log::at(&folder.join("log.txt"));
        let missing = folder.join("unpacked/Unluminous.app");
        let refused = swap_in(&missing, &target, &mut log).expect_err("there is nothing to move");
        assert!(refused.contains("could not move the new bundle"), "{refused}");
        assert!(target.join("old").exists(), "the old bundle is back where it was");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_failure_sentence_names_the_version_and_the_log() {
        assert_eq!(
            version_of(Path::new("/tmp/unluminous-update-1.31.0")).as_deref(),
            Some("1.31.0")
        );
        assert_eq!(version_of(Path::new("/tmp/elsewhere")), None);
        let said = failure_sentence(Some("1.31.0"), "permission denied", Path::new("/tmp/a.log"));
        assert!(said.contains("Unluminous 1.31.0") && said.contains("/tmp/a.log"), "{said}");
    }
}
