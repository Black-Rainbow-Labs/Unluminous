//! What is known about a skill that its files do not say, and the files themselves as they shipped.
//!
//! `skills/catalogue.json` in this crate is the list the site publishes and the plugin draws: the display name, the
//! author, the licence, where it came from, the picture, and the evidence it is popular. Every file under `skills/`
//! is compiled in by `build.rs` ([`BUNDLE`]), so the list, the pictures and the files are all here with no network.
//!
//! The site's copy, `https://unluminous.com/skills/catalogue.json`, is the same list with a `sha256` and the file
//! list added to each entry by `sync-skills.mjs`. A newer hash there than the one worked out here is an update.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

mod bundle {
    include!(concat!(env!("OUT_DIR"), "/skills_bundle.rs"));
}

/// Every file under `skills/`, by its path inside that folder.
pub static BUNDLE: &[(&str, &[u8])] = bundle::FILES;

/// Where the site's catalogue is, unless Settings says otherwise.
pub const SITE: &str = "https://unluminous.com/skills/catalogue.json";

/// One skill in the catalogue.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub author: String,
    pub summary: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub license: String,
    pub repository: String,
    pub path: String,
    pub homepage: String,
    pub popularity: String,
    pub sources: Vec<String>,
    /// The picture, as a path inside the skills folder: `images/<id>.jpg`.
    pub image: String,
    /// Made by Unluminous, which puts it first.
    pub unluminous: bool,
    /// The upstream project's own install line, such as `npx skills add obra/superpowers`, when it has one.
    pub install: String,
    /// The source repository's GitHub stars when the list was made, 0 when not known.
    pub stars: u64,
    /// Written by the site: the hash of the skill's files ([`hash`]).
    pub sha256: String,
    /// Written by the site: the skill's files.
    pub files: Vec<RemoteFile>,
    /// The commit its files were copied from.
    pub reference: String,
    /// Written by `tools/skills/snapshot.mjs`: `complete` when every file is in the binary, `partial` when only
    /// the `SKILL.md` is and the rest is fetched from the site the first time it is installed.
    pub bundled: String,
}

/// One file of a skill on the site.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoteFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

/// The whole list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalogue {
    pub schema: u32,
    pub updated: String,
    /// Where the files are on the site: a skill's file is `<base>/files/<id>/<path>`.
    pub base: String,
    pub skills: Vec<Entry>,
}

/// A string field of a JSON object, empty when it is missing.
fn text(value: &serde_json::Value, key: &str) -> String {
    value.get(key).and_then(|field| field.as_str()).unwrap_or_default().to_owned()
}

/// A list of strings, empty when it is missing.
fn texts(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|field| field.as_array())
        .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

