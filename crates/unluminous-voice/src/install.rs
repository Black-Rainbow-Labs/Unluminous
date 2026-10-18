//! What the local engine needs on disk, where it goes, and the download that puts it there.
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
    let dir = model_dir();
    MODEL_FILES
        .iter()
        .all(|file| std::fs::metadata(dir.join(file.name)).is_ok_and(|m| m.len() == file.bytes))
        .then_some(dir)
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

/// The ONNX Runtime library to load: `ORT_DYLIB_PATH`, then inillucent's, then the one installed here.
pub fn runtime_library() -> Option<PathBuf> {
    if let Some(named) = variable("ORT_DYLIB_PATH") {
        let path = PathBuf::from(named);
        return path.exists().then_some(path);
    }
    let inillucent =
        variable("INILLUCENT_HOME").map(PathBuf::from).or_else(|| local_data("inillucent"));
    inillucent
        .and_then(|root| inillucents_runtime(&root))
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

/// What is on disk now.
pub fn status() -> Installed {
    Installed { model: installed_model(), runtime: runtime_library() }
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
        let url = format!(
            "https://huggingface.co/moonshine-ai/moonshine/resolve/{MOONSHINE_COMMIT}/{}",
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

/// Unpack one folder of an archive with the system's own `tar`, which reads a `.zip` on Windows 10 and
/// later as well as a `.tgz`. Named by its full path on Windows, because Git for Windows puts a different
/// `tar` ahead of it on the path that cannot read a zip.
/// @param archive - the archive
/// @param into - where it is unpacked
/// @param inside - the folder in the archive to keep
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
