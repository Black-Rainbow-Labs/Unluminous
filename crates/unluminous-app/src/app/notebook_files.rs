//! A notebook on disk: opening one into a tab, writing it back, reading it again when something else
//! changed it, making a new one, converting to and from a `.py` file, and exporting it.
//!
//! **What is on disk is the `.ipynb` JSON and what the tab edits is text.** So a notebook tab is opened
//! by reading the JSON and building its text, and written by building the JSON from the text and the
//! outputs beside it (`unluminous_jupyter::text::merge` and `nbformat::serialize`), through
//! `Document::save_bytes_as`, which writes through a temporary and a rename exactly as an ordinary
//! save does. A notebook opened and saved with nothing changed is written back byte for byte, which
//! `unluminous-jupyter`'s tests hold it to.

use std::path::{Path, PathBuf};

use unluminous_core::{Command, Document};
use unluminous_jupyter::{export, nbformat};

use crate::app::files::OpenFile;
use crate::app::notebook::NotebookTab;
use crate::app::UnluminousApp;

/// True when `path` names a Jupyter notebook.
pub fn is_notebook(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ipynb"))
}

impl UnluminousApp {
    /// Open a notebook into a tab. A file that is not a notebook Unluminous can read opens as plain
    /// text instead, with the reason in the status bar, so nothing is ever hidden.
    pub(crate) fn open_a_notebook(&mut self, path: &Path, permanent: bool) -> Result<(), String> {
        let read = unluminous_core::document::read_file(path).map_err(|problem| {
            format!("Unluminous could not open {}: {problem}", path.display())
        })?;
        let json = read.text;
        let (mut tab, text) = match NotebookTab::open(&json) {
            Ok(opened) => opened,
            Err(problem) => {
                let said = format!(
                    "{} is not a notebook Unluminous can read ({problem}), so it is shown as text.",
                    path.display()
                );
                self.open_as_text_after_all(path, permanent)?;
                self.message = Some(said);
                return Ok(());
            }
        };
        tab.line_ending = read.line_ending;
        let mut document = Document::from_text(&text);
        document.set_path(path.to_path_buf());
        document.apply(Command::MoveDocumentStart { extend: false });
        let mut file = OpenFile::new(document);
        file.notebook = Some(Box::new(tab));
        self.files.open_file(file, permanent);
        let change = self.settings.as_style_change();
        self.document_mut().set_base_style(change);
        self.files.active_mut().note_what_is_on_disk();
        let index = self.files.active_index();
        self.refresh_the_notebook(index);
        // The first cell is chosen in command mode, which is where the reference editor and Jupyter both open a
        // notebook: nothing is being typed into yet, and the letters are commands.
        if self.files.at(index).notebook.as_deref().is_some_and(|tab| !tab.is_empty()) {
            self.choose_a_cell(index, 0, false);
        }
        self.message = None;
        self.forget_layout();
        Ok(())
    }

    /// The ordinary way of opening a file, for a notebook that would not read.
    fn open_as_text_after_all(&mut self, path: &Path, permanent: bool) -> Result<(), String> {
        let document = Document::open(path).map_err(|problem| {
            format!("Unluminous could not open {}: {problem}", path.display())
        })?;
        self.files.open(document, permanent);
        let change = self.settings.as_style_change();
        self.document_mut().set_base_style(change);
        self.files.active_mut().note_what_is_on_disk();
        self.forget_layout();
        Ok(())
    }

    /// Write the tab at `index` to `path`, or to its own file. A notebook is written as `.ipynb`
    /// JSON; anything else as its text, which is what `Document::save_as` does.
    pub(crate) fn write_a_tab(&mut self, index: usize, path: Option<&Path>) -> std::io::Result<()> {
        let file = self.files.at_mut(index);
        let target = match path {
            Some(path) => path.to_path_buf(),
            None => file
                .document
                .path()
                .map(Path::to_path_buf)
                .ok_or_else(|| std::io::Error::other("this document has no file to save to"))?,
        };
        match file.notebook.as_deref() {
            Some(tab) => {
                let json = tab.serialize(&file.document.text().to_string());
                file.document.save_bytes_as(&target, json.as_bytes())
            }
            None => file.document.save_as(&target),
        }
    }

    /// Read a notebook tab's file again, keeping what has run by cell id. Answers whether it was
    /// read.
    pub(crate) fn reread_a_notebook(&mut self, index: usize, path: &Path) -> Result<(), String> {
        let read =
            unluminous_core::document::read_file(path).map_err(|problem| problem.to_string())?;
        let model = nbformat::parse(&read.text)?;
        let (fresh, text) = NotebookTab::new(model);
        let mut document = Document::from_text(&text);
        document.set_path(path.to_path_buf());
        document.set_base_style(self.settings.as_style_change());
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else {
            return Err("not a notebook".to_owned());
        };
        // The file is what decides the cells, their outputs and their counts. What the kernel is, and
        // how far its cells have run, belongs to the window and stays.
        tab.model = fresh.model;
        tab.spans = fresh.spans;
        tab.line_ending = read.line_ending;
        tab.merged_at = 0;
        tab.drawn.clear();
        tab.rendered.clear();
        tab.rooms.clear();
        tab.bands_revision += 1;
        file.document = document;
        file.note_what_is_on_disk();
        file.forget_what_was_worked_out();
        Ok(())
    }

