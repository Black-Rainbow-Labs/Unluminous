//! The pictures the window can be drawn on, and the folder Unluminous keeps them in.
//!
//! `task-2004` asks for a Background Selector beside `appearance.background.opacity`: a grid whose
//! first option is the current behaviour — the desktop showing through — and whose other options are
//! pictures chosen from disk, *"copied over to the app's appropriate content folder so it's
//! retained"*, each with a way to remove it, applied at once.
//!
//! **The folder is the whole of the state.** What the grid shows is what is in
//! `<the person's settings folder>/backgrounds/`, and `appearance.background.image` is a file name in
//! it. There is no list written down beside the files, so a picture somebody drops in by hand appears
//! and one they delete by hand goes, and nothing can get out of step with the disk.
//!
//! **A picture is copied in rather than pointed at.** A setting holding a path into somebody's Pictures
//! folder stops working the day they tidy it up, and it makes one person's settings file depend on
//! another person's disk — which is the same reason `services::file_marks` writes paths relative to the
//! project. The copy is named after the file it came from with a number added when that name is taken.
//!
//! **Removing one deletes the copy.** That is a file this application made, in a folder this
//! application owns, which is the one kind of deletion it does.
//!
//! **Nothing is fetched.** A picture is read from the disk somebody pointed at, or out of the binary
//! itself, and from nowhere else — which is the rule the Markdown preview already keeps.
//!
//! **Five pictures ship inside the binary** and are written into the folder the first time it is made,
//! so a fresh Unluminous has something to choose from rather than an empty grid and a file dialog.
//! [`bundled`] is that list and the reasoning; once written they are ordinary files, so removing one
//! removes it for good.

pub mod bundled;

use std::path::{Path, PathBuf};

/// The folder the pictures live in, made when something is first put in it.
///
/// Beside `settings.conf` and `recent.txt` in the person's own settings folder, because a background is
/// a choice about their window rather than about a project — the line `task-1697` drew for where the
/// panels are, and the same one `appearance.theme` keeps.
pub fn folder() -> PathBuf {
    folder_in(&crate::services::store::folder_for_this_person())
}

/// The backgrounds folder inside a named settings folder.
///
/// **What a test uses, and what `UnluminousApp::use_store` uses.** The window is handed a `Store` — which
/// a test points at a folder of its own — so asking [`folder`] there would write the bundled pictures
/// into the settings of whoever is running the tests. That is the rule the whole suite keeps: a test must
/// not read or write the settings of the person running it.
pub fn folder_in(settings: &Path) -> PathBuf {
    settings.join("backgrounds")
}

/// The extensions a picture may have, which is what `services::picture` can decode.
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];

/// Whether this file is one of them, by its extension.
pub fn is_a_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_lowercase())
        .is_some_and(|extension| PICTURES.contains(&extension.as_str()))
}

/// The pictures in the folder, by name, in the order a person reads them.
///
/// Sorted by name rather than by when they were added: a grid whose cells move about between openings
/// is a grid nobody can point at twice. A folder that is not there is no pictures, which is what a
/// fresh Unluminous has and is not a fault.
pub fn list() -> Vec<String> {
    list_in(&folder())
}

/// The same, in a named folder, so a test needs no settings folder of its own.
pub fn list_in(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_a_picture(path))
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// Where a picture of this name is, whether or not it is there.
pub fn path_of(name: &str) -> PathBuf {
    folder().join(name)
}

/// Copy a picture into the folder and answer with the name it was given.
///
/// The name is the file's own, with ` (2)`, ` (3)` and so on added while that name is taken — which is
/// what every file manager does and is the one shape nobody has to be told about. A file that is
/// already *in* the folder is left where it is and answered with its own name, so choosing one from the
/// grid's own folder does not make a second copy of it.
pub fn add(source: &Path) -> Result<String, String> {
    add_into(&folder(), source)
}

/// The same, into a named folder.
pub fn add_into(folder: &Path, source: &Path) -> Result<String, String> {
    if !source.is_file() {
        return Err(format!("There is no file at {}", source.display()));
    }
    if !is_a_picture(source) {
        return Err(format!(
            "{} is not a picture Unluminous can read. It reads {}.",
            source.display(),
            PICTURES.join(", ")
        ));
    }
    if source.parent() == Some(folder) {
        return Ok(source.file_name().unwrap_or_default().to_string_lossy().into_owned());
    }
    std::fs::create_dir_all(folder)
        .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    let name = free_name(folder, source);
    std::fs::copy(source, folder.join(&name))
        .map_err(|problem| format!("{} could not be copied: {problem}", source.display()))?;
    Ok(name)
}

