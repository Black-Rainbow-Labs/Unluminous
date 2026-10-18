//! Which files are designs, listing them, and making a new one from the starter page.
//!
//! A design is any `*.html` or `*.htm` file in the designs folder or below it. [`is_a_design`] is the one
//! question: the window asks it before it opens a file, and the provider asks it when it lists them, so the
//! two cannot disagree about what a design is.
//!
//! Every path the provider keeps is **relative to the project** and compared by components, so a path
//! written with `/` and the same path written with `\` are one design.

use std::path::{Component, Path, PathBuf};

use super::config::{slashed, Starter};

/// The starter page, in the house style: the hand picture filling the page and the word DESIGN over it, and
/// nothing else (`task-2294`).
pub const HOUSE_STARTER: &str = include_str!("starter.html");

/// The picture the house starter is drawn on (`task-2293`), and where in the designs folder it is written, which
/// is the address the starter's own stylesheet names. A JPEG, so the binary carries 167 KB rather than the
/// 2.5 MB PNG it was made from.
pub const HOUSE_BACKGROUND: &[u8] = include_bytes!("design-background.jpg");
pub const HOUSE_BACKGROUND_PATH: &str = "assets/design-background.jpg";

/// An empty page, for a person who wants to start from nothing.
pub const BLANK_STARTER: &str = "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Untitled design</title>\n\
</head>\n<body>\n</body>\n</html>\n";

/// How deep below the designs folder a design is looked for.
const DEEPEST: usize = 8;

/// Folders that are never looked inside for a design.
const SKIPPED: [&str; 3] = ["node_modules", ".git", "target"];

/// The designs folder as a path on disk.
///
/// @param project - the folder the window has open
/// @param folder - the configured designs folder, relative to the project
pub fn designs_root(project: &Path, folder: &Path) -> PathBuf {
    project.join(folder)
}

/// The path relative to the project, whether it was given absolute or already relative.
///
/// @param project - the folder the window has open
/// @param path - a path inside the project
pub fn relative_to_project(project: &Path, path: &Path) -> Option<PathBuf> {
    let relative = match path.is_absolute() {
        true => path.strip_prefix(project).ok()?.to_path_buf(),
        false => path.to_path_buf(),
    };
    let tidy: PathBuf =
        relative.components().filter(|part| matches!(part, Component::Normal(_))).collect();
    (!tidy.as_os_str().is_empty()).then_some(tidy)
}

/// Whether a path is an HTML file by its extension.
///
/// @param path - any path
pub fn is_html(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_lowercase().as_str(), "html" | "htm"))
}

/// Whether a file is one of the project's designs.
///
/// @param project - the folder the window has open
/// @param folder - the configured designs folder, relative to the project
/// @param path - the file, absolute or relative to the project
pub fn is_a_design(project: &Path, folder: &Path, path: &Path) -> bool {
    let Some(relative) = relative_to_project(project, path) else { return false };
    is_html(&relative) && relative.starts_with(folder) && relative != *folder
}

/// A design's name as the pane and the command line show it: its path inside the designs folder.
///
/// @param folder - the configured designs folder, relative to the project
/// @param design - a design, relative to the project
pub fn name_of(folder: &Path, design: &Path) -> String {
    slashed(design.strip_prefix(folder).unwrap_or(design))
}

/// Every design, as paths relative to the project, in name order.
///
/// @param project - the folder the window has open
/// @param folder - the configured designs folder, relative to the project
pub fn list(project: &Path, folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(&designs_root(project, folder), folder.to_path_buf(), 0, &mut found);
    found.sort_by_key(|path| slashed(path).to_lowercase());
    found
}

/// Look in one folder, and in the folders below it.
fn walk(disk: &Path, relative: PathBuf, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(disk) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let below = relative.join(&name);
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() && depth < DEEPEST && !SKIPPED.contains(&name.as_str()) {
            walk(&entry.path(), below, depth + 1, found);
        } else if kind.is_file() && is_html(&below) {
            found.push(below);
        }
    }
}