impl Entry {
    /// Read one entry. An entry with no id is not one.
    pub fn from_value(value: &serde_json::Value) -> Option<Entry> {
        let id = text(value, "id");
        if id.is_empty() {
            return None;
        }
        let files = value
            .get("files")
            .and_then(|field| field.as_array())
            .map(|items| {
                items
                    .iter()
                    .map(|item| RemoteFile {
                        path: text(item, "path"),
                        sha256: text(item, "sha256"),
                        size: item.get("size").and_then(|size| size.as_u64()).unwrap_or(0),
                    })
                    .filter(|file| !file.path.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Some(Entry {
            id,
            name: text(value, "name"),
            author: text(value, "author"),
            summary: text(value, "summary"),
            description: text(value, "description"),
            category: text(value, "category"),
            tags: texts(value, "tags"),
            license: text(value, "license"),
            repository: text(value, "repository"),
            path: text(value, "path"),
            homepage: text(value, "homepage"),
            popularity: text(value, "popularity"),
            sources: texts(value, "sources"),
            image: text(value, "image"),
            unluminous: value.get("unluminous").and_then(|field| field.as_bool()).unwrap_or(false),
            install: text(value, "install"),
            stars: value.get("stars").and_then(|field| field.as_u64()).unwrap_or(0),
            sha256: text(value, "sha256"),
            files,
            reference: text(value, "ref"),
            bundled: text(value, "bundled"),
        })
    }
}

impl Catalogue {
    /// Read a catalogue, refusing one that is not this shape.
    pub fn read(text_of_it: &str) -> Result<Catalogue, String> {
        let value: serde_json::Value = serde_json::from_str(text_of_it)
            .map_err(|problem| format!("the skills catalogue could not be read: {problem}"))?;
        let schema = value.get("schema").and_then(|field| field.as_u64()).unwrap_or(0) as u32;
        if schema > 1 {
            return Err(format!("the skills catalogue is schema {schema}, and this version of Unluminous reads schema 1"));
        }
        let skills = value
            .get("skills")
            .and_then(|field| field.as_array())
            .ok_or("the skills catalogue has no list of skills")?
            .iter()
            .filter_map(Entry::from_value)
            .collect();
        Ok(Catalogue {
            schema,
            updated: text(&value, "updated"),
            base: text(&value, "base"),
            skills,
        })
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.skills.iter().find(|entry| entry.id == id)
    }
}

/// A star count as a person reads it: `950`, `12.4k`, `184k`.
pub fn stars_said(stars: u64) -> String {
    match stars {
        0..=999 => stars.to_string(),
        1_000..=99_999 => {
            let tenths = (stars as f64 / 100.0).round() / 10.0;
            format!("{}k", tenths.to_string().trim_end_matches(".0"))
        }
        _ => format!("{}k", (stars as f64 / 1000.0).round()),
    }
}

/// The catalogue that shipped in this binary.
pub fn bundled() -> Catalogue {
    let text = bundled_file("catalogue.json").map(String::from_utf8_lossy).unwrap_or_default();
    Catalogue::read(&text).unwrap_or_default()
}

/// One file of the bundle, by its path inside the skills folder.
pub fn bundled_file(path: &str) -> Option<&'static [u8]> {
    BUNDLE.iter().find(|(inside, _)| *inside == path).map(|(_, bytes)| *bytes)
}

/// The files of the skill `id` as they shipped, by their path inside its folder. Empty when none shipped.
pub fn bundled_files(id: &str) -> Files {
    let prefix = format!("{id}/");
    BUNDLE
        .iter()
        .filter_map(|(inside, bytes)| {
            inside.strip_prefix(&prefix).map(|path| (path.to_owned(), bytes.to_vec()))
        })
        .collect()
}

/// A skill's files, by their path inside its folder with `/` between the parts.
pub type Files = BTreeMap<String, Vec<u8>>;

/// The hash of a skill's files: SHA-256 over each file in path order as `path NUL bytes NUL`, with `\r\n` made
/// `\n` in any file that is text, so a checkout on Windows and one on a Mac agree. `sync-skills.mjs` on the site
/// writes the same bytes, which `the_hash_matches_what_the_site_script_writes` checks.
pub fn hash(files: &Files) -> String {
    let mut hasher = Sha256::new();
    for (path, bytes) in files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        match std::str::from_utf8(bytes) {
            Ok(text) => hasher.update(text.replace("\r\n", "\n").as_bytes()),
            Err(_) => hasher.update(bytes),
        }
        hasher.update([0]);
    }
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Read every file under `folder` into [`Files`], leaving out Unluminous's own record of where it came from.
pub fn read_folder(folder: &std::path::Path) -> Files {
    let mut files = Files::new();
    gather(folder, folder, &mut files, 0);
    files
}

/// The name of the file the library keeps beside a skill's own files.
pub const RECORD: &str = ".unluminous-skill.json";

fn gather(root: &std::path::Path, folder: &std::path::Path, into: &mut Files, depth: usize) {
    // A skill is a handful of files. A folder deeper than this, or a skill of thousands of files, is not one.
    if depth > 6 || into.len() > 500 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == RECORD || name == ".git" {
            continue;
        }
        if path.is_dir() {
            gather(root, &path, into, depth + 1);
        } else if let (Ok(inside), Ok(bytes)) = (path.strip_prefix(root), std::fs::read(&path)) {
            let parts: Vec<String> = inside
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            into.insert(parts.join("/"), bytes);
        }
    }
}

/// Write `files` into `folder`, making the folders they need.
pub fn write_folder(folder: &std::path::Path, files: &Files) -> Result<(), String> {
    for (path, bytes) in files {
        if path.split('/').any(|part| part == ".." || part.is_empty()) {
            return Err(format!("a skill file may not be called {path}"));
        }
        let target = folder.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|problem| format!("could not make {}: {problem}", parent.display()))?;
        }
        crate::services::store::write_a_source_file(&target, bytes)
            .map_err(|problem| format!("could not write {}: {problem}", target.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_catalogue_reads_and_every_entry_has_its_files_and_picture() {
        let catalogue = bundled();
        assert!(!catalogue.skills.is_empty(), "the catalogue shipped");
        for entry in &catalogue.skills {
            let files = bundled_files(&entry.id);
            let skill =
                files.get("SKILL.md").unwrap_or_else(|| panic!("{} has a SKILL.md", entry.id));
            let front = super::super::frontmatter::parse(&String::from_utf8_lossy(skill));
            assert_eq!(front.name(), Some(entry.id.as_str()), "{}'s name is its folder", entry.id);
            assert!(front.description().is_some(), "{} says when it applies", entry.id);
            assert!(
                bundled_file(&entry.image).is_some(),
                "{} has its picture {}",
                entry.id,
                entry.image
            );
            assert!(
                !entry.name.is_empty() && !entry.summary.is_empty(),
                "{} is described",
                entry.id
            );
        }
    }

    #[test]
    fn the_unluminous_skills_come_first() {
        let catalogue = bundled();
        let ids: Vec<&str> =
            catalogue.skills.iter().take(3).map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, ["unluminous-task", "unluminous-supervisor", "unluminous"]);
        assert!(catalogue.skills.iter().take(3).all(|entry| entry.unluminous));
    }

    #[test]
    fn the_unluminous_skill_is_the_one_mcp_install_writes() {
        let shipped = bundled_files("unluminous");
        let ours = String::from_utf8_lossy(&shipped["SKILL.md"]).replace("\r\n", "\n");
        assert_eq!(ours, unluminous_cli::mcp::skill::SKILL.replace("\r\n", "\n"));
    }

    #[test]
    fn the_hash_ignores_line_endings_and_follows_the_bytes() {
        let mut one = Files::new();
        one.insert("SKILL.md".into(), b"a\r\nb\n".to_vec());
        let mut two = Files::new();
        two.insert("SKILL.md".into(), b"a\nb\n".to_vec());
        assert_eq!(hash(&one), hash(&two));
        two.insert("SKILL.md".into(), b"a\nc\n".to_vec());
        assert_ne!(hash(&one), hash(&two));
    }

    #[test]
    fn the_hash_matches_what_the_site_script_writes() {
        // `node sites/unluminous/scripts/sync-skills.mjs --hash-check` prints this for the same two files.
        let mut files = Files::new();
        files.insert("SKILL.md".into(), b"---\nname: x\n---\n".to_vec());
        files.insert("scripts/run.sh".into(), b"echo hi\r\n".to_vec());
        assert_eq!(hash(&files), EXPECTED_SITE_HASH);
    }

    /// Worked out by the site script; see the test above.
    const EXPECTED_SITE_HASH: &str =
        "eeaaa3d17b80b1e05a779439bb042938459833c1f11091840ee6b54e8487d62e";

    #[test]
    fn stars_are_written_the_short_way() {
        assert_eq!(stars_said(950), "950");
        assert_eq!(stars_said(12_430), "12.4k");
        assert_eq!(stars_said(37_000), "37k");
        assert_eq!(stars_said(184_549), "185k");
    }

    #[test]
    fn a_path_that_climbs_out_is_refused() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-skills-write-{}", std::process::id()));
        let mut files = Files::new();
        files.insert("../escape.md".into(), b"x".to_vec());
        assert!(write_folder(&folder, &files).is_err());
    }
}
