//! Which grammar reads which file, worked out from the plugins that are switched on, and the
//! colour scheme a plugin's own tokens fall back to when the active theme names none.
//!
//! [`Grammars`] is the small, cheap value `Plugins::grammars` keeps rather than recomputing every
//! frame — `task-1805`'s measurement of what deep-cloning a grammar per extension cost. Nothing
//! here reads a manifest; that is `manifest::parse`'s job, and this file only reads what it built.

use std::path::Path;

use unluminous_core::syntax::Grammar;

use super::types::{ends_open, name_matches, Plugin, SyntaxTheme};

/// Which grammar reads which extension, taken from the plugins that are switched on.
///
/// A list rather than a map: five plugins claim a dozen extensions between them, and a linear walk
/// over a dozen short strings costs less than hashing one. It is `Clone` and holds nothing borrowed,
/// so a copy can be sent to a thread.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grammars {
    by_extension: Vec<(String, Grammar)>,
    /// `language.file_names` patterns, tried before any extension (`task-2384`).
    by_name: Vec<(String, Grammar)>,
}

impl Grammars {
    /// A set built by hand, for a test that needs one without a plugin folder behind it.
    ///
    /// Extensions are written without the dot, as `for_path` compares them.
    pub fn of(by_extension: Vec<(String, Grammar)>) -> Self {
        Self { by_extension, by_name: Vec::new() }
    }

    /// The same, with the file name patterns plugins claim, in lower case. A name is tried before
    /// an extension, which is what makes `tsconfig.json` JSON with comments and `Dockerfile.dev` a
    /// Dockerfile.
    pub fn with_names(mut self, by_name: Vec<(String, Grammar)>) -> Self {
        self.by_name = by_name;
        self
    }

    /// The grammar that reads this file, if a plugin that is switched on claims it.
    ///
    /// **A notebook is read as Python**, because its tab's text is its cells and nearly every notebook
    /// is Python: that is what gives a cell completion, go to definition across cells and `#` as its
    /// comment. The project's index never reads a notebook through this, because on the disk a
    /// notebook is JSON — `services::symbol_index` leaves `.ipynb` files out. `task-2220`.
    pub fn for_path(&self, path: &Path) -> Option<&Grammar> {
        // In the order `Plugins::for_path` asks, so a file's colours and its plugin always agree:
        // a whole name, then the extension, then a name whose `*` stands for the extension.
        let named = |open: bool| -> Option<&Grammar> {
            let name = path.file_name().and_then(|name| name.to_str())?.to_lowercase();
            self.by_name
                .iter()
                .filter(|(pattern, _)| ends_open(pattern) == open)
                .find(|(pattern, _)| name_matches(pattern, &name))
                .map(|(_, grammar)| grammar)
        };
        if self.by_name.is_empty() {
            return self.by_extension(path);
        }
        named(false).or_else(|| self.by_extension(path)).or_else(|| {
            match crate::services::file_kind::opens_its_own_way(path) {
                true => None,
                false => named(true),
            }
        })
    }

    /// The grammar the file's extension names, if a plugin claims it.
    fn by_extension(&self, path: &Path) -> Option<&Grammar> {
        let mut extension = path.extension().and_then(|name| name.to_str())?.to_lowercase();
        if extension == "ipynb" {
            extension = "py".to_owned();
        }
        self.by_extension.iter().find(|(known, _)| *known == extension).map(|(_, grammar)| grammar)
    }

    /// True when this file's language has said enough for a definition to be found in it.
    ///
    /// What the index reads a file at all for, and — through `services::file_kind` — what decides
    /// whether the three symbol entries are on the menu for it.
    pub fn defines_symbols(&self, path: &Path) -> bool {
        self.for_path(path).is_some_and(Grammar::defines_symbols)
    }

    /// How many extensions are claimed, for a test and for `symbol cost`.
    pub fn len(&self) -> usize {
        self.by_extension.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_extension.is_empty()
    }
}

/// The scheme a file of this plugin's language is coloured by.
///
/// **The active theme's if it names the nine tokens, and the plugin's own otherwise.** Until `task-1776`
/// there was no other answer: five language plugins each carried their own copy of Dracula, so choosing a
/// scheme meant editing five manifests and a sixth language would have arrived with a sixth copy. A theme
/// owns the scheme now, which is what the reference editor does and is the whole reason a scheme can be switched at
/// all.
///
/// Unluminous's own theme names none, so an Unluminous nobody has chosen a theme in colours every file exactly as it
/// did before — which is the rule every key added since `task-1671` keeps, stated for colour.
///
/// Asked at the moment of use, so choosing a theme recolours every open file in the same frame, the way
/// [`Plugins::renders`] withdraws a diagram.
pub fn scheme_of(plugin: &Plugin) -> SyntaxTheme {
    crate::theme::syntax().unwrap_or_else(|| plugin.theme.clone())
}
