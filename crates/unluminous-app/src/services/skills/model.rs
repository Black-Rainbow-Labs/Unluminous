//! What the Skills Manager knows about each skill: where it came from, where its files are now, whether the
//! person changed it, whether the site has a newer one, and every place on this machine it is installed.
//!
//! [`Shelf`] is worked out from the disk by [`Shelf::read`], which is cheap (a few dozen folders listed and the
//! files of the skills that are installed read) and is done when the provider opens, after every change it makes,
//! and every [`super::LOOK_AGAIN`] while it is open, because an agent or a person may change a skills folder
//! behind it.

use std::path::{Path, PathBuf};

use super::catalogue::{self, Catalogue, Entry, Files};
use super::frontmatter;
use super::harness::{Harness, Places, Scope, HARNESSES};

/// Where a skill came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// Ships with Unluminous: the Agent-Tasks skills and the `unluminous` skill.
    Unluminous,
    /// Made by the person, in the library only.
    Mine,
    /// In a harness folder on this machine, put there by something other than Unluminous.
    Found,
    /// From the catalogue unluminous.com publishes.
    Catalogue,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Unluminous => "unluminous",
            Source::Mine => "mine",
            Source::Found => "found",
            Source::Catalogue => "catalogue",
        }
    }

    /// The heading the pane groups it under.
    pub fn heading(self) -> &'static str {
        match self {
            Source::Unluminous => "Unluminous",
            Source::Mine => "My skills",
            Source::Found => "Found on this machine",
            Source::Catalogue => "Most used",
        }
    }
}

/// One copy of a skill in a skills folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    /// The skill's folder: `<skills folder>/<id>`.
    pub folder: PathBuf,
    /// The skills folder it is in.
    pub parent: PathBuf,
    pub scope: Scope,
    /// Unluminous wrote it, so Unluminous may replace and remove it.
    pub ours: bool,
    /// Its files are the skill's files as they are now.
    pub current: bool,
}

/// One skill.
#[derive(Debug, Clone)]
pub struct Skill {
    pub id: String,
    pub entry: Entry,
    pub source: Source,
    /// The person's working copy, when there is one.
    pub library: Option<PathBuf>,
    /// The hash of the files it was installed from, as they are now.
    pub hash: String,
    /// The hash of the files as they shipped or were last taken from the site.
    pub origin: String,
    /// The library copy is not what was shipped or taken.
    pub edited: bool,
    /// The site has a newer one.
    pub update: bool,
    /// The binary holds only its `SKILL.md`; the rest comes from the site the first time it is needed.
    pub partial: bool,
    pub copies: Vec<Copy>,
}

impl Skill {
    /// Whether `harness` reads this skill in `scope`, from its own folder or from one it shares.
    pub fn seen_by(
        &self,
        places: &Places,
        harness: &Harness,
        scope: Scope,
        project: Option<&Path>,
    ) -> bool {
        let folders = places.read_folders(harness, scope, project);
        self.copies.iter().any(|copy| copy.scope == scope && folders.contains(&copy.parent))
    }

    /// The copy in `harness`'s own folder for `scope`.
    pub fn own_copy(
        &self,
        places: &Places,
        harness: &Harness,
        scope: Scope,
        project: Option<&Path>,
    ) -> Option<&Copy> {
        let folder = places.write_folder(harness, scope, project)?;
        self.copies.iter().find(|copy| copy.parent == folder)
    }

    pub fn is_installed(&self) -> bool {
        !self.copies.is_empty()
    }
}

/// The record the library keeps beside a skill's files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    pub source: String,
    /// The hash of the files as they were written into the library.
    pub origin: String,
}

impl Record {
    pub fn read(folder: &Path) -> Option<Record> {
        let text = std::fs::read_to_string(folder.join(catalogue::RECORD)).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let field = |key: &str| {
            value.get(key).and_then(|field| field.as_str()).unwrap_or_default().to_owned()
        };
        Some(Record { source: field("source"), origin: field("origin") })
    }

    pub fn write(&self, folder: &Path) -> Result<(), String> {
        let value = serde_json::json!({ "source": self.source, "origin": self.origin });
        let text = serde_json::to_string_pretty(&value).unwrap_or_default();
        std::fs::write(folder.join(catalogue::RECORD), text).map_err(|problem| {
            format!("could not write the skill's record in {}: {problem}", folder.display())
        })
    }
}

/// Everything the provider reads the disk for.
#[derive(Debug, Clone)]
pub struct Shelf {
    pub skills: Vec<Skill>,
}

/// What [`Shelf::read`] is told.
pub struct Sources<'a> {
    pub places: &'a Places,
    pub project: Option<&'a Path>,
    /// The plugin's own folder, which holds the library.
    pub library: Option<&'a Path>,
    pub bundled: &'a Catalogue,
    pub remote: Option<&'a Catalogue>,
    /// More skills folders to look in, from Settings.
    pub extra: &'a [PathBuf],
}

