//! What the Designer plugin remembers about how it is set up, written in the plugin's own folder.
//!
//! The same `services::store::Values` format every settings file uses, in `settings.conf` inside the
//! plugin's folder, which is where `services::plugin_settings` reaches any plugin's configuration. So
//! `settings get plugins.designer.folder` and `settings set plugins.designer.device phone` work with no
//! code in this file for them.
//!
//! Five keys: `folder`, `device`, `starter`, `send_html` and `history_kept`. A key that is missing means
//! the default, and a key that is wrong is refused with a sentence and the default is used, which is the
//! rule `services::database::config` keeps for a row it cannot read.

use std::path::{Component, Path, PathBuf};

use crate::services::plugin_ui;
use crate::services::store::Values;

/// Where the designs are kept unless the Settings page says otherwise.
pub const DEFAULT_FOLDER: &str = ".designs";

/// How many copies of a design are kept before the oldest are removed.
pub const DEFAULT_HISTORY_KEPT: usize = 20;

/// The most copies a person can ask to keep.
pub const MOST_HISTORY_KEPT: usize = 200;

/// The width a page is laid out at, which is what a design looks like on a screen of that kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    Desktop,
    Laptop,
    Tablet,
    Phone,
    /// The width of the tab the design is shown in.
    #[default]
    Fit,
}

impl Device {
    /// Every device, in the order the toolbar lists them.
    pub const ALL: [Device; 5] =
        [Device::Desktop, Device::Laptop, Device::Tablet, Device::Phone, Device::Fit];

    /// The word the settings file, the command line and the toolbar's switch use.
    pub fn name(self) -> &'static str {
        match self {
            Device::Desktop => "desktop",
            Device::Laptop => "laptop",
            Device::Tablet => "tablet",
            Device::Phone => "phone",
            Device::Fit => "fit",
        }
    }

    /// The name with a capital, for a label.
    pub fn title(self) -> &'static str {
        match self {
            Device::Desktop => "Desktop",
            Device::Laptop => "Laptop",
            Device::Tablet => "Tablet",
            Device::Phone => "Phone",
            Device::Fit => "Fit",
        }
    }

    /// The device a name stands for, ignoring case.
    pub fn from_name(name: &str) -> Option<Device> {
        let wanted = name.trim().to_lowercase();
        Device::ALL.into_iter().find(|device| device.name() == wanted)
    }

    /// The width in points the page is laid out at, or nothing for the width of the tab.
    pub fn width(self) -> Option<f32> {
        match self {
            Device::Desktop => Some(1440.0),
            Device::Laptop => Some(1280.0),
            Device::Tablet => Some(834.0),
            Device::Phone => Some(390.0),
            Device::Fit => None,
        }
    }
}

/// Which page a new design starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Starter {
    /// The landing page in the house style.
    #[default]
    House,
    /// An empty page.
    Blank,
}

impl Starter {
    pub const ALL: [Starter; 2] = [Starter::House, Starter::Blank];

    /// The word the settings file, the command line and the Settings page use.
    pub fn name(self) -> &'static str {
        match self {
            Starter::House => "house",
            Starter::Blank => "blank",
        }
    }

    /// The name as a person reads it, with a capital.
    pub fn title(self) -> &'static str {
        match self {
            Starter::House => "House style",
            Starter::Blank => "Blank",
        }
    }

    /// What a typed name stands for, ignoring case, or nothing when it names none of them.
    pub fn from_name(name: &str) -> Option<Starter> {
        let wanted = name.trim().to_lowercase();
        Starter::ALL.into_iter().find(|starter| starter.name() == wanted)
    }
}

/// Everything the plugin remembers.
#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    /// The designs folder, relative to the project.
    pub folder: PathBuf,
    /// The device a design opens at.
    pub device: Device,
    /// The page `new` writes.
    pub starter: Starter,
    /// Whether a request carries the start of the element's HTML.
    pub send_html: bool,
    /// How many copies of a design are kept.
    pub history_kept: usize,
}

