//! What was cut or copied in the explorer, waiting to be pasted.
//!
//! This is Unluminous's own clipboard, and since `task-2194` it is not the only one: a copy or a cut in
//! the folder pane also puts the files on the operating system's clipboard, and a paste takes files
//! another program put there. `services::system_files` is that half, because the system's file
//! clipboard is a different interface on each platform. What only this one knows is whether the files
//! are to be **moved**: a file manager says so in a way of its own, and a cut here is a move here.
//!
//! Pasting is a copy or a move depending on which of the two put the paths here, which is what every
//! file manager does. Several paths are held at once, because several rows can be chosen and cut
//! together.

use std::path::{Path, PathBuf};

/// Whether the file is to be copied or moved when it is pasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    Copy,
    Move,
}

/// The paths waiting to be pasted, and what is to happen to them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileClipboard {
    held: Vec<PathBuf>,
    transfer: Option<Transfer>,
}

impl FileClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cut(&mut self, path: impl Into<PathBuf>) {
        self.cut_all(vec![path.into()]);
    }

    pub fn copy(&mut self, path: impl Into<PathBuf>) {
        self.copy_all(vec![path.into()]);
    }

    /// Hold `paths` to be moved by the next paste.
    pub fn cut_all(&mut self, paths: Vec<PathBuf>) {
        self.held = paths;
        self.transfer = Some(Transfer::Move);
    }

    /// Hold `paths` to be copied by every paste until something else is cut or copied.
    pub fn copy_all(&mut self, paths: Vec<PathBuf>) {
        self.held = paths;
        self.transfer = Some(Transfer::Copy);
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// What is held and what is to happen to it.
    pub fn held(&self) -> Option<(&[PathBuf], Transfer)> {
        match (self.held.is_empty(), self.transfer) {
            (false, Some(transfer)) => Some((&self.held, transfer)),
            _ => None,
        }
    }

    /// Whether exactly `paths` are held, which is how a paste tells files this clipboard put on the
    /// system's from files another program put there since.
    pub fn holds(&self, paths: &[PathBuf]) -> bool {
        !self.held.is_empty() && self.held == paths
    }

    pub fn clear(&mut self) {
        self.held.clear();
        self.transfer = None;
    }

    /// Put what is held into `folder`.
    ///
    /// Returns where each one ended up. A name already taken in the destination gets a number added
    /// rather than overwriting what is there: pasting must never quietly destroy a file, and asking
    /// would mean a dialog inside a dialog.
    pub fn paste_into(&mut self, folder: &Path) -> std::io::Result<Vec<PathBuf>> {
        let Some((held, transfer)) = self.held().map(|(held, transfer)| (held.to_vec(), transfer))
        else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "nothing has been cut or copied",
            ));
        };
        if let Some(gone) = held.iter().find(|path| !path.exists()) {
            self.clear();
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} is no longer there", gone.display()),
            ));
        }
        let pasted = transfer_into(&held, folder, transfer)?;
        // A move happens once. A copy can be pasted into several folders, which is what a file
        // manager does and what makes copy worth having as well as cut.
        if transfer == Transfer::Move {
            self.clear();
        }
        Ok(pasted)
    }
}

/// Copy or move every one of `paths` into `folder`, each under a name not already taken there.
///
/// Shared by a paste and by files dropped on the pane from another program, which is a copy: the
/// program they came from still has them. A folder is never put inside itself, which a person
/// pasting a folder into one of its own subfolders would otherwise do without end.
pub fn transfer_into(
    paths: &[PathBuf],
    folder: &Path,
    transfer: Transfer,
) -> std::io::Result<Vec<PathBuf>> {
    let mut placed = Vec::with_capacity(paths.len());
    for source in paths {
        if source.is_dir() && folder.starts_with(source) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{} cannot go inside itself", source.display()),
            ));
        }
        let name = source.file_name().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "a path with no name")
        })?;
        // Moving a file into the folder it is already in leaves it where it is.
        if transfer == Transfer::Move && source.parent() == Some(folder) {
            placed.push(source.clone());
            continue;
        }
        let target = free_name(folder, &name.to_string_lossy());
        if source.is_dir() {
            copy_folder(source, &target)?;
            if transfer == Transfer::Move {
                std::fs::remove_dir_all(source)?;
            }
        } else {
            std::fs::copy(source, &target)?;
            if transfer == Transfer::Move {
                std::fs::remove_file(source)?;
            }
        }
        placed.push(target);
    }
    Ok(placed)
}

