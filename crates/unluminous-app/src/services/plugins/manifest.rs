//! Turning a `plugin.conf` into a `Plugin`: every key Unluminous reads, and the checks that refuse a
//! manifest asking for something this version cannot do rather than half loading it.
//!
//! [`parse`] is the one entry point; everything else here is one manifest key or one small family
//! of them. `KNOWN_KEYS`, [`only_known_keys`] and [`letters_apart`] are kept together because they
//! are one feature — a misspelt key refused by name — and belong beside the parser whose keys they
//! check.

use unluminous_core::symbols::SymbolKind;
use unluminous_core::syntax::{Grammar, ImportStyle, PathRoot, Token};
use unluminous_core::Color;

use super::registries::{
    checked_against, optional_registry_entry, CHROME, DEBUGGERS, PANE_CONDITIONS, PANE_ICONS,
    PROJECT_RUNNERS, RENDERERS, UI_PROVIDERS,
};
use super::theme::themes;
use super::types::{
    Contributions, Kind, MenuContribution, MenuItem, PageContribution, PaneContribution, Plugin,
    RailGroup, SyntaxTheme, TabContribution,
};
use crate::services::store::Values;

/// Turn a manifest into a plugin.
pub fn parse(values: &Values, bundled: bool) -> Result<Plugin, String> {
    let id = values.text("plugin.id").ok_or("plugin.id is missing")?.to_owned();
    // Checked rather than assumed, so a manifest for something Unluminous cannot run is refused with a
    // message instead of loading as half a language.
    let kind = match values.text("plugin.kind").unwrap_or("language") {
        "language" => Kind::Language,
        "ui" => Kind::Ui,
        "theme" => Kind::Theme,
        other => {
            return Err(format!(
                "plugin.kind is `{other}`, and this version of Unluminous runs `language`, `ui` and `theme` plugins"
            ))
        }
    };
    let extensions: Vec<String> = list(values, "language.extensions")
        .into_iter()
        .map(|extension| extension.trim_start_matches('.').to_lowercase())
        .collect();
    // A language claiming no file type would never be used, which is what this has always said. A UI
    // plugin claims none by construction — Agent-Tasks is not a file type — so the check belongs to the
    // kind rather than to every manifest.
    if kind == Kind::Language && extensions.is_empty() {
        return Err(
            "language.extensions is empty, so nothing would ever use this plugin".to_owned()
        );
    }
    let contributions = contributions(values, kind)?;
    // Checked against what this version can actually draw, for the same reason `plugin.kind` is: a
    // manifest naming a picture Unluminous does not have should say so rather than load as a language
    // whose files silently never draw.
    let renders =
        match values.text("language.renders").map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => {
                checked_against(
                    "language.renders",
                    name,
                    RENDERERS.contains(&name),
                    RENDERERS,
                    "draws",
                )?;
                Some(name.to_owned())
            }
            None => None,
        };
    let name = values.text("plugin.name").unwrap_or(&id).to_owned();
    let grammar = Grammar {
        language: name.clone(),
        keywords: list(values, "language.keywords"),
        builtins: list(values, "language.builtins"),
        types: list(values, "language.types"),
        line_comment: values.text("language.line_comment").map(str::to_owned),
        block_comment: pair(values, "language.block_comment"),
        strings: values
            .text("language.strings")
            .unwrap_or("\", '")
            .split(',')
            .filter_map(|quote| quote.trim().chars().next())
            .collect(),
        escapes: values.flag("language.escapes").unwrap_or(true),
        operators: values.text("language.operators").unwrap_or_default().chars().collect(),
        numbers: values.flag("language.numbers").unwrap_or(true),
        // Comma separated single characters, the way `language.strings` names its quotes. Empty for
        // every language but CSS, where a hyphen is a letter.
        word_characters: values
            .text("language.word_characters")
            .unwrap_or_default()
            .split(',')
            .filter_map(|character| character.trim().chars().next())
            .collect(),
        hex_colors: values.flag("language.hex_colors").unwrap_or(false),
        // The two `task-1675` added, both off unless a language asks for them, which is the rule
        // every key added since `task-1671` has followed and which
        // `the_older_plugins_ask_for_none_of_what_the_symbols_added` keeps.
        definers: definers(values)?,
        brace_definitions: values.flag("language.brace_definitions").unwrap_or(false),
        // The nine `task-1680` added, and the same rule again: a plugin that names none of them
        // behaves exactly as it did before, which
        // `the_older_plugins_ask_for_none_of_what_the_imports_added` keeps.
        export_keyword: word(values, "language.export_keyword"),
        imports: import_style(values)?,
        import_keywords: list(values, "language.import_keywords"),
        import_extensions: list(values, "language.import_extensions")
            .into_iter()
            .map(|extension| match extension.starts_with('.') {
                true => extension,
                false => format!(".{extension}"),
            })
            .collect(),
        import_index: list(values, "language.import_index"),
        import_omit_extension: values.flag("language.import_omit_extension").unwrap_or(false),
        path_separator: word(values, "language.path_separator"),
        source_roots: list(values, "language.source_roots"),
        path_roots: path_roots(values)?,
        // The two `task-1694` added, and the same rule a sixth time: a language that names neither
        // is read by exactly the code that read it before, which
        // `the_older_plugins_ask_for_none_of_what_the_markup_added` keeps.
        markup: values.flag("language.markup").unwrap_or(false),
        raw_text: raw_text(values)?,
    };
    // A `theme` plugin's `theme.` keys are its themes, one group each, so the flat scheme a language
    // carries is not read for it — `theme.dracula.syntax.keyword` is not `theme.keyword`, and reading both
    // out of one prefix would be one namespace meaning two things.
    let colours: Vec<(Token, Color)> = match kind {
        Kind::Theme => Vec::new(),
        _ => Token::ALL
            .into_iter()
            .filter_map(|token| {
                let value = values.text(&format!("theme.{}", token.name()))?;
                colour(value).map(|colour| (token, colour))
            })
            .collect(),
    };
    let themes = themes(values, &id, kind)?;
    let syntax_scheme_name = match kind {
        Kind::Theme => name.clone(),
        _ => values.text("theme.name").unwrap_or("Dracula").to_owned(),
    };
    Ok(Plugin {
        id,
        name,
        version: values.text("plugin.version").unwrap_or("1.0.0").to_owned(),
        vendor: values.text("plugin.vendor").unwrap_or("Unluminous").to_owned(),
        description: values.text("plugin.description").unwrap_or_default().to_owned(),
        limitations: values.text("plugin.limitations").unwrap_or_default().to_owned(),
        kind,
        contributions,
        extensions,
        renders,
        run_file: run_file(values)?,
        run_project: run_project(values)?,
        debug_adapter: debug_adapter(values)?,
        grammar,
        theme: SyntaxTheme::of(syntax_scheme_name, colours),
        themes,
        icon: None,
        bundled,
        enabled: true,
    })
}
/// The `ui.`, `pane.`, `tab.`, `menu.` and `settings.` keys: what a plugin adds to the window.
///
/// Every one of them is refused with a sentence naming what was asked for and what this version has,
/// which is the rule `plugin.kind`, `language.renders`, `run.project` and `debug.adapter` already keep.
/// A `language` manifest that names none of these parses exactly as it did before, which
/// `the_older_plugins_ask_for_none_of_what_the_ui_added` keeps.
fn contributions(values: &Values, kind: Kind) -> Result<Contributions, String> {
    let provider = match word(values, "ui.provider") {
        Some(named) => {
            checked_against(
                "ui.provider",
                &named,
                UI_PROVIDERS.contains(&named.as_str()),
                UI_PROVIDERS,
                "has",
            )?;
            Some(named)
        }
        None if kind == Kind::Ui => {
            return Err(
                "ui.provider is missing, and a ui plugin with no provider would draw nothing"
                    .to_owned(),
            )
        }
        None => None,
    };
    let chrome = optional_registry_entry(word(values, "ui.chrome"), "ui.chrome", CHROME, "has")?;
    // A renderer with nothing to draw with it. `ui.chrome` says *how* a plugin's own pane is decorated,
    // and a language plugin has no pane — so this is a line that would do nothing, silently, which is what
    // every refusal in this function exists to prevent.
    if kind != Kind::Ui && chrome.is_some() {
        return Err(
            "ui.chrome is set on a plugin that is not a `ui` plugin, so there is no pane for it to draw"
                .to_owned(),
        );
    }
    let found = Contributions {
        provider,
        chrome,
        pane: pane(values)?,
        tab: tab(values),
        menu: menu(values)?,
        page: page(values)?,
    };
    // A key that asks for something the manifest did not declare is a line that does nothing, and a line
    // that does nothing silently is what every refusal here exists to prevent.
    only_known_keys(values)?;
    no_orphans(values, "pane.", found.pane.is_some(), "pane.id")?;
    no_orphans(values, "tab.", found.tab.is_some(), "tab.id")?;
    no_orphans(values, "settings.", found.page.is_some(), "settings.page")?;
    no_orphans(values, "menu.", found.menu.is_some(), "menu.name")?;
    // A plugin adding no button, no pane, no tab, no menu and no settings page has no way of being
    // reached, so it is refused rather than installed as a row in a list that does nothing.
    if kind == Kind::Ui && found.is_empty() {
        return Err(
            "a ui plugin contributes nothing, so there would be no way to reach it: name a pane, a tab, a menu or a settings page"
                .to_owned(),
        );
    }
    if kind != Kind::Ui && !found.is_empty() {
        return Err(format!(
            "plugin.kind is `{}` and the manifest contributes to the window, which only a `ui` plugin does",
            kind.name()
        ));
    }
    // A plugin that is not a `ui` one and names a provider is refused too, even though it contributes
    // nothing: it asked for code that only a `ui` plugin runs, and loading it would leave the provider
    // unreachable.
    if kind != Kind::Ui && found.provider.is_some() {
        return Err(format!(
            "plugin.kind is `{}` and it names a ui.provider, which only a `ui` plugin has",
            kind.name()
        ));
    }
    // And a plugin that draws must not carry a language's keys. A manifest naming both was read as a UI
    // plugin and its grammar, its renderer, its runner and its debugger were all silently dropped, which is
    // the outcome every other check here exists to prevent. A `theme` plugin is held to the same rule for
    // the same reason, minus `theme.name`, which is a theme's own group there.
    if kind != Kind::Language {
        let language_only = [
            "language.extensions",
            "language.renders",
            "language.keywords",
            "language.line_comment",
            "language.definers",
            "language.imports",
            "run.file",
            "run.project",
            "debug.adapter",
        ];
        for named in language_only.into_iter().chain(match kind {
            Kind::Ui => Some("theme.name"),
            _ => None,
        }) {
            if values.text(named).map(str::trim).is_some_and(|value| !value.is_empty()) {
                return Err(format!(
                    "plugin.kind is `{}` and the manifest sets {named}, which only a `language` plugin has",
                    kind.name()
                ));
            }
        }
    }
    Ok(found)
}

