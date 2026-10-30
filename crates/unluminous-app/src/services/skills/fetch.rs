//! Reading the site's catalogue and a skill's files, on a thread, so a frame never waits on the network.
//!
//! An address is a web address or a file on this machine. A file is what a test names in
//! `UNLUMINOUS_SKILLS_CATALOGUE`: a folder laid out as the site is, `catalogue.json` beside `files/<id>/...`.
//! Every file fetched is checked against the SHA-256 the catalogue gives for it before anything is written.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::catalogue::{Catalogue, Entry, Files, RemoteFile};

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

/// How many of a skill's files are asked for at once. HyperFrames has 77 and Archify 158, and one after another
/// a skill of that size took most of a minute while the pane said only that it was fetching. `task-2405`.
const AT_ONCE: usize = 6;

/// One file of a skill as it came back: its path and its bytes, or why not.
type Fetched = Result<(String, Vec<u8>), String>;

/// Every file of a skill, each checked against its hash, [`AT_ONCE`] at a time through one client.
fn files_of(base: &str, entry: &Entry) -> Result<Files, String> {
    if entry.files.is_empty() {
        return Err(format!("the catalogue lists no files for {}", entry.id));
    }
    let agent = agent();
    let next = AtomicUsize::new(0);
    let fetched: Mutex<Vec<Fetched>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..AT_ONCE.min(entry.files.len()) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = entry.files.get(index) else { break };
                let address = join(base, &format!("files/{}/{}", entry.id, file.path));
                let one = get_with(&agent, &address).and_then(|bytes| checked(file, bytes));
                let failed = one.is_err();
                if let Ok(mut fetched) = fetched.lock() {
                    fetched.push(one.map(|bytes| (file.path.clone(), bytes)));
                }
                if failed {
                    // The others finish the file they are on and stop: one refusal refuses the whole skill.
                    next.store(entry.files.len(), Ordering::Relaxed);
                }
            });
        }
    });
    let mut files = Files::new();
    for one in fetched.into_inner().unwrap_or_default() {
        let (path, bytes) = one?;
        files.insert(path, bytes);
    }
    if files.len() != entry.files.len() {
        return Err(format!(
            "only {} of the {} files of {} came back",
            files.len(),
            entry.files.len(),
            entry.id
        ));
    }
    Ok(files)
}

/// `bytes`, when they hash to what the catalogue gives for `file`. The hash is over the file with CRLF made LF, as
/// `sync-skills.mjs` takes it, so line endings a checkout turned round on either side never refuse a file.
fn checked(file: &RemoteFile, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let plain = match std::str::from_utf8(&bytes) {
        Ok(text) => text.replace("\r\n", "\n").into_bytes(),
        Err(_) => bytes.clone(),
    };
    let hash: String = Sha256::digest(&plain).iter().map(|byte| format!("{byte:02x}")).collect();
    if !file.sha256.is_empty() && hash != file.sha256 {
        return Err(format!(
            "{} came back different from what the catalogue says, so nothing was written",
            file.path
        ));
    }
    Ok(bytes)
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
    get_with(&agent(), address)
}

/// The client every request here is made with. It follows no redirect itself: [`get_with`] does, so that one to
/// another host is refused.
fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .tls_config(unluminous_chat::client::tls_config())
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .build();
    ureq::Agent::new_with_config(config)
}

/// How many redirects one request follows.
const REDIRECTS: usize = 3;

