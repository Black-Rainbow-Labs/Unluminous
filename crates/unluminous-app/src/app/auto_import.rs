//! Writing the import a completed name needs, when no language server is there to write it.
//! `task-2231` §6.5.
//!
//! A row the structural tier offers with `Source::Import` names something the project defines in a file
//! the one being typed in does not import. Accepting it inserts the name and this import in one
//! `Command::ReplaceMany`, so one undo takes both away. The import is composed the way `task-1681`'s move
//! refactor rewrites one, used in the other direction:
//!
//! - **Rust**: `use crate::module::Name;` for a file of the same crate, `use other_crate::module::Name;`
//!   for another crate of the workspace, read off the path by `services::imports::module_of`. When a
//!   `use` of the same module already names other items it is rewritten to name this one too, and
//!   otherwise the new line goes among the file's top level `use` lines in alphabetical order.
//! - **TypeScript and JavaScript**: `import { Name } from './relative';`, with the specifier from
//!   `services::imports::write_specifier`, merged into an existing named import from the same
//!   specifier.
//! - **Python**: `from .module import Name` for a file in the same folder, `from package.module import
//!   Name` otherwise, merged into an existing `from … import` of the same module.
//!
//! Anything else gets no import: a row is still inserted, and the import is left to the person, which
//! is better than a guess.

use std::path::Path;

use atrius_index::structure::Import;
use unluminous_core::completion::Edit;
use unluminous_core::ImportStyle;

use crate::app::UnluminousApp;
use crate::services::imports;

impl UnluminousApp {
    /// The edit that imports `name`, defined in `rel`, into the tab that is showing, or `None` when the
    /// language writes no import this can compose.
    ///
    /// @param name - the name being completed
    /// @param rel - the file that defines it, relative to the project
    pub(crate) fn import_edit(&mut self, name: &str, rel: &str) -> Option<Edit> {
        let here = self.files.active().path()?.to_path_buf();
        let there = self.project_symbols.as_ref()?.absolute(rel);
        let grammar = self.grammar_for(Some(&here))?.clone();
        let index = self.files.active_index();
        let existing = self.exact_tab_structure(index)?.imports.clone();
        let text = self.document().text().to_string();
        match grammar.imports? {
            ImportStyle::Path if grammar.path_separator.as_deref() == Some("::") => {
                let module = rust_module_path(&here, &there, &grammar)?;
                Some(path_import_edit(&text, &existing, &module, name, "::", "use", ";"))
            }
            ImportStyle::Path => {
                let module = python_module(&here, &there, self.tree.root())?;
                Some(python_import_edit(&text, &existing, &module, name))
            }
            ImportStyle::Quoted => {
                let specifier = imports::write_specifier(here.parent()?, &there, &grammar)?;
                Some(quoted_import_edit(&text, &existing, &specifier, name))
            }
        }
    }

    /// How the import a row would add is written, for the row's detail: `use crate::layout`.
    ///
    /// @param rel - the file that defines the row's name, relative to the project
    pub(crate) fn import_label(&self, rel: &str) -> Option<String> {
        let here = self.files.active().path()?.to_path_buf();
        let there = self.project_symbols.as_ref()?.absolute(rel);
        let grammar = self.grammar_for(Some(&here))?;
        match grammar.imports? {
            ImportStyle::Path if grammar.path_separator.as_deref() == Some("::") => {
                rust_module_path(&here, &there, grammar).map(|m| format!("use {}", m.join("::")))
            }
            ImportStyle::Path => {
                python_module(&here, &there, self.tree.root()).map(|m| format!("from {m}"))
            }
            ImportStyle::Quoted => imports::write_specifier(here.parent()?, &there, grammar)
                .map(|s| format!("from '{s}'")),
        }
    }
}

/// The module path a Rust file is reached by from another: `crate` and the segments for the same crate,
/// the other crate's name and its segments for another crate of the workspace.
///
/// @param here - the file being typed in
/// @param there - the file that defines the name
/// @param grammar - Rust's grammar, which names the source roots and the index files
fn rust_module_path(
    here: &Path,
    there: &Path,
    grammar: &unluminous_core::Grammar,
) -> Option<Vec<String>> {
    let target = imports::module_of(there, grammar)?;
    let from = imports::module_of(here, grammar);
    let head = match from.is_some_and(|f| f.root == target.root) {
        true => "crate".to_owned(),
        false => target.package.clone(),
    };
    Some(std::iter::once(head).chain(target.segments).collect())
}

/// The dotted module a Python file is imported by: `.stem` for a file in the same folder, the path from
/// the project's root otherwise.
///
/// @param here - the file being typed in
/// @param there - the file that defines the name
/// @param root - the project folder
fn python_module(here: &Path, there: &Path, root: &Path) -> Option<String> {
    let stem = there.file_stem()?.to_string_lossy().into_owned();
    if here.parent() == there.parent() {
        return Some(format!(".{stem}"));
    }
    let under = there.strip_prefix(root).ok()?.with_extension("");
    let parts: Vec<String> =
        under.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let parts: Vec<String> = match parts.last().map(String::as_str) {
        Some("__init__") => parts[..parts.len() - 1].to_vec(),
        _ => parts,
    };
    (!parts.is_empty()).then(|| parts.join("."))
}

