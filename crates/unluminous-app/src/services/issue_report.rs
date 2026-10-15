//! Reporting an issue to GitHub from inside Unluminous (`task-2276`).
//!
//! *"We want users of Unluminous to be able to easily report issues/bugs/feature requests to our github
//! through the app ... We want as little friction as possible for the user, as we value bug reports and
//! issues."* `tasks/task-2276-report-an-issue-tdd.md` is the design.
//!
//! **Unluminous never posts the issue itself.** It opens the person's own browser on GitHub's new issue
//! form with the title and the body already written, the way opencode does, and the person presses
//! GitHub's own `Submit new issue` while signed in as themselves. Posting directly would need a token, and
//! a token in the binary is a credential anybody can take out of it and use to write as Black Rainbow
//! Labs. The person is also the one GitHub tells when the issue is answered.
//!
//! **What cannot travel in an address is put where one key press attaches it.** A picture or a log file
//! cannot be part of a link. So the attachments are copied into one folder, and that folder's files are put
//! on the system clipboard as files: Chromium and Firefox both upload files pasted into a GitHub text area,
//! so attaching every one of them is a single `Ctrl`+`V` in the issue's description. A log that fits is
//! written into the body as text and needs nothing pasted at all.
//!
//! Everything here is a pure function of what it is given, apart from [`System::read`], [`logs`],
//! [`picture_from_the_clipboard`] and [`prepare`], which read or write the disk and the clipboard and say
//! so in their names.

use std::path::{Path, PathBuf};

/// The public repository the issue goes to. `jasonmcaffee/unluminous` is private, and nobody but its owner
/// can see it, let alone file an issue on it.
pub const REPOSITORY: &str = "Black-Rainbow-Labs/Unluminous";

/// The longest address GitHub reliably accepts for a new issue. Its servers refuse a request line much past
/// 8 KB, so the body is cut to keep the whole address under this.
pub const ADDRESS_LIMIT: usize = 8_000;

/// How many lines of the crash log go into a report.
pub const LOG_LINES: usize = 200;

/// What kind of report this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Bug,
    Feature,
    Question,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Bug, Kind::Feature, Kind::Question];

    /// What the kind is called on the modal's segmented control.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Bug => "Bug",
            Kind::Feature => "Feature request",
            Kind::Question => "Question",
        }
    }

    /// What the command line calls it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Bug => "bug",
            Kind::Feature => "feature",
            Kind::Question => "question",
        }
    }

    /// Read a kind from the command line, accepting the label GitHub uses as well.
    pub fn parse(said: &str) -> Option<Kind> {
        match said.trim().to_ascii_lowercase().as_str() {
            "bug" => Some(Kind::Bug),
            "feature" | "feature-request" | "feature request" | "enhancement" => Some(Kind::Feature),
            "question" => Some(Kind::Question),
            _ => None,
        }
    }

    /// What goes in front of the title. A label in the address is applied only when the person can triage
    /// the repository, so the kind is written into the title as well, where it always arrives.
    pub fn prefix(self) -> &'static str {
        match self {
            Kind::Bug => "[Bug]",
            Kind::Feature => "[Feature]",
            Kind::Question => "[Question]",
        }
    }

    /// GitHub's own label for the kind.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Bug => "bug",
            Kind::Feature => "enhancement",
            Kind::Question => "question",
        }
    }

    /// What the description box says before anything is typed into it.
    pub fn hint(self) -> &'static str {
        match self {
            Kind::Bug => "What happened, what you expected instead, and the steps that make it happen.",
            Kind::Feature => "What you would like Unluminous to do, and what it would help you with.",
            Kind::Question => "What you would like to know.",
        }
    }
}

/// One file attached to a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub path: PathBuf,
    /// The name it is attached under, which is its file name.
    pub name: String,
    pub bytes: u64,
    /// Whether it is a picture, which the modal shows as a thumbnail.
    pub picture: bool,
}