    /// Make a new notebook with one empty code cell, open it, and answer where it is. `path` is where
    /// to put it; with none it is `Untitled.ipynb` in the project folder, numbered when that is taken.
    pub(crate) fn make_a_new_notebook(&mut self, path: Option<&Path>) -> Result<PathBuf, String> {
        let path = match path {
            Some(path) if path.is_absolute() => path.to_path_buf(),
            Some(path) => self.tree.root().join(path),
            None => free_name(self.tree.root(), "Untitled", "ipynb"),
        };
        if path.exists() {
            return Err(format!("{} already exists.", path.display()));
        }
        let json = nbformat::serialize(&nbformat::empty());
        crate::services::store::write_a_source_file(&path, json.as_bytes()).map_err(|problem| {
            format!("Unluminous could not write {}: {problem}", path.display())
        })?;
        self.tree.reload();
        self.open_a_notebook(&path, true)?;
        Ok(path)
    }

    /// Write a `.py` file's cells into a notebook beside it, or a notebook's into a `.py` file, and
    /// open what was written. A file of that name already there is never written over.
    pub(crate) fn convert_between_a_notebook_and_python(&mut self, path: &Path, to_notebook: bool) {
        match self.convert(path, to_notebook) {
            Ok(written) => self.message = Some(format!("Wrote {}", written.display())),
            Err(problem) => self.message = Some(problem),
        }
    }

    /// See [`Self::convert_between_a_notebook_and_python`].
    pub(crate) fn convert(&mut self, path: &Path, to_notebook: bool) -> Result<PathBuf, String> {
        let source = unluminous_core::document::read_to_normalised_string(path)
            .map_err(|problem| problem.to_string())?;
        let folder = path.parent().unwrap_or(Path::new("."));
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "notebook".to_owned());
        let (written, contents) = match to_notebook {
            true => (
                free_name(folder, &stem, "ipynb"),
                nbformat::serialize(&export::from_python(&source)),
            ),
            false => {
                (free_name(folder, &stem, "py"), export::to_python(&nbformat::parse(&source)?))
            }
        };
        crate::services::store::write_a_source_file(&written, contents.as_bytes()).map_err(
            |problem| format!("Unluminous could not write {}: {problem}", written.display()),
        )?;
        self.tree.reload();
        self.open_path_permanently(&written)?;
        Ok(written)
    }

    /// Export the notebook that is showing as HTML, Markdown or Python, beside it. Answers the file
    /// written. Pictures in a Markdown export are written beside it, named after it.
    pub(crate) fn export_the_notebook(
        &mut self,
        format: &str,
        to: Option<&Path>,
    ) -> Result<PathBuf, String> {
        let file = self.files.active();
        let tab = file.notebook.as_deref().ok_or("The tab that is showing is not a notebook.")?;
        let path = file
            .path()
            .ok_or("Save the notebook first, so the export has somewhere to go.")?
            .to_path_buf();
        let model = unluminous_jupyter::text::merge(&file.document.text().to_string(), &tab.model);
        let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let stem =
            path.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
        let (extension, contents, pictures) = match format {
            "html" => ("html", export::to_html(&model, &stem), Vec::new()),
            "md" | "markdown" => {
                let mut pictures = Vec::new();
                let text = export::to_markdown(&model, &mut pictures, &stem);
                ("md", text, pictures)
            }
            "py" | "python" => ("py", export::to_python(&model), Vec::new()),
            other => {
                return Err(format!(
                    "{other} is not a format Unluminous exports to. Use html, md or py."
                ))
            }
        };
        let target = match to {
            Some(to) => to.to_path_buf(),
            None => free_name(&folder, &stem, extension),
        };
        crate::services::store::write_a_source_file(&target, contents.as_bytes()).map_err(
            |problem| format!("Unluminous could not write {}: {problem}", target.display()),
        )?;
        for (name, bytes) in pictures {
            let place = target.parent().unwrap_or(&folder).join(name);
            crate::services::store::write_a_source_file(&place, &bytes).map_err(|problem| {
                format!("Unluminous could not write {}: {problem}", place.display())
            })?;
        }
        self.tree.reload();
        Ok(target)
    }
}

/// `<stem>.<extension>` in `folder`, or `<stem> 2.<extension>` and on up when that is taken.
pub fn free_name(folder: &Path, stem: &str, extension: &str) -> PathBuf {
    let first = folder.join(format!("{stem}.{extension}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|number| folder.join(format!("{stem} {number}.{extension}")))
        .find(|candidate| !candidate.exists())
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notebook_is_known_by_its_extension_in_either_case() {
        assert!(is_notebook(Path::new("a/b.ipynb")));
        assert!(is_notebook(Path::new("B.IPYNB")));
        assert!(!is_notebook(Path::new("b.py")));
    }

    #[test]
    fn a_free_name_counts_up_past_the_ones_that_are_taken() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-free-name-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Untitled.ipynb"), "{}").unwrap();
        std::fs::write(folder.join("Untitled 2.ipynb"), "{}").unwrap();
        assert_eq!(free_name(&folder, "Untitled", "ipynb"), folder.join("Untitled 3.ipynb"));
        std::fs::remove_file(folder.join("Untitled.ipynb")).unwrap();
        std::fs::remove_file(folder.join("Untitled 2.ipynb")).unwrap();
        std::fs::remove_dir(&folder).unwrap();
    }
}
