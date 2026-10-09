//! Finding the Python that will run a kernel, and asking it questions before a kernel exists.
//!
//! A notebook needs a Python that has `jupyter_client` (which runs the kernel) and `ipykernel` (which
//! is the Python kernel). Machines have several Pythons: one in the project's virtual environment, one
//! from conda, one the operating system came with. The editor lists the candidates it can find, asks
//! each one directly which of the two packages it has, and lets the person choose. Each question runs
//! `python -c` with a short script, on its own thread, with a time limit, because a Python that hangs
//! (the Windows Store alias that opens a shop window is one) must not stop the others being found.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::KernelSpec;

/// How long a question to a Python may take before the Python is given up on.
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// How long listing the kernelspecs may take. It imports `jupyter_client`, which is slow on a cold disk
/// and slower still on a busy machine, where 30 seconds was measured not to be enough.
const SPECS_TIMEOUT: Duration = Duration::from_secs(60);

/// The script that reports a Python's version and which of the two packages it can import.
const PROBE_SCRIPT: &str = "import sys,json,importlib.util as u;print(json.dumps({'version':sys.version.split()[0],'ipykernel':u.find_spec('ipykernel') is not None,'jupyter_client':u.find_spec('jupyter_client') is not None}))";

/// The script that lists the installed kernelspecs.
const SPECS_SCRIPT: &str = "import json;from jupyter_client.kernelspec import KernelSpecManager as M;s=M().get_all_specs();print(json.dumps([{'name':n,'display_name':v['spec'].get('display_name',n),'language':v['spec'].get('language','')} for n,v in s.items()]))";

/// A Python interpreter that was found and asked about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Python {
    /// The interpreter's program.
    pub path: PathBuf,
    /// The version, such as `3.13.11`.
    pub version: String,
    /// Whether `ipykernel`, the Python kernel, can be imported.
    pub has_ipykernel: bool,
    /// Whether `jupyter_client`, which the editor uses to run a kernel, can be imported.
    pub has_jupyter_client: bool,
    /// Where it was found, such as `project .venv` or `PATH`.
    pub found_by: String,
}

/// A command for `program` that opens no console window on Windows.
pub(super) fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW, so a notebook does not flash a console for each Python it starts.
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Stop a process by id. Used only for a kernel the bridge could not stop itself.
pub(super) fn kill_process(pid: u32) {
    #[cfg(target_os = "windows")]
    let mut command = command(Path::new("taskkill"));
    #[cfg(target_os = "windows")]
    command.args(["/PID", &pid.to_string(), "/F", "/T"]);
    #[cfg(not(target_os = "windows"))]
    let mut command = Command::new("kill");
    #[cfg(not(target_os = "windows"))]
    command.args(["-9", &pid.to_string()]);
    let _ = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

/// What a finished program printed, and whether it succeeded.
struct Output {
    stdout: String,
    stderr: String,
    success: bool,
}

/// Run a command to its end, or stop it after `limit`. Output is read on threads so a full pipe cannot stall it.
fn run_with_limit(mut command: Command, limit: Duration) -> Result<Output, String> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|problem| problem.to_string())?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("did not answer within {} seconds", limit.as_secs()));
            }
            Err(problem) => return Err(problem.to_string()),
        }
    };
    let text = |handle: std::thread::JoinHandle<String>| handle.join().unwrap_or_default();
    Ok(Output { stdout: text(out), stderr: text(err), success: status.success() })
}

/// Read a pipe to its end on a new thread.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Ask a Python which version it is and whether it has `ipykernel` and `jupyter_client`. `None` when
/// it does not run, does not answer within fifteen seconds, or answers something else.
pub fn probe(path: &Path) -> Option<Python> {
    let mut asked = command(path);
    asked.args(["-c", PROBE_SCRIPT]).current_dir(std::env::temp_dir());
    let output = run_with_limit(asked, PROBE_TIMEOUT).ok().filter(|output| output.success)?;
    let line = output.stdout.lines().rev().find(|line| line.trim_start().starts_with('{'))?;
    let answer: Value = serde_json::from_str(line).ok()?;
    Some(Python {
        path: path.to_path_buf(),
        version: answer["version"].as_str()?.to_owned(),
        has_ipykernel: answer["ipykernel"].as_bool().unwrap_or(false),
        has_jupyter_client: answer["jupyter_client"].as_bool().unwrap_or(false),
        found_by: "given".to_owned(),
    })
}

