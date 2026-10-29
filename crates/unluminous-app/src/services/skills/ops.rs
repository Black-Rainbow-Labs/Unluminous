//! Every change the Skills Manager makes to the disk: installing a skill into a harness's folder, removing it,
//! keeping installed copies in step with their library copy, and the library itself (a new skill, an edit, a
//! reset, an update, a delete).
//!
//! Nothing here is run on a frame that was not asked for, and nothing here runs anything in a skill. A script in
//! a skill's folder is copied like any other file.

use std::path::{Path, PathBuf};

use super::catalogue::{self, Files};
use super::frontmatter;
use super::harness::{Harness, Places, Scope};
use super::model::{Record, Skill, Source};

/// Where things are, for one change.
#[derive(Debug, Clone)]
pub struct Here<'a> {
    pub places: &'a Places,
    pub project: Option<&'a Path>,
    /// The plugin's own folder. The library is `<folder>/library`.
    pub folder: Option<&'a Path>,
}

impl Here<'_> {
    /// The library's folder for `id`, or a refusal when this window keeps no files.
    pub fn library_for(&self, id: &str) -> Result<PathBuf, String> {
        self.folder.map(|folder| folder.join("library").join(id)).ok_or_else(|| {
            "This window keeps no skills of its own, because it has no settings folder.".to_owned()
        })
    }

    /// Put a folder somewhere it can be got back from, on the person's own machine, or take it away in a test.
    pub fn throw_away(&self, folder: &Path) -> Result<(), String> {
        let done = match self.places.follow_the_environment {
            true => crate::services::recycle::delete(folder),
            false => std::fs::remove_dir_all(folder),
        };
        done.map_err(|problem| format!("could not remove {}: {problem}", folder.display()))
    }
}

/// The files a skill is installed from: its library copy, or what shipped in the binary.
pub fn files_of(skill: &Skill) -> Result<Files, String> {
    if let Some(folder) = &skill.library {
        return Ok(catalogue::read_folder(folder));
    }
    if skill.partial {
        return Err(format!(
            "{} has to be fetched from unluminous.com before it can be installed.",
            skill.id
        ));
    }
    let files = catalogue::bundled_files(&skill.id);
    match files.is_empty() {
        true => Err(format!("{} has no files to install.", skill.id)),
        false => Ok(files),
    }
}

/// What one install or remove did, for each harness.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub lines: Vec<String>,
    /// The folders written or removed.
    pub changed: Vec<PathBuf>,
    /// The harnesses that were refused, with why.
    pub refused: Vec<(String, String)>,
}

impl Report {
    pub fn said(&self) -> String {
        self.lines.join("\n")
    }
}

/// Write `files` as an installed copy of `id` in `parent`, marked as Unluminous's.
fn write_copy(parent: &Path, id: &str, files: &Files) -> Result<PathBuf, String> {
    let folder = parent.join(id);
    if folder.exists() {
        std::fs::remove_dir_all(&folder)
            .map_err(|problem| format!("could not replace {}: {problem}", folder.display()))?;
    }
    let mut marked = files.clone();
    if let Some(text) =
        marked.get("SKILL.md").map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    {
        marked.insert("SKILL.md".into(), frontmatter::with_mark(&text, id).into_bytes());
    }
    catalogue::write_folder(&folder, &marked)?;
    Ok(folder)
}

