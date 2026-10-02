//! The languages the index reads definitions and chunks in: the language plugins Unluminous ships.
//!
//! The manifests are the window's own `plugin.conf` files, compiled in from the app's `plugins` folder
//! and read with the same parser the window reads them with (`unluminous_core::manifest`), so a language
//! the window colours is a language the index chunks, with the same definers. A plugin a person installs
//! into their own settings folder is not read here yet; its files are still searched exactly, and chunked
//! as plain text.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use unluminous_core::manifest::{language_extensions, language_grammar, Values};
use unluminous_core::syntax::Grammar;

/// The language plugins the window ships, as `(id, manifest)`.
const BUNDLED: &[(&str, &str)] = &[
    ("css", include_str!("../../unluminous-app/plugins/css/plugin.conf")),
    ("html", include_str!("../../unluminous-app/plugins/html/plugin.conf")),
    ("javascript", include_str!("../../unluminous-app/plugins/javascript/plugin.conf")),
    ("json", include_str!("../../unluminous-app/plugins/json/plugin.conf")),
    ("mermaid", include_str!("../../unluminous-app/plugins/mermaid/plugin.conf")),
    ("python", include_str!("../../unluminous-app/plugins/python/plugin.conf")),
    ("rust", include_str!("../../unluminous-app/plugins/rust/plugin.conf")),
    ("shell", include_str!("../../unluminous-app/plugins/shell/plugin.conf")),
    ("sql", include_str!("../../unluminous-app/plugins/sql/plugin.conf")),
    ("toml", include_str!("../../unluminous-app/plugins/toml/plugin.conf")),
    ("typescript", include_str!("../../unluminous-app/plugins/typescript/plugin.conf")),
    ("yaml", include_str!("../../unluminous-app/plugins/yaml/plugin.conf")),
];

/// One language: its plugin id and its grammar.
pub struct Language {
    /// The plugin id, such as `rust`.
    pub id: &'static str,
    /// The grammar the manifest describes.
    pub grammar: Grammar,
}

/// Every bundled language by file extension, read once.
pub fn by_extension() -> &'static HashMap<String, Arc<Language>> {
    static LANGUAGES: OnceLock<HashMap<String, Arc<Language>>> = OnceLock::new();
    LANGUAGES.get_or_init(|| {
        let mut out = HashMap::new();
        for (id, text) in BUNDLED {
            let values = Values::parse(text);
            let name = values.text("plugin.name").unwrap_or(id).to_owned();
            let Ok(grammar) = language_grammar(&values, &name) else { continue };
            let language = Arc::new(Language { id, grammar });
            for extension in language_extensions(&values) {
                out.entry(extension).or_insert_with(|| Arc::clone(&language));
            }
        }
        out
    })
}

/// The language of a file, by its extension, if a bundled plugin claims it.
///
/// @param rel - the file's path
pub fn for_path(rel: &str) -> Option<Arc<Language>> {
    let extension = crate::store::extension(rel);
    by_extension().get(&extension).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_language_reads_and_rust_defines_symbols() {
        assert_eq!(
            by_extension().values().map(|l| l.id).collect::<std::collections::HashSet<_>>().len(),
            BUNDLED.len()
        );
        let rust = for_path("src/main.rs").expect("rust");
        assert!(rust.grammar.defines_symbols());
        assert!(for_path("README.md").is_none());
    }
}