/// The bytes at an address, asked with `agent`.
///
/// **A redirect on the same host is followed.** unluminous.com is served by Cloudflare, which answers a request
/// for `caption-skin.html` with `307` to `caption-skin`, and 26 of HyperFrames' 77 files and one of Archify's
/// are `.html`. Refusing every redirect left those two skills impossible to open while the pane said it was
/// fetching them. A redirect to another host is still refused, and every file is checked against its hash
/// whichever address answered. `task-2405`.
fn get_with(agent: &ureq::Agent, address: &str) -> Result<Vec<u8>, String> {
    if !is_web(address) {
        return std::fs::read(address).map_err(|problem| format!("{address}: {problem}"));
    }
    let mut asked = address.to_owned();
    for _ in 0..=REDIRECTS {
        let mut reply = agent
            .get(&asked)
            .header("user-agent", concat!("Unluminous/", env!("CARGO_PKG_VERSION")))
            .call()
            .map_err(|problem| problem.to_string())?;
        let status = reply.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let to = reply.headers().get("location").and_then(|value| value.to_str().ok());
            let Some(to) = to.and_then(|to| same_host(&asked, to)) else {
                return Err(format!("{address} answered {status} with an address on another host"));
            };
            asked = to;
            continue;
        }
        if status != 200 {
            return Err(format!("{address} answered {status}"));
        }
        return reply
            .body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_to_vec()
            .map_err(|problem| problem.to_string());
    }
    Err(format!("{address} redirected more than {REDIRECTS} times"))
}

fn is_web(address: &str) -> bool {
    address.starts_with("http://") || address.starts_with("https://")
}

/// Where a `Location` of `to`, answered for `from`, points, when that is on `from`'s own scheme and host.
fn same_host(from: &str, to: &str) -> Option<String> {
    let (scheme, rest) = from.split_once("://")?;
    let host = rest.split('/').next()?;
    let origin = format!("{scheme}://{host}");
    if to.starts_with("//") {
        return None;
    }
    if to.starts_with('/') {
        return Some(format!("{origin}{to}"));
    }
    let on_this_host = to == origin || to.starts_with(&format!("{origin}/"));
    on_this_host.then(|| to.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2405`: every skill the binary ships only part of comes from unluminous.com whole, each file checked
    /// against its hash. HyperFrames and Archify could not, because the site answers a `.html` file with a
    /// redirect. Live, so ignored: `cargo test -p unluminous-app --lib every_skill_the_site_serves -- --ignored`.
    #[test]
    #[ignore = "reads unluminous.com"]
    fn every_skill_the_site_serves_in_part_comes_back_whole() {
        let catalogue = get(super::super::catalogue::SITE)
            .and_then(|bytes| Catalogue::read(&String::from_utf8_lossy(&bytes)))
            .expect("the site's catalogue");
        let base = base_of(super::super::catalogue::SITE, Some(&catalogue));
        let mut failed = Vec::new();
        // Which skills ship in part is said by the copy in the binary; the files and their hashes by the site's.
        let shipped = super::super::catalogue::bundled();
        let partial: Vec<&Entry> = shipped
            .skills
            .iter()
            .filter(|entry| entry.bundled == "partial")
            .filter_map(|entry| catalogue.skills.iter().find(|remote| remote.id == entry.id))
            .collect();
        assert!(!partial.is_empty(), "the site lists skills shipped in part");
        for entry in partial {
            match files_of(&base, entry) {
                Ok(files) => println!("{}: {} files", entry.id, files.len()),
                Err(why) => failed.push(format!("{}: {why}", entry.id)),
            }
        }
        assert!(failed.is_empty(), "{failed:#?}");
    }

    #[test]
    fn a_redirect_is_followed_only_on_the_same_host() {
        let from = "https://unluminous.com/skills/files/archify/assets/template.html";
        assert_eq!(
            same_host(from, "/skills/files/archify/assets/template").as_deref(),
            Some("https://unluminous.com/skills/files/archify/assets/template")
        );
        assert_eq!(
            same_host(from, "https://unluminous.com/skills/x").as_deref(),
            Some("https://unluminous.com/skills/x")
        );
        assert_eq!(same_host(from, "https://unluminous.com.example/x"), None);
        assert_eq!(same_host(from, "//elsewhere.example/x"), None);
        assert_eq!(
            same_host(from, "http://unluminous.com/x"),
            None,
            "another scheme is another origin"
        );
    }

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
