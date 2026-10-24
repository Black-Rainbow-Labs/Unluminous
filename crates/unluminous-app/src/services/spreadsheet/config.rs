//! The Spreadsheet plugin's own settings, in `settings.conf` in its folder under the settings folder.
//!
//! Two lines of `name = value`. Read when the plugin opens and written when the Settings page changes one.

use std::path::Path;

/// What a sheet is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Paper {
    /// The theme's own ground, with the workbook's dark colours lightened so they can be read on a dark one.
    #[default]
    Theme,
    /// White, as Excel draws a sheet, whatever the theme is.
    Light,
}

impl Paper {
    pub fn name(&self) -> &'static str {
        match self {
            Paper::Theme => "theme",
            Paper::Light => "light",
        }
    }

    pub fn parse(text: &str) -> Option<Paper> {
        match text.trim() {
            "theme" => Some(Paper::Theme),
            "light" => Some(Paper::Light),
            _ => None,
        }
    }
}

/// The settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub paper: Paper,
    /// Whether the list of functions opens while a formula is typed.
    pub function_list: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { paper: Paper::Theme, function_list: true }
    }
}

/// The file every plugin keeps its settings in, which `settings set plugins.spreadsheet.<key>` writes too.
pub const FILE: &str = crate::services::plugin_settings::FILE;

impl Config {
    /// Read the settings from the plugin's folder, answering the defaults and what was wrong when a line
    /// could not be read.
    pub fn read(folder: &Path) -> (Config, Option<String>) {
        let mut config = Config::default();
        let Ok(text) = std::fs::read_to_string(folder.join(FILE)) else {
            return (config, None);
        };
        let mut problems = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                problems.push(format!("`{line}` is not `name = value`"));
                continue;
            };
            match name.trim() {
                "paper" => match Paper::parse(value) {
                    Some(paper) => config.paper = paper,
                    None => problems
                        .push(format!("paper is `theme` or `light`, not `{}`", value.trim())),
                },
                "function_list" => config.function_list = value.trim() != "off",
                other => problems.push(format!("`{other}` is not a Spreadsheet setting")),
            }
        }
        (config, (!problems.is_empty()).then(|| problems.join("; ")))
    }

    /// Write the settings to the plugin's folder.
    pub fn write(&self, folder: &Path) -> Result<(), String> {
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        let text = format!(
            "# The Spreadsheet plugin's settings. `Settings -> Spreadsheet` writes this file.\npaper = {}\nfunction_list = {}\n",
            self.paper.name(),
            if self.function_list { "on" } else { "off" }
        );
        crate::services::store::write_atomically(&folder.join(FILE), text.as_bytes())
            .map_err(|e| e.to_string())
    }
}
