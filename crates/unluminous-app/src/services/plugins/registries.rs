//! The lists of names built into this version of Unluminous that a manifest may point at.
//!
//! Every one of these is a registry of the same shape: a plugin names something by a word, and the
//! word is checked against a fixed list of what Unluminous can actually do rather than taken on trust.
//! A manifest naming something this version has not got is refused with a message rather than
//! loading as a plugin whose feature quietly never works — the rule `plugin.kind` set first, kept
//! here for a renderer, a project detector, a debugger, a UI provider, a chrome renderer, a rail
//! icon, an icon set and a pane condition.
//!
//! [`checked_against`] is the one place the refusal is written, and [`optional_registry_entry`] is
//! the shape `run.project` and `debug.adapter` share on top of it: read an optional word, and check
//! it when it is there.

/// The renderers built into this version of Unluminous that a plugin may name.
///
/// Checked rather than taken on trust, so a manifest asking for a picture Unluminous cannot draw says so
/// plainly instead of loading as a language whose files quietly never draw.
pub const RENDERERS: &[&str] = &["mermaid"];

/// The project detectors built into this version of Unluminous that a plugin's `run.project` may name.
///
/// Checked the same way [`RENDERERS`] is, and for the same reason: a manifest asking for a detector
/// Unluminous does not have should say so plainly rather than load as a language whose projects are
/// quietly never noticed. `services::run_configurations::detect` is what each one does.
///
/// This is the answer to the question `task-1683` opens with — should running node mean a Node
/// plugin? No: node is how JavaScript runs, and a plugin with no language, no extensions and no
/// tokens, existing to carry one line of data, is not a plugin. The JavaScript manifest carries
/// that line itself, exactly as Mermaid named a built-in renderer rather than widening
/// `plugin.kind`.
pub const PROJECT_RUNNERS: &[&str] = &["cargo", "npm"];

/// The debuggers built into this version of Unluminous that a plugin's `debug.adapter` may name.
///
/// The third registry of this shape, checked the same way and for the same reason: a manifest naming
/// a debugger Unluminous cannot drive should say so plainly rather than load as a language whose files
/// quietly offer a Debug button that never works.
///
/// **Which debugger a language uses is data in the plugin, and the debugger itself is code in
/// Unluminous.** `services::debuggers` is what each name knows how to find and how to start — where
/// `lldb-dap` lives on `PATH`, how to translate a run configuration into that adapter's own launch
/// shape — so the most a third-party manifest can do is name an adapter that shipped in the binary,
/// visibly. Nothing in a plugin is executed and nothing is ever fetched.
pub const DEBUGGERS: &[&str] = &["lldb", "node"];

/// The UI providers built into this version of Unluminous that a plugin's `ui.provider` may name.
///
/// The fourth registry of this shape, checked the same way and for the same reason as [`RENDERERS`],
/// [`PROJECT_RUNNERS`] and [`DEBUGGERS`]: a manifest naming a provider Unluminous does not have should say
/// so plainly rather than load as a plugin whose pane is permanently empty.
///
/// **What a plugin contributes is data and the code that draws it is Unluminous's.** A manifest says there
/// is a pane, where it docks, what its button looks like and what its menu holds; the drawing shipped
/// with the binary. So the most a manifest can do is name a provider that is already here, visibly,
/// and nothing in a plugin is executed.
pub const UI_PROVIDERS: &[&str] = &["agent-tasks", "agent-chat", "database"];

/// The renderers a plugin's `ui.chrome` may name for the decoration `egui` cannot draw.
///
/// The fifth registry of this shape, checked the same way and for the same reason as the four above. A
/// manifest saying `ui.chrome = vello` asks for the soft shadows, inset shadows, gradients and rounded
/// clips of `services::vello_canvas`; a manifest naming anything else is refused with the list rather than
/// loading as a plugin whose pane is quietly flat.
///
/// **It is off unless a manifest asks**, which is the rule `language.word_characters`, `language.types`,
/// `language.markup` and every import key already keep, so no plugin that shipped before this changes by a
/// pixel. Switching it off in the manifest really withdraws the decoration, in the same frame, which is
/// the property `Plugins::renders` has for a Mermaid diagram.
pub const CHROME: &[&str] = &["vello"];

