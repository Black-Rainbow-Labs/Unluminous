//! The Designer pane's own tabs: which designs are open in it, which one is showing, and the blank tab.
//!
//! `task-2260`: *"The designer pane should include it's own set of tabs for the rendered html, not require the
//! file panel that's mixed with all the other files that are open. It should have a blank tab showing by
//! default, and remember which tabs/files are open between restarts."*
//!
//! A tab is `Some(design)` or `None`, and `None` is the blank tab, which shows the list of designs to open one
//! from. There is always at least one tab, so a pane whose last design was closed shows the blank tab rather than
//! nothing. Opening a design shows the tab already on it, or takes the blank tab when that is the one showing, or
//! adds a tab after the one showing, which is what a browser does with a link opened from its new tab page.
//!
//! The tabs are written to the project's `.unluminous/designs.conf` beside the device each design was last
//! viewed at, as `tabs.count`, `tabs.<n>` and `tabs.active`, and read back when the provider learns which project
//! is open. A design whose file has gone is left out when they are read, rather than coming back as a tab on
//! nothing. The page each tab shows is the window's, kept in `UnluminousApp::design_tabs`, and the window lets go
//! of the page of a tab that is no longer here.

use std::path::{Path, PathBuf};

use super::config::slashed;
use super::Designer;
use crate::services::store::Values;

/// What a blank tab is written as. A design is a path ending in `.html`, so it can never be this.
const BLANK: &str = "blank";

impl Designer {
    /// The pane's tabs, in the order they are drawn. `None` is a blank tab.
    pub fn tabs(&self) -> &[Option<PathBuf>] {
        &self.tabs
    }

    /// Which tab is showing.
    pub fn active_tab(&self) -> usize {
        self.active_tab.min(self.tabs.len().saturating_sub(1))
    }

    /// The design the showing tab is on, or `None` when the blank tab is showing.
    pub fn active_design(&self) -> Option<&Path> {
        self.tabs.get(self.active_tab()).and_then(|tab| tab.as_deref())
    }

    /// Show a design in the pane's tabs: the tab already on it, the blank tab when that is showing, or a new tab
    /// after the one showing.
    ///
    /// @param design - the design, relative to the project
    pub fn open_in_a_tab(&mut self, design: &Path) {
        if let Some(at) = self.tabs.iter().position(|tab| tab.as_deref() == Some(design)) {
            self.active_tab = at;
        } else if self.active_design().is_none() && !self.tabs.is_empty() {
            let at = self.active_tab();
            self.tabs[at] = Some(design.to_path_buf());
        } else {
            let at = (self.active_tab() + 1).min(self.tabs.len());
            self.tabs.insert(at, Some(design.to_path_buf()));
            self.active_tab = at;
        }
        self.remember_tabs();
    }

    /// Add a blank tab after the one showing and show it.
    pub fn new_tab(&mut self) {
        let at = (self.active_tab() + 1).min(self.tabs.len());
        self.tabs.insert(at, None);
        self.active_tab = at;
        self.remember_tabs();
    }

    /// Show the tab at `index`.
    ///
    /// @param index - the tab, counted from 0 at the left
    pub fn show_tab(&mut self, index: usize) -> Result<(), String> {
        if index >= self.tabs.len() {
            return Err(format!("there is no tab {index}: the Designer has {} open", self.tabs.len()));
        }
        self.active_tab = index;
        self.remember_tabs();
        Ok(())
    }

    /// Close the tab at `index`, and let go of what was known about its design. Closing the last tab leaves a
    /// blank one. Closing the tab showing shows the one that moves into its place, or the one to its left when it
    /// was the last, as in a browser.
    ///
    /// @param index - the tab, counted from 0 at the left
    pub fn close_tab(&mut self, index: usize) -> Result<(), String> {
        if index >= self.tabs.len() {
            return Err(format!("there is no tab {index}: the Designer has {} open", self.tabs.len()));
        }
        let active = self.active_tab();
        let closed = self.tabs.remove(index);
        if let Some(design) = closed {
            self.forget(&design);
        }
        if self.tabs.is_empty() {
            self.tabs.push(None);
        }
        self.active_tab = match index.cmp(&active) {
            std::cmp::Ordering::Less => active - 1,
            std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => active,
        }
        .min(self.tabs.len() - 1);
        self.remember_tabs();
        Ok(())
    }