impl Attachment {
    /// The file at `path`, refused with its path when it is not a file that can be read.
    pub fn of(path: &Path) -> Result<Attachment, String> {
        let metadata = std::fs::metadata(path)
            .map_err(|problem| format!("{} cannot be attached: {problem}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!("{} is not a file, so it cannot be attached.", path.display()));
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "attachment".to_owned());
        Ok(Attachment {
            path: path.to_path_buf(),
            picture: is_a_picture(&name),
            name,
            bytes: metadata.len(),
        })
    }

    /// How big it is, the way a person says it: `812 B`, `14 KB`, `2.4 MB`.
    pub fn size(&self) -> String {
        size_of(self.bytes)
    }
}

/// `812 B`, `14 KB`, `2.4 MB`.
pub fn size_of(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{} KB", bytes / 1024),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// Whether a file name is one of the pictures GitHub shows in an issue.
pub fn is_a_picture(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|extension| lower.ends_with(extension))
}

/// Everything a person has said in the modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub kind: Kind,
    pub title: String,
    pub description: String,
    pub attachments: Vec<Attachment>,
    /// Write the end of the crash log into the issue.
    pub include_logs: bool,
    /// Write the version and the system into the issue.
    pub include_system: bool,
}

impl Default for Report {
    fn default() -> Self {
        Report {
            kind: Kind::Bug,
            title: String::new(),
            description: String::new(),
            attachments: Vec::new(),
            include_logs: true,
            include_system: true,
        }
    }
}

impl Report {
    /// Whether there is enough to send: a title. Everything else is optional, because a report with one
    /// line in it is still a report somebody wanted to make.
    pub fn can_be_sent(&self) -> bool {
        !self.title.trim().is_empty()
    }

    /// Attach a file, once: the same path twice is one attachment. A second file with the name of one
    /// already attached is attached under `name-2.ext`, because the files are gathered into one folder and
    /// two of them cannot share a name there.
    pub fn attach(&mut self, mut attachment: Attachment) {
        if self.attachments.iter().any(|kept| kept.path == attachment.path) {
            return;
        }
        let taken = |name: &str, kept: &[Attachment]| kept.iter().any(|other| other.name == name);
        if taken(&attachment.name, &self.attachments) {
            let (stem, extension) = match attachment.name.rfind('.') {
                Some(dot) if dot > 0 => attachment.name.split_at(dot),
                _ => (attachment.name.as_str(), ""),
            };
            let free = (2..)
                .map(|number| format!("{stem}-{number}{extension}"))
                .find(|candidate| !taken(candidate, &self.attachments))
                .expect("an unused name is always found");
            attachment.name = free;
        }
        self.attachments.push(attachment);
    }

    /// Take an attachment off by its name. Answers whether there was one.
    pub fn detach(&mut self, name: &str) -> bool {
        let before = self.attachments.len();
        self.attachments.retain(|kept| kept.name != name);
        self.attachments.len() != before
    }

    /// The whole title, with the kind in front of it.
    pub fn full_title(&self) -> String {
        format!("{} {}", self.kind.prefix(), self.title.trim())
    }
}

/// The version and the system, written into the issue so the first question is already answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct System {
    pub version: String,
    pub built: String,
    pub os: String,
    pub architecture: String,
    pub theme: String,
    pub plugins: Vec<String>,
}

impl System {
    /// This build, on this machine, with the theme and the plugins the window has.
    pub fn read(theme: &str, plugins: Vec<String>) -> System {
        System {
            version: crate::build_info::VERSION.to_owned(),
            built: crate::build_info::BUILD_DATE.to_owned(),
            os: os_version(),
            architecture: std::env::consts::ARCH.to_owned(),
            theme: theme.to_owned(),
            plugins,
        }
    }

    /// One line saying what goes, for beside the switch on the modal, so nothing is sent unseen.
    pub fn line(&self) -> String {
        format!("Unluminous {} \u{00B7} {} \u{00B7} {}", self.version, self.os, self.architecture)
    }

    /// The table written into the issue.
    fn table(&self) -> String {
        let plugins = match self.plugins.is_empty() {
            true => "none".to_owned(),
            false => self.plugins.join(", "),
        };
        format!(
            "| | |\n|---|---|\n| Unluminous | {} (built {}) |\n| Operating system | {} |\n| Architecture | {} |\n| Theme | {} |\n| Plugins | {} |\n",
            cell(&self.version),
            cell(&self.built),
            cell(&self.os),
            cell(&self.architecture),
            cell(&self.theme),
            cell(&plugins),
        )
    }
}

