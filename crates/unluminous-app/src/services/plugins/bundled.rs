//! The plugins that ship inside the binary.
//!
//! They are bundled so that an Unluminous that has just been installed colours a `.rs` file the first time
//! it opens one, and so that the marketplace has something in it with no network involved.

/// Each entry is an id, its manifest, and its icon.
pub const ALL: &[(&str, &str, Option<&[u8]>)] = &[
    (
        "javascript",
        include_str!("../../../plugins/javascript/plugin.conf"),
        Some(include_bytes!("../../../plugins/javascript/icon.png")),
    ),
    (
        "typescript",
        include_str!("../../../plugins/typescript/plugin.conf"),
        Some(include_bytes!("../../../plugins/typescript/icon.png")),
    ),
    (
        "rust",
        include_str!("../../../plugins/rust/plugin.conf"),
        Some(include_bytes!("../../../plugins/rust/icon.png")),
    ),
    (
        "css",
        include_str!("../../../plugins/css/plugin.conf"),
        Some(include_bytes!("../../../plugins/css/icon.png")),
    ),
    // The first plugin that draws. Its rail button is `pane.icon`, which `theme::icon` draws
    // rather than a picture, so the button follows the window's colours; the icon here is the
    // mark the marketplace and its own page show, which every other plugin already had.
    (
        "agent-tasks",
        include_str!("../../../plugins/agent-tasks/plugin.conf"),
        Some(include_bytes!("../../../plugins/agent-tasks/icon.png")),
    ),
    // The second plugin that draws, and the first to ask for a pane since the machinery was
    // built. Its rail button is `pane.icon` for the same reason as Agent-Tasks'; the picture
    // here is the mark the marketplace draws.
    (
        "agent-chat",
        include_str!("../../../plugins/agent-chat/plugin.conf"),
        Some(include_bytes!("../../../plugins/agent-chat/icon.png")),
    ),
    // The Database plugin, and the SQL language its console is coloured through. Two plugins
    // rather than one, because a `.sql` file is coloured whether or not anybody opens the
    // database pane — see `plugins/sql/plugin.conf`.
    (
        "database",
        include_str!("../../../plugins/database/plugin.conf"),
        Some(include_bytes!("../../../plugins/database/icon.png")),
    ),
    (
        "sql",
        include_str!("../../../plugins/sql/plugin.conf"),
        Some(include_bytes!("../../../plugins/sql/icon.png")),
    ),
    (
        "mermaid",
        include_str!("../../../plugins/mermaid/plugin.conf"),
        Some(include_bytes!("../../../plugins/mermaid/icon.png")),
    ),
    (
        "html",
        include_str!("../../../plugins/html/plugin.conf"),
        Some(include_bytes!("../../../plugins/html/icon.png")),
    ),
    // The fifth of `task-1922`'s five: everything with no comment at all rather than one this
    // tokeniser can represent as a manifest value. See `plugins/json/plugin.conf`.
    (
        "json",
        include_str!("../../../plugins/json/plugin.conf"),
        Some(include_bytes!("../../../plugins/json/icon.png")),
    ),
    // `task-1922`. Triple quoted strings are named in this one's own `plugin.limitations` rather
    // than half coloured, because the tokeniser's string rule ends at a line break for every quote
    // but a backtick and that is not a manifest key — see `plugins/python/plugin.conf`.
    (
        "python",
        include_str!("../../../plugins/python/plugin.conf"),
        Some(include_bytes!("../../../plugins/python/icon.png")),
    ),
    // `task-1922`. A section header has no token of its own, so `[server.production]` is coloured
    // by its brackets and its dot and nothing else — see `plugins/toml/plugin.conf`.
    (
        "toml",
        include_str!("../../../plugins/toml/plugin.conf"),
        Some(include_bytes!("../../../plugins/toml/icon.png")),
    ),
    // `task-1922`. The awkward case named in the ticket - a bare word is a key or a value and this
    // tokeniser cannot tell which - is decided and written down in the manifest's own comment,
    // beside the four literals it does colour. See `plugins/yaml/plugin.conf`.
    (
        "yaml",
        include_str!("../../../plugins/yaml/plugin.conf"),
        Some(include_bytes!("../../../plugins/yaml/icon.png")),
    ),
    // `task-1922`. The one language here whose function definitions are found through
    // `brace_definitions` rather than a keyword, because `foo() { ... }` has no keyword to read -
    // see `plugins/shell/plugin.conf`.
    (
        "shell",
        include_str!("../../../plugins/shell/plugin.conf"),
        Some(include_bytes!("../../../plugins/shell/icon.png")),
    ),
    // The first plugin that is neither a language nor a pane: five palettes and nothing else. It
    // puts its mark in front of no file — what a theme looks like is the six swatches the Theme
    // page draws — but the marketplace lists it beside the others and a row with nothing where
    // every other row has a mark reads as a plugin that failed to load. `task-1795`.
    (
        "themes-bundle-1",
        include_str!("../../../plugins/themes-bundle-1/plugin.conf"),
        Some(include_bytes!("../../../plugins/themes-bundle-1/icon.png")),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use unluminous_core::symbols::SymbolKind;
    use unluminous_core::syntax::{ImportStyle, PathRoot};

    use crate::services::plugins::{Contributions, Kind, Plugins};

    #[test]
    fn the_older_plugins_ask_for_none_of_what_the_ui_added() {
        // The rule every round of keys has kept since `task-1671`: a language that names none of the
        // new keys is read by exactly the code that read it before. This is what proves the reader's
        // change has not moved Rust, CSS, HTML, JavaScript, TypeScript or Mermaid.
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        for plugin in plugins.all().iter().filter(|plugin| plugin.kind == Kind::Language) {
            assert_eq!(
                plugin.contributions,
                Contributions::default(),
                "{} contributes to the window and should not",
                plugin.id
            );
        }
        let mermaid = plugins.get("mermaid").expect("the mermaid plugin");
        assert_eq!(mermaid.kind, Kind::Language, "Mermaid is a language, not a plugin that draws");
    }

    /// Every plugin that ships carries a mark, whatever kind of plugin it is.
    ///
    /// `task-1795` reported the four that did not — Database, Agent-Chat, Agent-Tasks and the themes
    /// — as *"don't have an icon, and need one in the marketplace"*. The argument for leaving them
    /// out was that a plugin which draws puts its mark in front of no file; what that missed is that
    /// the marketplace lists every plugin in one column, so a row with nothing where every other row
    /// has a mark reads as a plugin that failed to load rather than as one with nothing to draw.
    #[test]
    fn every_bundled_plugin_carries_an_icon() {
        for (id, _, icon) in ALL {
            let bytes = icon.unwrap_or_else(|| panic!("{id} ships no icon"));
            assert!(bytes.len() > 100, "{id}'s icon is too small to be a picture");
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{id}'s icon is not a PNG");
        }
        let (plugins, _) = Plugins::load(None);
        for plugin in plugins.all() {
            assert!(plugin.icon.is_some(), "{} reached the window with no icon", plugin.id);
        }
    }

    #[test]
    fn the_bundled_plugins_all_parse_and_claim_what_they_should() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "a bundled plugin should always parse: {problems:?}");
        let ids: Vec<&str> = plugins.all().iter().map(|plugin| plugin.id.as_str()).collect();
        assert!(
            ids.contains(&"javascript") && ids.contains(&"typescript") && ids.contains(&"rust")
        );
        assert!(ids.contains(&"mermaid") && ids.contains(&"css") && ids.contains(&"html"));
        assert_eq!(plugins.for_path(Path::new("a.rs")).map(|p| p.id.as_str()), Some("rust"));
        assert_eq!(plugins.for_path(Path::new("a.ts")).map(|p| p.id.as_str()), Some("typescript"));
        assert_eq!(plugins.for_path(Path::new("a.js")).map(|p| p.id.as_str()), Some("javascript"));
        assert_eq!(plugins.for_path(Path::new("a.html")).map(|p| p.id.as_str()), Some("html"));
        assert_eq!(plugins.for_path(Path::new("a.htm")).map(|p| p.id.as_str()), Some("html"));
        assert_eq!(
            plugins.for_path(Path::new("a.md")),
            None,
            "Markdown is not a plugin's business"
        );
        // Every plugin ships an icon — see `every_bundled_plugin_carries_an_icon` — and every language
        // ships a colour scheme with it. A plugin that draws ships no scheme: a scheme it chose would
        // be the one thing a plugin is never allowed to decide. Its *rail* button is still
        // `pane.icon`, a drawn icon rather than a picture, so that button follows the window's
        // colours; the picture is what the marketplace row and its own page show.
        for plugin in plugins.all() {
            assert!(!plugin.description.is_empty(), "{} says nothing about itself", plugin.id);
            match plugin.kind {
                Kind::Language => {
                    assert!(plugin.icon.is_some(), "{} has no icon", plugin.id);
                    assert!(!plugin.theme.is_empty(), "{} has no colour scheme", plugin.id);
                }
                Kind::Ui => {
                    assert!(
                        plugin.theme.is_empty(),
                        "{} names colours, which no plugin may",
                        plugin.id
                    );
                    assert!(plugin.extensions.is_empty(), "{} claims a file type", plugin.id);
                }
                // A theme plugin is the one that *does* name colours, which is the whole of what it is
                // for. It claims no file type and contributes nothing to the window.
                Kind::Theme => {
                    assert!(!plugin.themes.is_empty(), "{} carries no themes", plugin.id);
                    assert!(plugin.extensions.is_empty(), "{} claims a file type", plugin.id);
                    assert!(
                        plugin.contributions.is_empty(),
                        "{} contributes to the window",
                        plugin.id
                    );
                }
            }
        }
    }

    #[test]
    fn the_css_plugin_reads_the_three_things_a_stylesheet_needs() {
        // `task-1671`. Each of the three is off unless a manifest asks for it, so this is also what
        // proves they reach the grammar at all rather than being read and dropped.
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let css = plugins.get("css").expect("the css plugin");
        assert!(css.claims(Path::new("site.css")));
        assert!(css.claims(Path::new("SITE.CSS")));
        assert!(!css.claims(Path::new("site.scss")), "Sass is a different language, deliberately");
        assert_eq!(css.grammar.word_characters, vec!['-', '@'], "a hyphen is a letter in CSS");
        assert!(css.grammar.hex_colors, "and #ff0000 is a number");
        assert!(css.grammar.line_comment.is_none(), "// is not a comment in CSS");
        assert!(css.grammar.types.contains(&"flex".to_owned()), "the third list is read");
        assert!(css.grammar.builtins.contains(&"background-color".to_owned()));
        assert!(css.grammar.keywords.contains(&"@media".to_owned()));
        assert_eq!(css.renders, None, "a stylesheet has no picture");

        // What that adds up to, read through the tokeniser the window uses.
        use unluminous_core::syntax::{highlight, Token};
        let text = "@media screen { .card { background-color: #ff79c6; display: flex; } }";
        let found: Vec<(&str, Token)> = highlight(text, &css.grammar)
            .into_iter()
            .map(|(range, token)| (&text[range], token))
            .collect();
        assert!(found.contains(&("@media", Token::Keyword)), "{found:?}");
        assert!(found.contains(&("background-color", Token::Builtin)), "{found:?}");
        assert!(found.contains(&("#ff79c6", Token::Number)), "{found:?}");
        assert!(found.contains(&("flex", Token::Type)), "{found:?}");
    }

    #[test]
    fn the_older_plugins_ask_for_none_of_what_css_added() {
        // The three keys are opt-in, which is what keeps a `.ts` file coloured exactly as it was.
        // Three plugins are let through and each says why in its own manifest: CSS reads a hyphen as a
        // letter and a `#ff0000` as a number, HTML reads a hyphen as a letter, and SQL uses the third
        // word list for its type names — `int`, `timestamptz` and the rest are neither keywords nor
        // functions, and colouring them as identifiers made a `CREATE TABLE` read as a list of column
        // names with nothing to tell the two halves apart.
        let (plugins, _) = Plugins::load(None);
        let older = |id: &str| id != "css" && id != "html" && id != "sql";
        for plugin in plugins.all().iter().filter(|plugin| older(&plugin.id)) {
            assert!(plugin.grammar.word_characters.is_empty(), "{}", plugin.id);
            assert!(!plugin.grammar.hex_colors, "{}", plugin.id);
            assert!(plugin.grammar.types.is_empty(), "{}", plugin.id);
        }
    }

    #[test]
    fn the_older_plugins_ask_for_none_of_what_the_symbols_added() {
        // The same rule for the two keys `task-1675` added: a language that does not name them is a
        // language nothing about it changed for. CSS and Mermaid are deliberately among them —
        // `--brand-hue: 280` defines a custom property by position rather than by keyword, and a
        // rule that read `:` as a definer would call every property a definition.
        let (plugins, _) = Plugins::load(None);
        for id in ["css", "mermaid"] {
            let plugin = plugins.get(id).expect(id);
            assert!(plugin.grammar.definers.is_empty(), "{id} names no definers");
            assert!(!plugin.grammar.brace_definitions, "{id} asks for no brace rule");
            assert!(!plugin.grammar.defines_symbols(), "so the entries are absent for {id}");
        }
    }

    #[test]
    fn the_four_code_plugins_say_how_their_imports_are_written() {
        // `task-1680`. The two families and what each one needs, read through the manifest reader
        // the window uses, so a key that never reached the grammar would fail here.
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        for id in ["typescript", "javascript"] {
            let grammar = &plugins.get(id).expect(id).grammar;
            assert_eq!(grammar.imports, Some(ImportStyle::Quoted), "{id}");
            assert!(grammar.import_keywords.contains(&"import".to_owned()), "{id}");
            assert!(grammar.import_omit_extension, "{id} writes ./layout, not ./layout.ts");
            assert_eq!(grammar.import_index, vec!["index".to_owned()], "{id}");
            assert_eq!(grammar.export_keyword.as_deref(), Some("export"), "{id}");
            assert!(grammar.completes_imports(), "{id}");
        }
        let css = &plugins.get("css").expect("css").grammar;
        assert_eq!(css.imports, Some(ImportStyle::Quoted));
        assert_eq!(css.import_keywords, vec!["@import".to_owned()]);
        assert!(!css.import_omit_extension, "a stylesheet names the file it imports");
        assert_eq!(css.export_keyword, None, "CSS declares nothing, so it hides nothing");

        let rust = &plugins.get("rust").expect("rust").grammar;
        assert_eq!(rust.imports, Some(ImportStyle::Path));
        assert_eq!(rust.path_separator.as_deref(), Some("::"));
        assert_eq!(rust.source_roots, vec!["src".to_owned()]);
        assert_eq!(rust.export_keyword.as_deref(), Some("pub"));
        assert_eq!(rust.path_root("crate"), Some(PathRoot::Package));
        assert_eq!(rust.path_root("self"), Some(PathRoot::Module));
        assert_eq!(rust.path_root("super"), Some(PathRoot::Parent));
        assert_eq!(rust.path_root("unluminous_core"), None, "a package is not a reserved word");
        assert_eq!(rust.import_index, vec!["mod".to_owned(), "lib".to_owned(), "main".to_owned()]);
    }

    #[test]
    fn the_older_plugins_ask_for_none_of_what_the_imports_added() {
        // The same rule once more, and Mermaid is what keeps it honest: a diagram imports nothing,
        // so nothing about it changed.
        let (plugins, _) = Plugins::load(None);
        let mermaid = &plugins.get("mermaid").expect("mermaid").grammar;
        assert_eq!(mermaid.imports, None);
        assert!(mermaid.import_keywords.is_empty());
        assert!(mermaid.import_extensions.is_empty());
        assert_eq!(mermaid.export_keyword, None);
        assert_eq!(mermaid.path_separator, None);
        assert!(mermaid.path_roots.is_empty());
        assert!(!mermaid.completes_imports(), "so no import is ever read out of a diagram");
    }

    /// The same rule a fifth time, for the two keys `task-1694` added. Every plugin that shipped
    /// before HTML names neither, so it is read by exactly the code that read it before — which is
    /// what keeps a `.ts` file, a stylesheet and a diagram all unchanged by a key they never asked
    /// for.
    #[test]
    fn the_older_plugins_ask_for_none_of_what_the_markup_added() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        for plugin in plugins.all().iter().filter(|plugin| plugin.id != "html") {
            assert!(!plugin.grammar.markup, "{} is not markup", plugin.id);
            assert!(plugin.grammar.raw_text.is_empty(), "{} names no raw text elements", plugin.id);
        }
    }

    /// The same rule a seventh time, for `task-1776`. Nothing that shipped before the themes carries a
    /// theme, so every one of them is read by exactly the code that read it before.
    #[test]
    fn the_older_plugins_ask_for_none_of_what_themes_added() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        for plugin in plugins.all().iter().filter(|plugin| plugin.kind != Kind::Theme) {
            assert!(plugin.themes.is_empty(), "{} carries no theme", plugin.id);
        }
    }

    /// `task-1694`. The two keys are opt-in, and this is what proves they reach the grammar at all
    /// rather than being read and dropped, the way the CSS test does for its three.
    #[test]
    fn the_html_plugin_reads_the_two_things_markup_needs() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let html = plugins.get("html").expect("the html plugin");
        assert!(html.claims(Path::new("page.html")));
        assert!(html.claims(Path::new("page.HTM")));
        assert!(!html.claims(Path::new("page.xml")), "XML is a different language, deliberately");
        assert!(html.grammar.markup, "the flag is read");
        assert!(
            html.grammar
                .raw_text
                .iter()
                .any(|(el, lang)| el == "script" && lang.as_deref() == Some("javascript")),
            "a script block holds javascript"
        );
        assert!(
            html.grammar
                .raw_text
                .iter()
                .any(|(el, lang)| el == "style" && lang.as_deref() == Some("css")),
            "a style block holds css"
        );
        assert!(
            html.grammar.raw_text.iter().any(|(el, lang)| el == "title" && lang.is_none()),
            "a title is escapable raw text, so it decodes its references"
        );
        assert_eq!(html.grammar.word_characters, vec!['-'], "a hyphen is a letter");
        assert_eq!(
            html.grammar.block_comment.as_ref(),
            Some(&("<!--".to_owned(), "-->".to_owned()))
        );
        assert!(html.grammar.keywords.contains(&"p".to_owned()), "an element name is a keyword");
        assert!(html.grammar.keywords.contains(&"DOCTYPE".to_owned()), "the declaration colours");
        assert!(
            html.grammar.builtins.contains(&"class".to_owned()),
            "an attribute name is a builtin"
        );
        assert!(
            html.grammar.types.is_empty(),
            "the third list is empty, and the type colour is the tag-name rule"
        );

        // What that adds up to, read through the tokeniser the window uses.
        use unluminous_core::syntax::{highlight, Token};
        let text = "<div class=\"card\">Tom &amp; Jerry 5 < 3</div>";
        let found: Vec<(&str, Token)> = highlight(text, &html.grammar)
            .into_iter()
            .map(|(range, token)| (&text[range], token))
            .collect();
        assert!(found.contains(&("div", Token::Keyword)), "the element name: {found:?}");
        assert!(found.contains(&("class", Token::Builtin)), "the attribute name: {found:?}");
        assert!(found.contains(&("\"card\"", Token::String)), "the value: {found:?}");
        assert!(found.contains(&("&amp;", Token::Number)), "the reference in prose: {found:?}");
        assert!(
            !found.iter().any(|(word, _)| *word == "Tom"
                || *word == "Jerry"
                || *word == "5"
                || *word == "3"),
            "a word of the prose is not coloured: {found:?}"
        );
    }

    /// The same rule a fourth time, for the key `task-1687` added. Mermaid and CSS name no debugger,
    /// so every debug control is **absent** for their files — which is Unluminous's rule for a control
    /// that can never apply, and is what keeps the key opt-in.
    #[test]
    fn the_older_plugins_ask_for_none_of_what_debugging_added() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        for id in ["mermaid", "css"] {
            let plugin = plugins.get(id).expect(id);
            assert_eq!(plugin.debug_adapter, None, "{id} names no debugger");
        }
        assert_eq!(plugins.debugger_for(Path::new("a.css")), None);
        assert_eq!(plugins.debugger_for(Path::new("a.mmd")), None);
        assert_eq!(plugins.debugger_for(Path::new("a.txt")), None, "and nothing claims a .txt");
    }

    /// The three that do name one, and the two shapes they name.
    #[test]
    fn the_languages_that_can_be_debugged_name_the_debugger_that_can_do_it() {
        let (plugins, _) = Plugins::load(None);
        assert_eq!(plugins.debugger_for(Path::new("src/main.rs")), Some("lldb"));
        assert_eq!(plugins.debugger_for(Path::new("server.js")), Some("node"));
        assert_eq!(plugins.debugger_for(Path::new("server.ts")), Some("node"));
        assert!(plugins.any_debugger());
    }

    #[test]
    fn the_three_code_plugins_say_which_keyword_defines_what() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let rust = plugins.get("rust").expect("rust");
        assert_eq!(rust.grammar.definer("fn"), Some(SymbolKind::Function));
        assert_eq!(rust.grammar.definer("struct"), Some(SymbolKind::Type));
        assert_eq!(rust.grammar.definer("let"), Some(SymbolKind::Variable));
        assert_eq!(rust.grammar.definer("mod"), Some(SymbolKind::Module));
        assert_eq!(rust.grammar.definer("const"), Some(SymbolKind::Constant));
        assert_eq!(rust.grammar.definer("impl"), None, "an impl block declares no name");
        assert!(!rust.grammar.brace_definitions, "Rust never hides a definition behind a brace");

        // JavaScript and TypeScript do, which is the whole reason the second key exists.
        for id in ["javascript", "typescript"] {
            let plugin = plugins.get(id).expect(id);
            assert_eq!(plugin.grammar.definer("function"), Some(SymbolKind::Function), "{id}");
            assert_eq!(plugin.grammar.definer("class"), Some(SymbolKind::Type), "{id}");
            assert_eq!(plugin.grammar.definer("const"), Some(SymbolKind::Variable), "{id}");
            assert!(plugin.grammar.brace_definitions, "{id} has methods with no keyword");
            assert!(plugin.grammar.defines_symbols());
        }
        // TypeScript adds the four words it has of its own.
        let typescript = plugins.get("typescript").expect("typescript");
        assert_eq!(typescript.grammar.definer("interface"), Some(SymbolKind::Type));
        assert_eq!(typescript.grammar.definer("enum"), Some(SymbolKind::Type));
        assert_eq!(typescript.grammar.definer("type"), Some(SymbolKind::Type));
        assert_eq!(typescript.grammar.definer("namespace"), Some(SymbolKind::Module));
        assert_eq!(plugins.get("javascript").expect("js").grammar.definer("interface"), None);
    }

    #[test]
    fn what_the_definers_add_up_to_read_through_the_reader_the_window_uses() {
        // The keys are data, so what proves they reached the grammar is what a file becomes.
        let (plugins, _) = Plugins::load(None);
        let rust = plugins.get("rust").expect("rust");
        let source = "pub fn draw(area: Rect) {}\npub struct Layout;\nconst LIMIT: usize = 4;\n";
        let found: Vec<(&str, SymbolKind)> =
            unluminous_core::symbols::file_definitions(source, &rust.grammar)
                .into_iter()
                .map(|definition| (&source[definition.name_range], definition.kind))
                .collect();
        assert!(found.contains(&("draw", SymbolKind::Function)), "{found:?}");
        assert!(found.contains(&("Layout", SymbolKind::Type)), "{found:?}");
        assert!(found.contains(&("LIMIT", SymbolKind::Constant)), "{found:?}");

        let typescript = plugins.get("typescript").expect("typescript");
        let source = "class Panel {\n  render(area: Rect) {\n    return area;\n  }\n}\n";
        let found: Vec<&str> =
            unluminous_core::symbols::file_definitions(source, &typescript.grammar)
                .into_iter()
                .map(|definition| &source[definition.name_range])
                .collect();
        assert_eq!(found, vec!["Panel", "render"], "the method has no keyword in front of it");
    }

    #[test]
    fn the_code_plugins_say_how_a_file_and_a_project_of_theirs_is_run() {
        // `task-1683` §8, and the answer to "should running node mean a Node plugin": no — node is
        // how JavaScript runs, and the JavaScript manifest says so itself.
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let javascript = plugins.get("javascript").expect("javascript");
        assert_eq!(javascript.run_file.as_deref(), Some("node {file}"));
        assert_eq!(javascript.run_project.as_deref(), Some("npm"));
        let typescript = plugins.get("typescript").expect("typescript");
        assert_eq!(typescript.run_file.as_deref(), Some("npx tsx {file}"));
        assert_eq!(typescript.run_project.as_deref(), Some("npm"));
        // Rust names a detector and no file runner: running one file of a Cargo project is not a
        // thing cargo does, so the entry is absent for a `.rs` file rather than offered and wrong.
        let rust = plugins.get("rust").expect("rust");
        assert_eq!(rust.run_file, None);
        assert_eq!(rust.run_project.as_deref(), Some("cargo"));

        // What that adds up to, asked the way the window asks it.
        assert_eq!(plugins.run_file(Path::new("server.js")), Some("node {file}"));
        assert_eq!(plugins.run_file(Path::new("main.rs")), None);
        assert_eq!(plugins.run_file(Path::new("notes.md")), None, "no plugin claims Markdown");
        // Named once, because JavaScript and TypeScript both being installed is not two projects.
        // In the order the plugins are held, which is by name, so it is the same on every run.
        assert_eq!(plugins.project_runners(), vec!["npm", "cargo"]);
    }

    #[test]
    fn the_older_plugins_ask_for_none_of_what_running_added() {
        // The rule every key since `task-1671` has followed: a language that names neither is a
        // language nothing about it changed for. CSS and Mermaid are what keep it honest — a
        // stylesheet is not run and neither is a diagram.
        let (plugins, _) = Plugins::load(None);
        for id in ["css", "mermaid"] {
            let plugin = plugins.get(id).expect(id);
            assert_eq!(plugin.run_file, None, "{id} runs no file");
            assert_eq!(plugin.run_project, None, "{id} detects no project");
        }
        assert_eq!(plugins.run_file(Path::new("site.css")), None);
        assert_eq!(plugins.run_file(Path::new("flow.mmd")), None);
    }

    #[test]
    fn the_mermaid_plugin_claims_diagram_files_and_names_a_renderer() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let mermaid = plugins.get("mermaid").expect("the mermaid plugin");
        assert_eq!(mermaid.kind, Kind::Language, "the seam is not widened: it is a language");
        assert!(mermaid.claims(Path::new("flow.mmd")));
        assert!(mermaid.claims(Path::new("flow.MERMAID")));
        assert!(!mermaid.claims(Path::new("notes.md")), "Markdown is not this plugin's business");
        assert_eq!(mermaid.renders.as_deref(), Some("mermaid"));
        assert!(plugins.renders("mermaid"), "the window asks this before it draws anything");
    }

    #[test]
    fn no_other_plugin_claims_to_render_anything() {
        let (plugins, _) = Plugins::load(None);
        for plugin in plugins.all().iter().filter(|plugin| plugin.id != "mermaid") {
            assert_eq!(plugin.renders, None, "{} should name no renderer", plugin.id);
        }
        assert!(!plugins.renders("something-else"), "a name nothing declares is not rendered");
    }

    // `task-1922`'s five: Python, JSON, TOML, YAML and shell. None of them names a manifest key
    // that did not already exist — `language.imports`, `language.definers`, `language.strings` and
    // the rest are all read by exactly the same reader that reads them for Rust and CSS — so there
    // is no new flag defaulting to off to prove is untouched for the plugins that shipped before, the way
    // `the_older_plugins_ask_for_none_of_what_css_added` and its siblings prove one for their own
    // round of keys. What those existing tests already do, by iterating every bundled plugin,
    // covers these five automatically: none of them sets `word_characters`, `hex_colors`, `types`,
    // `markup`, `raw_text` or `themes`, so every one of those tests passes over these five with no
    // change to its own body.

    #[test]
    fn the_five_new_language_plugins_load_and_claim_the_right_files() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let cases: &[(&str, &[&str], &[&str])] = &[
            ("python", &["a.py", "a.pyw", "A.PY"], &["a.pyc"]),
            ("json", &["a.json"], &["a.json5", "a.jsonc"]),
            ("toml", &["a.toml", "Cargo.toml"], &["a.tml"]),
            ("yaml", &["a.yaml", "a.yml"], &["a.yamlx"]),
            ("shell", &["a.sh", "a.bash", "a.zsh"], &["a.fish"]),
        ];
        for (id, claimed, not_claimed) in cases {
            let plugin = plugins.get(id).unwrap_or_else(|| panic!("the {id} plugin"));
            assert_eq!(plugin.kind, Kind::Language, "{id}");
            for path in *claimed {
                assert!(plugin.claims(Path::new(path)), "{id} should claim {path}");
            }
            for path in *not_claimed {
                assert!(!plugin.claims(Path::new(path)), "{id} should not claim {path}");
            }
        }
        // No two of the five, and none of the five against a plugin that already shipped, claim
        // the same extension - the collision every one of these was checked against by hand before
        // it was written down here.
        let every_file =
            ["a.py", "a.pyw", "a.json", "a.toml", "a.yaml", "a.yml", "a.sh", "a.bash", "a.zsh"];
        for file in every_file {
            let claiming: Vec<&str> = plugins
                .all()
                .iter()
                .filter(|plugin| plugin.claims(Path::new(file)))
                .map(|plugin| plugin.id.as_str())
                .collect();
            assert_eq!(
                claiming.len(),
                1,
                "{file} should be claimed by exactly one plugin: {claiming:?}"
            );
        }
    }

    /// The manifest format's own trap, found while writing these five: `services::store::Values`
    /// reads a `#` that ends a line, or is followed by whitespace, as the start of a comment **on
    /// the manifest line itself** - which is right for `size = 20  # a comment` and wrong for a
    /// value that is meant to be the character `#`, because a bare `language.line_comment = #`
    /// parses to an empty string rather than to `#`. An empty `line_comment` is worse than a
    /// missing one: `unluminous_core::syntax::comment` checks `rest.starts_with(opener)`, and every
    /// string starts with the empty string, so every position in every file would read as a
    /// comment running to the end of its line and nothing would ever be coloured as anything else.
    /// Each of the four manifests below writes the value as a doubled hash with a real trailing
    /// comment after it, which is what keeps the first `#` from being read as ending the line; this
    /// pins that the character each of them actually reads with is the one character `#`, and that
    /// it never regresses to the empty string the naive form would silently produce.
    #[test]
    fn a_bare_hash_line_comment_is_written_so_the_settings_parser_does_not_eat_it() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        for id in ["python", "toml", "yaml", "shell"] {
            let plugin = plugins.get(id).unwrap_or_else(|| panic!("the {id} plugin"));
            assert_eq!(
                plugin.grammar.line_comment.as_deref(),
                Some("#"),
                "{id}'s line comment should be exactly one character"
            );
        }
        // What that adds up to: a `#` really does start a comment, and nothing before it on the
        // same line is swallowed by it. `scan` rather than `highlight`, because `highlight` drops
        // `Token::Text` and the plain words either side of the comment are exactly what proves the
        // rest of the file was not swallowed with it.
        use unluminous_core::syntax::{scan, Token};
        let python = plugins.get("python").expect("python");
        let text = "x = 1  # a comment\ny = 2\n";
        let mut found: Vec<(&str, Token)> = Vec::new();
        scan(text, &python.grammar, |range, token| found.push((&text[range], token)));
        assert!(found.contains(&("x", Token::Text)), "{found:?}");
        assert!(found.contains(&("1", Token::Number)), "{found:?}");
        assert!(found.contains(&("# a comment", Token::Comment)), "{found:?}");
        assert!(found.contains(&("y", Token::Text)), "the next line is not swallowed: {found:?}");
        assert!(found.contains(&("2", Token::Number)), "{found:?}");
    }

    #[test]
    fn the_python_plugin_reads_keywords_definitions_and_the_docstring_gap() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let python = plugins.get("python").expect("the python plugin");
        assert_eq!(python.debug_adapter, None, "plugins::DEBUGGERS has no python entry");
        assert_eq!(python.grammar.definer("def"), Some(SymbolKind::Function));
        assert_eq!(python.grammar.definer("class"), Some(SymbolKind::Type));
        assert_eq!(python.grammar.imports, Some(ImportStyle::Path));
        assert_eq!(python.grammar.export_keyword, None, "nothing in Python hides a module name");

        use unluminous_core::syntax::{highlight, Token};
        let source = "def draw(area):\n    return area\n";
        let found: Vec<(&str, Token)> = highlight(source, &python.grammar)
            .into_iter()
            .map(|(range, token)| (&source[range], token))
            .collect();
        assert!(found.contains(&("def", Token::Keyword)), "{found:?}");
        assert!(found.contains(&("draw", Token::Function)), "{found:?}");
        assert!(found.contains(&("return", Token::Keyword)), "{found:?}");

        // A one-line docstring is unaffected, because it never crosses a line break.
        let one_line = "\"\"\"One line.\"\"\"\n";
        let found: Vec<(&str, Token)> = highlight(one_line, &python.grammar)
            .into_iter()
            .map(|(range, token)| (&one_line[range], token))
            .collect();
        assert!(
            found.iter().any(|(word, token)| word.contains("One line") && *token == Token::String),
            "a one-line docstring still reads as a string: {found:?}"
        );

        // A multi-line docstring is the gap named in `plugin.limitations`: the tokeniser's string
        // rule ends at the first line break for every quote but a backtick, so the body is read as
        // ordinary words rather than as one string. This is checked rather than assumed, so a later
        // change to `syntax.rs` that fixes it is noticed here rather than left stale in a comment.
        let multi_line = "\"\"\"\nSecond line.\n\"\"\"\n";
        let found: Vec<(&str, Token)> = highlight(multi_line, &python.grammar)
            .into_iter()
            .map(|(range, token)| (&multi_line[range], token))
            .collect();
        assert!(
            !found.iter().any(|(word, token)| *word == "Second" && *token == Token::String),
            "the docstring body is not read as a string, which is the known gap: {found:?}"
        );
    }

    #[test]
    fn the_json_plugin_reads_a_string_with_an_escape_and_the_three_literals() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let json = plugins.get("json").expect("the json plugin");
        assert!(json.grammar.line_comment.is_none(), "JSON has no comments");
        assert!(json.grammar.block_comment.is_none());

        use unluminous_core::syntax::{highlight, Token};
        // An escaped Rust string rather than a raw one: the source has to hold a literal backslash
        // in front of the `n` and in front of two of the quotes, exactly as a `.json` file on disk
        // would, and a raw string cannot end in the same character its own closing quote is without
        // one more quote added to tell the two apart.
        let source =
            "{\"name\": \"line1\\nline2 \\\"quoted\\\"\", \"n\": -1.5e10, \"ok\": true, \"gone\": null}";
        let found: Vec<(&str, Token)> = highlight(source, &json.grammar)
            .into_iter()
            .map(|(range, token)| (&source[range], token))
            .collect();
        assert!(
            found.contains(&("\"line1\\nline2 \\\"quoted\\\"\"", Token::String)),
            "the escape inside the string does not end it early: {found:?}"
        );
        assert!(found.contains(&("true", Token::Builtin)), "{found:?}");
        assert!(found.contains(&("null", Token::Builtin)), "{found:?}");
    }

    #[test]
    fn the_toml_plugin_reads_a_section_header_and_the_date_gap() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let toml = plugins.get("toml").expect("the toml plugin");
        assert!(
            toml.grammar.definers.is_empty(),
            "a key is defined by being written, not a keyword"
        );

        use unluminous_core::syntax::{highlight, Token};
        let source = "[server.production]\nport = 8080\n";
        let found: Vec<(&str, Token)> = highlight(source, &toml.grammar)
            .into_iter()
            .map(|(range, token)| (&source[range], token))
            .collect();
        assert!(found.contains(&("[", Token::Operator)), "{found:?}");
        assert!(found.contains(&(".", Token::Operator)), "{found:?}");
        assert!(found.contains(&("]", Token::Operator)), "{found:?}");
        assert!(found.contains(&("=", Token::Operator)), "{found:?}");
        assert!(found.contains(&("8080", Token::Number)), "{found:?}");

        // The date gap named in `plugin.limitations`: a letter directly after a digit is read as
        // the number's own suffix, so a date's letters and digits are swept together rather than
        // told apart from the punctuation around them.
        let date = "when = 1979-05-27T07:32:00Z\n";
        let found: Vec<(&str, Token)> = highlight(date, &toml.grammar)
            .into_iter()
            .map(|(range, token)| (&date[range], token))
            .collect();
        assert!(
            found.iter().any(|(word, token)| word.contains('T') && *token == Token::Number),
            "the letter after a digit is read as part of the number, which is the known gap: {found:?}"
        );
    }

    #[test]
    fn the_yaml_plugin_reads_a_bare_word_as_the_same_kind_of_word_a_key_is() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let yaml = plugins.get("yaml").expect("the yaml plugin");

        // `scan` rather than `highlight`, because `highlight` drops `Token::Text` and a plain bare
        // word reading as text is exactly the awkward case this test is about.
        use unluminous_core::syntax::{scan, Token};
        let source = "name: Jason\nactive: true\ncount: 3\nnickname: ~\n";
        let mut found: Vec<(&str, Token)> = Vec::new();
        scan(source, &yaml.grammar, |range, token| found.push((&source[range], token)));
        // The awkward case, read through the tokeniser rather than only described: a bare key and a
        // bare value are not told apart, and a value starting with a capital letter reads as a type
        // by the same heuristic every language here shares.
        assert!(found.contains(&("name", Token::Text)), "a lowercase bare word is text: {found:?}");
        assert!(
            found.contains(&("Jason", Token::Type)),
            "a bare word starting with a capital reads as a type: {found:?}"
        );
        assert!(found.contains(&("true", Token::Builtin)), "{found:?}");
        assert!(found.contains(&("3", Token::Number)), "{found:?}");
        assert!(
            found.contains(&("~", Token::Operator)),
            "null written as ~ is not a word: {found:?}"
        );
    }

    #[test]
    fn the_shell_plugin_reads_a_variable_a_builtin_and_a_posix_function() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let shell = plugins.get("shell").expect("the shell plugin");
        assert!(shell.grammar.definers.is_empty(), "no keyword-based definer is named");
        assert!(shell.grammar.brace_definitions, "a POSIX function has no keyword in front of it");

        use unluminous_core::syntax::{highlight, Token};
        let source = "echo \"hello $HOME and $USER and $NOBODY\"\n";
        let found: Vec<(&str, Token)> = highlight(source, &shell.grammar)
            .into_iter()
            .map(|(range, token)| (&source[range], token))
            .collect();
        assert!(found.contains(&("echo", Token::Builtin)), "{found:?}");
        // The whole string is one token: `$HOME` and `$USER` are inside it and are not coloured
        // separately from the string, which is what a shell's real double-quoted string is - a
        // variable inside a string is not read here, and this is checked rather than assumed.
        assert!(
            found.iter().any(|(word, token)| word.contains("$HOME") && *token == Token::String),
            "{found:?}"
        );

        // A bare `$HOME` outside a string is one word and is named in `language.builtins`.
        let bare = "cd $HOME\n";
        let found: Vec<(&str, Token)> = highlight(bare, &shell.grammar)
            .into_iter()
            .map(|(range, token)| (&bare[range], token))
            .collect();
        assert!(found.contains(&("$HOME", Token::Builtin)), "{found:?}");

        // `foo() { ... }`, found through `unluminous_core::symbols::file_definitions` exactly as a
        // JavaScript class method is - the same rule, applied to the shape most shell functions are
        // actually written in.
        let source = "foo() {\n    echo hi\n}\n";
        let found: Vec<&str> = unluminous_core::symbols::file_definitions(source, &shell.grammar)
            .into_iter()
            .map(|definition| &source[definition.name_range])
            .collect();
        assert_eq!(found, vec!["foo"], "the POSIX function shape is found with no keyword");
    }
}
