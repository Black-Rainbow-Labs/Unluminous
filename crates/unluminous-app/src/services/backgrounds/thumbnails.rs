//! Small copies of the background pictures, for the Background grid.
//!
//! `task-2293`: *"when I open the images modal for background, it loads really slowly. We should use
//! thumbnails ... generate thumbnails for what we have, and generate thumbnail when new image is imported."*
//! The grid decoded every picture in full on the window's thread each time it opened, nineteen 3840 by 2160
//! JPEGs, and uploaded each one whole to draw it in a cell a few hundred points wide.
//!
//! A thumbnail is a JPEG [`WIDTH`] pixels wide in `.thumbnails/` inside the backgrounds folder, named after its
//! picture. It is made once: for every picture already there when the window starts, on a thread of its own, and
//! for a picture the moment it is added. The grid reads the thumbnail, which decodes in a few milliseconds, and
//! asks for one to be made on a thread when it finds none, drawing the cell empty until it is there.
//!
//! A thumbnail older than its picture is made again, so a picture replaced by hand under the same name does
//! not keep showing the old one.

use std::path::{Path, PathBuf};

/// How wide a thumbnail is, in pixels: twice the widest a grid cell is drawn, so it stays sharp on a display
/// with two pixels a point.
pub const WIDTH: u32 = 480;

/// The folder the thumbnails are kept in, inside the backgrounds folder. It starts with a dot, and it is a
/// folder, so `list_in` never lists it as a picture.
pub const FOLDER: &str = ".thumbnails";

/// Where the thumbnail of the picture called `name` is.
///
/// @param folder - the backgrounds folder
/// @param name - the picture's file name
pub fn path_of(folder: &Path, name: &str) -> PathBuf {
    folder.join(FOLDER).join(format!("{name}.jpg"))
}

/// Whether the picture called `name` has a thumbnail that is not older than it.
pub fn is_current(folder: &Path, name: &str) -> bool {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
    match (modified(&path_of(folder, name)), modified(&folder.join(name))) {
        (Some(thumbnail), Some(picture)) => thumbnail >= picture,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Make the thumbnail of the picture called `name`, unless a current one is there already. Answers where it is.
///
/// @param folder - the backgrounds folder
/// @param name - the picture's file name
pub fn make(folder: &Path, name: &str) -> Result<PathBuf, String> {
    let out = path_of(folder, name);
    if is_current(folder, name) {
        return Ok(out);
    }
    let source = folder.join(name);
    let picture = image::open(&source)
        .map_err(|problem| format!("{} could not be read: {problem}", source.display()))?;
    let height = ((picture.height() as f32 / picture.width().max(1) as f32) * WIDTH as f32)
        .round()
        .max(1.0) as u32;
    let small = picture
        .resize_exact(
            WIDTH.min(picture.width()),
            height.min(picture.height()),
            image::imageops::FilterType::Triangle,
        )
        .to_rgb8();
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|problem| format!("{} could not be made: {problem}", parent.display()))?;
    }
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85)
        .encode_image(&small)
        .map_err(|problem| format!("the thumbnail of {name} could not be made: {problem}"))?;
    crate::services::store::write_atomically(&out, &bytes)
        .map_err(|problem| format!("{} could not be written: {problem}", out.display()))?;
    Ok(out)
}

/// Make every missing thumbnail in the folder. Answers how many were made.
pub fn make_the_missing(folder: &Path) -> usize {
    super::list_in(folder)
        .iter()
        .filter(|name| !is_current(folder, name))
        .filter(|name| make(folder, name).is_ok())
        .count()
}

/// The same on a thread of its own, calling `done` when it has finished, so nothing waits for it.
///
/// @param folder - the backgrounds folder
/// @param done - called once every missing thumbnail has been made, such as a request for a frame
pub fn make_the_missing_later(folder: PathBuf, done: impl FnOnce() + Send + 'static) {
    let started =
        std::thread::Builder::new().name("unluminous-thumbnails".to_owned()).spawn(move || {
            make_the_missing(&folder);
            done();
        });
    if let Err(problem) = started {
        eprintln!("Unluminous could not start making background thumbnails: {problem}");
    }
}

/// Take a picture's thumbnail away with it.
pub fn forget(folder: &Path, name: &str) {
    let _ = std::fs::remove_file(path_of(folder, name));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_folder_with_a_picture(test: &str) -> PathBuf {
        let folder = std::env::temp_dir().join("unluminous-thumbnails").join(test);
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("the folder");
        let picture = image::RgbImage::from_pixel(1920, 1080, image::Rgb([40, 90, 160]));
        picture.save(folder.join("wide.png")).expect("the picture");
        folder
    }

    #[test]
    fn a_thumbnail_is_made_once_and_is_small() {
        let folder = a_folder_with_a_picture("once");
        assert!(!is_current(&folder, "wide.png"));
        let made = make(&folder, "wide.png").expect("made");
        let small = image::open(&made).expect("it reads");
        assert_eq!((small.width(), small.height()), (WIDTH, 270), "the picture's shape, 480 wide");
        assert!(is_current(&folder, "wide.png"));
        assert_eq!(make_the_missing(&folder), 0, "nothing is made twice");
        assert_eq!(
            super::super::list_in(&folder),
            vec!["wide.png".to_owned()],
            "the folder is not a picture"
        );
        forget(&folder, "wide.png");
        assert!(!is_current(&folder, "wide.png"));
        assert_eq!(make_the_missing(&folder), 1);
    }
}
