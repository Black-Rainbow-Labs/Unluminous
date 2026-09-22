//! Downloading a newer Unluminous, checking it, and handing over to the helper that installs it.
//!
//! `task-2063`: *"Check for updates should provide me an option to install & restart."* Three steps,
//! each one refusing rather than guessing:
//!
//! 1. **Which file.** unluminous.com's manifest names the installer for each platform with its size and
//!    its SHA-256 (`update::Download`). An answer that came from GitHub names none, so the manifest is
//!    asked for directly. A platform the manifest names nothing for has nothing to install.
//! 2. **Download and check.** The file goes to `unluminous-update-<version>` in the temporary folder, and
//!    its length and its SHA-256 are compared with the manifest's. A file that does not match is deleted
//!    and the install stops with a sentence saying so. **Nothing unchecked is ever run.**
//! 3. **Hand over.** [`hand_over`] starts `unluminous-cli --apply-update`, from a copy in the download
//!    folder, and the window then closes the ordinary way. See `unluminous_cli::apply_update`.
//!
//! Steps 1 and 2 run on a thread, arranged as `update::Check` is, because a download on the drawing
//! thread would stop the window drawing for as long as it took.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::services::update::{self, Download};

/// How long a download is given. The installer is about 13 MB and the macOS zip about 33.
const TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Where an install has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Asking the manifest which file to download.
    Asking,
    /// Downloading, with how many bytes have arrived and how many there are.
    Downloading { done: u64, total: u64 },
    /// Downloaded and checked, and waiting for the window to hand over.
    Ready { installer: PathBuf },
    /// It stopped, and this says why.
    Failed(String),
}

impl Progress {
    /// One sentence for the status bar, the About box and `update status`.
    pub fn sentence(&self, version: &str) -> String {
        match self {
            Progress::Asking => format!("Finding the installer for Unluminous {version}..."),
            Progress::Downloading { done, total } => format!(
                "Downloading Unluminous {version}: {:.1} of {:.1} MB",
                *done as f64 / 1_048_576.0,
                *total as f64 / 1_048_576.0
            ),
            Progress::Ready { .. } => format!("Unluminous {version} is downloaded and checked."),
            Progress::Failed(problem) => {
                format!("Could not install Unluminous {version}: {problem}")
            }
        }
    }

    /// The name `update status` gives it.
    pub fn name(&self) -> &'static str {
        match self {
            Progress::Asking => "asking",
            Progress::Downloading { .. } => "downloading",
            Progress::Ready { .. } => "ready",
            Progress::Failed(_) => "failed",
        }
    }
}

/// An install running on a thread.
pub struct Install {
    pub version: String,
    /// Whether the window closes and the helper takes over once it is ready. False is
    /// `update install --no-restart`, which downloads and checks and stops there.
    pub restart: bool,
    updates: Receiver<Progress>,
    latest: Progress,
}

impl Install {
    /// Start downloading `version`. `download` is what the answer already said, when it said.
    pub fn start(
        version: String,
        download: Option<Download>,
        restart: bool,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, updates) = std::sync::mpsc::channel();
        let folder = folder_for(&version);
        let manifest = manifest_address();
        std::thread::Builder::new()
            .name("unluminous-update-install".to_owned())
            .spawn(move || {
                let tell = |progress: Progress| {
                    let _ = sender.send(progress);
                    wake();
                };
                let download = match download {
                    Some(download) => Some(download),
                    None => {
                        tell(Progress::Asking);
                        installer_from_the_manifest(&manifest)
                    }
                };
                let Some(download) = download else {
                    tell(Progress::Failed(
                        "unluminous.com names no installer for this platform".to_owned(),
                    ));
                    return;
                };
                let finished = fetch_and_check(&download, &folder, &|done, total| {
                    tell(Progress::Downloading { done, total })
                });
                tell(match finished {
                    Ok(installer) => Progress::Ready { installer },
                    Err(problem) => Progress::Failed(problem),
                });
            })
            .ok();
        Self { version, restart, updates, latest: Progress::Asking }
    }

    /// Take in whatever the thread has said, and answer where it has got to.
    pub fn poll(&mut self) -> &Progress {
        while let Ok(progress) = self.updates.try_recv() {
            self.latest = progress;
        }
        &self.latest
    }

    /// Where it had got to at the last [`Self::poll`].
    pub fn progress(&self) -> &Progress {
        &self.latest
    }

    /// True until it is ready or has failed.
    pub fn is_running(&self) -> bool {
        matches!(self.latest, Progress::Asking | Progress::Downloading { .. })
    }
}

