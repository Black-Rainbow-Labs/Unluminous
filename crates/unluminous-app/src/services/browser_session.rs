//! What a person's browsing leaves behind between runs: the session cookies, and where they have been.
//!
//! `task-2203`: *"we don't seem to be retaining session history. We want that to be retained between
//! restarts and shared across projects, survive upgrades of Unluminous."* The browser profile already
//! lived in the settings folder, which is per person and which no installer touches. Two things did not
//! survive it, and this module holds both:
//!
//! - **Session cookies**, the ones with no expiry. The engine keeps them for as long as its browser
//!   process lives and writes none of them down, so a sign in that set one was gone when the last window
//!   closed. Chrome and Edge keep them only under "Continue where you left off"; WebView2 has no such
//!   switch, so Unluminous reads them through the engine's cookie manager and gives them back the next
//!   time a view is made. [`SessionCookies`] is the file.
//! - **Where a person has been.** The engine keeps a history database of its own, in its own format,
//!   which nothing in Unluminous reads. [`Visits`] is Unluminous's own list, offered under the address
//!   field of every browser tab and node in every project.
//!
//! Nothing here touches an engine, so all of it is tested with no window.
//! `tasks/task-2203-browser-sessions-and-pinned-elements-tdd.md` is the design.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// One cookie as it is kept on disk: everything the engine needs to put it back, and no expiry, because
/// only the cookies with none are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    /// `Strict`, `Lax` or `None`, as the engine reported it, or empty when it reported nothing.
    pub same_site: String,
}

/// The session cookies, in the profile folder beside the engine's own files.
pub struct SessionCookies {
    file: PathBuf,
}

impl SessionCookies {
    /// The file inside a browser profile folder.
    pub fn in_profile(profile: &Path) -> Self {
        Self { file: profile.join("session-cookies") }
    }

    /// Where the file is.
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// What the file holds. A missing file, one that will not decrypt and one that will not parse are all
    /// read as no cookies, because a session that cannot be restored is a session that starts signed out,
    /// which is what happened before any of this existed.
    pub fn read(&self) -> Vec<KeptCookie> {
        let Ok(bytes) = std::fs::read(&self.file) else { return Vec::new() };
        let Some(plain) = unprotect(&bytes) else { return Vec::new() };
        from_json(&String::from_utf8_lossy(&plain))
    }

    /// Replace the file with these cookies, written aside and renamed so a window that dies part way
    /// through leaves the previous file.
    ///
    /// **The whole set every time.** Every Unluminous window shares one browser process and therefore one
    /// cookie jar, so whichever window writes last writes everything any of them knows.
    pub fn write(&self, cookies: &[KeptCookie]) -> Result<(), String> {
        let plain = to_json(cookies);
        let bytes =
            protect(plain.as_bytes()).ok_or("The session cookies could not be encrypted.")?;
        write_aside_and_rename(&self.file, &bytes)
    }
}

/// The cookies as one JSON array, which is what [`protect`] encrypts.
fn to_json(cookies: &[KeptCookie]) -> String {
    let list: Vec<Value> = cookies
        .iter()
        .map(|cookie| {
            json!({
                "name": cookie.name,
                "value": cookie.value,
                "domain": cookie.domain,
                "path": cookie.path,
                "secure": cookie.secure,
                "httpOnly": cookie.http_only,
                "sameSite": cookie.same_site,
            })
        })
        .collect();
    Value::Array(list).to_string()
}

/// Read [`to_json`] back, skipping any entry with no name or no domain.
fn from_json(text: &str) -> Vec<KeptCookie> {
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let text = |entry: &Value, key: &str| entry[key].as_str().unwrap_or_default().to_owned();
    list.iter()
        .map(|entry| KeptCookie {
            name: text(entry, "name"),
            value: text(entry, "value"),
            domain: text(entry, "domain"),
            path: text(entry, "path"),
            secure: entry["secure"].as_bool().unwrap_or(false),
            http_only: entry["httpOnly"].as_bool().unwrap_or(false),
            same_site: text(entry, "sameSite"),
        })
        .filter(|cookie| !cookie.name.is_empty() && !cookie.domain.is_empty())
        .collect()
}

/// Write `bytes` through a temporary beside `file`, readable by this person only on macOS and Linux.
/// Windows protects the cookies by encrypting them instead.
fn write_aside_and_rename(file: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder).map_err(|problem| {
            format!("Unluminous could not make {}: {problem}", folder.display())
        })?;
    }
    crate::services::store::write_atomically_private(file, bytes)
        .map_err(|problem| format!("Unluminous could not write {}: {problem}", file.display()))
}

/// Encrypt for the current Windows user with `CryptProtectData`, which is what Chromium does with its own
/// cookie values. Elsewhere the bytes are returned as they are and the file's permissions protect them.
#[cfg(windows)]
fn protect(plain: &[u8]) -> Option<Vec<u8>> {
    dpapi(plain, true)
}

#[cfg(not(windows))]
fn protect(plain: &[u8]) -> Option<Vec<u8>> {
    Some(plain.to_vec())
}

