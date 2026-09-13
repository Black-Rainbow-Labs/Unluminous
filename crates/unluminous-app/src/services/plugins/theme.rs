//! `plugin.kind = theme` manifests: the `themes` line and each `theme.<id>.` group it names.
//!
//! A theme plugin carries several themes rather than one, because that is what a themes bundle is:
//! switched on or off once, and its palettes arrive or leave together. Every refusal here names
//! what was asked for and what this version has, the rule `plugin.kind` set first.

use unluminous_core::syntax::Token;
use unluminous_core::Color;

use super::manifest::{colour, list};
use super::registries::{checked_against, ICON_SETS};
use super::types::{Kind, SyntaxTheme};
use crate::services::store::Values;

/// The `themes` line and the `theme.<id>.` groups: what a `plugin.kind = theme` manifest carries.
///
/// One plugin holds several, because that is what a themes bundle is: it is switched on or off once and
/// five palettes arrive or leave together. The groups are read with [`Values::starting_with`], which is the
/// same mechanism a plugin's submenus already use, so nothing new parses anything.
///
/// Every refusal here names what was asked for and what this version has, which is the rule
/// `plugin.kind`, `language.renders`, `ui.provider` and `ui.chrome` all keep. A `language` or `ui`
/// manifest reaches none of it, which `the_older_plugins_ask_for_none_of_what_themes_added` keeps.
pub(super) fn themes(
    values: &Values,
    plugin: &str,
    kind: Kind,
) -> Result<Vec<crate::theme::Theme>, String> {
    let declared = list(values, "themes");
    if kind != Kind::Theme {
        // A `themes` line on a plugin that is not a theme is a line that would do nothing, silently.
        if !declared.is_empty() {
            return Err(format!(
                "plugin.kind is `{}` and the manifest sets themes, which only a `theme` plugin has",
                kind.name()
            ));
        }
        return Ok(Vec::new());
    }
    if declared.is_empty() {
        return Err(
            "themes is empty, so this plugin would offer nothing: name its themes, as `themes = dracula, palenight`"
                .to_owned(),
        );
    }
    // Every group that was written, so one that is not on the `themes` line is refused rather than being
    // a block of colours nothing reads.
    let mut written: Vec<String> = Vec::new();
    for (rest, _) in values.starting_with("theme.") {
        if let Some((id, _)) = rest.split_once('.') {
            if !written.iter().any(|known| known == id) {
                written.push(id.to_owned());
            }
        }
    }
    for id in &written {
        if !declared.contains(id) {
            return Err(format!(
                "theme.{id} is set and `{id}` is not on the themes line, so nothing would ever read it"
            ));
        }
    }
    declared.iter().map(|id| one_theme(values, plugin, id)).collect()
}

