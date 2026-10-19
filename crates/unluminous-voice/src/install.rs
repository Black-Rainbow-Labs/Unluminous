//! What the local engine needs on disk, where it is, and the download that puts it there.
//!
//! **The installers ship both files with Unluminous** (`task-2306`), so an installed Unluminous never downloads
//! anything for speech to text. [`package`] fetches and checks them for a build, and [`packaged_beside`] is where
//! an installed copy finds them: `voice\` beside the program on Windows and Linux, and on macOS the model under
//! `Contents/Resources/voice` and the library in `Contents/Frameworks`. The download below is what a build no
//! installer made (a `cargo build`, a test) falls back to.
//!
//! Two things: Moonshine base (quantized ONNX, 67 MB, from a pinned commit of `moonshine-ai/moonshine` on
//! Hugging Face) and ONNX Runtime 1.22 (one library, from Microsoft's release). The runtime is only
//! downloaded when this machine has none: `ORT_DYLIB_PATH`, or the copy `inillucent setup-embeddings` put
//! under inillucent's folder, is used first, because `ort` loads one library per process and the code
//! index looks in the same places in the same order. `tasks/task-2290-speech-to-text-tdd.md` section 8.
//!
//! Every file is written to `downloads/` first and renamed into place once its size and SHA-256 match the
//! values written here, so a download that was cut off is never taken for a model.
//!
//! The folder is the platform's **local** application data, not the roaming folder the settings are in:
//! 70 MB of weights should not travel with a Windows profile.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// The environment variable that moves the whole folder, for tests.
pub const HOME_VAR: &str = "UNLUMINOUS_VOICE_HOME";

/// The environment variable that names another address to fetch the model from, in place of
/// `https://huggingface.co`, so a test can make a download fail without the network.
pub const MODEL_SOURCE_VAR: &str = "UNLUMINOUS_VOICE_MODEL_SOURCE";

/// The ONNX Runtime version downloaded, and the oldest minor version `ort` 2.0.0-rc.13 accepts.
pub const RUNTIME_VERSION: &str = "1.22.0";
const RUNTIME_MINOR: u64 = 22;

/// The commit of `moonshine-ai/moonshine` the files come from, so a later push cannot change what is
/// downloaded under the same hashes.
pub const MOONSHINE_COMMIT: &str = "48b4e427b587bcf67797a5be706d6ddc4a298149";

/// The model's folder name under `models/`.
pub const MODEL_NAME: &str = "moonshine-base";

/// One file of the model: where it comes from, how large it is and its SHA-256.
#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub name: &'static str,
    pub path: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

/// The three files Moonshine needs. The hashes are the repository's own LFS pointers, and the
/// tokenizer's was measured on the download at this commit.
pub const MODEL_FILES: [ModelFile; 3] = [
    ModelFile {
        name: "encoder_model.onnx",
        path: "onnx/merged/base/quantized/encoder_model.onnx",
        bytes: 20_513_063,
        sha256: "1dd9ab0a7f987113d30affcba5a068d11c8f90fa0223caa3e491ade431ad9751",
    },
    ModelFile {
        name: "decoder_model_merged.onnx",
        path: "onnx/merged/base/quantized/decoder_model_merged.onnx",
        bytes: 42_498_870,
        sha256: "cc9f3cd6698a369c6008b41aa60aa3fb3322e7f03c9bdf19d8e6b7200afca4f3",
    },
    ModelFile {
        name: "tokenizer.json",
        path: "onnx/merged/base/float/tokenizer.json",
        bytes: 3_761_751,
        sha256: "ad3a2ceb0e84e4da57451d86fa337f8116dcff5f5d106434f8aa0b0de89718b9",
    },
];

/// How long one file may take to arrive.
const TIMEOUT: Duration = Duration::from_secs(900);

