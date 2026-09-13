//! What the window does when a setting changes.
//!
//! `apply_settings` is the one place a changed setting turns into a changed window, which is
//! `run_action`'s rule applied to the Settings page. The font is the one that reaches furthest:
//! `set_the_font_everywhere` is what puts it into effect, and every path that changes it goes through
//! that one function.

use crate::services::store::Store;
use crate::settings::{self, Settings};
use crate::theme::{self};

use crate::app::UnluminousApp;

impl UnluminousApp {
    /// Read the settings and the recent projects from disk, and remember the project that is open.
    ///
    /// The released binary calls this and the tests do not, so a test neither reads nor writes the settings
    /// of the person running it.
    pub fn load_settings(&mut self) {
        self.use_store(Store::open());
    }

    /// Write the settings and the pane sizes, if there is anywhere to write them.
    pub(crate) fn write_settings(&mut self) {
        if let Some(store) = &self.store {
            // With the names of the contributed panes, so where one was dragged and how wide it was left
            // are written against its own name rather than being lost when Unluminous closes.
            settings::save_with(store, &self.settings, &self.panes, &self.plugin_ui.pane_keys());
        }
        self.unsaved_settings = false;
    }

    /// Change the settings from outside the Settings window, putting the change into effect at once.
    ///
    /// The window itself changes `self.settings` in place and the change is noticed in `ui`; this is for a
    /// caller that has a whole set of settings to hand, which is what a test and the command line have.
    pub fn set_settings(&mut self, settings: Settings) {
        let before = std::mem::replace(&mut self.settings, settings);
        self.apply_settings(&before);
    }

    /// A setting changed, so put it into effect and write it down.
    pub(crate) fn apply_settings(&mut self, before: &Settings) {
        // A `debug.<name>` may have moved, and the search that found the old one is now a lie.
        self.forget_the_adapter_search();
        if self.settings.font_family != before.font_family
            || self.settings.font_size != before.font_size
        {
            self.set_the_font_everywhere();
        }
        // The theme follows its four settings. Asked on every settings change rather than only when one
        // of the four moved, for the reason `reconcile_mcp` records beside itself: a list of the settings
        // that have to remember to say so is a list whose next entry is the one that forgot.
        self.apply_the_theme();
        // The interface's own family is bound into egui rather than read at drawing time, so it is
        // installed again when it moves — and only then, because `set_fonts` throws away the glyph atlas.
        // The editor's family counts as a move, because an empty `ui_font_family` means "the editor's":
        // without this, changing the editor's font would leave the menus in the family it had before.
        if self.settings.ui_font_family != before.ui_font_family
            || (self.settings.ui_font_family.trim().is_empty()
                && self.settings.font_family != before.font_family)
        {
            self.install_the_interface_font();
        }
        // The MCP endpoint follows its three settings. It is asked on every settings change rather
        // than only when one of the three moved, because `Hosted::reconcile` answers "nothing
        // changed" in two comparisons and a list of the settings that have to remember to tell it
        // is a list whose next entry will be the one that forgot.
        self.reconcile_mcp();
        // The project index follows `editor.exclude`, and only reloads when the line really moved --
        // `FileTree::set_exclude` compares before it walks. `task-1804` §7.3.
        self.tree.set_exclude(&self.settings.exclude);
        self.unsaved_settings = true;
    }