/// Where a new import line goes: before the first top level import whose text sorts after it, after
/// the last one when none does, or at the top of the file past its leading comments and attributes.
///
/// @param text - the file's text
/// @param existing - the file's imports
/// @param line - the new line, without its line break
fn new_line_at(text: &str, existing: &[Import], line: &str) -> Edit {
    let top: Vec<&Import> = existing
        .iter()
        .filter(|i| i.range.start == 0 || text.as_bytes().get(i.range.start - 1) == Some(&b'\n'))
        .collect();
    let sorts_after =
        |i: &Import| text[i.range.clone()].trim().to_lowercase() > line.to_lowercase();
    if let Some(after) = top.iter().find(|i| sorts_after(i)) {
        return Edit { range: after.range.start..after.range.start, text: format!("{line}\n") };
    }
    if let Some(last) = top.iter().max_by_key(|i| i.range.end) {
        let end =
            text[last.range.end..].find('\n').map_or(text.len(), |at| last.range.end + at + 1);
        let lead = if end == text.len() && !text.ends_with('\n') { "\n" } else { "" };
        return Edit { range: end..end, text: format!("{lead}{line}\n") };
    }
    // No import at all: after the leading comments and inner attributes, with a blank line after.
    let mut at = 0;
    for l in text.split_inclusive('\n') {
        let t = l.trim_start();
        if t.starts_with("//") || (t.starts_with('#') && !t.starts_with("#[")) {
            at += l.len();
        } else {
            break;
        }
    }
    // A blank line already under the comments is the gap; it is stepped over rather than doubled.
    let blank_follows = text[at..].starts_with('\n') || text[at..].starts_with("\r\n");
    if at > 0 && blank_follows {
        at += text[at..].find('\n').map_or(0, |end| end + 1);
    }
    let gap = if at > 0 && !blank_follows { "\n" } else { "" };
    Edit { range: at..at, text: format!("{gap}{line}\n\n") }
}

/// The edit that imports a name through a path import, Rust's `use`: into an existing statement for
/// the same module when there is one that can be rewritten, or as a new line.
///
/// @param text - the file's text
/// @param existing - the file's imports
/// @param module - the module's segments
/// @param name - the name
/// @param separator - `::`
/// @param keyword - `use`
/// @param end - `;`
fn path_import_edit(
    text: &str,
    existing: &[Import],
    module: &[String],
    name: &str,
    separator: &str,
    keyword: &str,
    end: &str,
) -> Edit {
    let same_module: Vec<&Import> = existing
        .iter()
        .filter(|i| {
            i.path.len() == module.len() + 1 && i.path[..module.len()] == *module && !i.glob
        })
        .collect();
    if let Some(first) = same_module.first() {
        let statement: Vec<&Import> = existing.iter().filter(|i| i.range == first.range).collect();
        // Only a statement every entry of which names this module directly can be rewritten whole.
        if statement.len() == same_module.iter().filter(|i| i.range == first.range).count() {
            let mut names: Vec<String> = statement
                .iter()
                .map(|i| match &i.alias {
                    Some(alias) => {
                        format!("{} as {alias}", i.path.last().cloned().unwrap_or_default())
                    }
                    None => i.path.last().cloned().unwrap_or_default(),
                })
                .collect();
            names.push(name.to_owned());
            names.sort_by_key(|n| n.to_lowercase());
            names.dedup();
            let lead = text[first.range.clone()].split(keyword).next().unwrap_or_default();
            let joined = format!(
                "{lead}{keyword} {}{separator}{{{}}}{end}",
                module.join(separator),
                names.join(", ")
            );
            return Edit { range: first.range.clone(), text: joined };
        }
    }
    let line = format!("{keyword} {}{separator}{name}{end}", module.join(separator));
    new_line_at(text, existing, &line)
}

/// The edit that imports a name from a quoted specifier: into an existing named import from it, or as
/// a new `import { Name } from '…';`.
///
/// @param text - the file's text
/// @param existing - the file's imports
/// @param specifier - the specifier
/// @param name - the name
fn quoted_import_edit(text: &str, existing: &[Import], specifier: &str, name: &str) -> Edit {
    let from_it: Vec<&Import> =
        existing.iter().filter(|i| i.path.first().map(String::as_str) == Some(specifier)).collect();
    let named: Vec<&&Import> =
        from_it.iter().filter(|i| i.path.len() == 2 && i.path[1] != "default").collect();
    let statement_text = |i: &Import| text[i.range.clone()].to_owned();
    if let Some(first) = named.first() {
        let written = statement_text(first);
        if !written.trim_start().starts_with("import type") {
            if let (Some(open), Some(close)) = (written.find('{'), written.rfind('}')) {
                let mut names: Vec<String> = written[open + 1..close]
                    .split(',')
                    .map(|n| n.trim().to_owned())
                    .filter(|n| !n.is_empty())
                    .collect();
                names.push(name.to_owned());
                names.sort_by_key(|n| n.to_lowercase());
                names.dedup();
                let rewritten = format!(
                    "{}{{ {} }}{}",
                    &written[..open],
                    names.join(", "),
                    &written[close + 1..]
                );
                return Edit { range: first.range.clone(), text: rewritten };
            }
        }
    }
    let quote = existing
        .iter()
        .find_map(|i| text[i.range.clone()].chars().find(|c| *c == '\'' || *c == '"'))
        .unwrap_or('\'');
    let line = format!("import {{ {name} }} from {quote}{specifier}{quote};");
    new_line_at(text, existing, &line)
}