/// What `status` says about the local engine's files.
#[derive(Debug, Clone, PartialEq)]
pub struct Installed {
    /// The model folder, when every file in it is there at its size.
    pub model: Option<PathBuf>,
    /// The ONNX Runtime library the engine would load.
    pub runtime: Option<PathBuf>,
    /// Whether the model is the one the installer put beside the program, so there is nothing to download.
    pub packaged: bool,
}

impl Installed {
    /// Whether the local engine has everything it needs.
    pub fn is_complete(&self) -> bool {
        self.model.is_some() && self.runtime.is_some()
    }

    /// How many megabytes a download would fetch.
    pub fn download_megabytes(&self) -> u64 {
        let model: u64 =
            if self.model.is_some() { 0 } else { MODEL_FILES.iter().map(|f| f.bytes).sum() };
        let runtime = if self.runtime.is_some() { 0 } else { runtime_archive().1 };
        (model + runtime) / 1_000_000
    }
}

/// The value of an environment variable, when it is set to something.
/// @param name - the variable
fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
}

/// The platform's per user local data folder, with `name` under it.
/// @param name - `Unluminous` or `inillucent`
fn local_data(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let found = variable("LOCALAPPDATA").map(|local| PathBuf::from(local).join(name));
    #[cfg(target_os = "macos")]
    let found = variable("HOME")
        .map(|home| PathBuf::from(home).join("Library").join("Application Support").join(name));
    #[cfg(not(any(windows, target_os = "macos")))]
    let found = variable("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| variable("HOME").map(|home| PathBuf::from(home).join(".local").join("share")))
        .map(|data| data.join(name.to_lowercase()));
    found
}

/// The voice folder: `UNLUMINOUS_VOICE_HOME`, or `voice` under the platform's local data for Unluminous.
pub fn home() -> PathBuf {
    if let Some(named) = variable(HOME_VAR) {
        return PathBuf::from(named);
    }
    local_data("Unluminous")
        .map(|folder| folder.join("voice"))
        .unwrap_or_else(|| PathBuf::from(".unluminous-voice"))
}

/// The model's folder.
pub fn model_dir() -> PathBuf {
    home().join("models").join(MODEL_NAME)
}

/// The model's folder when every file is there at its size, which is what a finished download leaves.
pub fn installed_model() -> Option<PathBuf> {
    complete_model(model_dir())
}

/// `dir` when every model file is in it at its size.
/// @param dir - a model folder
fn complete_model(dir: PathBuf) -> Option<PathBuf> {
    MODEL_FILES
        .iter()
        .all(|file| std::fs::metadata(dir.join(file.name)).is_ok_and(|m| m.len() == file.bytes))
        .then_some(dir)
}

/// What the installer put beside the program in `program_dir`: the model folder and the library, each when it is
/// there. On macOS `program_dir` is `Contents/MacOS`.
/// @param program_dir - the folder `unluminous` or `unluminous-cli` is in
pub fn packaged_beside(program_dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let (voice, library) = if cfg!(target_os = "macos") {
        let contents = program_dir.parent().unwrap_or(program_dir);
        (
            contents.join("Resources").join("voice"),
            contents.join("Frameworks").join(runtime_library_name()),
        )
    } else {
        let voice = program_dir.join("voice");
        let library = voice.join("runtime").join(runtime_library_name());
        (voice, library)
    };
    (complete_model(voice.join("models").join(MODEL_NAME)), library.is_file().then_some(library))
}

/// What the installer put beside this program. Nothing while `UNLUMINOUS_VOICE_HOME` is set, because a test
/// that names a folder means that folder and nothing else.
fn packaged() -> (Option<PathBuf>, Option<PathBuf>) {
    if variable(HOME_VAR).is_some() {
        return (None, None);
    }
    std::env::current_exe()
        .ok()
        .and_then(|program| program.parent().map(packaged_beside))
        .unwrap_or((None, None))
}

/// The library's file name on this platform.
pub fn runtime_library_name() -> &'static str {
    if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}