/// One `theme.<id>.` group.
fn one_theme(values: &Values, plugin: &str, id: &str) -> Result<crate::theme::Theme, String> {
    let at = |leaf: &str| values.text(&format!("theme.{id}.{leaf}")).map(str::trim);
    let name = at("name")
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            format!("theme.{id}.name is missing, so it would have nothing to be called in the list")
        })?
        .to_owned();
    // Dark unless it says otherwise, and light is refused with the reason rather than half-supported. The
    // window is drawn on a transparent ground, the depth recipe in `vello_canvas` lifts a surface and
    // darkens it with black, and every accepted screenshot is judged against a dark ground — so a light
    // theme is not a palette swap and shipping one nobody had looked at every screen in would be worse
    // than saying so. This is `plugin.kind`'s own move: name the seam and leave it closed.
    let dark = values.flag(&format!("theme.{id}.dark")).unwrap_or(true);
    if !dark {
        return Err(format!(
            "theme.{id}.dark is false, and this version of Unluminous draws dark themes only"
        ));
    }
    let icons = match at("icons").filter(|named| !named.is_empty()) {
        Some(named) => {
            // `IconSet::parse` is the actual membership test — it reads a name case-insensitively,
            // which `checked_against`'s callers elsewhere do not — so the parse runs first and the
            // question handed to `checked_against` is only "did it work", never `.contains`.
            let parsed = crate::theme::IconSet::parse(named);
            checked_against(
                &format!("theme.{id}.icons"),
                named,
                parsed.is_some(),
                ICON_SETS,
                "draws",
            )?;
            parsed.expect("checked_against just confirmed this name parses")
        }
        None => crate::theme::IconSet::default(),
    };

    // A role that is not named keeps Unluminous Dark's, which is the reference editor's `parentTheme` in one line and is
    // what keeps a manifest to the thirty colours that matter rather than all forty.
    let mut palette = crate::theme::Palette::UNLUMINOUS_DARK;
    for (role, value) in values.starting_with(&format!("theme.{id}.ui.")) {
        let Some(read) = colour(&value) else {
            return Err(format!(
                "theme.{id}.ui.{role} is `{value}`, which is not a colour such as #FF79C6"
            ));
        };
        if !palette.set(&role, egui::Color32::from_rgb(read.r, read.g, read.b)) {
            return Err(format!(
                "theme.{id}.ui.{role} names a colour Unluminous has not got. It has {}",
                crate::theme::Palette::NAMES.join(", ")
            ));
        }
    }

    // All nine tokens or none. Eight would half-recolour a file: the ninth would keep whichever language
    // plugin's colour it had, and the two schemes would be visible in one line of code.
    //
    // **A value that will not read as a colour is refused, not treated as absent**, and the difference
    // matters more here than anywhere else in this function: a theme whose nine were all mistyped would
    // otherwise load as a theme that names none, which is a *valid* thing to be — so the manifest would
    // be accepted, the window would quietly go on using each language plugin's own scheme, and nothing
    // would ever say why. The review on `task-1776` found it. Every other refusal here exists to stop
    // exactly that, and this is the one place where the silent outcome looks like a legitimate one.
    let mut named: Vec<(Token, Color)> = Vec::new();
    let mut missing: Vec<&str> = Vec::new();
    for token in Token::ALL {
        match at(&format!("syntax.{}", token.name())) {
            Some(value) => match colour(value) {
                Some(read) => named.push((token, read)),
                None => {
                    return Err(format!(
                        "theme.{id}.syntax.{} is `{value}`, which is not a colour such as #FF79C6",
                        token.name()
                    ))
                }
            },
            None => missing.push(token.name()),
        }
    }
    let syntax = match (named.is_empty(), missing.is_empty()) {
        (true, _) => None,
        (false, true) => Some(SyntaxTheme::of(name.clone(), named)),
        (false, false) => {
            return Err(format!(
                "theme.{id} colours some tokens and not others, which would leave a file in two schemes at once. It is missing {}",
                missing.join(", ")
            ))
        }
    };

    // And a key under this theme that is none of the five it reads is a line that would do nothing,
    // silently — the rule `no_orphans` already keeps for `pane.`, `tab.`, `menu.` and `settings.`. It is
    // last so the messages above, which say what is wrong with a key that *is* recognised, come first.
    for (leaf, _) in values.starting_with(&format!("theme.{id}.")) {
        let known = matches!(leaf.as_str(), "name" | "dark" | "icons")
            || leaf.strip_prefix("ui.").is_some()
            || leaf
                .strip_prefix("syntax.")
                .is_some_and(|token| Token::ALL.iter().any(|known| known.name() == token));
        if !known {
            return Err(format!(
                "theme.{id}.{leaf} is not a key a theme has. It reads name, dark, icons, ui.<colour> and syntax.<token>, where a token is one of {}",
                Token::ALL.map(Token::name).join(", ")
            ));
        }
    }

    Ok(crate::theme::Theme {
        key: format!("{plugin}/{id}"),
        name,
        plugin: plugin.to_owned(),
        dark,
        palette,
        syntax,
        icons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::services::plugins::{parse, Plugins};

    /// `task-1776`. One plugin, five themes, in the manifest's own order rather than alphabetical.
    #[test]
    fn a_theme_plugin_carries_several_themes() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let bundle = plugins.get("themes-bundle-1").expect("the themes bundle");
        assert_eq!(bundle.kind, Kind::Theme);
        let names: Vec<&str> = bundle.themes.iter().map(|theme| theme.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Islands Dracula Colorful",
                "Material Palenight",
                "Material Deep Ocean",
                "Monokai Pro",
                "One Dark"
            ],
            "the order is the manifest's `themes` line"
        );
        let dracula = &bundle.themes[0];
        assert_eq!(dracula.key, "themes-bundle-1/dracula", "named as a contributed pane is");
        assert_eq!(dracula.palette.editor, egui::Color32::from_rgb(0x28, 0x2A, 0x36));
        assert_eq!(dracula.palette.accent, egui::Color32::from_rgb(0xFF, 0x79, 0xC6));
        assert_eq!(dracula.icons, crate::theme::IconSet::Material);
        // The blue comment is what makes Dracula **Colorful** rather than plain Dracula, and it is the
        // one number a theme called Dracula is most likely to be wrong about.
        let scheme = dracula.syntax.as_ref().expect("it colours the tokens");
        assert_eq!(scheme.colour(Token::Comment), Some(Color::rgb(0x98, 0xAF, 0xFF)));
        assert_eq!(scheme.colour(Token::Keyword), Some(Color::rgb(0xFF, 0x79, 0xC6)));
        // And One Dark keeps the marks Unluminous shipped with, so the bundle has one of each.
        assert_eq!(bundle.themes[4].icons, crate::theme::IconSet::Classic);
    }

    /// A role a theme does not name keeps Unluminous Dark's, which is the reference editor's `parentTheme` in one line.
    #[test]
    fn a_theme_inherits_every_colour_it_does_not_name() {
        let manifest = "plugin.id = t\nplugin.kind = theme\nthemes = one\ntheme.one.name = One\ntheme.one.ui.accent = #FF0000\n";
        let plugin = parse(&Values::parse(manifest), false).expect("it parses");
        let theme = &plugin.themes[0];
        assert_eq!(theme.palette.accent, egui::Color32::RED, "the one it named");
        assert_eq!(
            theme.palette.editor,
            crate::theme::Palette::UNLUMINOUS_DARK.editor,
            "and every other is Unluminous's own"
        );
        assert!(theme.syntax.is_none(), "naming no token colours leaves the plugins alone");
        assert_eq!(
            theme.icons,
            crate::theme::IconSet::Material,
            "and the marks a window comes up in"
        );
    }

    /// Every refusal names what was asked for and what this version has, which is the rule
    /// `plugin.kind`, `language.renders`, `ui.provider` and `ui.chrome` all keep.
    #[test]
    fn a_theme_manifest_is_refused_rather_than_half_loaded() {
        let refused = |manifest: &str| {
            parse(&Values::parse(manifest), false).expect_err("it should be refused")
        };
        let base = "plugin.id = t\nplugin.kind = theme\nthemes = one\ntheme.one.name = One\n";

        let problem = refused(&format!("{base}theme.one.ui.editor_background = #FF0000\n"));
        assert!(problem.contains("editor_background"), "{problem}");
        assert!(problem.contains("explorer_footer"), "and it lists what Unluminous has: {problem}");

        let problem = refused(&format!("{base}theme.one.dark = false\n"));
        assert!(problem.contains("dark themes only"), "{problem}");

        let problem = refused(&format!("{base}theme.one.icons = atom\n"));
        assert!(problem.contains("material, classic"), "{problem}");

        // Eight of the nine would leave one line of code drawn in two schemes at once.
        let problem = refused(&format!("{base}theme.one.syntax.keyword = #FF0000\n"));
        assert!(problem.contains("missing"), "{problem}");
        assert!(problem.contains("comment"), "and it says which: {problem}");

        let problem = refused(&format!("{base}theme.one.ui.accent = magenta\n"));
        assert!(problem.contains("not a colour"), "{problem}");

        // **The one whose silent outcome looks legitimate**, which is why the review found it and the
        // eight-of-nine check above did not: a theme that names no token colours is a valid theme, so a
        // theme whose nine were all mistyped would have loaded as one and gone on using each language
        // plugin's own scheme with nothing said.
        let mut all_nine_mistyped = base.to_owned();
        for token in Token::ALL {
            all_nine_mistyped.push_str(&format!("theme.one.syntax.{} = #GGGGGG\n", token.name()));
        }
        let problem = refused(&all_nine_mistyped);
        assert!(problem.contains("not a colour"), "{problem}");
        assert!(problem.contains("syntax."), "and it says which key: {problem}");

        // A key under a theme that is none of the five it reads.
        let problem = refused(&format!("{base}theme.one.colour.editor = #FF0000\n"));
        assert!(problem.contains("not a key a theme has"), "{problem}");
        let problem = refused(&format!("{base}theme.one.syntax.keywrd = #FF0000\n"));
        assert!(problem.contains("keywrd"), "{problem}");
        assert!(problem.contains("keyword"), "and it lists the tokens: {problem}");

        // A group nothing lists is a block of colours that would never be read.
        let problem = refused(&format!("{base}theme.two.name = Two\n"));
        assert!(problem.contains("themes line"), "{problem}");

        let problem = refused("plugin.id = t\nplugin.kind = theme\n");
        assert!(problem.contains("themes is empty"), "{problem}");

        // And a theme plugin is held to the same rule a `ui` one is about a language's keys.
        let problem = refused(&format!("{base}language.extensions = .foo\n"));
        assert!(problem.contains("language.extensions"), "{problem}");
    }

    /// A theme's palette is the whole list, so a role added to `theme::Palette` later is a role the
    /// bundle can set without anything here being taught its name.
    #[test]
    fn every_role_the_palette_has_can_be_named_in_a_manifest() {
        let mut manifest =
            "plugin.id = t\nplugin.kind = theme\nthemes = one\ntheme.one.name = One\n".to_owned();
        for role in crate::theme::Palette::NAMES {
            manifest.push_str(&format!("theme.one.ui.{role} = #123456\n"));
        }
        let plugin = parse(&Values::parse(&manifest), false).expect("every name is a role");
        assert_eq!(plugin.themes[0].palette.editor, egui::Color32::from_rgb(0x12, 0x34, 0x56));
        assert_eq!(plugin.themes[0].palette.folder_open, egui::Color32::from_rgb(0x12, 0x34, 0x56));
    }

    /// The registry a manifest is checked against and the enum that draws are one list.
    ///
    /// Two lists of the same thing is how a manifest comes to be refused for naming a set that exists, or
    /// accepted for naming one that does not. The registry is a `&[&str]` because every other one here is,
    /// and this is what keeps it honest.
    #[test]
    fn the_icon_set_registry_and_the_enum_are_one_list() {
        let drawn: Vec<&str> =
            crate::theme::IconSet::ALL.into_iter().map(crate::theme::IconSet::name).collect();
        assert_eq!(drawn, ICON_SETS, "the names a manifest may use are the sets that are drawn");
        for name in ICON_SETS {
            assert!(crate::theme::IconSet::parse(name).is_some(), "{name} is drawn");
        }
    }
}
