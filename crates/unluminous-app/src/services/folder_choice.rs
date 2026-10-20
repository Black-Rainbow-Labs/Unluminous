//! The folder an agent runs in, as a list to choose from with `Other folder...` at the end (`task-2270`).
//!
//! *"In settings, allow me to select the project folder for the directory the agent chat uses, similar to agent
//! tasks task modal. Also allow me to select "Other" and see a folder picker where I can select the folder. Add
//! that for agent tasks task modal as well."*
//!
//! The Agent-Chat settings page and the ticket modal's `Project` field offer the same rows in the same order, so
//! the two cannot come to read differently: the default, which is the folder the window has open and is named
//! as such; the folders the window knows (its project and the recent projects); the folder chosen now when it
//! is none of those; and last [`OTHER`], which opens the system's folder picker. A row reads as the folder's
//! name followed by the useful end of its path, because two projects called `app` are told apart by where
//! they are.
//!
//! The picker belongs to the window: a plugin asks for it with `plugin_ui::Request::PickFolder`, and the folder
//! chosen comes back through `UiProvider::answered` as `{ "folder": <path or null> }`.

use std::path::{Path, PathBuf};

/// The last row, which opens the system's folder picker.
pub const OTHER: &str = "Other folder\u{2026}";

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// Nothing chosen: the folder the window has open.
    Default,
    /// A folder, by its path as written.
    Folder(String),
    /// Open the system's folder picker.
    Other,
}

/// The rows, in the order they are offered.
///
/// `chosen` is what is chosen now, `None` for the default. `known` is every other folder worth offering, the
/// window's project first; a folder named twice, or naming the chosen one, is offered once.
pub fn rows(chosen: Option<&str>, known: &[PathBuf]) -> Vec<Row> {
    let mut rows = vec![Row::Default];
    let mut seen: Vec<String> = Vec::new();
    let mut offer = |folder: String, rows: &mut Vec<Row>| {
        let key = folder.trim_end_matches(['/', '\\']).to_lowercase();
        if !folder.trim().is_empty() && !seen.contains(&key) {
            seen.push(key);
            rows.push(Row::Folder(folder));
        }
    };
    for folder in known {
        offer(folder.display().to_string(), &mut rows);
    }
    if let Some(chosen) = chosen {
        offer(chosen.to_owned(), &mut rows);
    }
    rows.push(Row::Other);
    rows
}

/// The words a person reads for each row. `default` is how the default row is said.
pub fn said(rows: &[Row], default: &str) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Default => default.to_owned(),
            Row::Folder(folder) => named(folder),
            Row::Other => OTHER.to_owned(),
        })
        .collect()
}

/// Which row is the one chosen now.
pub fn selected(rows: &[Row], chosen: Option<&str>) -> Option<usize> {
    let wanted = chosen.map(|folder| folder.trim_end_matches(['/', '\\']).to_lowercase());
    rows.iter().position(|row| match (row, &wanted) {
        (Row::Default, None) => true,
        (Row::Folder(folder), Some(wanted)) => {
            folder.trim_end_matches(['/', '\\']).to_lowercase() == *wanted
        }
        _ => false,
    })
}

/// A folder as a row says it: its own name, then the useful end of its path.
pub fn named(folder: &str) -> String {
    let path = Path::new(folder);
    let name = path.file_name().map(|name| name.to_string_lossy().to_string());
    let end = crate::services::paths::the_useful_end_of(folder);
    match name {
        Some(name) if end != name => format!("{name} \u{2014} {end}"),
        Some(name) => name,
        None => folder.to_owned(),
    }
}

/// The folder an answer from the window's picker names, or `None` when the person cancelled.
pub fn picked(answer: &serde_json::Value) -> Option<PathBuf> {
    answer.get("folder").and_then(serde_json::Value::as_str).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default is first and `Other folder...` last, with every known folder once between them, and a chosen
    /// folder nobody else knows is kept so the list can say what is chosen.
    #[test]
    fn the_rows_are_the_default_the_known_folders_the_chosen_one_and_other() {
        let known = vec![
            PathBuf::from("C:/dev/app"),
            PathBuf::from("C:/dev/site"),
            PathBuf::from("C:/dev/app/"),
        ];
        let rows = rows(Some("D:/elsewhere/tool"), &known);
        assert_eq!(
            rows,
            vec![
                Row::Default,
                Row::Folder("C:/dev/app".to_owned()),
                Row::Folder("C:/dev/site".to_owned()),
                Row::Folder("D:/elsewhere/tool".to_owned()),
                Row::Other,
            ]
        );
        assert_eq!(selected(&rows, Some("D:/elsewhere/tool")), Some(3));
        assert_eq!(selected(&rows, None), Some(0));
        let words = said(&rows, "The folder this window has open");
        assert_eq!(words[0], "The folder this window has open");
        assert_eq!(words[4], OTHER);
        assert!(words[3].starts_with("tool"), "{}", words[3]);
    }

    /// A cancelled picker answers null, which is no folder; a chosen one answers its path.
    #[test]
    fn the_pickers_answer_is_a_folder_or_nothing() {
        assert_eq!(
            picked(&serde_json::json!({ "folder": "C:/dev/app" })),
            Some(PathBuf::from("C:/dev/app"))
        );
        assert_eq!(picked(&serde_json::json!({ "folder": null })), None);
    }
}