/// `pane.*`: the button in the rail, and the pane it opens.
///
/// Five of the six keys have a default, so a manifest asking for a pane writes two lines. The
/// defaults are the explorer's width and the terminal's height, because those are the two numbers the
/// window already uses for a column at the side and a strip along the bottom.
fn pane(values: &Values) -> Result<Option<PaneContribution>, String> {
    let Some(id) = word(values, "pane.id") else {
        return Ok(None);
    };
    let group = match values.text("pane.group").map(str::trim).unwrap_or("top") {
        "top" => RailGroup::Top,
        "bottom" => RailGroup::Bottom,
        other => return Err(format!("pane.group is `{other}`, and the rail has top and bottom")),
    };
    let named_side = values.text("pane.side").map(str::trim).unwrap_or("right");
    let side = crate::app::dock::Side::from_name(named_side).ok_or_else(|| {
        format!("pane.side is `{named_side}`, and a panel docks to left, right, top or bottom")
    })?;
    let icon = match word(values, "pane.icon") {
        Some(named) => {
            checked_against(
                "pane.icon",
                &named,
                PANE_ICONS.contains(&named.as_str()),
                PANE_ICONS,
                "draws",
            )?;
            named
        }
        None => "board".to_owned(),
    };
    let applies = match word(values, "pane.applies") {
        Some(named) => {
            checked_against(
                "pane.applies",
                &named,
                PANE_CONDITIONS.contains(&named.as_str()),
                PANE_CONDITIONS,
                "knows",
            )?;
            named
        }
        None => "always".to_owned(),
    };
    Ok(Some(PaneContribution {
        label: word(values, "pane.label")
            .or_else(|| word(values, "plugin.name"))
            .unwrap_or_else(|| id.clone()),
        id,
        icon,
        group,
        // A pane that says nothing is a tile when its button is in the bottom group, which is what
        // `pane.group` alone used to decide — see [`PaneContribution::tile`] for why the two keys
        // came apart and why the default has to be exactly this.
        tile: values.flag("pane.tile").unwrap_or(group == RailGroup::Bottom),
        side,
        width: measurement(values, "pane.width", 320.0)?,
        height: measurement(values, "pane.height", 260.0)?,
        applies,
    }))
}