/// Install `skill` for each of `harnesses` in `scope`.
///
/// A harness that already reads a folder this install wrote to is not given a second copy. A folder holding a
/// skill of the same name that Unluminous did not write is left alone unless `force`, which puts it somewhere it
/// can be got back from first.
pub fn install(
    here: &Here<'_>,
    skill: &Skill,
    harnesses: &[&'static Harness],
    scope: Scope,
    force: bool,
) -> Result<Report, String> {
    let files = files_of(skill)?;
    let mut report = Report::default();
    let mut written: Vec<PathBuf> = Vec::new();
    for harness in harnesses {
        let Some(parent) = here.places.write_folder(harness, scope, here.project) else {
            report.refused.push((harness.id.into(), "no project is open".into()));
            report.lines.push(format!(
                "{}: no project is open, so there is no project folder.",
                harness.name
            ));
            continue;
        };
        let reads = here.places.read_folders(harness, scope, here.project);
        if let Some(shared) =
            written.iter().find(|folder| reads.contains(folder) && **folder != parent)
        {
            report.lines.push(format!(
                "{}: reads it from {}.",
                harness.name,
                super::harness::shown(here.places, shared, here.project)
            ));
            continue;
        }
        let target = parent.join(&skill.id);
        if written.contains(&parent) {
            report.lines.push(format!(
                "{}: shares {}.",
                harness.name,
                super::harness::shown(here.places, &parent, here.project)
            ));
            continue;
        }
        if target.join("SKILL.md").exists() && !is_ours(&target) {
            if !force {
                let why = format!(
                    "{} already holds a {} that Unluminous did not write; install with force to replace it",
                    super::harness::shown(here.places, &parent, here.project),
                    skill.id
                );
                report.lines.push(format!("{}: {why}.", harness.name));
                report.refused.push((harness.id.into(), why));
                continue;
            }
            here.throw_away(&target)?;
        }
        let folder = write_copy(&parent, &skill.id, &files)?;
        report.lines.push(format!(
            "{}: installed in {}.",
            harness.name,
            super::harness::shown(here.places, &folder, here.project)
        ));
        report.changed.push(folder);
        written.push(parent);
    }
    Ok(report)
}

/// Whether the skill folder at `folder` is a copy Unluminous wrote.
pub fn is_ours(folder: &Path) -> bool {
    std::fs::read_to_string(folder.join("SKILL.md")).is_ok_and(|text| frontmatter::has_mark(&text))
}

/// Remove `skill` from each of `harnesses`' own folders in `scope`.
///
/// A copy Unluminous did not write is left alone unless `force`. When a harness still reads the skill from a
/// shared folder, the report says which.
pub fn remove(
    here: &Here<'_>,
    skill: &Skill,
    harnesses: &[&'static Harness],
    scope: Scope,
    force: bool,
) -> Result<Report, String> {
    let mut report = Report::default();
    for harness in harnesses {
        let Some(parent) = here.places.write_folder(harness, scope, here.project) else { continue };
        let target = parent.join(&skill.id);
        if report.changed.contains(&target) {
            continue;
        }
        if target.join("SKILL.md").exists() {
            match is_ours(&target) || force {
                true => {
                    match is_ours(&target) {
                        true => std::fs::remove_dir_all(&target).map_err(|problem| {
                            format!("could not remove {}: {problem}", target.display())
                        })?,
                        false => here.throw_away(&target)?,
                    }
                    report.lines.push(format!(
                        "{}: removed from {}.",
                        harness.name,
                        super::harness::shown(here.places, &parent, here.project)
                    ));
                    report.changed.push(target);
                }
                false => {
                    let why = format!(
                        "the copy in {} was not written by Unluminous; remove with force to take it away",
                        super::harness::shown(here.places, &parent, here.project)
                    );
                    report.lines.push(format!("{}: {why}.", harness.name));
                    report.refused.push((harness.id.into(), why));
                }
            }
        }
        let still = here
            .places
            .read_folders(harness, scope, here.project)
            .into_iter()
            .find(|folder| folder.join(&skill.id).join("SKILL.md").exists());
        if let Some(folder) = still {
            report.lines.push(format!(
                "{} still reads it from {}.",
                harness.name,
                super::harness::shown(here.places, &folder, here.project)
            ));
        }
    }
    if report.lines.is_empty() {
        report.lines.push(format!("{} was not installed there.", skill.id));
    }
    Ok(report)
}

/// Write the skill's files again into every copy Unluminous wrote that no longer matches them. Answers how many
/// copies were written.
pub fn keep_in_step(skill: &Skill) -> Result<usize, String> {
    let stale: Vec<&super::model::Copy> =
        skill.copies.iter().filter(|copy| copy.ours && !copy.current).collect();
    if stale.is_empty() || skill.source == Source::Found {
        return Ok(0);
    }
    let files = files_of(skill)?;
    for copy in &stale {
        write_copy(&copy.parent, &skill.id, &files)?;
    }
    Ok(stale.len())
}

/// Make sure the skill has a library copy, writing what shipped there the first time. Answers its folder.
pub fn into_the_library(here: &Here<'_>, skill: &Skill) -> Result<PathBuf, String> {
    if let Some(folder) = &skill.library {
        return Ok(folder.clone());
    }
    if skill.source == Source::Found {
        return Err(format!(
            "{} is not one of Unluminous's skills; edit it where it is.",
            skill.id
        ));
    }
    let files = files_of(skill)?;
    let folder = here.library_for(&skill.id)?;
    catalogue::write_folder(&folder, &files)?;
    Record { source: skill.source.name().into(), origin: skill.origin.clone() }.write(&folder)?;
    Ok(folder)
}

/// The `SKILL.md` a person edits for this skill: the library copy, or for a found skill the file where it is.
pub fn file_to_edit(here: &Here<'_>, skill: &Skill) -> Result<PathBuf, String> {
    match skill.source {
        Source::Found => skill
            .copies
            .first()
            .map(|copy| copy.folder.join("SKILL.md"))
            .ok_or_else(|| format!("{} is not on this machine any more.", skill.id)),
        _ => Ok(into_the_library(here, skill)?.join("SKILL.md")),
    }
}

/// Make a new skill of the person's own, and answer its `SKILL.md`.
pub fn new_skill(
    here: &Here<'_>,
    name: &str,
    description: &str,
    taken: &[String],
) -> Result<PathBuf, String> {
    let id = frontmatter::tidy_name(name);
    if !frontmatter::is_a_good_name(&id) {
        return Err("A skill's name is letters, digits and dashes, such as release-notes.".into());
    }
    if taken.contains(&id) {
        return Err(format!("There is already a skill called {id}."));
    }
    let folder = here.library_for(&id)?;
    std::fs::create_dir_all(&folder)
        .map_err(|problem| format!("could not make {}: {problem}", folder.display()))?;
    let file = folder.join("SKILL.md");
    crate::services::store::write_a_source_file(
        &file,
        frontmatter::starter(&id, description).as_bytes(),
    )
    .map_err(|problem| format!("could not write {}: {problem}", file.display()))?;
    Record { source: Source::Mine.name().into(), origin: String::new() }.write(&folder)?;
    Ok(file)
}

/// Replace a skill's `SKILL.md` with `text`, writing what shipped into the library first.
pub fn write_text(here: &Here<'_>, skill: &Skill, text: &str) -> Result<PathBuf, String> {
    let file = file_to_edit(here, skill)?;
    crate::services::store::write_a_source_file(&file, text.as_bytes())
        .map_err(|problem| format!("could not write {}: {problem}", file.display()))?;
    Ok(file)
}

/// Put back what shipped, throwing the library copy away.
pub fn reset(here: &Here<'_>, skill: &Skill) -> Result<String, String> {
    match (skill.source, &skill.library) {
        (Source::Mine | Source::Found, _) => Err(format!(
            "{} did not ship with Unluminous, so there is nothing to put back.",
            skill.id
        )),
        (_, None) => Ok(format!("{} is already as it shipped.", skill.id)),
        (_, Some(folder)) => {
            here.throw_away(folder)?;
            Ok(format!("{} is back as it shipped.", skill.id))
        }
    }
}

/// Delete a skill of the person's own, or one found in a harness folder, everywhere it is.
pub fn delete(here: &Here<'_>, skill: &Skill) -> Result<String, String> {
    match skill.source {
        Source::Unluminous | Source::Catalogue => Err(format!(
            "{} ships with Unluminous; remove it from the agents instead, or reset it.",
            skill.id
        )),
        Source::Mine | Source::Found => {
            let mut gone = 0;
            for copy in &skill.copies {
                here.throw_away(&copy.folder)?;
                gone += 1;
            }
            if let Some(folder) = &skill.library {
                here.throw_away(folder)?;
            }
            Ok(format!("{} is deleted, with {gone} installed copies.", skill.id))
        }
    }
}

/// Take newer files from the site into the library. An edited skill keeps the person's edits unless `discard`.
pub fn take(
    here: &Here<'_>,
    skill: &Skill,
    files: &Files,
    origin: &str,
    discard: bool,
) -> Result<String, String> {
    if skill.edited && !discard {
        return Err(format!(
            "{} has edits of yours; update with discard to take the new one (your copy goes to {}).",
            skill.id,
            crate::services::recycle::destination().name()
        ));
    }
    let folder = here.library_for(&skill.id)?;
    if folder.exists() {
        here.throw_away(&folder)?;
    }
    catalogue::write_folder(&folder, files)?;
    Record { source: skill.source.name().into(), origin: origin.into() }.write(&folder)?;
    Ok(format!("{} is up to date.", skill.id))
}