/// The icons a `pane.icon` may name, drawn by [`crate::theme::icon`].
///
/// Checked for the same reason the three registries above are checked: a rail button drawn as nothing
/// is worse than a manifest that was refused with the list of icons in the message.
pub const PANE_ICONS: &[&str] = &[
    "board", "folder", "terminal", "run", "bug", "clock", "branch", "tick", "plus", "image",
    "chat", "database", "table",
];

/// The icon sets built into this version of Unluminous that a theme's `icons` may name.
///
/// The sixth registry of this shape, checked the same way and for the same reason as the five above: a
/// theme naming a set Unluminous has not got should say so plainly rather than load as a theme whose rail
/// buttons and folder arrows are drawn as nothing. `theme::icon` is what each name draws.
///
/// **The shapes are code and the choice is data**, which is `language.renders` again. The most a
/// third-party theme can do is pick one of the sets that shipped in the binary, visibly.
pub const ICON_SETS: &[&str] = &["material", "classic"];

/// The conditions a `pane.applies` may name.
///
/// Unluminous's answer to VS Code's `when` expressions, which are the most copied part of its contribution
/// model and the hardest to keep tested. A control that cannot apply is absent here, and the question
/// is a function rather than an expression, so there are two named conditions and a list to check
/// against instead of a language to parse.
pub const PANE_CONDITIONS: &[&str] = &["always", "in_project"];

/// Refuse `value` when `known` says it is not something Unluminous actually has, naming what was
/// asked for and what the option list really is.
///
/// `task-1922` B-review found nine places that built this sentence by hand — `language.renders`,
/// `theme.<id>.icons`, `ui.provider`, `ui.chrome`, `pane.icon`, `pane.applies`, `settings.icon`,
/// `run.project` and `debug.adapter` — each with its own `format!` and its own `.join(", ")`. This
/// is the one place the sentence is written now.
///
/// `known` is passed in rather than computed here, because one of the nine checks a name through
/// `crate::theme::IconSet::parse` rather than through `registry.contains`, and that difference has
/// to survive the move: `IconSet::parse` reads a name case-insensitively and the other eight do
/// not, so folding the membership test itself into this function would change one of the nine.
pub fn checked_against(
    key: &str,
    value: &str,
    known: bool,
    registry: &[&str],
    verb: &str,
) -> Result<(), String> {
    if known {
        return Ok(());
    }
    Err(format!(
        "{key} is `{value}`, and this version of Unluminous {verb} {}",
        registry.join(", ")
    ))
}

/// `named`, checked against `registry` when a manifest wrote anything at all.
///
/// `run.project` and `debug.adapter` are the same question asked of two different registries: an
/// optional name that, when a manifest gives one, has to be something Unluminous can actually detect
/// or drive. This is the shape the two of them share, on top of [`checked_against`]. The caller
/// reads the word itself and hands it over, so this module does not need to know how a manifest is
/// read.
pub fn optional_registry_entry(
    named: Option<String>,
    key: &str,
    registry: &[&str],
    verb: &str,
) -> Result<Option<String>, String> {
    let Some(named) = named else {
        return Ok(None);
    };
    checked_against(key, &named, registry.contains(&named.as_str()), registry, verb)?;
    Ok(Some(named))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_value_is_accepted() {
        assert!(
            checked_against("ui.chrome", "vello", CHROME.contains(&"vello"), CHROME, "has").is_ok()
        );
    }

    #[test]
    fn an_unknown_value_names_what_was_asked_for_and_what_this_version_has() {
        let problem = checked_against("ui.chrome", "crayons", false, CHROME, "has")
            .expect_err("crayons is not a chrome renderer");
        assert!(problem.contains("ui.chrome is `crayons`"), "{problem}");
        assert!(problem.contains("vello"), "and it names what this version does have: {problem}");
    }

    #[test]
    fn an_optional_entry_with_nothing_written_is_absent() {
        assert_eq!(
            optional_registry_entry(None, "run.project", PROJECT_RUNNERS, "detects"),
            Ok(None)
        );
    }

    #[test]
    fn an_optional_entry_naming_something_unluminous_does_not_have_is_refused() {
        let problem = optional_registry_entry(
            Some("gulp".to_owned()),
            "run.project",
            PROJECT_RUNNERS,
            "detects",
        )
        .expect_err("gulp is not a project runner");
        assert!(problem.contains("gulp") && problem.contains("cargo"), "{problem}");
    }
}