/// A value made safe to put in a Markdown table cell.
fn cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

/// The operating system and its version, as a person names it: `Windows 11 Pro 24H2 (build 26100)`,
/// `macOS Version 15.1 (Build 24B83)`, `Ubuntu 24.04.1 LTS`.
pub fn os_version() -> String {
    platform::os_version()
}

#[cfg(windows)]
mod platform {
    /// Read from the registry's `CurrentVersion` key, which is what `winver` reads.
    ///
    /// `ProductName` still says `Windows 10` on Windows 11, which Microsoft has left as it is for
    /// compatibility, so a build number of 22000 or more is Windows 11 whatever the name says.
    pub fn os_version() -> String {
        let name = read("ProductName").unwrap_or_else(|| "Windows".to_owned());
        let build = read("CurrentBuildNumber").unwrap_or_default();
        let release = read("DisplayVersion").unwrap_or_default();
        let name = match build.parse::<u32>() {
            Ok(number) if number >= 22_000 => name.replacen("Windows 10", "Windows 11", 1),
            _ => name,
        };
        let mut said = name;
        if !release.is_empty() {
            said = format!("{said} {release}");
        }
        if !build.is_empty() {
            said = format!("{said} (build {build})");
        }
        said
    }

    fn read(value: &str) -> Option<String> {
        use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
        let wide = |text: &str| text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let key = wide("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
        let name = wide(value);
        let mut buffer = [0u16; 256];
        let mut size = (buffer.len() * 2) as u32;
        // Safe: every pointer is to a buffer that outlives the call, and `size` says how large `buffer` is.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        let length = (size as usize / 2).saturating_sub(1).min(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..length]);
        let text = text.trim_end_matches('\0').trim().to_owned();
        (!text.is_empty()).then_some(text)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    /// `NSProcessInfo`'s own sentence, which says the version and the build.
    pub fn os_version() -> String {
        let said = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersionString();
        format!("macOS {said}")
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    /// `PRETTY_NAME` from `/etc/os-release`.
    pub fn os_version() -> String {
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|text| {
                text.lines().find_map(|line| {
                    line.strip_prefix("PRETTY_NAME=").map(|value| value.trim_matches('"').to_owned())
                })
            })
            .unwrap_or_else(|| std::env::consts::OS.to_owned())
    }
}

/// The end of the crash log in `settings`, older file first, at most `limit` lines. `None` when Unluminous
/// has never written one, which on most machines is always.
pub fn logs(settings: Option<&Path>, limit: usize) -> Option<String> {
    let settings = settings?;
    let mut lines = Vec::new();
    for file in [crate::services::crash_log::OLD_FILE, crate::services::crash_log::FILE] {
        if let Ok(text) = std::fs::read_to_string(settings.join(file)) {
            lines.extend(text.lines().map(str::to_owned));
        }
    }
    if lines.iter().all(|line| line.trim().is_empty()) {
        return None;
    }
    let start = lines.len().saturating_sub(limit);
    Some(lines[start..].join("\n"))
}

/// Encode `text` for a query string: the characters RFC 3986 calls unreserved pass as they are, and every
/// other byte of its UTF-8 is `%XX`.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The name a log that did not fit in the issue is attached under.
pub const LOG_FILE: &str = "unluminous-log.txt";
/// The name a description that did not fit is attached under.
pub const DESCRIPTION_FILE: &str = "description.txt";

/// The issue, ready to open: the address, the body written into it, and what had to be cut to fit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub address: String,
    pub title: String,
    pub body: String,
    /// The log did not fit whole, so it is attached as [`LOG_FILE`].
    pub logs_cut: bool,
    /// The description did not fit whole, so it is attached as [`DESCRIPTION_FILE`].
    pub description_cut: bool,
    /// The names of every file to attach, in order.
    pub attached: Vec<String>,
}

/// Write the issue for `report`, cutting the log and then the description until the address fits under
/// [`ADDRESS_LIMIT`]. Whatever is cut is attached whole, and the body says so where it was cut.
pub fn issue(report: &Report, system: Option<&System>, logs: Option<&str>) -> Issue {
    let title = report.full_title();
    let mut log_lines: Option<Vec<&str>> = logs.map(|logs| logs.lines().collect());
    let mut description: String = report.description.trim().to_owned();
    let mut logs_cut = false;
    let mut description_cut = false;
    loop {
        let mut attached: Vec<String> =
            report.attachments.iter().map(|attachment| attachment.name.clone()).collect();
        if logs_cut && logs.is_some() {
            attached.push(LOG_FILE.to_owned());
        }
        if description_cut {
            attached.push(DESCRIPTION_FILE.to_owned());
        }
        let shown_logs = log_lines.as_ref().map(|lines| lines.join("\n"));
        let body = body(
            report,
            &description,
            description_cut,
            system,
            shown_logs.as_deref(),
            logs_cut,
            &attached,
        );
        let address = address(&title, report.kind.label(), &body);
        if address.len() <= ADDRESS_LIMIT {
            return Issue { address, title, body, logs_cut, description_cut, attached };
        }
        // The log goes first, a half at a time from its oldest end, because its newest lines are the ones
        // that say what went wrong; then the description, from its end.
        match log_lines.as_mut() {
            Some(lines) if !lines.is_empty() => {
                logs_cut = true;
                let keep = lines.len() / 2;
                let start = lines.len() - keep;
                *lines = lines[start..].to_vec();
                if lines.is_empty() {
                    log_lines = None;
                }
            }
            _ => {
                logs_cut = logs_cut || logs.is_some();
                log_lines = None;
                let characters: Vec<char> = description.chars().collect();
                if characters.is_empty() {
                    // Nothing left to cut but the title, which GitHub cuts itself.
                    return Issue { address, title, body, logs_cut, description_cut, attached };
                }
                description_cut = true;
                let keep = characters.len() * 3 / 4;
                description = characters[..keep].iter().collect();
            }
        }
    }
}

/// The issue's Markdown.
fn body(
    report: &Report,
    description: &str,
    description_cut: bool,
    system: Option<&System>,
    logs: Option<&str>,
    logs_cut: bool,
    attached: &[String],
) -> String {
    let mut out = String::new();
    match description.is_empty() {
        true => out.push_str("_No description._\n"),
        false => {
            out.push_str(description);
            out.push('\n');
        }
    }
    if description_cut {
        out.push_str(&format!(
            "\n_The description was too long for a link and was cut here. All of it is in `{DESCRIPTION_FILE}`._\n"
        ));
    }
    if !attached.is_empty() {
        let names: Vec<String> = attached.iter().map(|name| format!("`{name}`")).collect();
        out.push_str("\n### Attachments\n\n");
        // A comment, so it shows in the editor while the issue is being written and is gone once it is
        // submitted.
        out.push_str(&format!(
            "<!-- Unluminous put {} on the clipboard: {}. Click on the empty line below and press Ctrl+V (Cmd+V on a Mac) to attach them. -->\n\n",
            match attached.len() {
                1 => "1 file".to_owned(),
                count => format!("{count} files"),
            },
            names.join(", ")
        ));
    }
    if let Some(system) = system {
        out.push_str("\n### System\n\n");
        out.push_str(&system.table());
    }
    if let Some(logs) = logs.filter(|logs| !logs.trim().is_empty()) {
        let said = match logs_cut {
            true => format!("Log (the newest lines; all of it is in `{LOG_FILE}`)"),
            false => "Log".to_owned(),
        };
        out.push_str(&format!("\n<details><summary>{said}</summary>\n\n```\n{logs}\n```\n\n</details>\n"));
    } else if logs_cut && report.include_logs {
        out.push_str(&format!("\n_The log is in `{LOG_FILE}`._\n"));
    }
    out.push_str("\n_Sent from Unluminous._\n");
    out
}

/// The new issue address on [`REPOSITORY`], with the title, the label and the body in its query string.
pub fn address(title: &str, label: &str, body: &str) -> String {
    format!(
        "https://github.com/{REPOSITORY}/issues/new?title={}&labels={}&body={}",
        encode(title),
        encode(label),
        encode(body)
    )
}

/// A folder of its own for one report's files, under the temporary folder: `unluminous-report-<stamp>`.
pub fn new_folder() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("unluminous-report-{stamp}"))
}