/// The inverse of [`protect`].
#[cfg(windows)]
fn unprotect(sealed: &[u8]) -> Option<Vec<u8>> {
    dpapi(sealed, false)
}

#[cfg(not(windows))]
fn unprotect(sealed: &[u8]) -> Option<Vec<u8>> {
    Some(sealed.to_vec())
}

/// One call to `CryptProtectData` or `CryptUnprotectData`, with no prompt and no extra entropy.
#[cfg(windows)]
fn dpapi(input: &[u8], seal: bool) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let given = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(input.len()).ok()?,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // Safety: `given` borrows `input` for the length of the call, and `out` is filled by the system with
    // memory this function frees with `LocalFree` after copying it.
    let worked = unsafe {
        match seal {
            true => CryptProtectData(
                &given,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            ),
            false => CryptUnprotectData(
                &given,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            ),
        }
    };
    if worked == 0 || out.pbData.is_null() {
        return None;
    }
    // Safety: the system says `out.pbData` holds `out.cbData` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    // Safety: the system allocated it with `LocalAlloc`, and it is freed once.
    unsafe { LocalFree(out.pbData.cast()) };
    Some(bytes)
}

// ------------------------------------------------------------------------------------- visits

/// How many addresses are kept. The oldest go first once there are more.
pub const MOST_VISITS: usize = 500;

/// How many are offered under an address field at once.
pub const MOST_OFFERED: usize = 6;

/// One address a person has been to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub url: String,
    pub title: String,
    /// How many times a page has finished loading there.
    pub count: u32,
    /// When it last did, in seconds since 1970.
    pub last: u64,
}

/// Where a person has been, in every project: `<settings folder>/browser-history.conf`.
///
/// One line an address, `<count> <last> <url> <title>` with tabs between them, newest first. A tab or a line
/// break in a title is written as a space, and an address cannot hold either.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Visits {
    file: Option<PathBuf>,
    visits: Vec<Visit>,
}

impl Visits {
    /// The list in a settings folder, read now.
    pub fn in_folder(folder: &Path) -> Self {
        let file = folder.join("browser-history.conf");
        let visits = std::fs::read_to_string(&file).map(|text| parse(&text)).unwrap_or_default();
        Self { file: Some(file), visits }
    }

    /// Every address, newest first.
    pub fn all(&self) -> &[Visit] {
        &self.visits
    }

    /// Read the file again, so a page another window visited is offered here too.
    pub fn reload(&mut self) {
        if let Some(file) = &self.file {
            self.visits =
                std::fs::read_to_string(file).map(|text| parse(&text)).unwrap_or_default();
        }
    }

    /// Note that a page finished loading at `url`, and write the list down.
    ///
    /// **A local page is not kept.** `unluminous://tab-3/page.html` names a tab and, through it, a project, so
    /// in another project or another run it names nothing.
    pub fn visited(&mut self, url: &str, now: u64) {
        if !is_worth_keeping(url) {
            return;
        }
        // Read first, so two windows writing in turn keep each other's visits.
        self.reload();
        let (count, title) = match self.visits.iter().position(|visit| visit.url == url) {
            Some(at) => {
                let was = self.visits.remove(at);
                (was.count.saturating_add(1), was.title)
            }
            None => (1, String::new()),
        };
        self.visits.insert(0, Visit { url: url.to_owned(), title, count, last: now });
        self.visits.truncate(MOST_VISITS);
        self.write();
    }

    /// Note the title a page gave itself, once it has one.
    pub fn titled(&mut self, url: &str, title: &str) {
        let title = one_line(title);
        let Some(visit) = self.visits.iter_mut().find(|visit| visit.url == url) else { return };
        if visit.title == title || title.is_empty() {
            return;
        }
        visit.title = title;
        self.write();
    }

    /// The addresses that match what is being typed, the most visited first and then the most recent.
    ///
    /// Every word typed has to appear in the address or the title, ignoring case. Nothing typed offers
    /// nothing, because a list that opens on an empty field covers the page for no reason.
    pub fn matching(&self, typed: &str, most: usize) -> Vec<&Visit> {
        let words: Vec<String> = typed.split_whitespace().map(|word| word.to_lowercase()).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<&Visit> = self
            .visits
            .iter()
            .filter(|visit| {
                let haystack = format!("{} {}", visit.url, visit.title).to_lowercase();
                words.iter().all(|word| haystack.contains(word.as_str()))
            })
            .filter(|visit| visit.url != typed.trim())
            .collect();
        found.sort_by(|a, b| b.count.cmp(&a.count).then(b.last.cmp(&a.last)));
        found.truncate(most);
        found
    }

    fn write(&self) {
        let Some(file) = &self.file else { return };
        let mut text = String::from(
            "# Unluminous: the addresses visited in its browser tabs and nodes, in every project.\n",
        );
        for visit in &self.visits {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                visit.count,
                visit.last,
                visit.url,
                one_line(&visit.title)
            ));
        }
        let _ = write_aside_and_rename(file, text.as_bytes());
    }
}

