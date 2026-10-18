//! The pictures that ship inside the binary, and the one a fresh Unluminous is drawn on.
//!
//! `services::plugins::bundled`'s argument, made about backgrounds: they are in the binary so that an
//! Unluminous somebody has just installed has something to choose from with no network involved and
//! nothing to find on their disk. The grid reads the backgrounds folder and nothing else, so what
//! "shipping a background" means is **writing these into that folder the first time the folder is
//! made** — after which they are ordinary files a person can remove like any other.
//!
//! **They are 3840 by 2160 JPEGs** (`task-2291`): nineteen photographs from Pexels, cropped to 16:9 from
//! their originals and saved at quality 82, 35 MB in all. They were 1920 by 1088 before, and on a 4K
//! screen a wallpaper that size is drawn at twice its own size and looks soft. A smaller screen does not
//! pay for the extra pixels at run time: `Wallpaper` shrinks a picture to the screen it covers when it is
//! chosen. `backgrounds/SOURCES.md` says where each one came from.
//!
//! **They are named by subject**, `abstract-`, `landscape-`, `nature-`, `space-` and `wood-`, because the
//! grid sorts by name, so each subject sits together in it.
//!
//! **Nothing is fetched**, which is the rule the whole of this module keeps and the Markdown preview
//! keeps: a picture is read from the binary or from a disk somebody pointed at, and from nowhere else.

/// Each entry is the file name it is written as and its bytes.
///
/// The order is the order the grid shows them in, which is what `list` sorts by name — so these are
/// named for what they are rather than numbered, and a person reading the grid sees the names.
pub const ALL: &[(&str, &[u8])] = &[
    (
        "abstract-paper-stripes.jpg",
        include_bytes!("../../../backgrounds/abstract-paper-stripes.jpg"),
    ),
    (
        "landscape-dolomites-autumn-peak.jpg",
        include_bytes!("../../../backgrounds/landscape-dolomites-autumn-peak.jpg"),
    ),
    (
        "landscape-dolomites-cliffs.jpg",
        include_bytes!("../../../backgrounds/landscape-dolomites-cliffs.jpg"),
    ),
    (
        "landscape-dolomites-lake.jpg",
        include_bytes!("../../../backgrounds/landscape-dolomites-lake.jpg"),
    ),
    ("landscape-green-hills.jpg", include_bytes!("../../../backgrounds/landscape-green-hills.jpg")),
    (
        "landscape-valley-light.jpg",
        include_bytes!("../../../backgrounds/landscape-valley-light.jpg"),
    ),
    ("nature-dark-foliage.jpg", include_bytes!("../../../backgrounds/nature-dark-foliage.jpg")),
    ("nature-dark-sand.jpg", include_bytes!("../../../backgrounds/nature-dark-sand.jpg")),
    ("nature-veined-leaves.jpg", include_bytes!("../../../backgrounds/nature-veined-leaves.jpg")),
    ("space-milky-way-amber.jpg", include_bytes!("../../../backgrounds/space-milky-way-amber.jpg")),
    ("space-milky-way-core.jpg", include_bytes!("../../../backgrounds/space-milky-way-core.jpg")),
    (
        "space-milky-way-diagonal.jpg",
        include_bytes!("../../../backgrounds/space-milky-way-diagonal.jpg"),
    ),
    ("space-milky-way-glow.jpg", include_bytes!("../../../backgrounds/space-milky-way-glow.jpg")),
    ("space-milky-way-teal.jpg", include_bytes!("../../../backgrounds/space-milky-way-teal.jpg")),
    (
        "space-milky-way-violet.jpg",
        include_bytes!("../../../backgrounds/space-milky-way-violet.jpg"),
    ),
    ("space-red-nebula.jpg", include_bytes!("../../../backgrounds/space-red-nebula.jpg")),
    ("wood-driftwood.jpg", include_bytes!("../../../backgrounds/wood-driftwood.jpg")),
    ("wood-rings.jpg", include_bytes!("../../../backgrounds/wood-rings.jpg")),
    ("wood-weathered.jpg", include_bytes!("../../../backgrounds/wood-weathered.jpg")),
];

/// The pictures earlier versions shipped, as the name each was written under, its length and its SHA-256.
///
/// `task-2291` took them out of the library. [`retire_from`] removes a copy that is still exactly what
/// an earlier Unluminous wrote, and nothing else: a file with the same name and different bytes is a
/// picture somebody put there.
const RETIRED: &[(&str, u64, &str)] = &[
    ("abstract-1.jpg", 199_056, "dd7a9118695c9430570ceb0d7d8fa65837cff24767adef4646030bfef8cffe7e"),
    ("desert-1.jpg", 316_569, "eabfbf63d223fdc0c3ccc669535bb76048c654b95bb7f4dd39d4866680fcfb8a"),
    ("forest-1.jpg", 590_133, "c035bfd087f04ea44dc19ba188c6dcd88dad5e289c058bf7bd6246e52383cbb5"),
    (
        "glass-spheres.jpg",
        348_035,
        "ca7005b0a24f5a368f5d6275557bffb5a3e3e3132d1670c7436ff6c3999fd4d3",
    ),
    ("moab.jpg", 680_895, "2914904ba50f2419844c12f7e555a3c94fe5cad767a13afa01b7d4acf3922d15"),
    (
        "mountain-aurora.jpg",
        578_381,
        "6043e3c6b0f5daf772e1442d714764d702eca1bf979e7f78bb91cd301f80e0fa",
    ),
];

/// The picture a fresh Unluminous is drawn on, and the opacity it is drawn at.
///
/// **A default that is a *name* rather than the empty string**, which is the one thing about this that
/// needed care: an empty `appearance.background.image` means *"let the desktop show through"* and is a
/// first class choice on the grid. So the default cannot simply be "no picture" — see
/// `Settings::background_image` for the pair of settings this needs and why a fresh install and somebody
/// who chose the desktop have to stay tellable apart.
pub const DEFAULT: &str = "landscape-dolomites-autumn-peak.jpg";