/// Gather everything to attach into `folder`: each attachment, copied there unless it is there already,
/// and the whole log and the whole description when the issue had to cut them. Answers the files, in the
/// order the issue names them.
pub fn prepare(
    report: &Report,
    issue: &Issue,
    logs: Option<&str>,
    folder: &Path,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(folder)
        .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    let mut files = Vec::new();
    for attachment in &report.attachments {
        let target = folder.join(&attachment.name);
        if attachment.path != target {
            std::fs::copy(&attachment.path, &target).map_err(|problem| {
                format!("{} could not be copied: {problem}", attachment.path.display())
            })?;
        }
        files.push(target);
    }
    if issue.logs_cut {
        if let Some(logs) = logs {
            let target = folder.join(LOG_FILE);
            std::fs::write(&target, logs)
                .map_err(|problem| format!("{} could not be written: {problem}", target.display()))?;
            files.push(target);
        }
    }
    if issue.description_cut {
        let target = folder.join(DESCRIPTION_FILE);
        std::fs::write(&target, report.description.trim())
            .map_err(|problem| format!("{} could not be written: {problem}", target.display()))?;
        files.push(target);
    }
    Ok(files)
}

/// A name for the next picture in `folder` that is not taken: `stem.png`, then `stem-2.png` and on.
pub fn free_name(folder: &Path, stem: &str) -> PathBuf {
    let first = folder.join(format!("{stem}.png"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|number| folder.join(format!("{stem}-{number}.png")))
        .find(|path| !path.exists())
        .expect("an unused name is always found")
}

/// Write a picture the window took into `path` as a PNG. egui holds colour premultiplied by alpha and a PNG
/// holds it straight, so it is converted on the way.
pub fn write_picture(image: &egui::ColorImage, path: &Path) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    }
    let mut bytes = Vec::with_capacity(image.pixels.len() * 4);
    for pixel in &image.pixels {
        bytes.extend_from_slice(&pixel.to_srgba_unmultiplied());
    }
    let buffer = image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, bytes)
        .ok_or_else(|| "the picture was the wrong size".to_owned())?;
    buffer.save(path).map_err(|problem| format!("{} could not be written: {problem}", path.display()))
}

