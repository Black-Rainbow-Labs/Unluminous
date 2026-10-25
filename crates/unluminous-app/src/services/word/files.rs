//! Opening and saving what is not a plain `.docx` (task-2359, TDD 5.6): a file Word has to convert, an
//! encrypted document waiting for its password, text and Markdown opened as a document, AutoSave and
//! AutoRecover, and a new document made from Word's own `Normal.dotm`.
//!
//! A document that is not ready yet is a [`Waiting`] under its tab's key, and the tab draws what it is
//! waiting for. A conversion runs on a thread, because Word takes seconds to start.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::components::controls::TextBoxes;

/// What a tab's document is waiting for before it can be shown.
#[derive(Debug)]
pub enum Waiting {
    /// An encrypted document: its bytes, and why the last password was refused.
    Password { path: PathBuf, bytes: Vec<u8>, problem: Option<String> },
    /// Word is converting the file; the thread answers with the `.docx` it made.
    Converting { path: PathBuf, answer: mpsc::Receiver<Result<Vec<u8>, String>> },
    /// The file could not be opened, and why.
    Failed { path: PathBuf, problem: String },
}

impl Waiting {
    /// What the tab says while it waits.
    pub fn sentence(&self) -> String {
        match self {
            Waiting::Password { path, problem, .. } => {
                let name = name_of(path);
                match problem {
                    Some(problem) => format!("{name} is encrypted, and {problem}. Type its password to open it."),
                    None => format!("{name} is encrypted. Type its password to open it, or run plugins run word password <password>."),
                }
            }
            Waiting::Converting { path, .. } => {
                format!("Word is converting {} into a document.", name_of(path))
            }
            Waiting::Failed { problem, .. } => problem.clone(),
        }
    }
}

