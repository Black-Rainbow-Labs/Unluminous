//! Reading, fetching and decoding the card pictures on a thread, so the pane is drawn the frame it is shown.
//!
//! `task-2401` measured the first opening of the Skills pane: every card's picture was decoded in the frame that
//! first drew it. The cards are drawn with the stand in picture ([`crate::components::skills::paint_placeholder`])
//! and each picture takes its place when it arrives, the way a web page fills its images in after its text.
//!
//! One worker thread takes the pictures in the order they were asked for, which is the order the cards were
//! drawn, so the cards on the screen come first. A picture is looked for in three places:
//!
//! 1. the binary, when the picture that shipped is the one the site lists now;
//! 2. the plugin's folder, `images/<id>.jpg`, where a picture fetched before is kept;
//! 3. unluminous.com, when the window may fetch: the bytes are checked against the catalogue's `imageSha256`
//!    and kept in the plugin's folder before they are shown.
//!
//! When the site cannot be reached the older picture is shown, if there is one, and otherwise the stand in.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

/// Where one skill's picture may be found.
#[derive(Debug, Clone)]
pub struct Wanted {
    /// The name the drawing asks about and the picture comes back under: the skill's id and the picture's hash.
    pub key: String,
    /// The picture that shipped in the binary.
    pub shipped: Option<&'static [u8]>,
    /// Where a fetched picture is kept: `images/<id>.jpg` in the plugin's folder.
    pub kept: Option<PathBuf>,
    /// Where the site has it, when this window may fetch.
    pub address: Option<String>,
    /// The hash the site lists for it, empty when the catalogue does not say.
    pub sha256: String,
}

/// One picture, decoded. `None` when it could not be found or read, which the drawing shows as the stand in.
pub struct Arrived {
    pub key: String,
    pub image: Option<egui::ColorImage>,
}

/// What the worker and the window share.
#[derive(Default)]
struct Shared {
    asked: HashSet<String>,
    arrived: VecDeque<Arrived>,
    /// Asked for and not yet handed to the window.
    waiting: usize,
}

/// Starts the worker the first time a picture is asked for and holds what it decoded until the window takes it.
#[derive(Default)]
pub struct Loader {
    shared: Arc<Mutex<Shared>>,
    worker: Mutex<Option<Sender<Wanted>>>,
    pub wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Loader {
    /// Ask for a picture. Asking again for one already asked for does nothing.
    pub fn ask(&self, wanted: Wanted) {
        {
            let Ok(mut shared) = self.shared.lock() else { return };
            if !shared.asked.insert(wanted.key.clone()) {
                return;
            }
            shared.waiting += 1;
        }
        let Ok(mut worker) = self.worker.lock() else { return };
        if worker.is_none() {
            *worker = start(self.shared.clone(), self.wake.clone());
        }
        let sent = worker.as_ref().is_some_and(|sender| sender.send(wanted.clone()).is_ok());
        if !sent {
            // No thread could be started: the picture is answered as missing, so the card keeps its stand in.
            self.hand_back(Arrived { key: wanted.key, image: None });
        }
    }

    /// Up to `most` pictures that have arrived, oldest first.
    pub fn take(&self, most: usize) -> Vec<Arrived> {
        let Ok(mut shared) = self.shared.lock() else { return Vec::new() };
        let count = most.min(shared.arrived.len());
        let taken: Vec<Arrived> = shared.arrived.drain(..count).collect();
        shared.waiting = shared.waiting.saturating_sub(taken.len());
        taken
    }

    /// How many pictures have been asked for and not yet taken.
    pub fn waiting(&self) -> usize {
        self.shared.lock().map(|shared| shared.waiting).unwrap_or(0)
    }

    fn hand_back(&self, arrived: Arrived) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.arrived.push_back(arrived);
        }
    }
}

/// The worker: one picture at a time, in the order asked.
fn start(
    shared: Arc<Mutex<Shared>>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) -> Option<Sender<Wanted>> {
    let (sender, receiver) = channel::<Wanted>();
    std::thread::Builder::new()
        .name("unluminous-skills-pictures".into())
        .spawn(move || {
            for wanted in receiver {
                let image = bytes_for(&wanted)
                    .and_then(|bytes| crate::services::picture::decode_bytes(&bytes).ok());
                if let Ok(mut shared) = shared.lock() {
                    shared.arrived.push_back(Arrived { key: wanted.key, image });
                }
                if let Some(wake) = &wake {
                    wake();
                }
            }
        })
        .ok()?;
    Some(sender)
}