/// `tab.*`: a tab in the editing area, opened from a menu, from its button in the rail, or by an action.
fn tab(values: &Values) -> Option<TabContribution> {
    let id = word(values, "tab.id")?;
    Some(TabContribution {
        label: word(values, "tab.label")
            .or_else(|| word(values, "plugin.name"))
            .unwrap_or_else(|| id.clone()),
        // The same drawn icons `pane.icon` names, from the same list, because a button in the rail is a
        // button in the rail whether it opens a pane or a tab. A tab that names none gets `board`, which
        // is what `activity_bar::pane_icon` falls back to anyway — said here as well so the manifest can
        // be read without knowing that.
        icon: word(values, "tab.icon").unwrap_or_else(|| "board".to_owned()),
        id,
    })
}

/// `menu.*`: the plugin's own menu, its entries, and any submenus nested inside it.
///
/// `menu.entries` is a comma list of `command=Name`, and a lone `-` is a separator.
/// `menu.submenu.<id>` names a submenu and `menu.submenu.<id>.entries` fills it, so
/// `menu.submenu.new.submenu.other` is a submenu inside a submenu and the reader is recursive.
fn menu(values: &Values) -> Result<Option<MenuContribution>, String> {
    let Some(name) = word(values, "menu.name") else {
        return Ok(None);
    };
    Ok(Some(MenuContribution { name, items: menu_items(values, "menu")? }))
}

/// The entries under one `menu.` or `menu.submenu.<id>.` prefix, and the submenus under it.
fn menu_items(values: &Values, prefix: &str) -> Result<Vec<MenuItem>, String> {
    let mut items = Vec::new();
    for entry in list(values, &format!("{prefix}.entries")) {
        if entry == "-" {
            items.push(MenuItem::Separator);
            continue;
        }
        let Some((command, label)) = entry.split_once('=') else {
            return Err(format!("{prefix}.entries holds `{entry}`, which is not `command=Name`"));
        };
        let command = command.trim();
        let label = label.trim();
        if command.is_empty() || label.is_empty() {
            return Err(format!("{prefix}.entries holds `{entry}`, which is not `command=Name`"));
        }
        items.push(MenuItem::Command { command: command.to_owned(), label: label.to_owned() });
    }
    // The submenus, in the order the manifest names them, which `Values` keeps sorted so that a menu
    // is the same shape every time it is read.
    for (key, label) in values.starting_with(&format!("{prefix}.submenu.")) {
        // `menu.submenu.new` names one; `menu.submenu.new.entries` fills it and is not a name.
        if key.contains('.') {
            continue;
        }
        let label = label.trim();
        if label.is_empty() {
            return Err(format!("{prefix}.submenu.{key} has no name"));
        }
        let nested = menu_items(values, &format!("{prefix}.submenu.{key}"))?;
        if nested.is_empty() {
            return Err(format!("{prefix}.submenu.{key} is empty, so it would open onto nothing"));
        }
        items.push(MenuItem::Submenu { label: label.to_owned(), items: nested });
    }
    Ok(items)
}

/// `settings.*`: the plugin's page in the Settings window.
fn page(values: &Values) -> Result<Option<PageContribution>, String> {
    let Some(name) = word(values, "settings.page") else {
        return Ok(None);
    };
    let icon = match word(values, "settings.icon") {
        Some(named) => {
            checked_against(
                "settings.icon",
                &named,
                PANE_ICONS.contains(&named.as_str()),
                PANE_ICONS,
                "draws",
            )?;
            named
        }
        None => "board".to_owned(),
    };
    Ok(Some(PageContribution { name, icon }))
}

/// A number from the manifest, with the default when it is absent and a refusal when it is not a
/// number, because a width of `wide` silently becoming 320 is the outcome every check here prevents.
fn measurement(values: &Values, name: &str, default: f32) -> Result<f32, String> {
    match values.text(name).map(str::trim).filter(|text| !text.is_empty()) {
        // A width of `wide` silently becoming 320 is the outcome every check here exists to prevent, so it
        // is refused with what it said and what a width is.
        Some(text) => match text.parse::<f32>() {
            Ok(number) if number > 0.0 => Ok(number),
            _ => Err(format!(
                "{name} is `{text}`, and a measurement is a number of points above zero"
            )),
        },
        None => Ok(default),
    }
}

/// Every key Unluminous reads from a manifest, in the namespaces whose keys are a fixed list.
///
/// `task-1922` B13. `no_orphans` covered `pane.`, `tab.`, `settings.` and `menu.` and nothing else,
/// and every key in the four namespaces below is read with `word()`, `list()` or `flag()`, each of
/// which answers with nothing for a name that is not there. So `language.keywrods = fn, let` loaded
/// as a language with no keywords at all, and said nothing: the file opened, it was simply not
/// coloured, and the manifest looked right.
///
/// `menu.` and `theme.` are deliberately not here. A menu's keys are recursive --
/// `menu.submenu.<id>.submenu.<other>.entries` is a submenu inside a submenu -- so there is no list
/// to check against; and a theme's are its own roles, which `read_theme` already refuses by name
/// with the list of roles Unluminous has. Both are checked, just not by this.
const KNOWN_KEYS: &[(&str, &[&str])] = &[
    ("plugin.", &["id", "name", "kind", "version", "vendor", "description", "limitations", "conf"]),
    (
        "language.",
        &[
            "extensions",
            "keywords",
            "builtins",
            "types",
            "operators",
            "numbers",
            "strings",
            "escapes",
            "line_comment",
            "block_comment",
            "word_characters",
            "hex_colors",
            "markup",
            "raw_text",
            "renders",
            "definers",
            "brace_definitions",
            "export_keyword",
            "imports",
            "import_keywords",
            "import_extensions",
            "import_index",
            "import_omit_extension",
            "path_separator",
            "source_roots",
            "path_roots",
        ],
    ),
    ("run.", &["file", "project"]),
    ("debug.", &["adapter"]),
    ("ui.", &["provider", "chrome"]),
    ("pane.", &["id", "label", "icon", "side", "group", "tile", "width", "height", "applies"]),
    ("tab.", &["id", "label", "icon"]),
    ("settings.", &["page", "icon"]),
];