/// The edit that imports a name in Python: into an existing `from module import …` of the module, or
/// as a new line.
///
/// @param text - the file's text
/// @param existing - the file's imports
/// @param module - the dotted module, `.layout` for a sibling
/// @param name - the name
fn python_import_edit(text: &str, existing: &[Import], module: &str, name: &str) -> Edit {
    let segments: Vec<String> = module.split('.').map(str::to_owned).collect();
    let same: Vec<&Import> = existing
        .iter()
        .filter(|i| i.path.len() == segments.len() + 1 && i.path[..segments.len()] == *segments)
        .collect();
    if let Some(first) = same.first() {
        let written = &text[first.range.clone()];
        if let Some(at) = written.find(" import ") {
            let mut names: Vec<String> = written[at + 8..]
                .trim()
                .trim_start_matches('(')
                .trim_end_matches(')')
                .split(',')
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty())
                .collect();
            names.push(name.to_owned());
            names.sort_by_key(|n| n.to_lowercase());
            names.dedup();
            return Edit {
                range: first.range.clone(),
                text: format!("{} import {}", &written[..at], names.join(", ")),
            };
        }
    }
    new_line_at(text, existing, &format!("from {module} import {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(path: &[&str], alias: Option<&str>, range: std::ops::Range<usize>) -> Import {
        Import {
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            alias: alias.map(str::to_owned),
            glob: false,
            range,
        }
    }

    /// The text after an edit.
    fn applied(text: &str, edit: &Edit) -> String {
        format!("{}{}{}", &text[..edit.range.start], edit.text, &text[edit.range.end..])
    }

    #[test]
    fn a_rust_use_of_the_same_module_names_the_new_item_too() {
        let text = "use crate::layout::Caret;\nuse std::fmt;\n\nfn main() {}\n";
        let existing = vec![
            import(&["crate", "layout", "Caret"], None, 0..25),
            import(&["std", "fmt"], None, 26..39),
        ];
        let module = vec!["crate".to_owned(), "layout".to_owned()];
        let edit = path_import_edit(text, &existing, &module, "Layout", "::", "use", ";");
        assert_eq!(
            applied(text, &edit),
            "use crate::layout::{Caret, Layout};\nuse std::fmt;\n\nfn main() {}\n"
        );
    }

    #[test]
    fn a_rust_use_of_another_module_goes_in_alphabetical_order() {
        let text = "use crate::caret::Caret;\nuse std::fmt;\n\nfn main() {}\n";
        let existing = vec![
            import(&["crate", "caret", "Caret"], None, 0..24),
            import(&["std", "fmt"], None, 25..38),
        ];
        let module = vec!["crate".to_owned(), "layout".to_owned()];
        let edit = path_import_edit(text, &existing, &module, "Layout", "::", "use", ";");
        assert_eq!(
            applied(text, &edit),
            "use crate::caret::Caret;\nuse crate::layout::Layout;\nuse std::fmt;\n\nfn main() {}\n"
        );
    }

    #[test]
    fn a_file_with_no_imports_gets_one_after_its_leading_comments() {
        let text = "//! A module.\n\nfn main() {}\n";
        let module = vec!["crate".to_owned(), "layout".to_owned()];
        let edit = path_import_edit(text, &[], &module, "Layout", "::", "use", ";");
        assert_eq!(
            applied(text, &edit),
            "//! A module.\n\nuse crate::layout::Layout;\n\nfn main() {}\n"
        );
    }

    #[test]
    fn a_typescript_named_import_from_the_same_specifier_is_extended() {
        let text = "import { b } from './board';\n\nconst x = 1;\n";
        let existing = vec![import(&["./board", "b"], None, 0..28)];
        let edit = quoted_import_edit(text, &existing, "./board", "Board");
        assert_eq!(applied(text, &edit), "import { b, Board } from './board';\n\nconst x = 1;\n");
        let edit = quoted_import_edit(text, &existing, "./card", "Card");
        assert_eq!(
            applied(text, &edit),
            "import { b } from './board';\nimport { Card } from './card';\n\nconst x = 1;\n"
        );
    }

    #[test]
    fn a_python_from_import_of_the_same_module_is_extended() {
        let text = "from .models import Request\n\nx = 1\n";
        let existing = vec![import(&["", "models", "Request"], None, 0..27)];
        let edit = python_import_edit(text, &existing, ".models", "Response");
        assert_eq!(applied(text, &edit), "from .models import Request, Response\n\nx = 1\n");
    }
}
