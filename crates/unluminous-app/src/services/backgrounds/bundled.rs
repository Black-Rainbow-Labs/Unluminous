//! The pictures that ship inside the binary, and the one a fresh Unluminous is drawn on.
//!
//! `services::plugins::bundled`'s argument, made about backgrounds: they are in the binary so that an
//! Unluminous somebody has just installed has something to choose from with no network involved and
//! nothing to find on their disk. The grid reads the backgrounds folder and nothing else, so what
//! "shipping a background" means is **writing these into that folder the first time the folder is
//! made** — after which they are ordinary files a person can remove like any other.
//!
//! **They are JPEG rather than PNG**, which is the one thing here worth measuring rather than assuming.
//! The five pictures are 1920 by 1088 photographs and abstract art; as PNG they are **13 MB**, which on
//! a 33 MB binary is a 39% increase to carry pictures nobody may use. At JPEG quality 82 they are
//! **2.3 MB**, and a background is drawn at 86% opacity behind a window full of text — the one place a
//! compression artefact cannot be seen. `design/background-images/` keeps the PNG originals, which is
//! where a better encoding would be made from.
//!
//! **Nothing is fetched**, which is the rule the whole of this module keeps and the Markdown preview
//! keeps: a picture is read from the binary or from a disk somebody pointed at, and from nowhere else.

/// Each entry is the file name it is written as and its bytes.
///
/// The order is the order the grid shows them in, which is what `list` sorts by name — so these are
/// named for what they are rather than numbered, and a person reading the grid sees the names.
pub const ALL: &[(&str, &[u8])] = &[
    ("abstract-1.jpg", include_bytes!("../../../backgrounds/abstract-1.jpg")),
    ("desert-1.jpg", include_bytes!("../../../backgrounds/desert-1.jpg")),
    ("forest-1.jpg", include_bytes!("../../../backgrounds/forest-1.jpg")),
    ("moab.jpg", include_bytes!("../../../backgrounds/moab.jpg")),
    ("mountain-aurora.jpg", include_bytes!("../../../backgrounds/mountain-aurora.jpg")),
];

/// The picture a fresh Unluminous is drawn on, and the opacity it is drawn at.
///
/// **A default that is a *name* rather than the empty string**, which is the one thing about this that
/// needed care: an empty `appearance.background.image` means *"let the desktop show through"* and is a
/// first class choice on the grid. So the default cannot simply be "no picture" — see
/// `Settings::background_image` for the pair of settings this needs and why a fresh install and somebody
/// who chose the desktop have to stay tellable apart.
pub const DEFAULT: &str = "forest-1.jpg";

/// How opaque the window is over [`DEFAULT`].
///
/// 86%, which is what was asked for. It is a little more opaque than [`crate::settings::DEFAULT_OPACITY`]
/// — the 83% a window with the desktop behind it uses — because a photograph carries more detail than a
/// desktop does and the text has to stay the thing being read.
pub const DEFAULT_OPACITY: f32 = 0.86;

/// Write the bundled pictures into `folder`, and answer with the names that were put there.
///
/// **Only the ones that are not already there**, so this is safe to call on every start: a person who
/// removed `moab.jpg` does not get it back, which is the whole point of the grid's remove button, and one
/// who has never opened the grid gets all five. A picture that cannot be written is skipped rather than
/// refused — an Unluminous with four backgrounds is better than one that will not start, which is
/// `plugins::Plugins::load`'s own rule about a manifest it cannot read.
///
/// **A picture already there under a different extension counts as there**, which is why the test is the
/// file's *stem* rather than its whole name. Somebody who put `forest-1.png` in the folder by hand has
/// that picture, and writing `forest-1.jpg` beside it would show them the same photograph twice in a grid
/// whose cells they cannot tell apart. Measured on the machine this was built for, which had four of the
/// five already in it as PNG.
pub fn write_into(folder: &std::path::Path) -> Vec<String> {
    let mut written = Vec::new();
    if std::fs::create_dir_all(folder).is_err() {
        return written;
    }
    let already: Vec<String> =
        super::list_in(folder).iter().map(|name| stem_of(name).to_lowercase()).collect();
    for (name, bytes) in ALL {
        let path = folder.join(name);
        if path.exists() || already.contains(&stem_of(name).to_lowercase()) {
            continue;
        }
        // Through `write_atomically`, which is the rule every file this application persists keeps: a
        // half written picture is one the grid would draw as an empty cell for ever, and it is read at
        // startup. `services::store`'s own test walks this crate and fails on a bare `fs::write`.
        if crate::services::store::write_atomically(&path, bytes).is_ok() {
            written.push((*name).to_owned());
        }
    }
    written
}

