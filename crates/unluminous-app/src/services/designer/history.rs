//! The copies of a design taken before each request to the agent, so an agent's edit can be put back.
//!
//! They are kept in the project's own `.unluminous` folder, not beside the design: they are what one person's
//! agent did in the minutes between commits, and git holds the commits. A copy is
//! `.unluminous/designs/<the design's path inside the designs folder, without its extension>/<stamp>-<slug>.html`
//! where the stamp is `YYYYMMDD-HHMMSS` in local time and the slug is the first five words of the request.
//!
//! Restoring copies the design as it is now under the slug `before-restore` first, so a restore can itself be
//! put back. Only the newest copies are kept, and an older one is removed by its own path as a new one is
//! written.

use std::path::{Path, PathBuf};

use super::config::slashed;

/// The longest a slug is.
const SLUG_LONGEST: usize = 40;

/// How many words of a request the slug holds.
const SLUG_WORDS: usize = 5;

/// How many characters a stamp takes: `20261012-234012`.
const STAMP_LENGTH: usize = 15;

/// One copy on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// The copy's file.
    pub file: PathBuf,
    /// The file's name, which is what `restore` is given.
    pub name: String,
    /// `YYYYMMDD-HHMMSS`.
    pub stamp: String,
    /// The request's first words, put back with spaces.
    pub words: String,
}

/// The time now on this machine's clock, counted in seconds as though it were UTC, which is how a copy's stamp
/// is read by [`Backup::friendly`].
pub fn local_now() -> u64 {
    let now = crate::services::clock::date_time();
    let iso = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year, now.month, now.day, now.hour, now.minute, now.second
    );
    super::notes::seconds_from_iso(&iso).unwrap_or_default()
}

impl Backup {
    /// The time as a thread says it: `Today 14:32`, `Yesterday 17:02`, or `5 Jan 14:32`.
    ///
    /// A copy's stamp is this machine's local time, so `now` has to be local time read the same way, which is
    /// what [`local_now`] answers. Compared against seconds since 1970 in UTC, every copy taken before midnight
    /// UTC and after local midnight read `Yesterday`.
    ///
    /// @param now - the time now on this machine's clock, as [`local_now`] gives it
    pub fn friendly(&self, now: u64) -> String {
        let iso = self.when().replacen(' ', "T", 1);
        let Some(then) = super::notes::seconds_from_iso(&iso) else { return self.when() };
        let clock = iso.get(11..16).unwrap_or_default();
        match (now / 86_400).saturating_sub(then / 86_400) {
            0 => format!("Today {clock}"),
            1 => format!("Yesterday {clock}"),
            _ => {
                let months = [
                    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov",
                    "Dec",
                ];
                let month: usize = iso.get(5..7).and_then(|part| part.parse().ok()).unwrap_or(1);
                let day: u32 = iso.get(8..10).and_then(|part| part.parse().ok()).unwrap_or(1);
                format!("{day} {} {clock}", months[month.clamp(1, 12) - 1])
            }
        }
    }

    /// The time as a person reads it: `2026-10-12 23:40:12`.
    pub fn when(&self) -> String {
        let s = &self.stamp;
        match s.len() == STAMP_LENGTH && s.is_char_boundary(8) {
            true => format!(
                "{}-{}-{} {}:{}:{}",
                &s[0..4],
                &s[4..6],
                &s[6..8],
                &s[9..11],
                &s[11..13],
                &s[13..15]
            ),
            false => s.clone(),
        }
    }

    /// A copy from its file, read from the file's name.
    fn of(file: PathBuf) -> Option<Self> {
        let name = file.file_name()?.to_string_lossy().into_owned();
        let stem = name.strip_suffix(".html")?;
        let stamp: String = stem.chars().take(STAMP_LENGTH).collect();
        let words =
            stem.get(STAMP_LENGTH..).unwrap_or_default().trim_start_matches('-').replace('-', " ");
        Some(Self { file, name, stamp, words })
    }
}

/// The folder a design's copies are kept in.
///
/// @param project - the folder the window has open
/// @param folder - the configured designs folder, relative to the project
/// @param design - the design, relative to the project
pub fn folder_for(project: &Path, folder: &Path, design: &Path) -> PathBuf {
    let inside = design.strip_prefix(folder).unwrap_or(design);
    let mut kept = project.join(".unluminous").join("designs");
    if let Some(parent) = inside.parent() {
        kept = kept.join(parent);
    }
    kept.join(inside.file_stem().unwrap_or_default())
}