/// The picture's bytes from the first place that has the one the site lists, or the best older one.
pub fn bytes_for(wanted: &Wanted) -> Option<Vec<u8>> {
    let matches = |bytes: &[u8]| wanted.sha256.is_empty() || sha256_of(bytes) == wanted.sha256;
    if let Some(shipped) = wanted.shipped.filter(|bytes| matches(bytes)) {
        return Some(shipped.to_vec());
    }
    let kept = wanted.kept.as_ref().and_then(|path| std::fs::read(path).ok());
    if let Some(kept) = kept.as_ref().filter(|bytes| matches(bytes)) {
        return Some(kept.clone());
    }
    if let Some(fetched) = wanted.address.as_deref().and_then(|address| fetch(address, wanted)) {
        return Some(fetched);
    }
    wanted.shipped.map(<[u8]>::to_vec).or(kept)
}

/// Fetch a picture, check it, and keep it in the plugin's folder.
fn fetch(address: &str, wanted: &Wanted) -> Option<Vec<u8>> {
    let bytes = super::fetch::get(address).ok()?;
    if !wanted.sha256.is_empty() && sha256_of(&bytes) != wanted.sha256 {
        return None;
    }
    if let Some(kept) = &wanted.kept {
        if let Some(folder) = kept.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = crate::services::store::write_atomically(kept, &bytes);
    }
    Some(bytes)
}

/// The SHA-256 of some bytes, as lowercase hex.
pub fn sha256_of(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-skill-pictures-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn wanted(key: &str) -> Wanted {
        Wanted { key: key.into(), shipped: None, kept: None, address: None, sha256: String::new() }
    }

    #[test]
    fn the_shipped_picture_is_used_while_it_is_the_one_the_site_lists() {
        let shipped: &'static [u8] = b"shipped";
        let mut asked = wanted("a");
        asked.shipped = Some(shipped);
        assert_eq!(bytes_for(&asked).as_deref(), Some(shipped));
        asked.sha256 = sha256_of(shipped);
        assert_eq!(bytes_for(&asked).as_deref(), Some(shipped));
    }

    #[test]
    fn a_kept_picture_with_the_listed_hash_is_used_before_the_older_shipped_one() {
        let here = folder("kept");
        let kept = here.join("images").join("a.jpg");
        std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
        std::fs::write(&kept, b"newer").unwrap();
        let mut asked = wanted("a");
        asked.shipped = Some(b"older");
        asked.kept = Some(kept);
        asked.sha256 = sha256_of(b"newer");
        assert_eq!(bytes_for(&asked).as_deref(), Some(&b"newer"[..]));
    }

    #[test]
    fn a_picture_on_the_site_is_checked_and_kept_and_a_wrong_one_is_refused() {
        let site = folder("site");
        std::fs::write(site.join("a.jpg"), b"from the site").unwrap();
        let here = folder("fetched");
        let kept = here.join("images").join("a.jpg");
        let mut asked = wanted("a");
        asked.address = Some(site.join("a.jpg").display().to_string());
        asked.kept = Some(kept.clone());
        asked.sha256 = sha256_of(b"something else");
        assert_eq!(bytes_for(&asked), None, "a picture that is not the one listed is refused");
        assert!(!kept.exists(), "and is not kept");
        asked.sha256 = sha256_of(b"from the site");
        assert_eq!(bytes_for(&asked).as_deref(), Some(&b"from the site"[..]));
        assert_eq!(std::fs::read(&kept).unwrap(), b"from the site", "it is kept for next time");
    }

    #[test]
    fn with_nothing_newer_to_be_had_the_older_picture_is_shown() {
        let mut asked = wanted("a");
        asked.shipped = Some(b"older");
        asked.sha256 = sha256_of(b"newer");
        assert_eq!(bytes_for(&asked).as_deref(), Some(&b"older"[..]));
    }

    #[test]
    fn the_worker_hands_each_picture_back_once_and_counts_what_is_waiting() {
        let loader = Loader::default();
        let mut asked = wanted("missing");
        asked.shipped = Some(b"not a picture");
        loader.ask(asked.clone());
        loader.ask(asked);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut arrived = Vec::new();
        while arrived.is_empty() && std::time::Instant::now() < deadline {
            arrived = loader.take(4);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(arrived.len(), 1, "asked twice, answered once");
        assert!(arrived[0].image.is_none(), "bytes that are not a picture are the stand in");
        assert_eq!(loader.waiting(), 0);
    }
}