/// A file name without its extension, which is what one bundled picture is recognised by.
fn stem_of(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// The name of the bundled picture a fresh Unluminous should be drawn on, as it is in `folder`.
///
/// [`DEFAULT`] is the file this binary would write; what this answers is the file that is really there,
/// which is not the same thing when the folder already holds that picture under another extension. On the
/// machine this was built for `forest-1.png` was already in it, so naming `forest-1.jpg` would have been a
/// setting pointing at a file `write_into` deliberately did not write — and a name that is not there falls
/// back to the desktop, so a fresh Unluminous would have come up on no picture at all.
pub fn default_in(folder: &std::path::Path) -> String {
    let wanted = stem_of(DEFAULT).to_lowercase();
    super::list_in(folder)
        .into_iter()
        .find(|name| stem_of(name).to_lowercase() == wanted)
        .unwrap_or_else(|| DEFAULT.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join("unluminous-bundled-backgrounds").join(name);
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    /// A one pixel PNG, so a test can put a picture somewhere `list_in` will count.
    fn a_png(at: &std::path::Path) {
        let bytes: [u8; 67] = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(at, bytes).expect("write the picture");
    }

    /// The five are written into a folder that is not there yet, which is a fresh install.
    #[test]
    fn a_fresh_folder_is_given_every_bundled_picture() {
        let folder = temporary("fresh");
        let written = write_into(&folder);
        assert_eq!(written.len(), ALL.len(), "{written:?}");
        assert_eq!(super::super::list_in(&folder).len(), ALL.len());
        // And the default is one of them, or a fresh Unluminous would name a picture it has not got.
        assert!(ALL.iter().any(|(name, _)| *name == DEFAULT), "{DEFAULT} is not bundled");
    }

    /// **One that was removed stays removed**, which is what makes this safe to run on every start.
    #[test]
    fn a_picture_somebody_removed_is_not_put_back() {
        let folder = temporary("removed");
        write_into(&folder);
        std::fs::remove_file(folder.join("moab.jpg")).expect("remove one");
        let again = write_into(&folder);
        assert_eq!(again, vec!["moab.jpg"], "only the missing one is written");

        // Removed a second time and asked again: it comes back, because this function's promise is
        // about a folder rather than about a person's history. The *caller* is what only asks once --
        // see `UnluminousApp::use_store`.
        std::fs::remove_file(folder.join("moab.jpg")).expect("remove it again");
        assert_eq!(write_into(&folder).len(), 1);
    }

    /// **The same picture under another extension is not written a second time.** Measured on the machine
    /// this was built for, which already held four of the five as PNG: without this, the grid showed every
    /// one of those photographs twice in cells nobody could tell apart.
    #[test]
    fn a_picture_already_there_as_a_png_is_not_written_again_as_a_jpeg() {
        let folder = temporary("already-a-png");
        std::fs::create_dir_all(&folder).expect("make the folder");
        // A real PNG, because `list_in` only counts files it would show.
        a_png(&folder.join("forest-1.png"));

        let written = write_into(&folder);
        assert!(!written.contains(&"forest-1.jpg".to_owned()), "{written:?}");
        assert_eq!(written.len(), ALL.len() - 1, "the other four are still written");
        assert!(!folder.join("forest-1.jpg").exists(), "the photograph is in the grid twice");

        // And the default names the copy that is really there, or it would name a missing file — which
        // falls back to the desktop, so a fresh Unluminous would come up on no picture at all.
        assert_eq!(default_in(&folder), "forest-1.png");
    }

    /// With none of them there, the default is the name this binary writes.
    #[test]
    fn the_default_is_the_bundled_name_when_nothing_was_there_first() {
        let folder = temporary("default-plain");
        write_into(&folder);
        assert_eq!(default_in(&folder), DEFAULT);
    }

    /// Nothing is written twice, so a second start costs five `exists` calls and no bytes.
    #[test]
    fn a_folder_that_already_has_them_is_left_alone() {
        let folder = temporary("again");
        write_into(&folder);
        assert!(write_into(&folder).is_empty(), "something was written a second time");
    }

    /// Every bundled picture really is a picture this application can read, by the same test the grid
    /// filters the folder with. A file bundled under a name `is_a_picture` refuses would be written and
    /// then never shown.
    #[test]
    fn every_bundled_picture_is_one_the_grid_will_show() {
        for (name, bytes) in ALL {
            assert!(
                super::super::is_a_picture(std::path::Path::new(name)),
                "{name} is not a name the grid lists"
            );
            assert!(!bytes.is_empty(), "{name} is empty");
            // The first two bytes of a JPEG. Checked because `include_bytes!` will happily bake in
            // whatever is at that path, and a file that is not a picture would only show up as an
            // empty cell in the grid.
            assert_eq!(&bytes[..2], &[0xFF, 0xD8], "{name} is not a JPEG");
        }
    }
}
