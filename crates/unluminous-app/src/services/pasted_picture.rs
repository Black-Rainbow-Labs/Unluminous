//! A picture pasted into a Markdown file is written beside it, under `images/`, and linked (`task-2634`).
//!
//! *"We need to be able to paste an image into an md file, etc. and have it show up. The image should go in the
//! same folder the md file is in, under a sub-dir called images."*
//!
//! A clipboard holds pixels rather than a file, so the picture is written as a PNG, which loses nothing, and the
//! text typed at the caret is `![<name>](images/<name>-1.png)` as a paragraph of its own, which is the form the preview draws
//! as a picture (see `services::preview_images`). The name is the Markdown file's own name with the next free
//! number after it, so the pictures of two documents in one folder are told apart by name.
//!
//! **A clipboard that holds text is a text paste**, whatever else it holds. Copying from a page or a document
//! often puts a picture and its words on the clipboard together, and a person pasting there means the words.
//! So [`from_the_clipboard`] answers `None` whenever there is text, and also when there is no picture at all,
//! because the paste chord's key going up arrives after an ordinary text paste as well and must do nothing then.

use std::path::{Path, PathBuf};

/// The folder beside the Markdown file that pasted pictures are written into.
pub const FOLDER: &str = "images";

/// Whether a file at `path` is Markdown, which is where a pasted picture is written and linked.
///
/// @param path - the file the tab is showing
pub fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| {
        ["md", "markdown", "mdx", "mdown", "mkd"]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}

/// The picture on the clipboard as PNG bytes, or `None` when there is text on it or no picture at all.
///
/// A picture that is there and cannot be read is a refusal, because a person pasting it has every reason to
/// expect it to appear.
pub fn from_the_clipboard() -> Result<Option<Vec<u8>>, String> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|problem| format!("the clipboard could not be opened: {problem}"))?;
    if clipboard.get_text().is_ok_and(|text| !text.is_empty()) {
        return Ok(None);
    }
    let held = match clipboard.get_image() {
        Ok(held) => held,
        Err(arboard::Error::ContentNotAvailable) => return Ok(None),
        Err(problem) => {
            return Err(format!("the picture on the clipboard could not be read: {problem}"))
        }
    };
    let buffer =
        image::RgbaImage::from_raw(held.width as u32, held.height as u32, held.bytes.into_owned())
            .ok_or_else(|| "the picture on the clipboard could not be read.".to_owned())?;
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(buffer)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|problem| format!("the picture could not be encoded: {problem}"))?;
    Ok(Some(bytes))
}

/// The picture in a file, as the bytes to write and the extension to write them under.
///
/// The agent's half of a paste: `editor paste-picture --file <picture>` copies a picture that is already on
/// disk, so an agent that has no clipboard to put a picture on reaches the same writing and linking. Refused
/// when the file is not a picture `image` can read, so a link is never written to something the preview cannot
/// draw.
///
/// @param path - the picture to copy
pub fn from_a_file(path: &Path) -> Result<(Vec<u8>, String), String> {
    let bytes = std::fs::read(path)
        .map_err(|problem| format!("{} could not be read: {problem}", path.display()))?;
    let format =
        image::guess_format(&bytes).map_err(|_| format!("{} is not a picture", path.display()))?;
    let extension = format.extensions_str().first().copied().unwrap_or("png").to_owned();
    Ok((bytes, extension))
}

/// Write a picture into the `images` folder beside `document` and answer the Markdown that links it.
///
/// The folder is made when it is not there. The name is the document's name, made safe for a link, with the
/// first number after it that no file in the folder has, so nothing already there is ever written over.
///
/// @param document - the Markdown file the picture is pasted into
/// @param bytes - the picture, already in the format `extension` names
/// @param extension - the file extension to write it under, such as `png`
pub fn save_beside(document: &Path, bytes: &[u8], extension: &str) -> Result<Saved, String> {
    let folder = document.parent().unwrap_or(Path::new(".")).join(FOLDER);
    std::fs::create_dir_all(&folder)
        .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    let stem = safe_stem(document);
    let path = free_name(&folder, &stem, extension);
    // Through a temporary file, so a picture is never left half written under the name the link points at.
    crate::services::store::write_atomically(&path, bytes)
        .map_err(|problem| format!("{} could not be written: {problem}", path.display()))?;
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_owned();
    let link = format!("![{stem}](images/{name})");
    Ok(Saved { path, link })
}

/// A picture written beside a Markdown file, and the text that links it.
#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    /// Where the picture was written.
    pub path: PathBuf,
    /// The Markdown that draws it, relative to the document: `![name](images/name-1.png)`.
    pub link: String,
}