/// Make a design from the starter page, and answer where it is, relative to the project.
///
/// A name that holds a separator is refused, because the folder structure of the designs is the person's
/// own to make, and one that exists is refused, because overwriting somebody's design would lose it.
///
/// @param project - the folder the window has open
/// @param folder - the configured designs folder, relative to the project
/// @param name - the name typed, with or without `.html`
/// @param starter - which page to start from
pub fn make(
    project: &Path,
    folder: &Path,
    name: &str,
    starter: Starter,
) -> Result<PathBuf, String> {
    let file = a_file_name(name)?;
    let relative = folder.join(&file);
    let disk = project.join(&relative);
    if disk.exists() {
        return Err(format!("There is already a design called `{file}`."));
    }
    std::fs::create_dir_all(designs_root(project, folder)).map_err(|problem| {
        format!("{} could not be made: {problem}", designs_root(project, folder).display())
    })?;
    let page = match starter {
        Starter::House => HOUSE_STARTER,
        Starter::Blank => BLANK_STARTER,
    };
    // The house starter's picture goes beside it the first time, and is never written over: a person who
    // changed it changed it on purpose.
    if starter == Starter::House {
        let picture = designs_root(project, folder).join(HOUSE_BACKGROUND_PATH);
        if !picture.exists() {
            if let Some(parent) = picture.parent() {
                std::fs::create_dir_all(parent).map_err(|problem| {
                    format!("{} could not be made: {problem}", parent.display())
                })?;
            }
            crate::services::store::write_atomically(&picture, HOUSE_BACKGROUND).map_err(
                |problem| format!("{} could not be written: {problem}", picture.display()),
            )?;
        }
    }
    crate::services::store::write_atomically(&disk, page.as_bytes())
        .map_err(|problem| format!("{} could not be written: {problem}", disk.display()))?;
    Ok(relative)
}

/// The file name a typed name stands for.
fn a_file_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Say what the design is called.".to_owned());
    }
    if name.contains(['/', '\\']) {
        return Err(format!("`{name}` has a slash in it. A design's name is one file name."));
    }
    if name.contains([':', '*', '?', '"', '<', '>', '|']) || name.starts_with('.') {
        return Err(format!("`{name}` cannot be a file name."));
    }
    Ok(match is_html(Path::new(name)) {
        true => name.to_owned(),
        false => format!("{name}.html"),
    })
}

/// The design a name stands for, as a path relative to the project.
///
/// The name can be written the way the pane lists it (`landing.html`, `pages/landing`), without the
/// extension, or in full from the project (`.designs/landing.html`).
///
/// @param project - the folder the window has open
/// @param folder - the configured designs folder, relative to the project
/// @param name - what was typed
pub fn find(project: &Path, folder: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Say which design.".to_owned());
    }
    let given = PathBuf::from(name);
    let mut tries: Vec<PathBuf> = vec![given.clone(), folder.join(&given)];
    if !is_html(&given) {
        tries.push(PathBuf::from(format!("{name}.html")));
        tries.push(folder.join(format!("{name}.html")));
    }
    for tried in tries {
        let Some(relative) = relative_to_project(project, &tried) else { continue };
        if is_a_design(project, folder, &relative) && project.join(&relative).is_file() {
            return Ok(relative);
        }
    }
    let names: Vec<String> =
        list(project, folder).iter().map(|design| name_of(folder, design)).collect();
    Err(format!(
        "There is no design called `{name}`. The designs are: {}.",
        match names.is_empty() {
            true => "none yet".to_owned(),
            false => names.join(", "),
        }
    ))
}

/// The notes file that belongs to a design: `<name>.comments.json`, beside it.
///
/// @param design - the design, relative to the project or absolute
pub fn comments_path(design: &Path) -> PathBuf {
    let stem =
        design.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    design.with_file_name(format!("{stem}.comments.json"))
}