/// The ONNX Runtime library to load: `ORT_DYLIB_PATH`, then inillucent's, then the one that came with
/// Unluminous, then one downloaded here.
///
/// Inillucent's comes before Unluminous's own because `ort` loads one library per process and the code index
/// looks for inillucent's first, so the two have to agree.
pub fn runtime_library() -> Option<PathBuf> {
    runtime_library_after(packaged().1)
}

/// [`runtime_library`], with the packaged library already found.
/// @param packaged - the library beside the program, when there is one
fn runtime_library_after(packaged: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(named) = variable("ORT_DYLIB_PATH") {
        let path = PathBuf::from(named);
        return path.exists().then_some(path);
    }
    let inillucent =
        variable("INILLUCENT_HOME").map(PathBuf::from).or_else(|| local_data("inillucent"));
    inillucent
        .and_then(|root| inillucents_runtime(&root))
        .or(packaged)
        .or_else(|| newest_runtime(&home().join("runtime")))
}

/// The runtime inillucent recorded in its `embeddings.json`, or the newest one in its `runtime` folder.
/// @param root - inillucent's folder
fn inillucents_runtime(root: &Path) -> Option<PathBuf> {
    let recorded = std::fs::read_to_string(root.join("embeddings.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|state| state["runtime"]["library"].as_str().map(PathBuf::from))
        .filter(|path| {
            path.exists()
                && path
                    .parent()
                    .and_then(Path::parent)
                    .is_some_and(|dir| version_is_new_enough(&dir.to_string_lossy()))
        });
    recorded.or_else(|| newest_runtime(&root.join("runtime")))
}

/// Whether a folder named `onnxruntime-<version>` holds a version `ort` accepts.
/// @param folder - the folder's path or name
fn version_is_new_enough(folder: &str) -> bool {
    let Some(version) =
        folder.rsplit(['/', '\\']).next().and_then(|name| name.strip_prefix("onnxruntime-"))
    else {
        return false;
    };
    let parts: Vec<u64> = version.split(['.', '-']).map(|part| part.parse().unwrap_or(0)).collect();
    parts.first() == Some(&1) && parts.get(1).is_some_and(|minor| *minor >= RUNTIME_MINOR)
}

/// The library of the newest accepted version under a folder of `onnxruntime-<version>` folders.
/// @param root - the folder
fn newest_runtime(root: &Path) -> Option<PathBuf> {
    let mut best: Option<(Vec<u64>, PathBuf)> = None;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let library = entry.path().join("lib").join(runtime_library_name());
        if !version_is_new_enough(&name) || !library.exists() {
            continue;
        }
        let key: Vec<u64> = name
            .trim_start_matches("onnxruntime-")
            .split(['.', '-'])
            .map(|p| p.parse().unwrap_or(0))
            .collect();
        if best.as_ref().is_none_or(|(seen, _)| key > *seen) {
            best = Some((key, library));
        }
    }
    best.map(|(_, library)| library)
}

/// What is on disk now: the packaged model first, then a downloaded one.
pub fn status() -> Installed {
    let (model, runtime) = packaged();
    let packaged = model.is_some();
    Installed {
        model: model.or_else(installed_model),
        runtime: runtime_library_after(runtime),
        packaged,
    }
}

/// The runtime archive for this platform, its approximate size, and the folder inside it.
fn runtime_archive() -> (String, u64, String) {
    let (platform, size, extension) = if cfg!(windows) {
        ("win-x64", 72_368_545, "zip")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        ("osx-arm64", 25_943_843, "tgz")
    } else if cfg!(target_os = "macos") {
        ("osx-x86_64", 27_889_590, "tgz")
    } else {
        ("linux-x64", 7_798_730, "tgz")
    };
    let folder = format!("onnxruntime-{platform}-{RUNTIME_VERSION}");
    (format!("https://github.com/microsoft/onnxruntime/releases/download/v{RUNTIME_VERSION}/{folder}.{extension}"), size, folder)
}

/// The HTTP client: the machine's own certificates, as `unluminous_chat::client::tls_config` sets them,
/// because `ureq`'s default provider is Rustls whether or not that feature is built and would panic.
fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder().tls_config(tls).timeout_global(Some(TIMEOUT)).build(),
    )
}

