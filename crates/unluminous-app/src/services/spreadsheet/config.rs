//! The Spreadsheet plugin's own settings, in `settings.conf` in its folder under the settings folder.
//!
//! Lines of `name = value`. Read when the plugin opens and written when the Settings page changes one.
//! These are the options Excel keeps for a person rather than in a workbook: where Enter goes, whether
//! typing completes from the column, what the status bar adds up, the custom fill lists.

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

/// Where the active cell goes after Enter, as Excel's "After pressing Enter, move selection" option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnterDirection {
    #[default]
    Down,
    Right,
    Up,
    Left,
    /// Enter finishes the cell and stays on it.
    None,
}

impl EnterDirection {
    pub const ALL: [EnterDirection; 5] = [
        EnterDirection::Down,
        EnterDirection::Right,
        EnterDirection::Up,
        EnterDirection::Left,
        EnterDirection::None,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EnterDirection::Down => "down",
            EnterDirection::Right => "right",
            EnterDirection::Up => "up",
            EnterDirection::Left => "left",
            EnterDirection::None => "none",
        }
    }

    pub fn parse(text: &str) -> Option<EnterDirection> {
        EnterDirection::ALL.into_iter().find(|d| d.name() == text.trim())
    }

    /// The step Enter takes, rows then columns; Shift+Enter takes the opposite one.
    pub fn step(self) -> (i64, i64) {
        match self {
            EnterDirection::Down => (1, 0),
            EnterDirection::Right => (0, 1),
            EnterDirection::Up => (-1, 0),
            EnterDirection::Left => (0, -1),
            EnterDirection::None => (0, 0),
        }
    }
}

/// The figures the status bar can show about a selection, in the order Excel's menu lists them.
pub const AGGREGATES: [&str; 6] = ["average", "count", "numbers", "min", "max", "sum"];

/// The settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub paper: Paper,
    /// Whether the list of functions opens while a formula is typed.
    pub function_list: bool,
    pub enter: EnterDirection,
    /// Typing in a cell offers what the column already holds that starts with the same letters.
    pub autocomplete: bool,
    /// Typing corrects two initial capitals and Excel's list of common slips.
    pub autocorrect: bool,
    /// A web or mail address typed into a cell becomes a link.
    pub auto_links: bool,
    /// Cells that break one of Excel's error checking rules show a green triangle.
    pub error_checking: bool,
    /// The figures the status bar shows, from [`AGGREGATES`].
    pub aggregates: Vec<String>,
    /// Fill lists a person added, each a comma separated list, after Excel's own.
    pub custom_lists: Vec<String>,
    /// The name a new note or comment is written under; empty means the person signed in.
    pub author: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            paper: Paper::Theme,
            function_list: true,
            enter: EnterDirection::Down,
            autocomplete: true,
            autocorrect: true,
            auto_links: true,
            error_checking: true,
            aggregates: vec!["average".to_owned(), "count".to_owned(), "sum".to_owned()],
            custom_lists: Vec::new(),
            author: String::new(),
        }
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
            if let Err(problem) = config.set(name.trim(), value.trim()) {
                problems.push(problem);
            }
        }
        (config, (!problems.is_empty()).then(|| problems.join("; ")))
    }

    /// Set one setting by its name, as a line of the file says it.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let on = value != "off";
        match name {
            "paper" => {
                self.paper = Paper::parse(value)
                    .ok_or_else(|| format!("paper is `theme` or `light`, not `{value}`"))?
            }
            "function_list" => self.function_list = on,
            "enter" => {
                self.enter = EnterDirection::parse(value).ok_or_else(|| {
                    format!("enter is down, right, up, left or none, not `{value}`")
                })?
            }
            "autocomplete" => self.autocomplete = on,
            "autocorrect" => self.autocorrect = on,
            "auto_links" => self.auto_links = on,
            "error_checking" => self.error_checking = on,
            "aggregates" => {
                let chosen: Vec<String> = value
                    .split(',')
                    .map(|w| w.trim().to_lowercase())
                    .filter(|w| !w.is_empty())
                    .collect();
                if let Some(bad) = chosen.iter().find(|w| !AGGREGATES.contains(&w.as_str())) {
                    return Err(format!("`{bad}` is not one of {}", AGGREGATES.join(", ")));
                }
                self.aggregates = chosen;
            }
            "custom_lists" => {
                self.custom_lists = value
                    .split('|')
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            }
            "author" => self.author = value.to_owned(),
            other => return Err(format!("`{other}` is not a Spreadsheet setting")),
        }
        Ok(())
    }

    /// Write the settings to the plugin's folder.
    pub fn write(&self, folder: &Path) -> Result<(), String> {
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        let switch = |on: bool| if on { "on" } else { "off" };
        let text = format!(
            "# The Spreadsheet plugin's settings. `Settings -> Spreadsheet` writes this file.\n\
             paper = {}\nfunction_list = {}\nenter = {}\nautocomplete = {}\nautocorrect = {}\n\
             auto_links = {}\nerror_checking = {}\naggregates = {}\ncustom_lists = {}\nauthor = {}\n",
            self.paper.name(),
            switch(self.function_list),
            self.enter.name(),
            switch(self.autocomplete),
            switch(self.autocorrect),
            switch(self.auto_links),
            switch(self.error_checking),
            self.aggregates.join(","),
            self.custom_lists.join(" | "),
            self.author,
        );
        crate::services::store::write_atomically(&folder.join(FILE), text.as_bytes())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_reads_back_as_it_was_written() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-sheet-config-{}", std::process::id()));
        let config = Config {
            enter: EnterDirection::Right,
            autocomplete: false,
            aggregates: vec!["min".to_owned(), "max".to_owned()],
            custom_lists: vec!["Low,Medium,High".to_owned()],
            author: "Sam".to_owned(),
            ..Config::default()
        };
        config.write(&folder).unwrap();
        let (read, problem) = Config::read(&folder);
        assert_eq!(problem, None);
        assert_eq!(read, config);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