/// The folder a version is downloaded into.
pub fn folder_for(version: &str) -> PathBuf {
    std::env::temp_dir().join(format!("unluminous-update-{version}"))
}

/// The manifest's address, which a test points at a scripted server through `UNLUMINOUS_RELEASES`.
fn manifest_address() -> String {
    match std::env::var("UNLUMINOUS_RELEASES") {
        Ok(named) => named.split(',').next().unwrap_or(update::MANIFEST).trim().to_owned(),
        Err(_) => update::MANIFEST.to_owned(),
    }
}

/// Ask the manifest which installer is this platform's.
fn installer_from_the_manifest(address: &str) -> Option<Download> {
    let body = agent(update::TIMEOUT)
        .get(address)
        .header("user-agent", "Unluminous")
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let value: serde_json::Value = serde_json::from_str(&body).ok()?;
    update::download_in(&value, update::PLATFORM_FIELD)
}

/// The same client `update` asks with, with a longer wait for a file of megabytes.
fn agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .tls_config(unluminous_chat::client::tls_config())
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .build();
    ureq::Agent::new_with_config(config)
}

/// Download `download` into `folder`, and answer the file once its length and hash have been checked.
///
/// A file that does not match is deleted before the refusal is returned, so nothing that failed the
/// check is left where a later step could find it.
pub fn fetch_and_check(
    download: &Download,
    folder: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|problem| problem.to_string())?;
    let name = download
        .url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty() && !name.contains(['\\', ':']))
        .unwrap_or("installer");
    let path = folder.join(name);
    let result = fetch_into(download, &path, progress);
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
    }
    result.map(|()| path)
}

fn fetch_into(download: &Download, path: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
    let mut reply = agent(TIMEOUT)
        .get(&download.url)
        .header("user-agent", "Unluminous")
        .call()
        .map_err(|problem| problem.to_string())?;
    let mut body = reply.body_mut().with_config().limit(download.bytes + 1).reader();
    let mut file = std::fs::File::create(path).map_err(|problem| problem.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let read = body.read(&mut buffer).map_err(|problem| problem.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|problem| problem.to_string())?;
        done += read as u64;
        progress(done, download.bytes);
    }
    file.flush().map_err(|problem| problem.to_string())?;
    if done != download.bytes {
        return Err(format!(
            "the download was {done} bytes and unluminous.com says it is {}, so it was not run",
            download.bytes
        ));
    }
    let hash: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    if hash != download.sha256 {
        return Err(format!(
            "the download's SHA-256 is {hash} and unluminous.com says it is {}, so it was not run",
            download.sha256
        ));
    }
    Ok(())
}

/// Where the running Unluminous is installed, when an installer put it there.
///
/// On Windows that is the folder holding `unluminous.exe` when the uninstaller Inno Setup writes is
/// beside it. On macOS it is the `.app` bundle the executable is inside. A build run from
/// `target/release` is neither, and the answer then is a sentence saying why it cannot be updated in
/// place, because installing would put the new version somewhere else and start that one instead.
pub fn installed_at() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|problem| problem.to_string())?;
    if cfg!(windows) {
        let folder = exe.parent().ok_or("the program has no folder")?;
        return match folder.join("unins000.exe").exists() {
            true => Ok(exe),
            false => Err(format!(
                "this Unluminous is running from {}, which the installer did not put there",
                folder.display()
            )),
        };
    }
    if cfg!(target_os = "macos") {
        return exe
            .ancestors()
            .find(|path| path.extension().is_some_and(|extension| extension == "app"))
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("{} is not inside an application bundle", exe.display()));
    }
    Err("there is no installer to run on this platform".to_owned())
}