/// Download whatever is missing, saying what it is doing through `progress` (a sentence and a fraction).
///
/// Answers what is on disk afterwards. A failure leaves nothing half written where the engine would look.
/// @param progress - called with what is happening and how far through it is, 0 to 1
pub fn install(progress: &dyn Fn(&str, f32)) -> Result<Installed, String> {
    let before = status();
    if before.model.is_none() {
        install_model(progress)?;
    }
    if before.runtime.is_none() {
        install_runtime(progress)?;
    }
    let after = status();
    match after.is_complete() {
        true => Ok(after),
        false => Err("the download finished but the files are not where the engine looks for them"
            .to_owned()),
    }
}

/// Fetch and check the three model files, then move them into the model's folder.
/// @param progress - as for [`install`]
fn install_model(progress: &dyn Fn(&str, f32)) -> Result<(), String> {
    let staging = home().join("downloads");
    std::fs::create_dir_all(&staging)
        .map_err(|e| format!("{} could not be made: {e}", staging.display()))?;
    let total: u64 = MODEL_FILES.iter().map(|f| f.bytes).sum();
    let mut before = 0u64;
    for file in &MODEL_FILES {
        let source =
            variable(MODEL_SOURCE_VAR).unwrap_or_else(|| "https://huggingface.co".to_owned());
        let url = format!(
            "{}/moonshine-ai/moonshine/resolve/{MOONSHINE_COMMIT}/{}",
            source.trim_end_matches('/'),
            file.path
        );
        let into = staging.join(file.name);
        let said = format!("Downloading the speech model ({} MB)", total / 1_000_000);
        fetch(&url, &into, file.bytes, Some(file.sha256), &|done| {
            progress(&said, (before + done) as f32 / total as f32)
        })?;
        before += file.bytes;
    }
    let dir = model_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("{} could not be made: {e}", dir.display()))?;
    for file in &MODEL_FILES {
        std::fs::rename(staging.join(file.name), dir.join(file.name))
            .map_err(|e| format!("{} could not be moved into place: {e}", file.name))?;
    }
    Ok(())
}

/// Fetch ONNX Runtime's release archive and keep its library folder.
/// @param progress - as for [`install`]
fn install_runtime(progress: &dyn Fn(&str, f32)) -> Result<(), String> {
    let (url, size, folder) = runtime_archive();
    let staging = home().join("downloads");
    std::fs::create_dir_all(&staging)
        .map_err(|e| format!("{} could not be made: {e}", staging.display()))?;
    let archive = staging.join(url.rsplit('/').next().unwrap_or("onnxruntime.archive"));
    let said = format!("Downloading ONNX Runtime {RUNTIME_VERSION} ({} MB)", size / 1_000_000);
    fetch(&url, &archive, size, None, &|done| progress(&said, done as f32 / size as f32))?;
    progress("Unpacking ONNX Runtime", 1.0);
    let unpacked = staging.join("runtime");
    let _ = std::fs::remove_dir_all(&unpacked);
    std::fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
    let extracted = extract(&archive, &unpacked, &format!("{folder}/lib"));
    let _ = std::fs::remove_file(&archive);
    extracted?;
    let lib = unpacked.join(&folder).join("lib");
    // The Windows archive carries the debug symbols (about 200 MB) and the import library beside the DLL,
    // and the engine needs neither, so they go before anything is moved into place.
    for entry in std::fs::read_dir(&lib).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.ends_with(".pdb") || name.ends_with(".lib") || entry.path().is_dir() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let library = lib.join(runtime_library_name());
    if !library.exists() {
        return Err(format!("the ONNX Runtime archive did not hold {}", runtime_library_name()));
    }
    let target = home().join("runtime").join(format!("onnxruntime-{RUNTIME_VERSION}"));
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(target.parent().unwrap_or(&target)).map_err(|e| e.to_string())?;
    std::fs::rename(unpacked.join(&folder), &target)
        .map_err(|e| format!("ONNX Runtime could not be moved into place: {e}"))?;
    let _ = std::fs::remove_dir_all(&unpacked);
    Ok(())
}