/// The name a copy of `source` gets in `folder`, counting past whatever is already there.
fn free_name(folder: &Path, source: &Path) -> String {
    let stem = source.file_stem().map(|stem| stem.to_string_lossy().into_owned());
    let stem = stem.filter(|stem| !stem.is_empty()).unwrap_or_else(|| "background".to_owned());
    let extension = source
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let first = format!("{stem}{extension}");
    if !folder.join(&first).exists() {
        return first;
    }
    for number in 2..1000 {
        let tried = format!("{stem} ({number}){extension}");
        if !folder.join(&tried).exists() {
            return tried;
        }
    }
    first
}

/// Delete one of the copies.
///
/// **A name rather than a path**, and it is joined onto the folder here, so nothing outside the folder
/// can be reached by asking for one: a name with a separator in it, or one that climbs, is refused
/// before anything is opened. That is the same rule `services::browser`'s local root keeps about a page
/// asking for a file.
pub fn remove(name: &str) -> Result<(), String> {
    remove_from(&folder(), name)
}

/// The same, in a named folder.
pub fn remove_from(folder: &Path, name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!("{name} is not the name of a background."));
    }
    let path = folder.join(name);
    if !path.is_file() {
        return Err(format!("There is no background called {name}."));
    }
    std::fs::remove_file(&path)
        .map_err(|problem| format!("{} could not be removed: {problem}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join("unluminous-backgrounds").join(name);
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("make the folder");
        folder
    }

    /// A one pixel PNG, so the tests copy something a decoder would accept.
    fn a_picture(at: &Path) {
        let bytes: [u8; 67] = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(at, bytes).expect("write the picture");
    }

    #[test]
    fn a_picture_is_copied_in_and_the_second_of_a_name_is_numbered() {
        let holding = temporary("copy-holding");
        let from = temporary("copy-from");
        a_picture(&from.join("wallpaper.png"));
        let first = add_into(&holding, &from.join("wallpaper.png")).expect("copied");
        assert_eq!(first, "wallpaper.png");
        let second = add_into(&holding, &from.join("wallpaper.png")).expect("copied again");
        assert_eq!(second, "wallpaper (2).png", "the second of a name counts up");
        assert_eq!(list_in(&holding), vec!["wallpaper (2).png", "wallpaper.png"]);
    }

    #[test]
    fn a_picture_already_in_the_folder_is_not_copied_again() {
        let holding = temporary("copy-inside");
        a_picture(&holding.join("already.png"));
        let name = add_into(&holding, &holding.join("already.png")).expect("answered");
        assert_eq!(name, "already.png");
        assert_eq!(list_in(&holding).len(), 1, "and there is still only one of it");
    }

    #[test]
    fn a_file_that_is_not_a_picture_and_one_that_is_not_there_are_both_refused() {
        let holding = temporary("copy-refusals");
        let from = temporary("copy-refusals-from");
        std::fs::write(from.join("notes.txt"), "hello").expect("write");
        assert!(add_into(&holding, &from.join("notes.txt")).is_err());
        assert!(add_into(&holding, &from.join("nothing-here.png")).is_err());
        assert!(list_in(&holding).is_empty());
    }

    #[test]
    fn only_a_name_can_be_removed_and_only_one_that_is_there() {
        let holding = temporary("remove");
        a_picture(&holding.join("one.png"));
        assert!(remove_from(&holding, "two.png").is_err(), "one that is not there");
        assert!(remove_from(&holding, "../one.png").is_err(), "a name that climbs out");
        assert!(remove_from(&holding, "a/one.png").is_err(), "and one with a separator in it");
        assert!(remove_from(&holding, "one.png").is_ok());
        assert!(list_in(&holding).is_empty());
    }

    #[test]
    fn a_folder_that_is_not_there_holds_no_pictures_and_that_is_not_a_fault() {
        let missing = std::env::temp_dir().join("unluminous-backgrounds").join("never-made");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(list_in(&missing).is_empty());
    }

    #[test]
    fn only_the_extensions_a_decoder_reads_are_offered() {
        assert!(is_a_picture(Path::new("a.PNG")), "the extension is read whatever its case");
        assert!(is_a_picture(Path::new("a.jpeg")));
        assert!(!is_a_picture(Path::new("a.svg")), "which nothing here decodes");
        assert!(!is_a_picture(Path::new("a")));
    }
}

/// The picture the window is drawn on, decoded once and kept.
///
/// **Keyed on the name and the file's own modified time**, so a picture replaced on disk is read again
/// and one nobody has touched costs a `metadata` call — and that call is made at most every
/// [`Wallpaper::CHECK`] rather than once a frame, which is `task-1805`'s rule about a number on the
/// screen not being a reason to ask the disk.
#[derive(Default)]
pub struct Wallpaper {
    name: String,
    stamp: Option<std::time::SystemTime>,
    texture: Option<egui::TextureHandle>,
    looked: Option<std::time::Instant>,
}