/// The file name of a path, for a sentence.
pub fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Start Word converting `path` into a `.docx`, on a thread.
pub fn start_converting(path: &Path) -> Waiting {
    let (send, answer) = mpsc::channel();
    let source = path.to_path_buf();
    std::thread::spawn(move || {
        let target = std::env::temp_dir().join(format!(
            "unluminous-converted-{}-{}.docx",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let result = unluminous_word::convert::convert(&source, &target, "docx").and_then(|()| {
            std::fs::read(&target).map_err(|e| format!("Word's conversion could not be read: {e}"))
        });
        let _ = std::fs::remove_file(&target);
        let _ = send.send(result);
    });
    Waiting::Converting { path: path.to_path_buf(), answer }
}

/// Write a document Unluminous holds as `.docx` bytes to `path` in a format only Word writes.
pub fn save_through_word(bytes: &[u8], path: &Path, format: &str) -> Result<(), String> {
    let source = std::env::temp_dir().join(format!(
        "unluminous-save-{}-{}.docx",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::write(&source, bytes)
        .map_err(|e| format!("{} could not be written: {e}", source.display()))?;
    let result = unluminous_word::convert::convert(&source, path, format);
    let _ = std::fs::remove_file(&source);
    result
}

/// Only the released binary reads Word's own template and the machine's locale, so a test's new
/// document is always the shipped blank.
static OFFICE_TEMPLATE: AtomicBool = AtomicBool::new(false);

/// Let new documents start from Word's `Normal.dotm`. `main` calls this.
pub fn allow_the_office_template() {
    OFFICE_TEMPLATE.store(true, Ordering::Relaxed);
}

/// Word's `Normal.dotm`, when this machine has one and the released binary is running.
pub fn office_template() -> Option<PathBuf> {
    if !OFFICE_TEMPLATE.load(Ordering::Relaxed) {
        return None;
    }
    let folder =
        std::env::var_os("APPDATA").map(PathBuf::from)?.join("Microsoft").join("Templates");
    let normal = folder.join("Normal.dotm");
    normal.is_file().then_some(normal)
}

/// The page size a new document gets with no template: Letter where the locale is the United States or
/// Canada, A4 everywhere else, which is what Word's installer chooses.
pub fn locale_page_size(locale: &str) -> &'static str {
    let locale = locale.replace('_', "-").to_ascii_lowercase();
    if locale.ends_with("-us") || locale.ends_with("-ca") || locale == "en" || locale.is_empty() {
        "letter"
    } else {
        "a4"
    }
}

/// The locale of this machine, as `en-US`, when the released binary is running.
pub fn machine_locale() -> Option<String> {
    if !OFFICE_TEMPLATE.load(Ordering::Relaxed) {
        return None;
    }
    #[cfg(windows)]
    {
        let mut buffer = [0u16; 85];
        // SAFETY: the buffer is the size the call is told, and the call writes at most that many units.
        let written = unsafe {
            windows_sys::Win32::Globalization::GetUserDefaultLocaleName(
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            )
        };
        if written > 1 {
            return Some(String::from_utf16_lossy(&buffer[..written as usize - 1]));
        }
    }
    std::env::var("LANG").ok().map(|l| l.split('.').next().unwrap_or_default().to_owned())
}

/// How often AutoRecover writes a copy of a changed document.
pub const RECOVER_EVERY: Duration = Duration::from_secs(10 * 60);
/// How long AutoSave waits after the last change.
pub const AUTOSAVE_AFTER: Duration = Duration::from_secs(5);

/// The AutoRecover copy of the document under `key` in the plugin's settings folder.
pub fn recovery_path(folder: &Path, key: &str) -> PathBuf {
    // FNV-1a, so the name is the same in every build.
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    });
    folder.join("recovery").join(format!("{hash:016x}.asd"))
}

/// The AutoRecover copy for `key` when it is newer than the file it recovers, with when it was written.
pub fn recovery_for(
    folder: &Path,
    key: &str,
    file: Option<&Path>,
) -> Option<(PathBuf, std::time::SystemTime)> {
    let copy = recovery_path(folder, key);
    let written = std::fs::metadata(&copy).ok()?.modified().ok()?;
    let saved = file.and_then(|f| std::fs::metadata(f).ok()).and_then(|m| m.modified().ok());
    match saved {
        Some(saved) if saved >= written => None,
        _ => Some((copy, written)),
    }
}

/// How long ago a copy was written, for the sentence that offers it.
pub fn when(at: std::time::SystemTime) -> String {
    let minutes = std::time::SystemTime::now().duration_since(at).map_or(0, |d| d.as_secs() / 60);
    match minutes {
        0 => "less than a minute ago".to_owned(),
        1 => "a minute ago".to_owned(),
        m if m < 120 => format!("{m} minutes ago"),
        m if m < 48 * 60 => format!("{} hours ago", m / 60),
        m => format!("{} days ago", m / (24 * 60)),
    }
}

/// Take the AutoRecover copy for `key` away, which saving and closing a document does.
pub fn forget_recovery(folder: &Path, key: &str) {
    let _ = std::fs::remove_file(recovery_path(folder, key));
}

/// When a document last changed and was last copied, for AutoSave and AutoRecover.
#[derive(Debug, Clone, Default)]
pub struct Timing {
    /// The revision last seen, and when it was first seen.
    pub seen: (u64, Option<Instant>),
    /// When an AutoRecover copy was last written.
    pub recovered: Option<Instant>,
    /// An AutoRecover copy newer than the file, offered when the document opened.
    pub offer: Option<(PathBuf, String)>,
    /// Whether this document has an AutoRecover copy that saving should take away.
    pub copied: bool,
}

impl Timing {
    /// Note the document's revision now, answering how long it has been since it last changed.
    pub fn note(&mut self, revision: u64, now: Instant) -> Duration {
        if self.seen.0 != revision {
            self.seen = (revision, Some(now));
        }
        self.seen.1.map_or(Duration::ZERO, |at| now.duration_since(at))
    }
}

impl super::Doc {
    /// Put the AutoRecover copy the document was opened with in its place, as a change to save.
    pub fn restore_recovery(&mut self) -> Result<(), String> {
        let (copy, _) = self.timing.offer.take().ok_or("there is no recovered copy to restore")?;
        let mut bytes = std::fs::read(&copy)
            .map_err(|e| format!("{} could not be read: {e}", copy.display()))?;
        if unluminous_word::crypto::is_encrypted(&bytes) {
            let password = self.password.clone().unwrap_or_default();
            bytes = unluminous_word::crypto::decrypt(&bytes, &password)?;
        }
        let document = unluminous_word::read::open(&bytes)?;
        let mut session = unluminous_word::session::Session::new(document);
        session.author = self.session.author.clone();
        session.file = self.session.file.clone();
        session.autocorrect_rules = self.session.autocorrect_rules.clone();
        session.modified = true;
        session.revision = self.session.revision + 1;
        self.session = session;
        self.saved = 0;
        self.laid = 0;
        self.layout_cache = Default::default();
        Ok(())
    }

    /// Keep the file, and throw the AutoRecover copy away.
    pub fn discard_recovery(&mut self) {
        if let Some((copy, _)) = self.timing.offer.take() {
            let _ = std::fs::remove_file(copy);
        }
    }
}

impl super::Word {
    /// AutoSave, AutoRecover and conversions, once a frame. Answers whether a conversion is still running,
    /// so the window keeps drawing until it finishes.
    pub fn keep_the_files(&mut self, now: Instant) -> bool {
        let converting: Vec<String> = self
            .waiting
            .iter()
            .filter(|(_, w)| matches!(w, Waiting::Converting { .. }))
            .map(|(k, _)| k.clone())
            .collect();
        for key in &converting {
            self.finish_waiting(key);
        }
        let (autosave, autorecover) = (self.config.autosave, self.config.autorecover);
        let folder = self.folder.clone();
        let mut said = Vec::new();
        for doc in &mut self.docs {
            let quiet = doc.timing.note(doc.session.revision, now);
            if !doc.modified() {
                if doc.timing.copied {
                    if let Some(folder) = &folder {
                        forget_recovery(folder, &doc.key);
                    }
                    doc.timing.copied = false;
                }
                doc.timing.recovered = None;
                continue;
            }
            if autosave && doc.path.is_some() && quiet >= AUTOSAVE_AFTER {
                match doc.save(None) {
                    Ok(_) => continue,
                    Err(problem) => {
                        said.push(format!("AutoSave could not save {}: {problem}", doc.label()))
                    }
                }
            }
            let (true, Some(folder)) = (autorecover, &folder) else { continue };
            let since = *doc.timing.recovered.get_or_insert(now);
            if now.duration_since(since) < RECOVER_EVERY {
                continue;
            }
            doc.timing.recovered = Some(now);
            let written = unluminous_word::write::save(&doc.session.document).and_then(|bytes| {
                match doc.password.as_deref() {
                    Some(password) if !password.is_empty() => {
                        unluminous_word::crypto::encrypt(&bytes, password)
                    }
                    _ => Ok(bytes),
                }
            });
            let copy = recovery_path(folder, &doc.key);
            if let Ok(bytes) = written {
                if copy.parent().is_some_and(|p| std::fs::create_dir_all(p).is_ok())
                    && crate::services::store::write_atomically(&copy, &bytes).is_ok()
                {
                    doc.timing.copied = true;
                }
            }
        }
        // A failed AutoSave is said once, not once a frame: it stops until the setting is chosen again.
        if !said.is_empty() {
            self.config.autosave = false;
        }
        for text in said {
            self.outbox.push(super::Request::Notice {
                text,
                kind: crate::components::toast::Kind::Problem,
            });
        }
        !converting.is_empty()
    }

    /// What a tab whose document is not ready draws: the sentence, and the password field when it is
    /// waiting for one.
    pub fn draw_waiting(
        &mut self,
        key: &str,
        sentence: &str,
        ui: &mut egui::Ui,
        look: &super::Look<'_>,
    ) -> Vec<super::Request> {
        let mut requests = Vec::new();
        let area = ui.max_rect().shrink(24.0);
        let waiting = self.waiting.iter().find(|(k, _)| k == key).map(|(_, w)| match w {
            Waiting::Password { .. } => 1,
            Waiting::Converting { .. } => 2,
            Waiting::Failed { .. } => 3,
        });
        let mut give = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
            ui.label(
                egui::RichText::new(sentence).size(look.font_size).color(look.palette.text_dim),
            );
            match waiting {
                Some(1) => {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let field = ui.text_box(
                            egui::TextEdit::singleline(&mut self.password_field)
                                .password(true)
                                .id(egui::Id::new(("word-open-password", key)))
                                .desired_width(220.0),
                        );
                        field.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Password")
                        });
                        let pressed =
                            field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let open = ui.button("Open");
                        open.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Open with password",
                            )
                        });
                        if open.clicked() || pressed {
                            give = Some(self.password_field.clone());
                        }
                    });
                }
                Some(2) => {
                    ui.add_space(8.0);
                    ui.spinner();
                    ui.ctx().request_repaint_after(Duration::from_millis(200));
                }
                _ => {}
            }
        });
        if let Some(password) = give {
            if let Err(problem) = self.give_password(key, &password) {
                requests.push(super::Request::Message(problem));
            }
        }
        requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_size_follows_the_locale() {
        assert_eq!(locale_page_size("en-US"), "letter");
        assert_eq!(locale_page_size("fr_CA"), "letter");
        assert_eq!(locale_page_size("en-GB"), "a4");
        assert_eq!(locale_page_size("de-DE"), "a4");
    }

    #[test]
    fn a_recovery_copy_older_than_the_file_is_not_offered() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-recovery-test-{}", std::process::id()));
        let copy = recovery_path(&folder, "word:a.docx");
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        std::fs::write(&copy, b"copy").unwrap();
        assert!(recovery_for(&folder, "word:a.docx", None).is_some());
        std::thread::sleep(Duration::from_millis(20));
        let file = folder.join("a.docx");
        std::fs::write(&file, b"saved").unwrap();
        assert!(recovery_for(&folder, "word:a.docx", Some(&file)).is_none());
        forget_recovery(&folder, "word:a.docx");
        assert!(!copy.exists());
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(copy.parent().unwrap()).unwrap();
        std::fs::remove_dir(&folder).unwrap();
    }

    #[test]
    fn timing_counts_from_the_last_change() {
        let mut timing = Timing::default();
        let start = Instant::now();
        assert_eq!(timing.note(1, start), Duration::ZERO);
        assert_eq!(timing.note(1, start + Duration::from_secs(3)), Duration::from_secs(3));
        assert_eq!(timing.note(2, start + Duration::from_secs(4)), Duration::ZERO);
    }
}