impl Default for Configuration {
    /// The defaults the PRD names: `.designs`, fit, the house starter, the markup sent, twenty copies.
    fn default() -> Self {
        Self {
            folder: PathBuf::from(DEFAULT_FOLDER),
            device: Device::default(),
            starter: Starter::default(),
            send_html: true,
            history_kept: DEFAULT_HISTORY_KEPT,
        }
    }
}

impl Configuration {
    /// The file, inside the plugin's folder.
    pub const FILE: &'static str = crate::services::plugin_settings::FILE;

    /// Read it from the plugin's folder, and say what was refused.
    ///
    /// @param folder - the plugin's own folder under the settings folder
    pub fn read(folder: &Path) -> (Self, Vec<String>) {
        let text = std::fs::read_to_string(folder.join(Self::FILE)).unwrap_or_default();
        Self::of(&Values::parse(&text))
    }

    /// Read it from values already parsed.
    ///
    /// @param values - the plugin's `settings.conf`
    pub fn of(values: &Values) -> (Self, Vec<String>) {
        let mut out = Configuration::default();
        let mut refused = Vec::new();
        if let Some(wanted) = values.text("folder") {
            match checked_folder(wanted) {
                Ok(folder) => out.folder = folder,
                Err(why) => refused.push(why),
            }
        }
        if let Some(wanted) = values.text("device") {
            match Device::from_name(wanted) {
                Some(device) => out.device = device,
                None => refused.push(unknown("device", wanted, &Device::ALL.map(Device::name))),
            }
        }
        if let Some(wanted) = values.text("starter") {
            match Starter::from_name(wanted) {
                Some(starter) => out.starter = starter,
                None => refused.push(unknown("starter", wanted, &Starter::ALL.map(Starter::name))),
            }
        }
        if let Some(flag) = values.flag("send_html") {
            out.send_html = flag;
        }
        if let Some(kept) = values.number("history_kept") {
            out.history_kept = (kept as usize).clamp(1, MOST_HISTORY_KEPT);
        }
        (out, refused)
    }

    /// Write it to the plugin's folder, keeping nothing but the five keys.
    ///
    /// @param folder - the plugin's own folder under the settings folder
    pub fn write(&self, folder: &Path) -> Result<(), String> {
        let existing = std::fs::read_to_string(folder.join(Self::FILE)).unwrap_or_default();
        let mut values = Values::parse(&existing);
        values.set("folder", slashed(&self.folder));
        values.set("device", self.device.name());
        values.set("starter", self.starter.name());
        values.set("send_html", self.send_html.to_string());
        values.set("history_kept", self.history_kept.to_string());
        plugin_ui::write_values(
            folder,
            Self::FILE,
            &values,
            "How the Designer plugin is set up.\nWritten by Unluminous; safe to edit by hand.",
        )
    }
}

/// A sentence for a word that names none of the choices there are.
fn unknown(key: &str, wanted: &str, choices: &[&str]) -> String {
    format!("{key} is `{wanted}`, and the Designer has {}.", choices.join(", "))
}

/// A designs folder, if what was written is a path inside the project.
///
/// An absolute path or one that climbs out with `..` is refused, because a design is part of the project
/// and the files beside it are committed with it.
///
/// @param wanted - the folder as it was typed
pub fn checked_folder(wanted: &str) -> Result<PathBuf, String> {
    let trimmed = wanted.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return Err("The designs folder is empty, so the default is used.".to_owned());
    }
    let path = Path::new(trimmed);
    let climbs = path.components().any(|part| matches!(part, Component::ParentDir));
    if path.is_absolute() || trimmed.starts_with('/') || trimmed.starts_with('\\') || climbs {
        return Err(format!(
            "The designs folder `{trimmed}` has to be a folder inside the project, written \
             relative to it, so `.designs` is used."
        ));
    }
    Ok(path.components().collect())
}

/// A path written with `/` whatever the platform, which is how a file in the project is named.
///
/// @param path - a path relative to the project
pub fn slashed(path: &Path) -> String {
    path.components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<String>>()
        .join("/")
}