/// The text to type at the caret for a picture: the link as a paragraph of its own, with a blank line before
/// and after it where there is text, because the preview draws a picture only when its mark is a paragraph on
/// its own. A link on the line straight after some words is read as more of that paragraph, which is
/// Markdown's lazy continuation, and is drawn as its alt text.
///
/// @param link - the Markdown link to the picture
/// @param before - the text before the caret, of which the end is read
/// @param after - the text after the caret, of which the start is read
pub fn on_a_line_of_its_own(link: &str, before: &str, after: &str) -> String {
    let lead = match () {
        _ if before.is_empty() || before.ends_with("\n\n") => "",
        _ if before.ends_with('\n') => "\n",
        _ => "\n\n",
    };
    let tail = match () {
        _ if after.is_empty() => "\n",
        _ if after.starts_with("\n\n") => "",
        _ if after.starts_with('\n') => "\n",
        _ => "\n\n",
    };
    format!("{lead}{link}{tail}")
}

/// The document's name without its extension, with anything a link would have to escape made a dash.
fn safe_stem(document: &Path) -> String {
    let stem = document.file_stem().and_then(|stem| stem.to_str()).unwrap_or("pasted");
    let safe: String = stem
        .chars()
        .map(|c| match c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
            true => c,
            false => '-',
        })
        .collect();
    match safe.trim_matches('-').is_empty() {
        true => "pasted".to_owned(),
        false => safe,
    }
}

/// `<stem>-<n>.<extension>` in `folder`, with the first `n` from 1 that names no file there.
fn free_name(folder: &Path, stem: &str, extension: &str) -> PathBuf {
    (1..)
        .map(|number| folder.join(format!("{stem}-{number}.{extension}")))
        .find(|path| !path.exists())
        .expect("an unused name is always found")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-pasted-picture-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");
        folder
    }

    #[test]
    fn a_picture_is_written_under_images_beside_the_document_and_linked_from_it() {
        let folder = a_folder("beside");
        let document = folder.join("Design notes.md");
        let first = save_beside(&document, b"one", "png").expect("written");
        assert_eq!(first.path, folder.join("images").join("Design-notes-1.png"));
        assert_eq!(first.link, "![Design-notes](images/Design-notes-1.png)");
        assert_eq!(std::fs::read(&first.path).expect("read"), b"one");
        // A second paste takes the next name and leaves the first picture alone.
        let second = save_beside(&document, b"two", "png").expect("written");
        assert_eq!(second.path, folder.join("images").join("Design-notes-2.png"));
        assert_eq!(std::fs::read(&first.path).expect("read"), b"one");
        // The preview finds the picture from the link, relative to the document's folder.
        let found =
            crate::services::preview_images::resolve(Some(&folder), "images/Design-notes-2.png");
        assert_eq!(found, Some(second.path.clone()));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn only_markdown_files_take_a_pasted_picture() {
        assert!(is_markdown(Path::new("a/README.md")));
        assert!(is_markdown(Path::new("notes.Markdown")));
        assert!(is_markdown(Path::new("page.mdx")));
        assert!(!is_markdown(Path::new("main.rs")));
        assert!(!is_markdown(Path::new("Makefile")));
    }

    #[test]
    fn the_link_is_put_on_a_line_of_its_own() {
        let link = "![a](images/a-1.png)";
        assert_eq!(on_a_line_of_its_own(link, "", ""), format!("{link}\n"));
        assert_eq!(on_a_line_of_its_own(link, "Some words", ""), format!("\n\n{link}\n"));
        assert_eq!(on_a_line_of_its_own(link, "Some words\n", ""), format!("\n{link}\n"));
        assert_eq!(on_a_line_of_its_own(link, "Some words\n\n", "\n\nmore"), link);
        assert_eq!(on_a_line_of_its_own(link, "", "more"), format!("{link}\n\n"));
        assert_eq!(on_a_line_of_its_own(link, "before", "after"), format!("\n\n{link}\n\n"));
        assert_eq!(on_a_line_of_its_own(link, "before", "\nafter"), format!("\n\n{link}\n"));
    }

    #[test]
    fn a_file_that_is_not_a_picture_is_refused() {
        let folder = a_folder("refused");
        let text = folder.join("words.txt");
        std::fs::write(&text, "not a picture").expect("written");
        assert!(from_a_file(&text).is_err());
        let picture = folder.join("dot.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .save(&picture)
            .expect("a picture");
        let (bytes, extension) = from_a_file(&picture).expect("read");
        assert_eq!(extension, "png");
        assert!(!bytes.is_empty());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