/// The picture the Unluminous Glass theme is shown over (`task-2281`): sharp and busy, because glass
/// bends what is behind it and a flat colour shows nothing bent. The paper stripes are straight coloured
/// lines, which is what shows a bend best.
pub const GLASS: &str = "abstract-paper-stripes.jpg";

/// How opaque the window is over [`DEFAULT`].
///
/// 86%, which is what was asked for. It is a little more opaque than [`crate::settings::DEFAULT_OPACITY`]
/// — the 83% a window with the desktop behind it uses — because a photograph carries more detail than a
/// desktop does and the text has to stay the thing being read.
pub const DEFAULT_OPACITY: f32 = 0.86;

/// Remove the pictures earlier versions shipped from `folder`, and answer with the names removed.
///
/// Only a file whose name, length and SHA-256 are exactly one an earlier Unluminous wrote, so a picture
/// somebody added by hand is never touched, whatever it is called. It is safe on every start: once they
/// are gone it costs one `metadata` call a name.
///
/// @param folder - the backgrounds folder
pub fn retire_from(folder: &std::path::Path) -> Vec<String> {
    use sha2::Digest;
    let mut removed = Vec::new();
    for (name, length, digest) in RETIRED {
        let path = folder.join(name);
        if std::fs::metadata(&path).map(|found| found.len()).ok() != Some(*length) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let found: String =
            sha2::Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect();
        if found == *digest && std::fs::remove_file(&path).is_ok() {
            removed.push((*name).to_owned());
        }
    }
    removed
}

/// Whether `name` is one of the pictures earlier versions shipped and [`retire_from`] removes.
pub fn is_retired(name: &str) -> bool {
    RETIRED.iter().any(|(retired, _, _)| *retired == name)
}

/// Write the bundled pictures into `folder`, and answer with the names that were put there.
///
/// **Only the ones that are not already there**, so this is safe to call on every start: a person who
/// removed `wood-rings.jpg` does not get it back, which is the whole point of the grid's remove button, and one
/// who has never opened the grid gets every one. A picture that cannot be written is skipped rather than
/// refused — an Unluminous with four backgrounds is better than one that will not start, which is
/// `plugins::Plugins::load`'s own rule about a manifest it cannot read.
///
/// **A picture already there under a different extension counts as there**, which is why the test is the
/// file's *stem* rather than its whole name. Somebody who put `wood-rings.png` in the folder by hand has
/// that picture, and writing `wood-rings.jpg` beside it would show them the same photograph twice in a grid
/// whose cells they cannot tell apart. Measured on the machine this was built for, which had four of the
/// first five already in it as PNG.
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

    /// They are all written into a folder that is not there yet, which is a fresh install.
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
        std::fs::remove_file(folder.join("wood-rings.jpg")).expect("remove one");
        let again = write_into(&folder);
        assert_eq!(again, vec!["wood-rings.jpg"], "only the missing one is written");

        // Removed a second time and asked again: it comes back, because this function's promise is
        // about a folder rather than about a person's history. The *caller* is what only asks once --
        // see `UnluminousApp::use_store`.
        std::fs::remove_file(folder.join("wood-rings.jpg")).expect("remove it again");
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
        a_png(&folder.join("landscape-dolomites-autumn-peak.png"));

        let written = write_into(&folder);
        assert!(
            !written.contains(&"landscape-dolomites-autumn-peak.jpg".to_owned()),
            "{written:?}"
        );
        assert_eq!(written.len(), ALL.len() - 1, "the others are still written");
        assert!(
            !folder.join("landscape-dolomites-autumn-peak.jpg").exists(),
            "the photograph is in the grid twice"
        );

        // And the default names the copy that is really there, or it would name a missing file — which
        // falls back to the desktop, so a fresh Unluminous would come up on no picture at all.
        assert_eq!(default_in(&folder), "landscape-dolomites-autumn-peak.png");
    }

    /// With none of them there, the default is the name this binary writes.
    #[test]
    fn the_default_is_the_bundled_name_when_nothing_was_there_first() {
        let folder = temporary("default-plain");
        write_into(&folder);
        assert_eq!(default_in(&folder), DEFAULT);
    }

    /// Nothing is written twice, so a second start costs one `exists` call a picture and no bytes.
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

    /// **A picture an earlier version shipped is removed only when it is byte for byte what that version
    /// wrote** (`task-2291`). One with the same name and other bytes is somebody's own, and stays.
    #[test]
    fn only_an_untouched_copy_of_a_retired_picture_is_removed() {
        let folder = temporary("retired");
        std::fs::create_dir_all(&folder).expect("make the folder");
        // Not the real bytes, so this one stays whatever it is called.
        a_png(&folder.join("moab.jpg"));
        a_png(&folder.join("my-own.png"));
        assert!(retire_from(&folder).is_empty());
        assert!(folder.join("moab.jpg").exists());
        assert!(folder.join("my-own.png").exists());
        assert!(is_retired("moab.jpg") && is_retired("glass-spheres.jpg"));
        assert!(!is_retired(DEFAULT) && !is_retired(GLASS));
    }

    /// Every entry in the retired list names a SHA-256 and none of them is still shipped.
    #[test]
    fn no_retired_picture_is_still_shipped() {
        for (name, length, digest) in RETIRED {
            assert_eq!(digest.len(), 64, "{name}");
            assert!(*length > 0, "{name}");
            assert!(
                ALL.iter().all(|(shipped, _)| shipped != name),
                "{name} is shipped and retired"
            );
        }
    }
}