/// Every Python that can be found, best candidate first.
///
/// In order: the project's `.venv`, `venv`, `env` and `.env` folders, `$VIRTUAL_ENV`, `$CONDA_PREFIX`,
/// `python3` and `python` on `PATH`, the `py` launcher on Windows, and the `miniconda3` and
/// `anaconda3` folders in the home directory. The same interpreter found twice is listed once, under
/// the first way it was found. Each is asked about itself on its own thread.
pub fn find_pythons(project: Option<&Path>) -> Vec<Python> {
    let mut seen = Vec::new();
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();
    for (path, found_by) in candidate_paths(project) {
        let Ok(canonical) = path.canonicalize() else { continue };
        if !seen.contains(&canonical) {
            seen.push(canonical);
            candidates.push((path, found_by));
        }
    }
    let handles: Vec<_> = candidates
        .into_iter()
        .map(|(path, found_by)| {
            std::thread::spawn(move || probe(&path).map(|python| Python { found_by, ..python }))
        })
        .collect();
    handles.into_iter().filter_map(|handle| handle.join().ok().flatten()).collect()
}

/// Every place a Python might be, with how it was found, before any is run.
fn candidate_paths(project: Option<&Path>) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    if let Some(project) = project {
        for folder in [".venv", "venv", "env", ".env"] {
            found.push((environment_python(&project.join(folder)), format!("project {folder}")));
        }
    }
    for variable in ["VIRTUAL_ENV", "CONDA_PREFIX"] {
        if let Some(root) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            found.push((environment_python(Path::new(&root)), format!("${variable}")));
        }
    }
    for name in ["python3", "python"] {
        found.extend(on_path(name).into_iter().map(|path| (path, "PATH".to_owned())));
    }
    if let Some(path) = from_py_launcher() {
        found.push((path, "py launcher".to_owned()));
    }
    if let Some(home) = home_folder() {
        for folder in ["miniconda3", "anaconda3"] {
            found.push((environment_python(&home.join(folder)), folder.to_owned()));
        }
    }
    found
}

/// The interpreter inside a virtual environment or conda folder.
fn environment_python(root: &Path) -> PathBuf {
    if cfg!(windows) {
        let scripts = root.join("Scripts").join("python.exe");
        if scripts.is_file() {
            scripts
        } else {
            root.join("python.exe")
        }
    } else {
        root.join("bin").join("python")
    }
}

/// The user's home folder.
fn home_folder() -> Option<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(variable).map(PathBuf::from)
}

/// Programs called `name` in each `PATH` folder, skipping the Windows Store alias.
fn on_path(name: &str) -> Vec<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    let folders = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&folders)
        .map(|folder| folder.join(&file))
        .filter(|path| path.is_file() && !is_store_alias(path))
        .collect()
}

/// Whether a path is the Windows Store's empty `python.exe` alias, which opens the Store instead of running.
fn is_store_alias(path: &Path) -> bool {
    let in_store_folder = path.to_string_lossy().to_lowercase().contains("windowsapps");
    in_store_folder && std::fs::metadata(path).map(|meta| meta.len() == 0).unwrap_or(true)
}

/// The Python the `py` launcher would choose, on Windows.
fn from_py_launcher() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let mut asked = command(Path::new("py"));
    asked.args(["-3", "-c", "import sys;print(sys.executable)"]);
    let output = run_with_limit(asked, PROBE_TIMEOUT).ok().filter(|output| output.success)?;
    let path = PathBuf::from(output.stdout.lines().last()?.trim());
    path.is_file().then_some(path)
}

/// The command that installs `ipykernel` into `python`.
pub fn install_command(python: &Path) -> Vec<String> {
    vec![
        python.to_string_lossy().into_owned(),
        "-m".to_owned(),
        "pip".to_owned(),
        "install".to_owned(),
        "ipykernel".to_owned(),
    ]
}

/// The kernels installed for `python`, without starting a bridge. The reason is the Python's own
/// error text when it has no `jupyter_client` or does not answer within thirty seconds.
pub fn list_kernelspecs(python: &Path) -> Result<Vec<KernelSpec>, String> {
    let mut asked = command(python);
    asked.args(["-c", SPECS_SCRIPT]).current_dir(std::env::temp_dir());
    let output = run_with_limit(asked, SPECS_TIMEOUT)?;
    if !output.success {
        return Err(output.stderr.lines().last().unwrap_or("the Python failed").to_owned());
    }
    let line = output
        .stdout
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('['))
        .ok_or("the Python listed no kernels")?;
    let value: Value = serde_json::from_str(line).map_err(|problem| problem.to_string())?;
    Ok(specs_of(&value))
}

/// Kernelspecs from a JSON array of `{name, display_name, language}` objects.
pub(super) fn specs_of(value: &Value) -> Vec<KernelSpec> {
    let text = |row: &Value, key: &str| row[key].as_str().unwrap_or_default().to_owned();
    let rows = value.as_array().cloned().unwrap_or_default();
    rows.iter()
        .map(|row| KernelSpec {
            name: text(row, "name"),
            display_name: text(row, "display_name"),
            language: text(row, "language"),
        })
        .collect()
}