impl Shelf {
    /// Read every skill: the bundled catalogue, the library, and every skills folder of every harness.
    pub fn read(from: &Sources<'_>) -> Shelf {
        let mut skills: Vec<Skill> =
            from.bundled.skills.iter().map(|entry| shipped(entry, from)).collect();
        // A skill the site added after this binary was built.
        if let Some(remote) = from.remote {
            for entry in &remote.skills {
                if !skills.iter().any(|skill| skill.id == entry.id) {
                    skills.push(newer_than_the_binary(entry, from));
                }
            }
        }
        for (id, folder) in library_folders(from.library) {
            if !skills.iter().any(|skill| skill.id == id) {
                skills.push(mine(&id, &folder));
            }
        }
        let folders = every_folder(from);
        for (parent, scope) in &folders {
            for (id, folder) in skill_folders(parent) {
                if !skills.iter().any(|skill| skill.id == id) {
                    skills.push(found(&id, &folder));
                }
                let skill = skills.iter_mut().find(|skill| skill.id == id).expect("added above");
                skill.copies.push(copy_of(skill.hash.as_str(), &folder, parent, *scope));
            }
        }
        skills.sort_by_key(|skill| (skill.source, order_of(from, &skill.id)));
        Shelf { skills }
    }

    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.skills.iter().find(|skill| skill.id == id)
    }
}

/// The catalogue's order for a skill, so the pane keeps the site's order inside each heading.
fn order_of(from: &Sources<'_>, id: &str) -> (usize, String) {
    let place = from.bundled.skills.iter().position(|entry| entry.id == id).unwrap_or(usize::MAX);
    (place, id.to_owned())
}

/// Every skills folder this window looks in, with its scope, each once.
fn every_folder(from: &Sources<'_>) -> Vec<(PathBuf, Scope)> {
    let mut folders: Vec<(PathBuf, Scope)> = Vec::new();
    for scope in [Scope::Project, Scope::Global] {
        for folder in from.places.every_folder(scope, from.project) {
            if !folders.iter().any(|(known, _)| *known == folder) {
                folders.push((folder, scope));
            }
        }
    }
    for folder in from.extra {
        if !folders.iter().any(|(known, _)| known == folder) {
            folders.push((folder.clone(), Scope::Global));
        }
    }
    folders
}

/// The folders inside a skills folder that hold a `SKILL.md`, by name.
pub fn skill_folders(parent: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
    let mut out: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .map(|entry| (entry.file_name().to_string_lossy().into_owned(), entry.path()))
        .collect();
    out.sort();
    out
}

/// The skills in the library, by name.
fn library_folders(library: Option<&Path>) -> Vec<(String, PathBuf)> {
    library.map(|folder| skill_folders(&folder.join("library"))).unwrap_or_default()
}

/// A skill that shipped in the binary, with its library copy if the person has one.
fn shipped(entry: &Entry, from: &Sources<'_>) -> Skill {
    let partial = entry_is_partial(entry);
    let shipped_hash = match (partial, shipped_hash_of(&entry.id)) {
        (false, Some(hash)) => hash,
        _ => entry.sha256.clone(),
    };
    let source = if entry.unluminous { Source::Unluminous } else { Source::Catalogue };
    let mut skill = Skill {
        id: entry.id.clone(),
        entry: entry.clone(),
        source,
        library: None,
        hash: shipped_hash.clone(),
        origin: shipped_hash,
        edited: false,
        update: false,
        partial,
        copies: Vec::new(),
    };
    apply_the_library(&mut skill, from.library);
    apply_the_site(&mut skill, from.remote);
    skill
}

/// The hash of what shipped for `id`, worked out once for the life of the process, since the binary cannot change
/// under it. `None` when nothing shipped.
fn shipped_hash_of(id: &str) -> Option<String> {
    static HASHES: std::sync::OnceLock<std::collections::HashMap<String, String>> =
        std::sync::OnceLock::new();
    HASHES
        .get_or_init(|| {
            catalogue::bundled()
                .skills
                .iter()
                .filter_map(|entry| {
                    let files = catalogue::bundled_files(&entry.id);
                    (!files.is_empty()).then(|| (entry.id.clone(), catalogue::hash(&files)))
                })
                .collect()
        })
        .get(id)
        .cloned()
}

/// Whether the binary holds only part of this skill. `snapshot.mjs` writes `bundled: partial` for it.
pub fn entry_is_partial(entry: &Entry) -> bool {
    entry.bundled == "partial"
}