/// A list with no file behind it, which is what a window that was given no settings folder has.
impl Visits {
    pub fn in_memory() -> Self {
        Self::default()
    }
}

/// Whether an address belongs in the list: an ordinary web address, not one of Unluminous's own pages.
fn is_worth_keeping(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.starts_with("http://unluminous.")
}

/// A title with its tabs and line breaks turned into spaces.
fn one_line(title: &str) -> String {
    title.replace(['\t', '\n', '\r'], " ").trim().to_owned()
}

/// Read the lines [`Visits::write`] writes, skipping any that are not four fields.
fn parse(text: &str) -> Vec<Visit> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\t');
            let count = fields.next()?.trim().parse().ok()?;
            let last = fields.next()?.trim().parse().ok()?;
            let url = fields.next()?.trim().to_owned();
            let title = fields.next().unwrap_or_default().trim().to_owned();
            is_worth_keeping(&url).then_some(Visit { url, title, count, last })
        })
        .take(MOST_VISITS)
        .collect()
}

/// Seconds since 1970, which is what a visit is stamped with.
pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|since| since.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-browser-session-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("folder");
        folder
    }

    fn a_cookie(name: &str) -> KeptCookie {
        KeptCookie {
            name: name.to_owned(),
            value: "v=1; with \"quotes\"".to_owned(),
            domain: ".example.com".to_owned(),
            path: "/".to_owned(),
            secure: true,
            http_only: true,
            same_site: "Lax".to_owned(),
        }
    }

    /// The session cookies come back exactly as they went, and on Windows the file does not hold them in
    /// the clear.
    #[test]
    fn session_cookies_go_round_the_file_and_are_not_written_in_the_clear() {
        let folder = a_folder("cookies");
        let kept = SessionCookies::in_profile(&folder);
        assert!(kept.read().is_empty(), "no file is no cookies");
        let cookies = vec![a_cookie("session"), a_cookie("other")];
        kept.write(&cookies).expect("written");
        assert_eq!(kept.read(), cookies);
        let on_disk = std::fs::read(kept.file()).expect("file");
        if cfg!(windows) {
            assert!(!String::from_utf8_lossy(&on_disk).contains("session"), "encrypted on Windows");
        }
        std::fs::write(kept.file(), b"not what was written").expect("spoiled");
        assert!(kept.read().is_empty(), "a file that will not read is no cookies");
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The list keeps web addresses only, newest first, counts repeats, keeps the title and is read back.
    #[test]
    fn visits_are_counted_kept_in_order_and_read_back() {
        let folder = a_folder("visits");
        let mut visits = Visits::in_folder(&folder);
        visits.visited("https://example.com/", 10);
        visits.visited("unluminous://tab-3/page.html", 11);
        visits.visited("http://unluminous.tab-3/page.html", 12);
        visits.visited("https://news.ycombinator.com/", 13);
        visits.visited("https://example.com/", 14);
        visits.titled("https://example.com/", "Example\tDomain\n");
        let urls: Vec<&str> = visits.all().iter().map(|visit| visit.url.as_str()).collect();
        assert_eq!(urls, ["https://example.com/", "https://news.ycombinator.com/"]);
        assert_eq!(visits.all()[0].count, 2);
        let back = Visits::in_folder(&folder);
        assert_eq!(back.all(), visits.all());
        assert_eq!(back.all()[0].title, "Example Domain");
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Every word has to match, the address being typed is not offered back, the most visited is first.
    #[test]
    fn what_is_offered_matches_every_word_and_puts_the_most_visited_first() {
        let mut visits = Visits::in_memory();
        visits.visits = vec![
            Visit {
                url: "https://docs.rs/egui".into(),
                title: "egui docs".into(),
                count: 1,
                last: 9,
            },
            Visit {
                url: "https://github.com/emilk/egui".into(),
                title: "egui".into(),
                count: 5,
                last: 1,
            },
            Visit {
                url: "https://example.com/".into(),
                title: "Example".into(),
                count: 9,
                last: 9,
            },
        ];
        let offered: Vec<&str> =
            visits.matching("EGUI", 6).iter().map(|visit| visit.url.as_str()).collect();
        assert_eq!(offered, ["https://github.com/emilk/egui", "https://docs.rs/egui"]);
        assert_eq!(visits.matching("egui docs", 6).len(), 1);
        assert!(visits.matching("   ", 6).is_empty());
        assert!(visits.matching("https://example.com/", 6).is_empty());
        assert_eq!(visits.matching("e", 1).len(), 1);
    }

    /// The list never grows past [`MOST_VISITS`], dropping the oldest.
    #[test]
    fn the_list_stops_at_its_limit() {
        let folder = a_folder("limit");
        let mut visits = Visits::in_folder(&folder);
        for at in 0..(MOST_VISITS + 20) {
            visits.visited(&format!("https://e.test/{at}"), 0);
        }
        visits.visited("https://e.test/last", 1);
        assert_eq!(visits.all().len(), MOST_VISITS);
        assert_eq!(visits.all()[0].url, "https://e.test/last");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