/// The platform an installer is built for, which decides which ONNX Runtime archive its package holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    /// Both Apple architectures in one library, because `Unluminous.app` is one universal program.
    Macos,
    Linux,
}

impl Platform {
    /// The platform this program was built for.
    pub fn this() -> Platform {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else {
            Platform::Linux
        }
    }

    /// `windows`, `macos` or `linux`, the way a build script names it.
    /// @param name - the name
    pub fn parse(name: &str) -> Option<Platform> {
        match name {
            "windows" => Some(Platform::Windows),
            "macos" => Some(Platform::Macos),
            "linux" => Some(Platform::Linux),
            _ => None,
        }
    }

    /// The archive's name, its size and SHA-256 (measured on the release), the library inside it, and the
    /// name the library is given in the package.
    fn archive(self) -> (&'static str, u64, &'static str, &'static str, &'static str) {
        match self {
            Platform::Windows => (
                "onnxruntime-win-x64-1.22.0.zip",
                72_368_545,
                "174c616efc0271194488642a72f1a514e01487da4dfe84c49296d66e40ebe0da",
                "onnxruntime-win-x64-1.22.0/lib/onnxruntime.dll",
                "onnxruntime.dll",
            ),
            // The archive's `libonnxruntime.dylib` is a link to this file, and a link is not something every
            // `tar` can write, so the file itself is taken and given the short name.
            Platform::Macos => (
                "onnxruntime-osx-universal2-1.22.0.tgz",
                54_820_264,
                "cfa6f6584d87555ed9f6e7e8a000d3947554d589efe3723b8bfa358cd263d03c",
                "onnxruntime-osx-universal2-1.22.0/lib/libonnxruntime.1.22.0.dylib",
                "libonnxruntime.dylib",
            ),
            Platform::Linux => (
                "onnxruntime-linux-x64-1.22.0.tgz",
                7_798_730,
                "8344d55f93d5bc5021ce342db50f62079daf39aaafb5d311a451846228be49b3",
                "onnxruntime-linux-x64-1.22.0/lib/libonnxruntime.so.1.22.0",
                "libonnxruntime.so",
            ),
        }
    }
}

/// Fill `into` with what an installer ships for speech to text: `models/moonshine-base/` and
/// `runtime/<library>`. Every file is checked against the size and SHA-256 written here, and a file already
/// there and right is not fetched again, so a release that runs this every time downloads once.
///
/// Answers the model folder and the library.
/// @param into - the folder, which is made when it is missing
/// @param platform - which ONNX Runtime library to take
/// @param progress - as for [`install`]
pub fn package(
    into: &Path,
    platform: Platform,
    progress: &dyn Fn(&str, f32),
) -> Result<(PathBuf, PathBuf), String> {
    let models = into.join("models").join(MODEL_NAME);
    std::fs::create_dir_all(&models)
        .map_err(|e| format!("{} could not be made: {e}", models.display()))?;
    let total: u64 = MODEL_FILES.iter().map(|f| f.bytes).sum();
    let mut before = 0u64;
    for file in &MODEL_FILES {
        let target = models.join(file.name);
        if !holds(&target, file.bytes, file.sha256) {
            let url = format!(
                "https://huggingface.co/moonshine-ai/moonshine/resolve/{MOONSHINE_COMMIT}/{}",
                file.path
            );
            let said = format!("Downloading the speech model ({} MB)", total / 1_000_000);
            let partial = target.with_extension("part");
            fetch(&url, &partial, file.bytes, Some(file.sha256), &|done| {
                progress(&said, (before + done) as f32 / total as f32)
            })?;
            std::fs::rename(&partial, &target)
                .map_err(|e| format!("{} could not be moved into place: {e}", file.name))?;
        }
        before += file.bytes;
    }
    let (archive_name, size, sha256, inside, library_name) = platform.archive();
    let runtime = into.join("runtime");
    let library = runtime.join(library_name);
    let marker = runtime.join("from.sha256");
    let current = library.is_file()
        && std::fs::read_to_string(&marker).is_ok_and(|text| text.trim() == sha256);
    if !current {
        std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
        let archive = runtime.join(archive_name);
        let url = format!(
            "https://github.com/microsoft/onnxruntime/releases/download/v{RUNTIME_VERSION}/{archive_name}"
        );
        let said = format!("Downloading ONNX Runtime {RUNTIME_VERSION} ({} MB)", size / 1_000_000);
        fetch(&url, &archive, size, Some(sha256), &|done| {
            progress(&said, done as f32 / size as f32)
        })?;
        let unpacked = runtime.join("unpacked");
        let _ = std::fs::remove_dir_all(&unpacked);
        std::fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
        let extracted = extract(&archive, &unpacked, inside);
        let _ = std::fs::remove_file(&archive);
        extracted?;
        let _ = std::fs::remove_file(&library);
        std::fs::rename(unpacked.join(inside), &library)
            .map_err(|e| format!("{library_name} could not be moved into place: {e}"))?;
        let _ = std::fs::remove_dir_all(&unpacked);
        std::fs::write(&marker, sha256).map_err(|e| e.to_string())?;
    }
    Ok((models, library))
}