/// A skill the site lists and this binary does not.
fn newer_than_the_binary(entry: &Entry, from: &Sources<'_>) -> Skill {
    let mut skill = Skill {
        id: entry.id.clone(),
        entry: entry.clone(),
        source: if entry.unluminous { Source::Unluminous } else { Source::Catalogue },
        library: None,
        hash: entry.sha256.clone(),
        origin: entry.sha256.clone(),
        edited: false,
        update: false,
        partial: true,
        copies: Vec::new(),
    };
    apply_the_library(&mut skill, from.library);
    skill
}

/// Take the library copy into account: its files are the skill now.
fn apply_the_library(skill: &mut Skill, library: Option<&Path>) {
    let Some(folder) = library.map(|folder| folder.join("library").join(&skill.id)) else { return };
    if !folder.join("SKILL.md").is_file() {
        return;
    }
    let files = catalogue::read_folder(&folder);
    skill.hash = catalogue::hash(&files);
    if let Some(record) = Record::read(&folder) {
        skill.origin = record.origin;
    }
    skill.edited = skill.hash != skill.origin;
    skill.partial = false;
    skill.library = Some(folder);
}

/// Take the site's catalogue into account: a different hash there than the one this skill came from is an update.
fn apply_the_site(skill: &mut Skill, remote: Option<&Catalogue>) {
    if let Some(entry) = remote.and_then(|remote| remote.entry(&skill.id)) {
        skill.update =
            !entry.sha256.is_empty() && entry.sha256 != skill.origin && entry.sha256 != skill.hash;
    }
}

/// A skill the person made, which is only in the library.
fn mine(id: &str, folder: &Path) -> Skill {
    let files = catalogue::read_folder(folder);
    let hash = catalogue::hash(&files);
    Skill {
        id: id.to_owned(),
        entry: entry_from_files(id, &files, "You"),
        source: Source::Mine,
        library: Some(folder.to_path_buf()),
        origin: hash.clone(),
        hash,
        edited: false,
        update: false,
        partial: false,
        copies: Vec::new(),
    }
}

/// A skill in a harness folder that nothing here knows about.
fn found(id: &str, folder: &Path) -> Skill {
    let files = catalogue::read_folder(folder);
    let hash = catalogue::hash(&strip_marks(&files));
    Skill {
        id: id.to_owned(),
        entry: entry_from_files(id, &files, ""),
        source: Source::Found,
        library: None,
        origin: hash.clone(),
        hash,
        edited: false,
        update: false,
        partial: false,
        copies: Vec::new(),
    }
}

/// What can be said about a skill from its `SKILL.md` alone.
fn entry_from_files(id: &str, files: &Files, author: &str) -> Entry {
    let text = files
        .get("SKILL.md")
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();
    let front = frontmatter::parse(&text);
    let description = front.description().unwrap_or("").to_owned();
    let summary = first_sentence(&description);
    Entry {
        id: id.to_owned(),
        name: title_of(front.name().unwrap_or(id)),
        author: front.get("author").unwrap_or(author).to_owned(),
        summary,
        description,
        category: "Mine".into(),
        license: front.get("license").unwrap_or("").to_owned(),
        ..Entry::default()
    }
}

/// The first sentence of some words, cut to 140 characters.
fn first_sentence(words: &str) -> String {
    let end = words.find(". ").map(|at| at + 1).unwrap_or(words.len());
    let sentence = words[..end].trim();
    match sentence.chars().count() > 140 {
        true => format!("{}…", sentence.chars().take(139).collect::<String>().trim_end()),
        false => sentence.to_owned(),
    }
}

/// `release-notes` as `Release Notes`.
pub fn title_of(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut letters = part.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().chain(letters).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// A skill's files with the mark taken out of its `SKILL.md`, which is how an installed copy is compared with
/// what it was installed from.
pub fn strip_marks(files: &Files) -> Files {
    let mut out = files.clone();
    if let Some(text) = out.get("SKILL.md").map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    {
        out.insert("SKILL.md".into(), frontmatter::without_mark(&text).into_bytes());
    }
    out
}

/// One installed copy: whether it is ours and whether it is the skill as it is now.
fn copy_of(hash: &str, folder: &Path, parent: &Path, scope: Scope) -> Copy {
    let files = catalogue::read_folder(folder);
    let text = files
        .get("SKILL.md")
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();
    let ours = frontmatter::has_mark(&text);
    let current = catalogue::hash(&strip_marks(&files)) == hash;
    Copy { folder: folder.to_path_buf(), parent: parent.to_path_buf(), scope, ours, current }
}

/// The harnesses that read `skill` in `scope`, as their ids.
pub fn seen_by_ids(
    skill: &Skill,
    places: &Places,
    scope: Scope,
    project: Option<&Path>,
) -> Vec<&'static str> {
    HARNESSES
        .iter()
        .filter(|harness| skill.seen_by(places, harness, scope, project))
        .map(|harness| harness.id)
        .collect()
}