/// Refuse a key in one of [`KNOWN_KEYS`]'s namespaces that Unluminous does not read.
///
/// The refusal names the nearest key it does read, because a misspelling is what this is for and a
/// list of twenty-six names is not an answer to one typed letter. When nothing is near enough, the
/// whole list of that namespace is given instead.
fn only_known_keys(values: &Values) -> Result<(), String> {
    for (prefix, known) in KNOWN_KEYS {
        for (rest, _) in values.starting_with(prefix) {
            let leaf = rest.split('.').next().unwrap_or(&rest);
            if known.contains(&leaf) {
                continue;
            }
            let nearest = known
                .iter()
                .map(|name| (letters_apart(leaf, name), *name))
                .filter(|(apart, _)| *apart * 3 <= leaf.len().max(1))
                .min_by_key(|(apart, _)| *apart);
            return Err(match nearest {
                Some((_, name)) => format!(
                    "the manifest sets {prefix}{rest}, which Unluminous does not read. Did it mean \
                     {prefix}{name}?"
                ),
                None => format!(
                    "the manifest sets {prefix}{rest}, which Unluminous does not read. It reads {}",
                    known
                        .iter()
                        .map(|name| format!("{prefix}{name}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
    }
    Ok(())
}

/// How many single-letter changes turn `one` into `other`: the ordinary edit distance.
///
/// Two rows of a table rather than the whole of it, because the names being compared are short and
/// the answer only ever needs the row before.
fn letters_apart(one: &str, other: &str) -> usize {
    let one: Vec<char> = one.chars().collect();
    let other: Vec<char> = other.chars().collect();
    let mut previous: Vec<usize> = (0..=other.len()).collect();
    let mut current = vec![0; other.len() + 1];
    for (row, letter) in one.iter().enumerate() {
        current[0] = row + 1;
        for (column, theirs) in other.iter().enumerate() {
            let substitution = previous[column] + usize::from(letter != theirs);
            current[column + 1] =
                substitution.min(previous[column + 1] + 1).min(current[column] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[other.len()]
}

/// Refuse a `pane.`, `tab.` or `settings.` key on a manifest that asks for no such contribution.
///
/// `pane.width` with no `pane.id` is a line somebody wrote expecting it to do something, and it does
/// nothing. Saying so is the difference between a manifest that is wrong and a manifest that is wrong and
/// silent about it.
fn no_orphans(values: &Values, prefix: &str, present: bool, needs: &str) -> Result<(), String> {
    if present {
        return Ok(());
    }
    let orphans = values.starting_with(prefix);
    match orphans.first() {
        Some((rest, _)) => Err(format!(
            "the manifest sets {prefix}{rest} and has no {needs}, so nothing would read it"
        )),
        None => Ok(()),
    }
}

/// `run.file`: the command that runs one file of this language, with `{file}` for the path.
///
/// The placeholder is **required**, because a template without it would run the same file whatever
/// tab was open — a manifest that appears to work and quietly does the wrong thing, which is the
/// one outcome every other check in this file exists to prevent. Nothing else about it is checked:
/// it is a command line, and `services::run_configurations::split_command` reads it the way it
/// reads every other one.
fn run_file(values: &Values) -> Result<Option<String>, String> {
    let Some(template) = word(values, "run.file") else {
        return Ok(None);
    };
    if !template.contains(crate::services::run_configurations::FILE_PLACEHOLDER) {
        return Err(format!(
            "run.file is `{template}`, which has no {} in it, so it would run the same file whatever was open",
            crate::services::run_configurations::FILE_PLACEHOLDER
        ));
    }
    Ok(Some(template))
}

/// `run.project`: the name of a project detector built into Unluminous.
///
/// Checked against [`PROJECT_RUNNERS`] exactly as `language.renders` is checked against
/// [`RENDERERS`]. **Nothing in a plugin is executed**: the manifest says "a project of this
/// language announces itself, and this is which detector notices", and the code that reads
/// `Cargo.toml` and `package.json` shipped with the binary. The most a third-party manifest can do
/// is suggest text, visibly.
fn run_project(values: &Values) -> Result<Option<String>, String> {
    optional_registry_entry(word(values, "run.project"), "run.project", PROJECT_RUNNERS, "detects")
}

/// `debug.adapter`: the name of a debugger built into Unluminous.
///
/// Checked against [`DEBUGGERS`] exactly as `run.project` is checked against [`PROJECT_RUNNERS`],
/// and the refusal reads the same way, because it is the same decision made a third time: the
/// manifest says "files of this language can be debugged, and this is which debugger knows how", and
/// the code that finds and drives that debugger shipped with the binary.
fn debug_adapter(values: &Values) -> Result<Option<String>, String> {
    optional_registry_entry(word(values, "debug.adapter"), "debug.adapter", DEBUGGERS, "drives")
}

/// `language.definers`: a comma list of `keyword=kind` saying which keyword makes the word after
/// it a definition, and of what.
///
/// The kind is checked against what `unluminous_core::symbols` actually has, for the same reason
/// `plugin.kind` and `language.renders` are checked: a manifest asking for something this version
/// does not know should say so plainly rather than load as a language whose declarations are
/// quietly never found. An entry that is not a pair is refused for the same reason — silently
/// dropping it would leave a language half able to answer.
fn definers(values: &Values) -> Result<Vec<(String, SymbolKind)>, String> {
    let mut found = Vec::new();
    for (keyword, kind) in pairs(values, "language.definers") {
        let Some(kind) = kind else {
            return Err(format!(
                "language.definers holds `{keyword}`, which is not `keyword=kind`"
            ));
        };
        let Some(parsed) = SymbolKind::parse(&kind) else {
            let known: Vec<&str> = SymbolKind::ALL.iter().map(|kind| kind.name()).collect();
            return Err(format!(
                "language.definers says `{keyword}={kind}`, and a definition in Unluminous is one of {}",
                known.join(", ")
            ));
        };
        if keyword.is_empty() {
            return Err(format!(
                "language.definers holds `{keyword}={kind}`, which names no keyword"
            ));
        }
        found.push((keyword, parsed));
    }
    Ok(found)
}

/// `language.imports`: which of the two shapes of import this language writes.
///
/// Checked against what this version can actually read, for the same reason `plugin.kind`,
/// `language.renders` and `language.definers` are: a manifest asking for a third shape should say
/// so plainly rather than load as a language whose imports quietly never complete.
fn import_style(values: &Values) -> Result<Option<ImportStyle>, String> {
    let Some(named) = word(values, "language.imports") else {
        return Ok(None);
    };
    match ImportStyle::parse(&named) {
        Some(style) => Ok(Some(style)),
        None => {
            let known: Vec<&str> = ImportStyle::ALL.iter().map(|style| style.name()).collect();
            Err(format!(
                "language.imports is `{named}`, and an import in Unluminous is written {}",
                known.join(" or ")
            ))
        }
    }
}

/// `language.path_roots`: a comma list of `word=meaning` naming the segments of a module path that
/// are not module names — `crate=package, self=module, super=parent`.
fn path_roots(values: &Values) -> Result<Vec<(String, PathRoot)>, String> {
    let mut found = Vec::new();
    for (word, meaning) in pairs(values, "language.path_roots") {
        let Some(meaning) = meaning else {
            return Err(format!("language.path_roots holds `{word}`, which is not `word=meaning`"));
        };
        let Some(parsed) = PathRoot::parse(&meaning) else {
            let known: Vec<&str> = PathRoot::ALL.iter().map(|root| root.name()).collect();
            return Err(format!(
                "language.path_roots says `{word}={meaning}`, and a root in Unluminous is one of {}",
                known.join(", ")
            ));
        };
        if word.is_empty() {
            return Err(format!(
                "language.path_roots holds `{word}={meaning}`, which names no word"
            ));
        }
        found.push((word, parsed));
    }
    Ok(found)
}

/// `language.raw_text`: a comma list of `element` or `element=language`, the elements of a markup
/// language whose contents are not markup — `script=javascript, style=css, textarea, title`.
///
/// The right hand side is a language name and it is **not** checked here, which is the one
/// registry-shaped key in Unluminous that is not validated against a list: the name is resolved by
/// `Plugins::for_language` at the moment of use, the same function a fence in a Markdown document
/// is resolved by, which already answers with nothing for a language nothing claims. Checking it
/// would mean a plugin refusing to load because another plugin was switched off. An entry that
/// names a language is a raw text element and one that names none is an escapable raw text one,
/// which is the HTML Standard's own distinction and is derived rather than written down twice.
fn raw_text(values: &Values) -> Result<Vec<(String, Option<String>)>, String> {
    let mut found = Vec::new();
    for (element, language) in pairs(values, "language.raw_text") {
        if element.is_empty() {
            let entry = match &language {
                Some(language) => format!("{element}={language}"),
                None => element.clone(),
            };
            return Err(format!("language.raw_text holds `{entry}`, which names no element"));
        }
        // A bare element (no `=` at all) has no language, and that is an escapable one rather
        // than a mistake — `language` is `None` for it. An `=` with nothing, or only spaces,
        // after it is the mistake: it named an element and asked for a language it did not name.
        if language.as_deref().is_some_and(str::is_empty) {
            return Err(format!(
                "language.raw_text holds `{element}=`, which names an element and no language"
            ));
        }
        found.push((element, language));
    }
    Ok(found)
}

/// One trimmed word, or nothing when the manifest left the key out or left it empty.
fn word(values: &Values, name: &str) -> Option<String> {
    values.text(name).map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned)
}

/// A comma separated value as a list, with the spaces trimmed and the empty entries dropped.
///
/// `pub(super)` because `theme::themes` reads the plugin's `themes` line the same way this reads
/// every other comma list.
pub(super) fn list(values: &Values, name: &str) -> Vec<String> {
    values
        .text(name)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Two comma separated values, which is what a block comment's opener and terminator are.
fn pair(values: &Values, name: &str) -> Option<(String, String)> {
    let parts = list(values, name);
    match parts.as_slice() {
        [open, close] => Some((open.clone(), close.clone())),
        _ => None,
    }
}

/// `list(values, name)`, with each entry split on its first `=` into a left and right half.
///
/// `language.definers`, `language.path_roots` and `language.raw_text` each read a comma list of
/// `a=b` pairs by hand, with three near identical splits and three near identical "that is not
/// `a=b`" refusals. This is the split the three of them share: an entry with no `=` at all comes
/// back with no right side, which is what `language.raw_text` means by a bare, escapable element
/// and what the other two treat as a name with nothing after it, to refuse in their own words.
pub(super) fn pairs(values: &Values, name: &str) -> Vec<(String, Option<String>)> {
    list(values, name)
        .into_iter()
        .map(|entry| match entry.split_once('=') {
            Some((left, right)) => (left.trim().to_owned(), Some(right.trim().to_owned())),
            None => (entry, None),
        })
        .collect()
}

/// `#RRGGBB`, or `RRGGBB`.
pub fn colour(text: &str) -> Option<Color> {
    let text = text.trim().trim_start_matches('#');
    if text.len() != 6 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(text, 16).ok()?;
    Some(Color::rgb((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn manifest() -> String {
        [
            "plugin.id = sample",
            "plugin.name = Sample",
            "plugin.version = 2.1.0",
            "plugin.vendor = Someone",
            "plugin.description = A sample.",
            "language.extensions = .smp, SMPL",
            "language.keywords = if, else, while",
            "language.builtins = print",
            "language.line_comment = //",
            "language.block_comment = /*, */",
            "language.strings = \", '",
            "language.operators = +-=",
            "theme.keyword = #FF79C6",
            "theme.comment = 6272A4",
            "theme.number = not a colour",
        ]
        .join("\n")
    }

    /// A manifest for a plugin that draws, with every contribution on it. Used by the tests below and
    /// by the surfaces tests, so that a change to the keys is a change in one place.
    fn ui_manifest() -> String {
        [
            "plugin.id = board-plugin",
            "plugin.name = Board",
            "plugin.kind = ui",
            "ui.provider = agent-tasks",
            "pane.id = board",
            "pane.icon = board",
            "pane.side = right",
            "pane.width = 420",
            "tab.id = board",
            "menu.name = Board",
            "menu.entries = open=Open Board, -, sync=Sync",
            "menu.submenu.new = New",
            "menu.submenu.new.entries = task=Task, epic=Epic",
            "settings.page = Board",
        ]
        .join("\n")
    }

    #[test]
    fn a_manifest_becomes_a_plugin() {
        let plugin = parse(&Values::parse(&manifest()), false).expect("it should parse");
        assert_eq!(plugin.id, "sample");
        assert_eq!(plugin.name, "Sample");
        assert_eq!(plugin.version, "2.1.0");
        assert_eq!(plugin.vendor, "Someone");
        assert_eq!(plugin.kind, Kind::Language);
        // The dot is optional and the case does not matter, because a person writing a manifest
        // should not have to know which Unluminous wanted.
        assert_eq!(plugin.extensions, vec!["smp", "smpl"]);
        assert_eq!(plugin.grammar.keywords, vec!["if", "else", "while"]);
        assert_eq!(plugin.grammar.block_comment, Some(("/*".to_owned(), "*/".to_owned())));
        assert_eq!(plugin.grammar.strings, vec!['"', '\'']);
    }

    #[test]
    fn a_colour_is_read_with_or_without_its_hash_and_a_bad_one_is_left_out() {
        let plugin = parse(&Values::parse(&manifest()), false).expect("it should parse");
        assert_eq!(plugin.theme.colour(Token::Keyword), Some(Color::rgb(0xFF, 0x79, 0xC6)));
        assert_eq!(plugin.theme.colour(Token::Comment), Some(Color::rgb(0x62, 0x72, 0xA4)));
        assert_eq!(
            plugin.theme.colour(Token::Number),
            None,
            "a value that is not a colour is skipped"
        );
        assert_eq!(
            plugin.theme.colour(Token::String),
            None,
            "a colour that was not named is absent"
        );
    }

    #[test]
    fn a_manifest_with_no_id_or_no_extensions_is_refused_with_a_reason() {
        let problem = parse(&Values::parse("language.extensions = .a"), false).expect_err("no id");
        assert!(problem.contains("plugin.id"));
        let problem = parse(&Values::parse("plugin.id = a"), false).expect_err("no extensions");
        assert!(problem.contains("extensions"));
    }

    #[test]
    fn a_kind_this_version_cannot_run_is_refused_rather_than_half_loaded() {
        // The seam a later version widens. A manifest asking for something Unluminous cannot do must say
        // so plainly rather than loading as a language with no grammar.
        let text = "plugin.id = a\nplugin.kind = wasm\nlanguage.extensions = .a";
        let problem = parse(&Values::parse(text), false).expect_err("wasm is not a kind yet");
        assert!(problem.contains("wasm") && problem.contains("language"), "{problem}");
    }

    #[test]
    fn a_plugin_claims_its_own_extensions_and_no_others() {
        let plugin = parse(&Values::parse(&manifest()), false).expect("it should parse");
        assert!(plugin.claims(Path::new("thing.smp")));
        assert!(plugin.claims(Path::new("thing.SMP")), "the extension check ignores case");
        assert!(plugin.claims(Path::new("thing.smpl")));
        assert!(!plugin.claims(Path::new("thing.rs")));
        assert!(!plugin.claims(Path::new("thing")));
    }

    /// `pane.tile` says whether a pane may share a strip, and `pane.group` says where its button is.
    ///
    /// They were one key until `task-1949`, which is why the default here matters more than a default
    /// usually does: every manifest written before the split says nothing, and every one of them has to
    /// go on meaning exactly what it meant. The Agent-Tasks board is the case that forced the split —
    /// its button belongs at the top of the rail and the board still may not be given half the bottom
    /// strip — so it is the case tested.
    #[test]
    fn a_pane_is_a_tile_when_its_button_is_at_the_bottom_and_when_it_says_so_at_the_top() {
        let head = "plugin.id = a
plugin.kind = ui
ui.provider = agent-tasks
pane.id = b
";
        let read = |manifest: &str| {
            parse(&Values::parse(manifest), false)
                .expect("a ui manifest")
                .contributions
                .pane
                .expect("a pane")
        };
        assert!(
            read(&format!("{head}pane.group = bottom")).tile,
            "a manifest written before the split still means what it meant"
        );
        assert!(!read(&format!("{head}pane.group = top")).tile, "and so does a top one");
        // And either default can be said out loud, which is the whole point of the key: the board puts
        // its button at the top and keeps the strip to itself.
        assert!(
            read(&format!(
                "{head}pane.group = top
pane.tile = yes"
            ))
            .tile
        );
        assert!(
            !read(&format!(
                "{head}pane.group = bottom
pane.tile = no"
            ))
            .tile
        );
    }

    /// The board's own manifest is what `task-1949` changed, so it is what is checked.
    #[test]
    fn the_agent_tasks_button_is_in_the_top_group_and_the_board_is_still_a_tile() {
        let manifest = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("plugins/agent-tasks/plugin.conf"),
        )
        .expect("the bundled Agent-Tasks manifest");
        let pane = parse(&Values::parse(&manifest), true)
            .expect("the Agent-Tasks manifest")
            .contributions
            .pane
            .expect("a pane");
        assert_eq!(pane.group, RailGroup::Top, "its button goes under Agent-Chat's");
        assert!(pane.tile, "and the board still keeps the bottom strip to itself");
        assert_eq!(pane.side, crate::app::dock::Side::Bottom, "where it still docks");
    }

    #[test]
    fn a_ui_plugin_reads_all_five_contributions_and_needs_no_file_type() {
        let plugin = parse(&Values::parse(&ui_manifest()), false).expect("a ui manifest");
        assert_eq!(plugin.kind, Kind::Ui);
        // The check that refuses a language with no extensions belongs to the kind: Agent-Tasks is not
        // a file type and never will be.
        assert!(plugin.extensions.is_empty());
        let contributed = &plugin.contributions;
        assert_eq!(contributed.provider.as_deref(), Some("agent-tasks"));
        let pane = contributed.pane.as_ref().expect("a pane");
        assert_eq!(pane.id, "board");
        assert_eq!(pane.label, "Board", "the label falls back to plugin.name");
        assert_eq!(pane.group, RailGroup::Top, "top is the default");
        assert!(!pane.tile, "and a pane in the top group is not a tile unless it says so");
        assert_eq!(pane.side, crate::app::dock::Side::Right);
        assert_eq!(pane.width, 420.0);
        assert_eq!(pane.height, 260.0, "the terminal's height is the default for a strip");
        assert_eq!(pane.applies, "always");
        assert_eq!(contributed.tab.as_ref().expect("a tab").label, "Board");
        assert_eq!(contributed.page.as_ref().expect("a page").name, "Board");
        let menu = contributed.menu.as_ref().expect("a menu");
        assert_eq!(menu.name, "Board");
        assert_eq!(
            menu.items,
            vec![
                MenuItem::Command { command: "open".to_owned(), label: "Open Board".to_owned() },
                MenuItem::Separator,
                MenuItem::Command { command: "sync".to_owned(), label: "Sync".to_owned() },
                MenuItem::Submenu {
                    label: "New".to_owned(),
                    items: vec![
                        MenuItem::Command { command: "task".to_owned(), label: "Task".to_owned() },
                        MenuItem::Command { command: "epic".to_owned(), label: "Epic".to_owned() },
                    ],
                },
            ],
            "the entries are in the order the manifest names them, and a submenu comes after them"
        );
    }

    #[test]
    fn a_submenu_inside_a_submenu_is_read() {
        let text = [
            "plugin.id = a",
            "plugin.kind = ui",
            "ui.provider = agent-tasks",
            "menu.name = A",
            "menu.submenu.new = New",
            "menu.submenu.new.entries = task=Task",
            "menu.submenu.new.submenu.from = From",
            "menu.submenu.new.submenu.from.entries = jira=JIRA",
        ]
        .join("\n");
        let plugin = parse(&Values::parse(&text), false).expect("nested submenus");
        let menu = plugin.contributions.menu.expect("a menu");
        let MenuItem::Submenu { label, items } = &menu.items[0] else {
            panic!("the first item should be the New submenu, got {:?}", menu.items[0]);
        };
        assert_eq!(label, "New");
        assert_eq!(items.len(), 2, "one command and one submenu inside it");
        assert!(matches!(items[1], MenuItem::Submenu { .. }), "{:?}", items[1]);
    }

    #[test]
    fn every_refusal_names_what_was_asked_for_and_what_this_version_has() {
        // The rule `plugin.kind`, `language.renders`, `run.project` and `debug.adapter` already keep,
        // made once for each new key. A message that only says `invalid` sends somebody to the source.
        let cases: &[(&str, &[&str])] = &[
            ("plugin.id = a\nplugin.kind = wasm", &["wasm", "language", "ui"]),
            ("plugin.id = a\nplugin.kind = ui", &["ui.provider is missing"]),
            ("plugin.id = a\nplugin.kind = ui\nui.provider = chat", &["chat", "agent-tasks"]),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks",
                &["contributes nothing", "pane", "tab", "menu", "settings page"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\npane.id = b\npane.group = middle",
                &["middle", "top", "bottom"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\npane.id = b\npane.side = middle",
                &["middle", "left", "right", "top", "bottom"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\npane.id = b\npane.icon = sparkle",
                &["sparkle", "board", "terminal"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\npane.id = b\npane.applies = has_git",
                &["has_git", "always", "in_project"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\nmenu.name = A\nmenu.entries = open",
                &["open", "command=Name"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\nmenu.name = A\nmenu.submenu.new = New",
                &["menu.submenu.new", "empty"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\nsettings.page = A\nsettings.icon = sparkle",
                &["sparkle", "board"],
            ),
            // A measurement that is not a measurement. Silently becoming the default is the outcome every
            // check here exists to prevent.
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\npane.id = b\npane.width = wide",
                &["pane.width", "wide", "number of points"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\npane.id = b\npane.height = 0",
                &["pane.height", "above zero"],
            ),
            // A key that asks for a contribution the manifest did not declare.
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\nmenu.name = A\nmenu.entries = x=X\npane.width = 400",
                &["pane.width", "pane.id", "nothing would read it"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\npane.id = b\ntab.label = Board",
                &["tab.label", "tab.id"],
            ),
            // A plugin that draws must not carry a language's keys, and a language must not name a provider.
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\npane.id = b\nlanguage.extensions = .a",
                &["language.extensions", "only a `language` plugin"],
            ),
            (
                "plugin.id = a\nplugin.kind = ui\nui.provider = agent-tasks\npane.id = b\ndebug.adapter = lldb",
                &["debug.adapter", "only a `language` plugin"],
            ),
            (
                "plugin.id = a\nlanguage.extensions = .a\nui.provider = agent-tasks",
                &["ui.provider", "only a `ui` plugin"],
            ),
            // Unchanged: a language with no file type is still refused, and for the same reason.
            ("plugin.id = a\nlanguage.extensions =", &["language.extensions is empty"]),
            // A language manifest that contributes to the window is refused rather than half-loaded,
            // because a pane drawn by a plugin with no provider would be a pane nothing fills.
            (
                "plugin.id = a\nlanguage.extensions = .a\npane.id = b",
                &["language", "only a `ui` plugin"],
            ),
        ];
        for (text, expected) in cases {
            let problem = parse(&Values::parse(text), false)
                .expect_err(&format!("this should be refused:\n{text}"));
            for word in *expected {
                assert!(problem.contains(word), "`{word}` is not in `{problem}`");
            }
        }
    }

    #[test]
    fn a_renderer_this_version_does_not_have_is_refused_with_the_list_of_the_ones_it_does() {
        // The rule `plugin.kind`, `language.renders`, `run.project` and `debug.adapter` all keep: a manifest
        // naming something Unluminous has not got says so plainly rather than loading as a plugin whose pane is
        // quietly flat, which is the exact outcome a checked registry exists to prevent.
        let manifest = "plugin.id = a-board\nplugin.name = A Board\nplugin.kind = ui\n\
                        ui.provider = agent-tasks\nui.chrome = crayons\ntab.id = board\ntab.label = A Board\n";
        let problem =
            parse(&Values::parse(manifest), false).expect_err("crayons is not a renderer");
        assert!(problem.contains("ui.chrome is `crayons`"), "{problem}");
        assert!(problem.contains("vello"), "and it names what this version does have: {problem}");

        // And a renderer on a plugin with no pane to draw is refused too, rather than parsing into a line
        // that does nothing. A language plugin has no surface of its own.
        let language = "plugin.id = a-language
plugin.name = A Language
language.extensions = .aa
                        ui.chrome = vello
";
        let problem = parse(&Values::parse(language), false).expect_err("a language has no pane");
        assert!(problem.contains("not a `ui` plugin"), "{problem}");
    }

    /// A manifest that asks for a pane still gets one, read out of the file the way every other key is.
    ///
    /// This used to be covered by Agent-Tasks\'s own manifest and stopped being when `task-28` removed its
    /// pane. Reading a pane is Unluminous\'s side of the plugin contract rather than one plugin\'s arrangement, so
    /// it is tested against a manifest written here.
    #[test]
    fn a_manifest_may_contribute_a_pane() {
        let manifest = "plugin.id = a-board\nplugin.name = A Board\nplugin.kind = ui\n\
                        ui.provider = agent-tasks\npane.id = board\npane.label = A Board\n\
                        pane.icon = board\npane.group = top\npane.side = right\npane.width = 420\n";
        let plugin = parse(&Values::parse(manifest), false).expect("a manifest with a pane in it");
        let pane = plugin.contributions.pane.as_ref().expect("a pane");
        assert_eq!(pane.id, "board");
        assert_eq!(pane.label, "A Board");
        assert_eq!(pane.side, crate::app::dock::Side::Right);
        assert_eq!(pane.group, RailGroup::Top);
        assert_eq!(pane.width, 420.0);
    }

    /// **A misspelt key in any namespace is refused, naming the key it meant.** `task-1922` B13.
    ///
    /// `no_orphans` covered four namespaces and `language.`, `run.`, `debug.` and `plugin.` were not
    /// among them -- and every key in those is read with a helper that answers nothing for a name
    /// that is not there. So `language.keywrods` loaded as a language with no keywords, said nothing,
    /// and the file simply opened uncoloured while the manifest looked right.
    #[test]
    fn a_misspelt_manifest_key_in_any_namespace_is_refused() {
        let refused = |text: &str| {
            let mut values = Values::new();
            for line in text.lines() {
                if let Some((name, value)) = line.split_once('=') {
                    values.set(name.trim(), value.trim());
                }
            }
            only_known_keys(&values).err()
        };

        let said = refused("language.keywrods = fn, let").expect("a misspelt key is refused");
        assert!(said.contains("language.keywrods"), "it says what was written: {said}");
        assert!(said.contains("language.keywords"), "and what it meant: {said}");

        for (wrong, meant) in [
            ("run.fil = node {file}", "run.file"),
            ("debug.adaptor = lldb", "debug.adapter"),
            ("plugin.vendorr = Somebody", "plugin.vendor"),
            ("pane.lable = Board", "pane.label"),
        ] {
            let said = refused(wrong).unwrap_or_else(|| panic!("{wrong} should be refused"));
            assert!(said.contains(meant), "{wrong} should suggest {meant}, said: {said}");
        }

        // A key that is nothing like any of them gets the list rather than a guess.
        let said = refused("language.xyzzy = 1").expect("refused");
        assert!(said.contains("language.keywords"), "the list is given instead: {said}");

        // And a manifest of only real keys is accepted.
        assert!(
            refused("language.keywords = fn, let\nplugin.id = rust\nrun.project = cargo").is_none()
        );
    }

    /// The refusal reads the way `run.project`'s does, because it is the same decision made again.
    #[test]
    fn a_debugger_this_version_cannot_drive_is_refused_rather_than_half_loaded() {
        let head = "plugin.id = a\nlanguage.extensions = .a\n";
        let refused = parse(&Values::parse(&format!("{head}debug.adapter = gdb")), false)
            .expect_err("gdb is not one Unluminous drives");
        assert!(refused.contains("gdb"), "{refused}");
        assert!(refused.contains("lldb, node"), "it says what this version does drive: {refused}");
        let accepted = parse(&Values::parse(&format!("{head}debug.adapter = lldb")), false)
            .expect("one it does drive");
        assert_eq!(accepted.debug_adapter.as_deref(), Some("lldb"));
    }

    #[test]
    fn a_manifest_asking_for_an_import_shape_or_a_root_unluminous_does_not_have_is_refused() {
        // The rule `plugin.kind`, `language.renders` and `language.definers` already keep: a
        // manifest naming something this version does not have should say so plainly rather than
        // load as a language whose imports quietly never complete.
        let head = "plugin.id = a\nlanguage.extensions = .a\n";
        let refused = |text: &str| parse(&Values::parse(text), false).expect_err(text);
        assert!(refused(&format!("{head}language.imports = sideways")).contains("quoted or path"));
        assert!(refused(&format!("{head}language.path_roots = crate")).contains("word=meaning"));
        let unknown = format!("{head}language.path_roots = crate=universe");
        assert!(refused(&unknown).contains("package, module, parent"), "{}", refused(&unknown));
        // And the shapes that are right are read.
        let good = format!("{head}language.imports = path\nlanguage.path_roots = crate=package");
        let plugin = parse(&Values::parse(&good), false).expect("a path family language");
        assert_eq!(plugin.grammar.path_root("crate"), Some(PathRoot::Package));
    }

    #[test]
    fn a_definers_entry_that_is_not_a_pair_or_names_an_unknown_kind_is_refused() {
        // The rule `plugin.kind` and `language.renders` already keep: a manifest asking for
        // something this version does not know says so rather than half loading.
        let text = "plugin.id = a\nlanguage.extensions = .a\nlanguage.definers = fn";
        let problem = parse(&Values::parse(text), false).expect_err("`fn` is not a pair");
        assert!(problem.contains("keyword=kind"), "{problem}");
        let text = "plugin.id = a\nlanguage.extensions = .a\nlanguage.definers = fn=gadget";
        let problem = parse(&Values::parse(text), false).expect_err("there is no gadget kind");
        assert!(problem.contains("gadget") && problem.contains("function"), "{problem}");
    }

    #[test]
    fn a_manifest_naming_a_detector_unluminous_does_not_have_is_refused_with_a_reason() {
        // The rule `plugin.kind`, `language.renders`, `language.definers` and `language.imports`
        // already keep.
        let head = "plugin.id = a\nlanguage.extensions = .a\n";
        let problem = parse(&Values::parse(&format!("{head}run.project = gradle")), false)
            .expect_err("gradle is not a detector this version has");
        assert!(problem.contains("gradle"), "{problem}");
        assert!(
            problem.contains("cargo") && problem.contains("npm"),
            "and it says what there is: {problem}"
        );
        // And a run.file with no placeholder in it, which would run the same file whatever was open.
        let problem = parse(&Values::parse(&format!("{head}run.file = node server.js")), false)
            .expect_err("a template with no placeholder");
        assert!(problem.contains("{file}"), "{problem}");
        // The shapes that are right are read.
        let good = format!("{head}run.file = ruby {{file}}\nrun.project = cargo");
        let plugin = parse(&Values::parse(&good), false).expect("a language that runs");
        assert_eq!(plugin.run_file.as_deref(), Some("ruby {file}"));
        assert_eq!(plugin.run_project.as_deref(), Some("cargo"));
    }

    #[test]
    fn a_manifest_naming_a_renderer_unluminous_does_not_have_is_refused_with_a_reason() {
        // The same rule `plugin.kind` keeps, and for the same reason: a manifest asking for a
        // picture this version cannot draw must say so rather than load as a language whose files
        // quietly never draw.
        let text = "plugin.id = a\nlanguage.extensions = .a\nlanguage.renders = holograms";
        let problem = parse(&Values::parse(text), false).expect_err("holograms are not a renderer");
        assert!(problem.contains("holograms"), "{problem}");
        assert!(problem.contains("mermaid"), "and it says what there is: {problem}");
    }
}
