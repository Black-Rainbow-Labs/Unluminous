//! Finding the program for a server, where the toolchain puts it. Nothing is downloaded.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{Adapter, ServerSpec};

/// How long a candidate has to answer `--version`.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// Finds the program to start for an adapter in a project, or says why there is none.
///
/// `Err` is the reason for `ServerState::Absent`: the command that would install the server, or what
/// was looked for.
///
/// @param adapter - which protocol the language's server speaks
/// @param root - the project folder
/// @param file - the file that was opened, where the search for a project's own copy starts
/// @param command - a manifest's `language.server_command`, or `None`
/// @param args - a manifest's `language.server_args`
pub fn find_program(
    adapter: Adapter,
    root: &Path,
    file: &Path,
    command: Option<&str>,
    args: &[String],
) -> Result<ServerSpec, String> {
    match adapter {
        Adapter::Lsp => find_lsp(root, command, args),
        Adapter::TsServer => find_tsserver(root, file, command, args),
    }
}

/// rust-analyzer: the manifest's command, else the toolchain's own, else one on `PATH`.
fn find_lsp(root: &Path, command: Option<&str>, args: &[String]) -> Result<ServerSpec, String> {
    let program = match command {
        Some(command) => resolve_command(command)?,
        None => {
            rust_analyzer(root).ok_or_else(|| "rustup component add rust-analyzer".to_owned())?
        }
    };
    let label = match command {
        Some(_) => program
            .file_stem()
            .map_or_else(|| "language server".to_owned(), |s| s.to_string_lossy().into_owned()),
        None => "rust-analyzer".to_owned(),
    };
    Ok(ServerSpec {
        adapter: Adapter::Lsp,
        root: root.to_path_buf(),
        program,
        args: args.to_vec(),
        label,
    })
}

/// tsserver: `node` running the nearest `node_modules/typescript/lib/tsserver.js`.
fn find_tsserver(
    root: &Path,
    file: &Path,
    command: Option<&str>,
    args: &[String],
) -> Result<ServerSpec, String> {
    let script = nearest_tsserver(file)
        .or_else(global_tsserver)
        .ok_or_else(|| "no typescript in node_modules".to_owned())?;
    let node = match command {
        Some(command) => resolve_command(command)?,
        None => on_path("node").ok_or_else(|| "node is not on PATH".to_owned())?,
    };
    let mut all = vec![
        script.to_string_lossy().into_owned(),
        "--disableAutomaticTypingAcquisition".to_owned(),
        "--suppressDiagnosticEvents".to_owned(),
    ];
    all.extend(args.iter().cloned());
    Ok(ServerSpec {
        adapter: Adapter::TsServer,
        root: root.to_path_buf(),
        program: node,
        args: all,
        label: "tsserver".to_owned(),
    })
}

/// A manifest's command: a path that exists, or a name found on `PATH`.
fn resolve_command(command: &str) -> Result<PathBuf, String> {
    let path = Path::new(command);
    if path.components().count() > 1 || path.is_absolute() {
        return if path.is_file() {
            Ok(path.to_path_buf())
        } else {
            Err(format!("{command} does not exist"))
        };
    }
    on_path(command).ok_or_else(|| format!("{command} is not on PATH"))
}

/// rust-analyzer that actually runs: `rustup which` in the project (which reads `rust-toolchain.toml`),
/// else one on `PATH`. The `rustup` proxy in `~/.cargo/bin` fails when the component is not installed,
/// so every candidate is run with `--version` before it is believed.
fn rust_analyzer(root: &Path) -> Option<PathBuf> {
    let from_rustup = rustup_which(root);
    from_rustup
        .filter(|p| runs(p, root))
        .or_else(|| on_path("rust-analyzer").filter(|p| runs(p, root)))
        .or_else(|| another_toolchains(root))
}

/// rust-analyzer from another installed toolchain, the newest first, when the project's own toolchain
/// has no rust-analyzer component. A server from a different toolchain still reads the project, and
/// none at all answers nothing: `stable` without the component is the default on many machines while a
/// pinned toolchain beside it has one.
///
/// @param root - the project, where each candidate is run with `--version`
fn another_toolchains(root: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("RUSTUP_HOME").map(PathBuf::from).or_else(|| {
        let profile = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
        Some(PathBuf::from(profile).join(".rustup"))
    })?;
    let name = if cfg!(windows) { "rust-analyzer.exe" } else { "rust-analyzer" };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> =
        std::fs::read_dir(home.join("toolchains"))
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("bin").join(name))
            .filter(|path| path.is_file())
            .filter_map(|path| Some((path.metadata().ok()?.modified().ok()?, path)))
            .collect();
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    found.into_iter().map(|(_, path)| path).find(|path| runs(path, root))
}

/// What `rustup which rust-analyzer` says, if it says a file that exists.
fn rustup_which(root: &Path) -> Option<PathBuf> {
    let rustup = on_path("rustup")?;
    let mut command = Command::new(rustup);
    command
        .args(["which", "rust-analyzer"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    hide_window(&mut command);
    let out = command.output().ok().filter(|o| o.status.success())?;
    let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).lines().next()?.trim());
    path.is_file().then_some(path)
}

/// True when the program exits successfully on `--version` within [`VERSION_TIMEOUT`], run in the project
/// folder because the `rustup` proxy picks its toolchain from the folder it is run in.
fn runs(program: &Path, root: &Path) -> bool {
    let mut command = Command::new(program);
    command
        .arg("--version")
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_window(&mut command);
    let Ok(mut child) = command.spawn() else { return false };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if started.elapsed() < VERSION_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// The nearest `node_modules/typescript/lib/tsserver.js` at or above the file's folder.
fn nearest_tsserver(file: &Path) -> Option<PathBuf> {
    let start = if file.is_dir() { Some(file) } else { file.parent() };
    start?
        .ancestors()
        .map(|dir| dir.join("node_modules/typescript/lib/tsserver.js"))
        .find(|p| p.is_file())
}

/// A globally installed TypeScript, asked of `npm root -g`.
fn global_tsserver() -> Option<PathBuf> {
    let npm = on_path("npm")?;
    let mut command = Command::new(npm);
    command.args(["root", "-g"]).stdin(Stdio::null()).stderr(Stdio::null());
    hide_window(&mut command);
    let out = command.output().ok().filter(|o| o.status.success())?;
    let root = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().to_owned();
    let script = Path::new(&root).join("typescript/lib/tsserver.js");
    script.is_file().then_some(script)
}

/// A program on `PATH`, with the extensions Windows runs.
fn on_path(name: &str) -> Option<PathBuf> {
    let extensions: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat"] } else { &[""] };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|dir| extensions.iter().map(move |ext| dir.join(format!("{name}{ext}"))))
        .find(|candidate| candidate.is_file())
}

/// Stops a console window flashing up for a helper process on Windows.
fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}