/// A path in `folder` called `name`, or `name 2`, `name 3` and so on if that is taken.
///
/// The number goes before the extension, so `notes 2.md` rather than `notes.md 2`.
pub fn free_name(folder: &Path, name: &str) -> PathBuf {
    let candidate = folder.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, extension) = match name.rsplit_once('.') {
        // A leading dot is the whole name of a hidden file, not an extension.
        Some((stem, extension)) if !stem.is_empty() => (stem, format!(".{extension}")),
        _ => (name, String::new()),
    };
    for number in 2..1000 {
        let candidate = folder.join(format!("{stem} {number}{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    folder.join(name)
}

/// Copy a folder and everything under it.
fn copy_folder(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_folder(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join("unluminous-file-clipboard").join(name);
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("from")).expect("make from");
        std::fs::create_dir_all(root.join("to")).expect("make to");
        std::fs::write(root.join("from/note.md"), "a note\n").expect("write the file");
        root
    }

    #[test]
    fn a_copy_leaves_the_original_and_a_cut_does_not() {
        let root = folder("copy-and-cut");
        let mut clipboard = FileClipboard::new();
        clipboard.copy(root.join("from/note.md"));
        let pasted = clipboard.paste_into(&root.join("to")).expect("paste");
        assert_eq!(pasted, vec![root.join("to/note.md")]);
        assert!(root.join("from/note.md").is_file(), "a copy leaves the original where it was");
        assert!(!clipboard.is_empty(), "a copy can be pasted into a second folder");

        std::fs::create_dir_all(root.join("second")).expect("make second");
        clipboard.cut(root.join("from/note.md"));
        clipboard.paste_into(&root.join("second")).expect("paste");
        assert!(!root.join("from/note.md").exists(), "a cut moves the file");
        assert!(root.join("second/note.md").is_file());
        assert!(clipboard.is_empty(), "a move happens once");
    }

    #[test]
    fn pasting_over_a_name_that_is_taken_adds_a_number_rather_than_overwriting() {
        let root = folder("no-overwrite");
        std::fs::write(root.join("to/note.md"), "something else\n").expect("write the other one");
        let mut clipboard = FileClipboard::new();
        clipboard.copy(root.join("from/note.md"));
        let pasted = clipboard.paste_into(&root.join("to")).expect("paste");
        assert_eq!(pasted, vec![root.join("to/note 2.md")], "the number goes before the extension");
        assert_eq!(
            std::fs::read_to_string(root.join("to/note.md")).expect("read"),
            "something else\n",
            "the file that was there is untouched"
        );
    }

    #[test]
    fn a_folder_is_pasted_with_everything_under_it() {
        let root = folder("whole-folder");
        std::fs::create_dir_all(root.join("from/inner")).expect("make inner");
        std::fs::write(root.join("from/inner/deep.txt"), "deep\n").expect("write deep");
        let mut clipboard = FileClipboard::new();
        clipboard.copy(root.join("from"));
        clipboard.paste_into(&root.join("to")).expect("paste");
        assert!(root.join("to/from/inner/deep.txt").is_file());
    }

    #[test]
    fn pasting_something_that_has_gone_says_so_and_forgets_it() {
        let root = folder("gone");
        let mut clipboard = FileClipboard::new();
        clipboard.cut(root.join("from/missing.md"));
        let problem = clipboard.paste_into(&root.join("to")).expect_err("it is not there");
        assert!(problem.to_string().contains("no longer there"));
        assert!(clipboard.is_empty());
    }

    /// `task-2194`: several rows chosen and cut together move together, and a folder pasted into one of
    /// its own subfolders is refused rather than copied into itself for ever.
    #[test]
    fn several_paths_move_together_and_a_folder_never_goes_inside_itself() {
        let root = folder("several");
        std::fs::write(root.join("from/second.md"), "two").expect("write the second");
        let mut clipboard = FileClipboard::new();
        clipboard.cut_all(vec![root.join("from/note.md"), root.join("from/second.md")]);
        let pasted = clipboard.paste_into(&root.join("to")).expect("paste");
        assert_eq!(pasted, vec![root.join("to/note.md"), root.join("to/second.md")]);
        assert!(!root.join("from/note.md").exists() && !root.join("from/second.md").exists());

        std::fs::create_dir_all(root.join("from/inner")).expect("make inner");
        clipboard.copy(root.join("from"));
        let refused = clipboard.paste_into(&root.join("from/inner")).expect_err("refused");
        assert!(refused.to_string().contains("inside itself"), "{refused}");
    }

    #[test]
    fn a_hidden_file_keeps_its_whole_name() {
        let root = folder("hidden");
        std::fs::write(root.join("to/.unluminousrc"), "one\n").expect("write it");
        assert_eq!(free_name(&root.join("to"), ".unluminousrc"), root.join("to/.unluminousrc 2"));
    }
}
