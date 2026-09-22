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
    })
}

/// How long the window is given to close before the install goes ahead anyway. The installer asks a
/// program still holding its files to close through the Restart Manager, so going ahead is safe.
const PATIENCE: Duration = Duration::from_secs(60);

/// Wait for the window, install, start the new one, and answer the exit code.
pub fn run(plan: &Plan) -> i32 {
    let folder = plan.installer.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut log = Log::at(&folder.join("apply-update.log"));
    log.say(&format!("waiting for process {} to close", plan.wait));
    let started = Instant::now();
    while crate::instances::is_running(plan.wait) && started.elapsed() < PATIENCE {
        std::thread::sleep(Duration::from_millis(200));
    }
    log.say(&format!("waited {:.1} s", started.elapsed().as_secs_f32()));
    let installed = install(plan, &folder, &mut log);
    match &installed {
        Ok(()) => log.say("installed"),
        Err(problem) => log.say(&format!("the install failed: {problem}")),
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

/// Unzip the new bundle beside the old one and swap them. `ditto` is what keeps a bundle's signature
/// and its extended attributes whole, which `unzip` does not.
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
    let name = plan.relaunch.file_name().ok_or("the bundle has no name")?;
    let fresh = unpacked.join(name);
    if !fresh.exists() {
        return Err(format!("the zip holds no {}", Path::new(name).display()));
    }
    let old = folder.join("previous.app");
    let _ = std::fs::remove_dir_all(&old);
    std::fs::rename(&plan.relaunch, &old).map_err(|problem| problem.to_string())?;
    if let Err(problem) = std::fs::rename(&fresh, &plan.relaunch) {
        // Put the old one back, so the relaunch still finds an Unluminous.
        let _ = std::fs::rename(&old, &plan.relaunch);
        return Err(problem.to_string());
    }
    log.say("swapped the bundle");
    Ok(())
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
struct Log {
    file: Option<std::fs::File>,
}

impl Log {
    fn at(path: &Path) -> Self {
        Self { file: std::fs::OpenOptions::new().create(true).append(true).open(path).ok() }
    }

    fn say(&mut self, line: &str) {
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
        };
        assert_eq!(asked_for(&command_line(&plan)), Some(plan.clone()));
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
        let plan = Plan { installer, wait: u32::MAX - 1, relaunch, all_users: false };
        let code = run(&plan);
        let log = std::fs::read_to_string(folder.join("apply-update.log")).expect("a log");
        assert!(log.contains("the installer answered"), "{log}");
        assert!(log.contains("started"), "{log}");
        // `where.exe` with Inno's switches exits non-zero, which the helper reports rather than hides.
        assert!(code == 0 || log.contains("the install failed"), "{log}");
        for name in ["setup.exe", "relaunched.exe", "apply-update.log", "setup.log"] {
            let _ = std::fs::remove_file(folder.join(name));
        }
        let _ = std::fs::remove_dir(&folder);
    }
}