/// Start the helper that installs `installer` once this process has gone, and starts `relaunch`.
///
/// The window closes itself afterwards, the ordinary way, so what the project remembers is written
/// exactly as closing it by hand writes it.
pub fn hand_over(installer: &Path, relaunch: &Path) -> Result<(), String> {
    let folder = installer.parent().ok_or("the installer has no folder")?;
    let here = std::env::current_exe().map_err(|problem| problem.to_string())?;
    let cli_name = if cfg!(windows) { "unluminous-cli.exe" } else { "unluminous-cli" };
    let cli = here.with_file_name(cli_name);
    let copy = folder.join(cli_name);
    std::fs::copy(&cli, &copy)
        .map_err(|problem| format!("could not copy {}: {problem}", cli.display()))?;
    let all_users = cfg!(windows) && is_under_program_files(relaunch);
    let plan = unluminous_cli::apply_update::Plan {
        installer: installer.to_path_buf(),
        wait: std::process::id(),
        relaunch: relaunch.to_path_buf(),
        all_users,
    };
    let mut command = std::process::Command::new(&copy);
    command.args(unluminous_cli::apply_update::command_line(&plan)).current_dir(folder);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window, and not tied to this process's console or job.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        command.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB);
    }
    match command.spawn() {
        Ok(_) => Ok(()),
        // A job that forbids breaking away refuses the flag, so it is asked again without it.
        #[cfg(windows)]
        Err(_) => {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000 | 0x0000_0008);
            command.spawn().map(|_| ()).map_err(|problem| problem.to_string())
        }
        #[cfg(not(windows))]
        Err(problem) => Err(problem.to_string()),
    }
}

/// Whether a path is inside `Program Files`, which is where an install for every user goes.
fn is_under_program_files(path: &Path) -> bool {
    ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"].iter().any(|name| {
        std::env::var_os(name).is_some_and(|folder| {
            let folder = folder.to_string_lossy().to_lowercase();
            !folder.is_empty() && path.to_string_lossy().to_lowercase().starts_with(&folder)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// One request answered with `body`, and the address to ask it at.
    fn scripted(body: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let address = listener.local_addr().expect("the address");
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        });
        format!("http://{address}/downloads/UnluminousSetup-9.9.9-x64.exe")
    }

    fn folder(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("unluminous-install-test-{name}-{}", std::process::id()))
    }

    #[test]
    fn a_download_that_matches_is_kept() {
        let body = b"an installer".to_vec();
        let download = Download { url: scripted(body.clone()), bytes: 12, sha256: sha(&body) };
        let place = folder("good");
        let seen = std::sync::Mutex::new(0u64);
        let path = fetch_and_check(&download, &place, &|done, _| *seen.lock().unwrap() = done)
            .expect("it matches");
        assert_eq!(std::fs::read(&path).expect("the file"), body);
        assert_eq!(*seen.lock().unwrap(), 12, "progress reached the end");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&place);
    }

    #[test]
    fn a_download_with_the_wrong_hash_is_deleted_and_refused() {
        let body = b"an installer".to_vec();
        let download = Download { url: scripted(body), bytes: 12, sha256: "0".repeat(64) };
        let place = folder("hash");
        let refused = fetch_and_check(&download, &place, &|_, _| {}).expect_err("the hash differs");
        assert!(refused.contains("SHA-256"), "{refused}");
        assert!(!place.join("UnluminousSetup-9.9.9-x64.exe").exists(), "nothing unchecked is left");
        let _ = std::fs::remove_dir(&place);
    }

    #[test]
    fn a_download_of_the_wrong_length_is_deleted_and_refused() {
        let body = b"an installer".to_vec();
        let download = Download { url: scripted(body.clone()), bytes: 99, sha256: sha(&body) };
        let place = folder("length");
        let refused = fetch_and_check(&download, &place, &|_, _| {}).expect_err("too short");
        assert!(refused.contains("bytes"), "{refused}");
        assert!(!place.join("UnluminousSetup-9.9.9-x64.exe").exists());
        let _ = std::fs::remove_dir(&place);
    }

    #[test]
    fn a_download_folder_names_its_version() {
        assert!(folder_for("0.55.0").ends_with("unluminous-update-0.55.0"));
    }
}
