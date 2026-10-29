//! Reading the site's catalogue and a skill's files, on a thread, so a frame never waits on the network.
//!
//! An address is a web address or a file on this machine. A file is what a test names in
//! `UNLUMINOUS_SKILLS_CATALOGUE`: a folder laid out as the site is, `catalogue.json` beside `files/<id>/...`.
//! Every file fetched is checked against the SHA-256 the catalogue gives for it before anything is written.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::catalogue::{Catalogue, Entry, Files};

/// How long one request may take.
const TIMEOUT: Duration = Duration::from_secs(30);

/// What came back.
#[derive(Debug)]
pub enum Done {
    Catalogue(Result<Catalogue, String>),
    Files { id: String, origin: String, files: Result<Files, String> },
}

/// Starts fetches and holds what they brought back until the provider takes it.
#[derive(Default)]
pub struct Fetcher {
    done: Arc<Mutex<Vec<Done>>>,
    pub wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Fetcher {
    /// What has come back since the last call.
    pub fn take(&self) -> Vec<Done> {
        self.done.lock().map(|mut done| std::mem::take(&mut *done)).unwrap_or_default()
    }

    /// Read the catalogue at `address`.
    pub fn catalogue(&self, address: &str, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        let address = address.to_owned();
        let done = self.done.clone();
        std::thread::Builder::new()
            .name("unluminous-skills-catalogue".into())
            .spawn(move || {
                let result = get(&address)
                    .and_then(|bytes| Catalogue::read(&String::from_utf8_lossy(&bytes)));
                hand_back(&done, Done::Catalogue(result), wake.as_deref());
            })
            .ok();
    }

    /// Read every file of one skill from under `base`.
    pub fn files(&self, base: &str, entry: Entry, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        let base = base.to_owned();
        let done = self.done.clone();
        std::thread::Builder::new()
            .name("unluminous-skills-files".into())
            .spawn(move || {
                let files = files_of(&base, &entry);
                let result =
                    Done::Files { id: entry.id.clone(), origin: entry.sha256.clone(), files };
                hand_back(&done, result, wake.as_deref());
            })
            .ok();
    }
}

fn hand_back(done: &Mutex<Vec<Done>>, result: Done, wake: Option<&(dyn Fn() + Send + Sync)>) {
    if let Ok(mut done) = done.lock() {
        done.push(result);
    }
    if let Some(wake) = wake {
        wake();
    }
}

/// Every file of a skill, each checked against its hash.
fn files_of(base: &str, entry: &Entry) -> Result<Files, String> {
    if entry.files.is_empty() {
        return Err(format!("the catalogue lists no files for {}", entry.id));
    }
    let mut files = Files::new();
    for file in &entry.files {
        let bytes = get(&join(base, &format!("files/{}/{}", entry.id, file.path)))?;
        // Over the file with CRLF made LF, as `sync-skills.mjs` takes it, so line endings a checkout turned round on
        // either side never refuse a file.
        let plain = match std::str::from_utf8(&bytes) {
            Ok(text) => text.replace("\r\n", "\n").into_bytes(),
            Err(_) => bytes.clone(),
        };
        let hash: String =
            Sha256::digest(&plain).iter().map(|byte| format!("{byte:02x}")).collect();
        if !file.sha256.is_empty() && hash != file.sha256 {
            return Err(format!(
                "{} came back different from what the catalogue says, so nothing was written",
                file.path
            ));
        }
        files.insert(file.path.clone(), bytes);
    }
    Ok(files)
}

/// Where a skill's files are, given where the catalogue was read from: the catalogue's own `base` for a web
/// address, the catalogue file's folder for a file on this machine.
pub fn base_of(address: &str, remote: Option<&Catalogue>) -> String {
    let web = address.starts_with("http://") || address.starts_with("https://");
    if web {
        if let Some(base) =
            remote.map(|remote| remote.base.as_str()).filter(|base| !base.is_empty())
        {
            return base.trim_end_matches('/').to_owned();
        }
        return address.rsplit_once('/').map(|(folder, _)| folder.to_owned()).unwrap_or_default();
    }
    std::path::Path::new(address)
        .parent()
        .map(|folder| folder.display().to_string())
        .unwrap_or_default()
}

fn join(base: &str, inside: &str) -> String {
    match base.starts_with("http://") || base.starts_with("https://") {
        true => format!("{}/{}", base.trim_end_matches('/'), inside),
        false => {
            let mut path = std::path::PathBuf::from(base);
            for part in inside.split('/') {
                path.push(part);
            }
            path.display().to_string()
        }
    }
}

/// The bytes at an address.
pub fn get(address: &str) -> Result<Vec<u8>, String> {
    if !(address.starts_with("http://") || address.starts_with("https://")) {
        return std::fs::read(address).map_err(|problem| format!("{address}: {problem}"));
    }
    let config = ureq::Agent::config_builder()
        .tls_config(unluminous_chat::client::tls_config())
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut reply = agent
        .get(address)
        .header("user-agent", concat!("Unluminous/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|problem| problem.to_string())?;
    let status = reply.status().as_u16();
    if status != 200 {
        return Err(format!("{address} answered {status}"));
    }
    reply
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_vec()
        .map_err(|problem| problem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_files_base_is_the_catalogue_files_folder_or_the_sites_own() {
        assert_eq!(
            base_of("https://unluminous.com/skills/catalogue.json", None),
            "https://unluminous.com/skills"
        );
        let remote =
            Catalogue { base: "https://cdn.example/skills/".into(), ..Catalogue::default() };
        assert_eq!(
            base_of("https://unluminous.com/skills/catalogue.json", Some(&remote)),
            "https://cdn.example/skills"
        );
        let local = std::path::Path::new("C:/fixture/catalogue.json");
        assert_eq!(base_of(&local.display().to_string(), Some(&remote)), "C:/fixture");
        assert_eq!(
            join("https://x/skills", "files/a/SKILL.md"),
            "https://x/skills/files/a/SKILL.md"
        );
    }
}