    /// Paint this window in the theme the settings name, and tell egui about it.
    ///
    /// **The one place a theme becomes a change**, which is `run_action`'s rule and `run_cli`'s: the
    /// Settings page, `theme set`, `settings set appearance.theme` and switching a theme plugin off all
    /// come here, so none of them can leave the two halves — the palette Unluminous draws with and the copy
    /// egui keeps in its style — half done.
    ///
    /// A theme whose plugin is switched off, uninstalled or failing to parse is not in `Plugins::themes`,
    /// so this falls back to Unluminous's own rather than leaving the window in a palette nothing can name.
    /// That is `Plugins::renders`' rule applied to colour: switching a plugin off withdraws what it
    /// contributed in the same frame.
    pub(crate) fn apply_the_theme(&mut self) {
        let wanted = match self.settings.theme.trim().is_empty() {
            true => theme::Theme::unluminous_dark(),
            false => self
                .plugins
                .theme(&self.settings.theme)
                .unwrap_or_else(theme::Theme::unluminous_dark),
        };
        let wanted = match self.settings.accent_colour() {
            Some(accent) => wanted.with_accent(accent),
            None => wanted,
        };
        // The setting wins over the theme's own choice, and `Follow the theme` is an empty setting rather
        // than a third value — the rule `terminal_shell` and `appearance.theme` both keep.
        let wanted = match self.settings.icon_set() {
            Some(set) => theme::Theme { icons: set, ..wanted },
            None => wanted,
        };
        // Nothing to do when the answer is the one already showing, which it is on every settings change
        // that was not about colour. Asking always and acting only on a difference is `reconcile_mcp`'s
        // own shape, and it is what keeps this off the list of settings that have to remember to say so —
        // and, since a theme change asks for a repaint, off the list of things that could keep an idle
        // window awake.
        if theme::active() == wanted && self.interface_scale == self.settings.interface_scale() {
            return;
        }
        self.interface_scale = self.settings.interface_scale();
        theme::activate(wanted);
        // **Everything that holds a colour it has already worked out is thrown away**, and only here,
        // because this is the one point at which the answer to "what colour is that" changed.
        //
        // `colour_the_file` is asked once a *revision* rather than once a frame, a preview is laid out
        // with its colours baked into the glyphs, and a Mermaid scene is built through
        // `mermaid_scene::theme`, which reads the palette. Without this, choosing a theme repainted the
        // window's furniture and left every open document, every preview and every diagram in the
        // colours they were built in until they were typed in — which the review on `task-1776` found.
        //
        // Every open file rather than the one showing, which is the rule `set_plugin_enabled` already
        // keeps for exactly these caches: a theme is a setting for the window, and with panes there is
        // more than one file being drawn.
        for file in self.files.iter_mut() {
            file.coloured_revision = None;
            file.document.syntax_is_wholly_dirty();
            file.cached.preview = None;
            file.cached.preview_diagrams.clear();
            file.cached.stale = true;
        }
        self.mermaid_scenes.forget();
        if let Some(context) = &self.context {
            theme::apply_scaled(context, self.interface_scale);
            context.request_repaint();
        }
    }

    /// Bind the family the window's own text is set in, which is the editor's until somebody chooses one.
    ///
    /// Separate from `set_the_font_everywhere`, which changes the **document**. This one calls
    /// `Context::set_fonts`, which throws away the glyph atlas and takes effect at the start of the next
    /// frame, so it is called when the family moves rather than on every settings change.
    pub(crate) fn install_the_interface_font(&mut self) {
        let Some(context) = self.context.clone() else {
            return;
        };
        let family = match self.settings.ui_font_family.trim().is_empty() {
            true => self.settings.font_family.clone(),
            false => self.settings.ui_font_family.clone(),
        };
        let family = match family.is_empty() {
            true => self.renderer.default_family(),
            false => family,
        };
        let regular = self.renderer.face_bytes(&family, false);
        let bold = self.renderer.face_bytes(&family, true);
        let has_bold = bold.is_some();
        theme::install_fonts(&context, &family, regular, bold);
        if has_bold {
            self.bold_family = egui::FontFamily::Name(theme::BOLD_FAMILY.into());
        }
    }

    /// Whether this window is hosting an MCP endpoint right now.
    ///
    /// False in every window a test builds, because a test never opens one — the same rule the
    /// command channel keeps. It is here so a test can say so rather than reaching into the field.
    pub fn is_serving_mcp(&self) -> bool {
        self.mcp.as_ref().and_then(|hosted| hosted.state().port()).is_some()
    }

    /// Bring the MCP endpoint into line with the settings.
    ///
    /// Does nothing in a window that never opened one, which is every window a test builds.
    pub(crate) fn reconcile_mcp(&mut self) {
        let has_channel = self.control.is_some();
        let (enabled, port, shape) =
            (self.settings.mcp_enabled, self.settings.mcp_port, self.settings.mcp_tools);
        let areas = self.settings.mcp_area_filter();
        let folder = self.tree.root().to_path_buf();
        if let Some(hosted) = &mut self.mcp {
            hosted.reconcile(enabled, port, shape, &areas, has_channel, &folder);
        }
    }
}