/// The picture on the clipboard, written into `folder` as `pasted.png`. `None` when the clipboard holds no
/// picture, which is not a fault: the paste chord arrives after an ordinary text paste too.
pub fn picture_from_the_clipboard(folder: &Path) -> Result<Option<PathBuf>, String> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|problem| format!("the clipboard could not be opened: {problem}"))?;
    let held = match clipboard.get_image() {
        Ok(held) => held,
        Err(arboard::Error::ContentNotAvailable) => return Ok(None),
        Err(problem) => return Err(format!("the picture on the clipboard could not be read: {problem}")),
    };
    let buffer =
        image::RgbaImage::from_raw(held.width as u32, held.height as u32, held.bytes.into_owned())
            .ok_or_else(|| "the picture on the clipboard could not be read.".to_owned())?;
    std::fs::create_dir_all(folder)
        .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    let path = free_name(folder, "pasted");
    buffer.save(&path).map_err(|problem| format!("{} could not be written: {problem}", path.display()))?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_system() -> System {
        System {
            version: "0.86.1".into(),
            built: "2026-10-15 1:00pm".into(),
            os: "Windows 11 Pro 24H2 (build 26100)".into(),
            architecture: "x86_64".into(),
            theme: "Unluminous Dark".into(),
            plugins: vec!["Rust".into(), "CSS".into()],
        }
    }

    fn a_report() -> Report {
        Report {
            title: "The flag does nothing".into(),
            description: "I pressed it.\nNothing opened.".into(),
            ..Report::default()
        }
    }

    /// Undo [`encode`], for reading an address back in a test.
    fn decode(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] == b'%' {
                out.push(u8::from_str_radix(&text[at + 1..at + 3], 16).unwrap());
                at += 3;
            } else {
                out.push(bytes[at]);
                at += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    fn query(address: &str, name: &str) -> String {
        let (_, query) = address.split_once('?').unwrap();
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")).map(decode))
            .unwrap_or_default()
    }

    #[test]
    fn encoding_passes_the_unreserved_characters_and_escapes_every_other_byte() {
        assert_eq!(encode("abc-XYZ_0.9~"), "abc-XYZ_0.9~");
        assert_eq!(encode("a b&c=d?e#f/g"), "a%20b%26c%3Dd%3Fe%23f%2Fg");
        assert_eq!(encode("\n"), "%0A");
        assert_eq!(encode("é"), "%C3%A9", "UTF-8, one escape a byte");
        assert_eq!(encode("🚩"), "%F0%9F%9A%A9");
    }

    #[test]
    fn the_address_is_a_new_issue_on_the_public_repository() {
        let issue = issue(&a_report(), None, None);
        assert!(issue
            .address
            .starts_with("https://github.com/Black-Rainbow-Labs/Unluminous/issues/new?title="));
        assert_eq!(query(&issue.address, "title"), "[Bug] The flag does nothing");
        assert_eq!(query(&issue.address, "labels"), "bug");
        assert!(query(&issue.address, "body").starts_with("I pressed it.\nNothing opened."));
    }

    #[test]
    fn each_kind_has_its_own_prefix_and_label() {
        for (kind, prefix, label) in [
            (Kind::Bug, "[Bug]", "bug"),
            (Kind::Feature, "[Feature]", "enhancement"),
            (Kind::Question, "[Question]", "question"),
        ] {
            let report = Report { kind, ..a_report() };
            let issue = issue(&report, None, None);
            assert!(issue.title.starts_with(prefix), "{kind:?}");
            assert_eq!(query(&issue.address, "labels"), label);
            assert_eq!(Kind::parse(kind.name()), Some(kind));
        }
        assert_eq!(Kind::parse("enhancement"), Some(Kind::Feature));
        assert_eq!(Kind::parse("nonsense"), None);
    }

    #[test]
    fn the_system_goes_in_as_a_table_and_only_when_asked() {
        let with = issue(&a_report(), Some(&a_system()), None);
        assert!(with.body.contains("### System"));
        assert!(with.body.contains("| Unluminous | 0.86.1 (built 2026-10-15 1:00pm) |"));
        assert!(with.body.contains("| Plugins | Rust, CSS |"));
        let without = issue(&a_report(), None, None);
        assert!(!without.body.contains("### System"));
    }

    #[test]
    fn a_log_that_fits_is_written_in_and_nothing_is_attached_for_it() {
        let issue = issue(&a_report(), None, Some("panicked at src/app.rs:12\nsecond line"));
        assert!(issue.body.contains("<details><summary>Log</summary>"));
        assert!(issue.body.contains("panicked at src/app.rs:12\nsecond line"));
        assert!(!issue.logs_cut);
        assert!(issue.attached.is_empty());
    }

    #[test]
    fn an_enormous_log_is_cut_from_its_oldest_end_and_attached_whole() {
        let logs: Vec<String> =
            (0..5000).map(|line| format!("line {line} of a very chatty log file")).collect();
        let logs = logs.join("\n");
        let issue = issue(&a_report(), Some(&a_system()), Some(&logs));
        assert!(issue.address.len() <= ADDRESS_LIMIT, "{}", issue.address.len());
        assert!(issue.logs_cut);
        assert!(issue.attached.contains(&LOG_FILE.to_owned()));
        assert!(issue.body.contains("line 4999 of"), "the newest line is kept");
        assert!(!issue.body.contains("line 0 of"), "the oldest is what is cut");
        assert!(issue.body.contains(LOG_FILE), "the body says where the rest is");
    }

    #[test]
    fn an_enormous_description_is_cut_last_and_attached_whole() {
        let report = Report { description: "word ".repeat(4000), ..a_report() };
        let issue = issue(&report, Some(&a_system()), Some("a short log"));
        assert!(issue.address.len() <= ADDRESS_LIMIT);
        assert!(issue.description_cut);
        assert!(issue.attached.contains(&DESCRIPTION_FILE.to_owned()));
        assert!(issue.body.contains("was cut here"));
        assert!(issue.body.contains("### System"), "the system table survives the cut");
    }

    #[test]
    fn the_attachments_are_named_in_a_comment_that_says_how_to_attach_them() {
        let mut report = a_report();
        report.attachments.push(Attachment {
            path: PathBuf::from("window.png"),
            name: "window.png".into(),
            bytes: 10,
            picture: true,
        });
        let issue = issue(&report, None, None);
        assert!(issue.body.contains("<!-- Unluminous put 1 file on the clipboard: `window.png`."));
        assert_eq!(issue.attached, ["window.png"]);
    }

    #[test]
    fn a_report_needs_a_title_and_nothing_else() {
        assert!(!Report::default().can_be_sent());
        assert!(!Report { title: "   ".into(), ..Report::default() }.can_be_sent());
        assert!(Report { title: "x".into(), ..Report::default() }.can_be_sent());
    }

    #[test]
    fn attaching_the_same_file_twice_is_one_attachment_and_detaching_is_by_name() {
        let folder = std::env::temp_dir().join(format!("unluminous-report-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("notes.txt");
        std::fs::write(&file, "hello").unwrap();
        let mut report = a_report();
        report.attach(Attachment::of(&file).unwrap());
        report.attach(Attachment::of(&file).unwrap());
        assert_eq!(report.attachments.len(), 1);
        assert_eq!(report.attachments[0].size(), "5 B");
        assert!(!report.attachments[0].picture);
        let twin = folder.join("twin");
        std::fs::create_dir_all(&twin).unwrap();
        std::fs::write(twin.join("notes.txt"), "other").unwrap();
        report.attach(Attachment::of(&twin.join("notes.txt")).unwrap());
        assert_eq!(report.attachments[1].name, "notes-2.txt", "two files cannot share a name");
        assert!(report.detach("notes-2.txt"));
        std::fs::remove_file(twin.join("notes.txt")).unwrap();
        std::fs::remove_dir(&twin).unwrap();
        assert!(report.detach("notes.txt"));
        assert!(!report.detach("notes.txt"));
        let missing = Attachment::of(&folder.join("not-there.png")).unwrap_err();
        assert!(missing.contains("not-there.png"), "{missing}");
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(&folder).unwrap();
    }

    #[test]
    fn preparing_copies_the_attachments_and_writes_what_was_cut() {
        let source =
            std::env::temp_dir().join(format!("unluminous-report-source-{}", std::process::id()));
        let folder = std::env::temp_dir().join(format!("unluminous-report-out-{}", std::process::id()));
        std::fs::create_dir_all(&source).unwrap();
        let file = source.join("trace.txt");
        std::fs::write(&file, "trace").unwrap();
        let mut report = a_report();
        report.attach(Attachment::of(&file).unwrap());
        let logs = "x".repeat(20_000);
        let issue = issue(&report, None, Some(&logs));
        let files = prepare(&report, &issue, Some(&logs), &folder).unwrap();
        assert_eq!(files, [folder.join("trace.txt"), folder.join(LOG_FILE)]);
        assert_eq!(std::fs::read_to_string(folder.join(LOG_FILE)).unwrap().len(), 20_000);
        for path in [folder.join("trace.txt"), folder.join(LOG_FILE), file] {
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_dir(&folder).unwrap();
        std::fs::remove_dir(&source).unwrap();
    }

    #[test]
    fn sizes_are_said_the_way_a_person_says_them() {
        assert_eq!(size_of(812), "812 B");
        assert_eq!(size_of(14 * 1024 + 3), "14 KB");
        assert_eq!(size_of(2_500_000), "2.4 MB");
    }

    #[test]
    fn the_operating_system_is_named() {
        let said = os_version();
        assert!(!said.trim().is_empty());
        #[cfg(windows)]
        assert!(said.starts_with("Windows"), "{said}");
    }
}