/// The first five words of a request as part of a file name.
///
/// @param words - what the request said
pub fn slug(words: &str) -> String {
    let joined: String = words
        .split_whitespace()
        .take(SLUG_WORDS)
        .map(|word| {
            word.chars()
                .filter(|letter| letter.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<String>>()
        .join("-");
    let cut: String = joined.chars().take(SLUG_LONGEST).collect();
    let cut = cut.trim_matches('-').to_owned();
    match cut.is_empty() {
        true => "request".to_owned(),
        false => cut,
    }
}

/// Copy the design as it is now, and remove the copies beyond the newest `kept`.
///
/// @param project - the folder the window has open
/// @param folder - the configured designs folder, relative to the project
/// @param design - the design, relative to the project
/// @param words - the request the copy is taken for
/// @param kept - how many copies to keep
pub fn keep(
    project: &Path,
    folder: &Path,
    design: &Path,
    words: &str,
    kept: usize,
) -> Result<Backup, String> {
    let into = folder_for(project, folder, design);
    std::fs::create_dir_all(&into)
        .map_err(|problem| format!("{} could not be made: {problem}", into.display()))?;
    let stamp = crate::services::clock::date_time().stamp();
    let base = format!("{stamp}-{}", slug(words));
    let mut file = into.join(format!("{base}.html"));
    let mut again = 2;
    while file.exists() {
        file = into.join(format!("{base}-{again}.html"));
        again += 1;
    }
    std::fs::copy(project.join(design), &file)
        .map_err(|problem| format!("{} could not be copied: {problem}", design.display()))?;
    prune(project, folder, design, kept);
    Backup::of(file).ok_or_else(|| "The copy could not be named.".to_owned())
}

/// Every copy of a design, newest first.
///
/// @param project - the folder the window has open
/// @param folder - the configured designs folder, relative to the project
/// @param design - the design, relative to the project
pub fn list(project: &Path, folder: &Path, design: &Path) -> Vec<Backup> {
    let Ok(entries) = std::fs::read_dir(folder_for(project, folder, design)) else {
        return Vec::new();
    };
    let mut copies: Vec<Backup> =
        entries.flatten().filter_map(|entry| Backup::of(entry.path())).collect();
    copies.sort_by(|a, b| b.name.cmp(&a.name));
    copies
}

/// Remove every copy beyond the newest `kept`, each by its own path.
fn prune(project: &Path, folder: &Path, design: &Path, kept: usize) {
    for old in list(project, folder, design).into_iter().skip(kept.max(1)) {
        let _ = std::fs::remove_file(&old.file);
    }
}

/// The copy a name stands for: its file name, the start of it, or its place in the list (`1` is the newest).
///
/// @param copies - the copies, newest first
/// @param wanted - what was typed
pub fn find<'a>(copies: &'a [Backup], wanted: &str) -> Result<&'a Backup, String> {
    let wanted = wanted.trim();
    if let Ok(place) = wanted.parse::<usize>() {
        if wanted.len() < STAMP_LENGTH && place >= 1 {
            return copies.get(place - 1).ok_or_else(|| {
                format!("There are {} copies, so there is no copy {place}.", copies.len())
            });
        }
    }
    let matches: Vec<&Backup> = copies
        .iter()
        .filter(|copy| copy.name == wanted || (!wanted.is_empty() && copy.name.starts_with(wanted)))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("There is no copy called `{wanted}`.")),
        _ => Err(format!("`{wanted}` is the start of more than one copy. Give more of it.")),
    }
}

/// Put a copy back as the design, after copying the design as it is now.
///
/// @param project - the folder the window has open
/// @param folder - the configured designs folder, relative to the project
/// @param design - the design, relative to the project
/// @param wanted - which copy, as `find` reads it
/// @param kept - how many copies to keep
pub fn restore(
    project: &Path,
    folder: &Path,
    design: &Path,
    wanted: &str,
    kept: usize,
) -> Result<Backup, String> {
    let copies = list(project, folder, design);
    let chosen = find(&copies, wanted)?.clone();
    keep(project, folder, design, "before restore", kept)?;
    crate::services::store::write_a_source_file(
        &project.join(design),
        &std::fs::read(&chosen.file)
            .map_err(|problem| format!("{} could not be read: {problem}", chosen.file.display()))?,
    )
    .map_err(|problem| format!("{} could not be written: {problem}", design.display()))?;
    Ok(chosen)
}

/// The design as the pane names it, for a sentence.
///
/// @param design - the design, relative to the project
pub fn said(design: &Path) -> String {
    slashed(design)
}
