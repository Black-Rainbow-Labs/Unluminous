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
}