    /// The tab a command names: a number counted from 0, or the name of a design open in one.
    ///
    /// @param named - what the command said
    pub fn tab_named(&self, named: &str) -> Result<usize, String> {
        let named = named.trim();
        if let Ok(index) = named.parse::<usize>() {
            return Ok(index);
        }
        self.tabs
            .iter()
            .position(|tab| {
                tab.as_deref().is_some_and(|design| {
                    let name = self.name_of(design);
                    name == named || name.trim_end_matches(".html") == named
                })
            })
            .ok_or_else(|| format!("no tab is open on `{named}`"))
    }

    /// What a tab is called on its strip: the design's name, or `New tab`.
    ///
    /// @param tab - a tab
    pub fn tab_title(&self, tab: &Option<PathBuf>) -> String {
        match tab {
            Some(design) => design
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.name_of(design)),
            None => "New tab".to_owned(),
        }
    }

    /// Read the tabs the project remembers, keeping only the designs whose file is still there.
    pub(super) fn read_tabs(&mut self) {
        let Some(project) = self.project.clone() else { return };
        if self.tabs_read_for.as_deref() == Some(project.as_path()) {
            return;
        }
        self.tabs_read_for = Some(project.clone());
        let values = self
            .device_file()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| Values::parse(&text))
            .unwrap_or_default();
        let (tabs, active) = tabs_from(&values, |design| project.join(design).is_file());
        self.tabs = tabs;
        self.active_tab = active;
    }

    /// Write the tabs down in the project's `.unluminous/designs.conf`.
    pub(super) fn remember_tabs(&self) {
        let Some(file) = self.device_file() else { return };
        let mut values = Values::parse(&std::fs::read_to_string(&file).unwrap_or_default());
        write_tabs(&mut values, &self.tabs, self.active_tab());
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = crate::services::store::write_atomically(
            &file,
            values
                .to_text_headed("# What one person chose in the Designer, in this project.")
                .as_bytes(),
        );
    }
}

/// The tabs written in `values`, keeping the designs `exists` says are there. Never empty.
///
/// @param values - the project's `designs.conf`
/// @param exists - whether a design's file is still on the disk
pub(super) fn tabs_from(
    values: &Values,
    exists: impl Fn(&Path) -> bool,
) -> (Vec<Option<PathBuf>>, usize) {
    let count = values.number("tabs.count").map(|count| count.max(0.0) as usize).unwrap_or(0);
    let wanted = values.number("tabs.active").map(|active| active.max(0.0) as usize).unwrap_or(0);
    let mut tabs = Vec::new();
    let mut active = 0;
    for index in 0..count {
        let Some(written) = values.text(&format!("tabs.{index}")) else { continue };
        let tab = match written {
            BLANK => None,
            design => Some(PathBuf::from(design)),
        };
        if tab.as_deref().is_some_and(|design| !exists(design)) {
            continue;
        }
        if index == wanted {
            active = tabs.len();
        }
        tabs.push(tab);
    }
    if tabs.is_empty() {
        tabs.push(None);
    }
    let last = tabs.len() - 1;
    (tabs, active.min(last))
}

/// Write the tabs into `values`, taking out any left over from a longer list.
///
/// @param values - the project's `designs.conf`
/// @param tabs - the tabs
/// @param active - the one showing
pub(super) fn write_tabs(values: &mut Values, tabs: &[Option<PathBuf>], active: usize) {
    for (name, _) in values.starting_with("tabs.") {
        values.remove(&format!("tabs.{name}"));
    }
    values.set("tabs.count", tabs.len().to_string());
    values.set("tabs.active", active.to_string());
    for (index, tab) in tabs.iter().enumerate() {
        let written = match tab {
            Some(design) => slashed(design),
            None => BLANK.to_owned(),
        };
        values.set(&format!("tabs.{index}"), written);
    }
}