impl Wallpaper {
    /// How often the file behind the picture is asked whether it has changed.
    const CHECK: std::time::Duration = std::time::Duration::from_millis(1500);

    /// The texture for `name`, decoding it when the name or the file has changed.
    ///
    /// An empty name is no picture, which is the setting saying the desktop shows through. A name that
    /// is not there, or a file that will not decode, is also no picture — a window drawn on nothing at
    /// all would be worse than the desktop, and it is what a picture somebody deleted by hand should do.
    pub fn texture(&mut self, ctx: &egui::Context, name: &str) -> Option<egui::TextureHandle> {
        if name.trim().is_empty() {
            self.forget();
            return None;
        }
        let due = self.looked.is_none_or(|at| at.elapsed() >= Self::CHECK);
        if name == self.name && !due {
            return self.texture.clone();
        }
        let path = path_of(name);
        let stamp = std::fs::metadata(&path).ok().and_then(|found| found.modified().ok());
        self.looked = Some(std::time::Instant::now());
        if name == self.name && stamp == self.stamp {
            return self.texture.clone();
        }
        self.name = name.to_owned();
        self.stamp = stamp;
        self.texture = crate::services::picture::decode(&path).ok().map(|image| {
            crate::services::picture::upload(
                ctx,
                format!("unluminous-background-{name}"),
                image,
                // Linear, because a wallpaper is almost always drawn at a size other than its own and
                // nearest neighbour would make every edge in it ragged.
                egui::TextureOptions::LINEAR,
            )
        });
        self.texture.clone()
    }

    /// Drop what is held, which is what choosing the desktop means.
    fn forget(&mut self) {
        self.name.clear();
        self.stamp = None;
        self.texture = None;
        self.looked = None;
    }
}

/// The part of a picture that fills `area` without distorting it, as texture coordinates.
///
/// **Scaled to cover rather than to fit**, which is what a desktop does with a wallpaper: a picture
/// that fitted would leave bands of nothing down two sides of the window, and the window has no colour
/// to put there — it is drawn on a transparent ground. What is cut is taken evenly off both sides, so
/// the middle of the picture is the middle of the window.
pub fn cover(picture: egui::Vec2, area: egui::Vec2) -> egui::Rect {
    if picture.x <= 0.0 || picture.y <= 0.0 || area.x <= 0.0 || area.y <= 0.0 {
        return egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    }
    let wanted = area.x / area.y;
    let has = picture.x / picture.y;
    let (across, down) = match has > wanted {
        // Wider than the window, so the sides are cut.
        true => (wanted / has, 1.0),
        false => (1.0, has / wanted),
    };
    egui::Rect::from_min_size(
        egui::pos2((1.0 - across) / 2.0, (1.0 - down) / 2.0),
        egui::vec2(across, down),
    )
}

#[cfg(test)]
mod covering {
    use super::*;

    #[test]
    fn a_picture_wider_than_the_window_is_cut_down_its_sides_and_evenly() {
        let taken = cover(egui::vec2(4000.0, 1000.0), egui::vec2(1000.0, 1000.0));
        assert!((taken.width() - 0.25).abs() < 0.001, "a quarter of its width fills a square");
        assert_eq!(taken.height(), 1.0, "and all of its height");
        assert!((taken.center().x - 0.5).abs() < 0.001, "the middle of the picture is the middle");
    }

    #[test]
    fn a_picture_taller_than_the_window_is_cut_top_and_bottom() {
        let taken = cover(egui::vec2(1000.0, 4000.0), egui::vec2(1000.0, 1000.0));
        assert_eq!(taken.width(), 1.0);
        assert!((taken.height() - 0.25).abs() < 0.001);
        assert!((taken.center().y - 0.5).abs() < 0.001);
    }

    #[test]
    fn a_picture_the_shape_of_the_window_is_not_cut_at_all() {
        let taken = cover(egui::vec2(1600.0, 900.0), egui::vec2(800.0, 450.0));
        assert!((taken.width() - 1.0).abs() < 0.001);
        assert!((taken.height() - 1.0).abs() < 0.001);
    }

    #[test]
    fn a_picture_with_no_size_asks_for_the_whole_of_itself_rather_than_dividing_by_nothing() {
        let taken = cover(egui::vec2(0.0, 0.0), egui::vec2(800.0, 600.0));
        assert_eq!(taken, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)));
    }
}