/// Whether `path` is `bytes` long with SHA-256 `sha256`.
/// @param path - the file
/// @param bytes - its length
/// @param sha256 - its hash
fn holds(path: &Path, bytes: u64, sha256: &str) -> bool {
    if !std::fs::metadata(path).is_ok_and(|m| m.len() == bytes) {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let mut hasher = Sha256::new();
    if std::io::copy(&mut file, &mut hasher).is_err() {
        return false;
    }
    let hash: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    hash == sha256
}

/// Unpack one folder or file of an archive with the system's own `tar`, which reads a `.zip` on Windows 10 and
/// later as well as a `.tgz`. Named by its full path on Windows, because Git for Windows puts a different
/// `tar` ahead of it on the path that cannot read a zip.
/// @param archive - the archive
/// @param into - where it is unpacked
/// @param inside - the folder or file in the archive to keep
fn extract(archive: &Path, into: &Path, inside: &str) -> Result<(), String> {
    let tar = match variable("SystemRoot") {
        Some(root) if cfg!(windows) => PathBuf::from(root).join("System32").join("tar.exe"),
        _ => PathBuf::from("tar"),
    };
    let output = std::process::Command::new(&tar)
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .arg(inside)
        .output()
        .map_err(|e| format!("{} could not be run: {e}", tar.display()))?;
    match output.status.success() {
        true => Ok(()),
        false => Err(format!(
            "unpacking ONNX Runtime failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

/// Download one file, checking its length and, when given, its SHA-256. A file that does not match is
/// deleted before the refusal is returned.
/// @param url - where it is
/// @param into - where it goes
/// @param bytes - how long it should be
/// @param sha256 - its hash, when one is known
/// @param progress - called with the bytes so far
fn fetch(
    url: &str,
    into: &Path,
    bytes: u64,
    sha256: Option<&str>,
    progress: &dyn Fn(u64),
) -> Result<(), String> {
    let result = fetch_into(url, into, bytes, sha256, progress);
    if result.is_err() {
        let _ = std::fs::remove_file(into);
    }
    result
}

/// The body of [`fetch`].
fn fetch_into(
    url: &str,
    into: &Path,
    bytes: u64,
    sha256: Option<&str>,
    progress: &dyn Fn(u64),
) -> Result<(), String> {
    let mut reply = agent()
        .get(url)
        .header("user-agent", "Unluminous")
        .call()
        .map_err(|e| format!("{url} could not be fetched: {e}"))?;
    let mut body = reply.body_mut().with_config().limit(bytes * 2 + 1).reader();
    let mut file = std::fs::File::create(into)
        .map_err(|e| format!("{} could not be written: {e}", into.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let read =
            body.read(&mut buffer).map_err(|e| format!("the download of {url} stopped: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        done += read as u64;
        progress(done);
    }
    file.flush().map_err(|e| e.to_string())?;
    if let Some(expected) = sha256 {
        let hash: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
        if done != bytes || hash != expected {
            return Err(format!("{url} arrived as {done} bytes with SHA-256 {hash}, and {bytes} bytes with {expected} was expected"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_runtime_ort_accepts_is_taken() {
        assert!(version_is_new_enough("onnxruntime-1.22.0"));
        assert!(version_is_new_enough("C:/x/runtime/onnxruntime-1.23.1"));
        assert!(!version_is_new_enough("onnxruntime-1.17.0"));
        assert!(!version_is_new_enough("onnxruntime-nightly"));
        assert!(!version_is_new_enough("lib"));
    }

    #[test]
    fn the_newest_accepted_runtime_wins_and_an_old_one_is_passed_over() {
        let root =
            std::env::temp_dir().join(format!("unluminous-voice-runtimes-{}", std::process::id()));
        for version in ["1.17.0", "1.22.0", "1.23.0"] {
            let lib = root.join(format!("onnxruntime-{version}")).join("lib");
            std::fs::create_dir_all(&lib).unwrap();
            std::fs::write(lib.join(runtime_library_name()), b"x").unwrap();
        }
        let found = newest_runtime(&root).unwrap();
        assert!(found.to_string_lossy().contains("onnxruntime-1.23.0"), "{}", found.display());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn what_an_installer_put_beside_the_program_is_found_there() {
        let root =
            std::env::temp_dir().join(format!("unluminous-voice-packaged-{}", std::process::id()));
        let program_dir = if cfg!(target_os = "macos") {
            root.join("Unluminous.app").join("Contents").join("MacOS")
        } else {
            root.join("Unluminous")
        };
        std::fs::create_dir_all(&program_dir).unwrap();
        assert_eq!(packaged_beside(&program_dir), (None, None));
        let (models, library) = if cfg!(target_os = "macos") {
            let contents = program_dir.parent().unwrap();
            (contents.join("Resources/voice/models"), contents.join("Frameworks"))
        } else {
            (program_dir.join("voice/models"), program_dir.join("voice/runtime"))
        };
        let model = models.join(MODEL_NAME);
        std::fs::create_dir_all(&model).unwrap();
        std::fs::create_dir_all(&library).unwrap();
        std::fs::write(library.join(runtime_library_name()), b"x").unwrap();
        for file in &MODEL_FILES {
            let handle = std::fs::File::create(model.join(file.name)).unwrap();
            handle.set_len(file.bytes).unwrap();
        }
        let (found_model, found_library) = packaged_beside(&program_dir);
        assert_eq!(found_model.as_deref(), Some(model.as_path()));
        assert_eq!(found_library, Some(library.join(runtime_library_name())));
        // One file cut short and the model is not taken, so a half copied install downloads instead.
        std::fs::File::create(model.join("tokenizer.json")).unwrap().set_len(10).unwrap();
        assert_eq!(packaged_beside(&program_dir).0, None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_file_is_held_only_at_its_length_and_hash() {
        let path =
            std::env::temp_dir().join(format!("unluminous-voice-holds-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(holds(&path, 3, abc));
        assert!(!holds(&path, 4, abc));
        assert!(!holds(&path, 3, &abc.replace('b', "c")));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_model_folder_counts_only_when_every_file_is_there_at_its_size() {
        let dir =
            std::env::temp_dir().join(format!("unluminous-voice-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let complete = |dir: &Path| {
            MODEL_FILES.iter().all(|file| {
                std::fs::metadata(dir.join(file.name)).is_ok_and(|m| m.len() == file.bytes)
            })
        };
        std::fs::write(dir.join("tokenizer.json"), b"short").unwrap();
        assert!(!complete(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
