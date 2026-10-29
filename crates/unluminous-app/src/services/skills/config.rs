//! How the Skills Manager is set up, in `settings.conf` in the plugin's own folder, so
//! `settings set plugins.skills.scope global` works with no code here for it.
//!
//! Four keys: `scope`, the default for where an install goes; `harnesses`, the agents an install is for when none
//! is named; `catalogue`, where the site's list is read from; and `folders`, more skills folders to look in.

use std::path::{Path, PathBuf};

use super::catalogue;
use super::harness::{self, Places, Scope};
use crate::services::plugin_ui;
use crate::services::store::Values;

#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    pub scope: Scope,
    /// The ids of the harnesses an install is for. Empty means the ones found on this machine.
    pub harnesses: Vec<String>,
    pub catalogue: String,
    pub folders: Vec<PathBuf>,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            scope: Scope::Project,
            harnesses: Vec::new(),
            catalogue: catalogue::SITE.into(),
            folders: Vec::new(),
        }
    }
}

impl Configuration {
    pub const FILE: &'static str = "settings.conf";

    /// Read it from the plugin's folder, and say what was refused.
    pub fn read(folder: &Path) -> (Self, Vec<String>) {
        let text = std::fs::read_to_string(folder.join(Self::FILE)).unwrap_or_default();
        Self::of(&Values::parse(&text))
    }

    /// Read it from values already parsed.
    pub fn of(values: &Values) -> (Self, Vec<String>) {
        let mut out = Configuration::default();
        let mut refused = Vec::new();
        if let Some(word) = values.text("scope") {
            match Scope::parse(word) {
                Some(scope) => out.scope = scope,
                None => refused.push(format!("scope is `{word}`, and it is project or global.")),
            }
        }
        if let Some(list) = values.text("harnesses") {
            for word in list.split(',').map(str::trim).filter(|word| !word.is_empty()) {
                match harness::find(word) {
                    Some(found) => out.harnesses.push(found.id.to_owned()),
                    None => refused.push(format!(
                        "harnesses names `{word}`, which is not an agent Unluminous knows."
                    )),
                }
            }
        }
        if let Some(address) = values.text("catalogue").filter(|address| !address.trim().is_empty())
        {
            out.catalogue = address.trim().to_owned();
        }
        if let Some(list) = values.text("folders") {
            out.folders = list
                .split('|')
                .map(str::trim)
                .filter(|folder| !folder.is_empty())
                .map(PathBuf::from)
                .collect();
        }
        (out, refused)
    }

    /// Write it to the plugin's folder.
    pub fn write(&self, folder: &Path) -> Result<(), String> {
        let existing = std::fs::read_to_string(folder.join(Self::FILE)).unwrap_or_default();
        let mut values = Values::parse(&existing);
        values.set("scope", self.scope.name());
        values.set("harnesses", self.harnesses.join(", "));
        values.set("catalogue", self.catalogue.clone());
        values.set(
            "folders",
            self.folders
                .iter()
                .map(|folder| folder.display().to_string())
                .collect::<Vec<String>>()
                .join(" | "),
        );
        plugin_ui::write_values(
            folder,
            Self::FILE,
            &values,
            "How the Skills Manager is set up.\nWritten by Unluminous; safe to edit by hand.",
        )
    }

    /// The harnesses an install is for when none is named: the ones Settings names, or every one used on this
    /// machine, or Claude Code and Codex when none is.
    pub fn chosen_harnesses(&self, places: &Places) -> Vec<&'static harness::Harness> {
        let named: Vec<&'static harness::Harness> =
            self.harnesses.iter().filter_map(|id| harness::find(id)).collect();
        if !named.is_empty() {
            return named;
        }
        let here: Vec<&'static harness::Harness> =
            harness::HARNESSES.iter().filter(|harness| places.is_here(harness)).collect();
        match here.is_empty() {
            true => ["claude", "codex"].iter().filter_map(|id| harness::find(id)).collect(),
            false => here,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_its_four_keys() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-skills-config-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let wanted = Configuration {
            scope: Scope::Global,
            harnesses: vec!["claude".into(), "pi".into()],
            catalogue: "C:/somewhere/catalogue.json".into(),
            folders: vec![PathBuf::from("D:/skills")],
        };
        wanted.write(&folder).unwrap();
        let (read, refused) = Configuration::read(&folder);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(read, wanted);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_wrong_word_is_refused_with_a_sentence() {
        let (read, refused) =
            Configuration::of(&Values::parse("scope = sideways\nharnesses = claude, notepad\n"));
        assert_eq!(read.scope, Scope::Project);
        assert_eq!(read.harnesses, ["claude"]);
        assert_eq!(refused.len(), 2);
    }

    #[test]
    fn with_nothing_named_and_nothing_found_it_is_claude_and_codex() {
        let home =
            std::env::temp_dir().join(format!("unluminous-skills-nobody-{}", std::process::id()));
        let chosen = Configuration::default().chosen_harnesses(&Places::at(&home));
        let ids: Vec<&str> = chosen.iter().map(|harness| harness.id).collect();
        assert_eq!(ids, ["claude", "codex"]);
    }
}
