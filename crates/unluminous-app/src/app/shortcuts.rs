//! The shortcuts on the rail and the top bar, as the window knows them. `task-2404`.
//!
//! `services::shortcuts` holds the arithmetic of the four settings and `components::shortcut_gallery` draws the
//! gallery, the top bar's chips and a tool's flyout. This is what joins them: which shortcuts there are in this
//! window, the one function a pin changes through, and the popups drawn over everything else.

use egui::{Pos2, Rect};

use crate::app::actions::{self, Action, ShortcutAction};
use crate::app::{dock, UnluminousApp};
use crate::components::activity_bar;
use crate::components::shortcut_gallery::{self, Flyout, GalleryCard};
use crate::components::{context_menu, run_widget, text_tools};
use crate::services::shortcuts::{self, Bar, Kind, Shortcut, Tool};

/// A shortcut's right click menu: where it was opened, the shortcut, the bar, and the panel it is about.
#[derive(Debug, Clone, PartialEq)]
pub struct ShortcutMenu {
    pub at: Pos2,
    pub name: String,
    pub bar: Bar,
    pub panel: Option<dock::Panel>,
}

impl UnluminousApp {
    /// What the rail needs to know to draw itself.
    pub(crate) fn rail_state(&self) -> activity_bar::RailState {
        let (text_options, view_modes) = self.tools_that_apply();
        activity_bar::RailState {
            explorer_visible: self.explorer_visible,
            editor_visible: self.editor_visible,
            git_open: self.git.as_ref().is_some_and(|git| git.panel.open),
            in_repository: self.repository_controls_apply(),
            terminal_visible: self.terminal.visible,
            realm_visible: self.realm.visible,
            realm_available: self.realm_is_on(),
            run_visible: self.run.visible,
            debug_visible: self.debug_panel.visible,
            text_options_apply: text_options,
            view_modes_apply: view_modes,
            open_tool: self.rail_flyout.map(|flyout| flyout.tool),
        }
    }

    /// Whether Text Options and View Modes can apply to the tab showing: only to a document, and then by its kind.
    pub(crate) fn tools_that_apply(&self) -> (bool, bool) {
        match self.files.active().is_a_document() {
            true => text_tools::which_apply(self.document().path()),
            false => (false, false),
        }
    }

    /// Every shortcut this window offers, in the gallery's order: the panes and tabs, then the three tools.
    pub fn shortcuts(&self) -> Vec<Shortcut> {
        let mut out = Vec::new();
        let buttons = self.plugin_rail_buttons();
        activity_bar::cards(self.rail_state(), &buttons, |card| {
            out.push(Shortcut { name: card.name.to_owned(), kind: card.kind, home: card.home });
        });
        out
    }

    /// Whether the shortcut `name` is on `bar`. A name this window does not offer is on neither.
    pub fn shortcut_is_pinned(&self, name: &str, bar: Bar) -> bool {
        let all = self.shortcuts();
        shortcuts::find(&all, name)
            .is_some_and(|one| shortcuts::is_pinned(&self.settings, &one.name, one.home, bar))
    }

    /// Put a shortcut on a bar or take it off: the one place a pin changes, whichever of the right click menus,
    /// the gallery and `window pin` asked. Answers whether anything changed, or the names there are when `name`
    /// is not one of them.
    /// @param name - the shortcut's name, in any case
    /// @param bar - the bar
    /// @param pinned - true to pin it, false to unpin it
    pub fn pin_the_shortcut(&mut self, name: &str, bar: Bar, pinned: bool) -> Result<bool, String> {
        let all = self.shortcuts();
        let Some(found) = shortcuts::find(&all, name) else {
            let names: Vec<&str> = all.iter().map(|one| one.name.as_str()).collect();
            return Err(format!(
                "There is no shortcut called {name}. The shortcuts are: {}.",
                names.join(", ")
            ));
        };
        let changed = match pinned {
            true => shortcuts::pin(&mut self.settings, &found.name, found.home, bar),
            false => shortcuts::unpin(&mut self.settings, &found.name, found.home, bar),
        };
        if changed {
            self.unsaved_settings = true;
        }
        // A tool taken off the rail takes its open flyout with it.
        if !pinned
            && bar == Bar::Rail
            && self.rail_flyout.is_some_and(|f| f.tool.name() == found.name)
        {
            self.rail_flyout = None;
        }
        Ok(changed)
    }

    /// Carry out a [`ShortcutAction`], which `run_action` hands here.
    pub(crate) fn run_a_shortcut_action(
        &mut self,
        what: ShortcutAction,
        ctx: Option<&egui::Context>,
    ) {
        match what {
            ShortcutAction::Pin { name, bar } => self.report_a_pin(&name, bar, true),
            ShortcutAction::Unpin { name, bar } => self.report_a_pin(&name, bar, false),
            ShortcutAction::Reset(bar) => {
                if shortcuts::reset(&mut self.settings, bar) {
                    self.unsaved_settings = true;
                }
                self.message = Some(format!(
                    "The {} is back to its default shortcuts.",
                    bar.label().to_lowercase()
                ));
            }
            ShortcutAction::Gallery(bar) => {
                let screen = ctx
                    .map(|ctx| ctx.content_rect())
                    .unwrap_or(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0)));
                let at = match bar {
                    Bar::Rail => Pos2::new(
                        screen.left() + crate::theme::size::ACTIVITY_BAR,
                        screen.top() + crate::theme::size::TITLE_BAR + 30.0,
                    ),
                    Bar::Top => Pos2::new(
                        screen.right() - 380.0,
                        screen.top() + crate::theme::size::TITLE_BAR - 6.0,
                    ),
                };
                self.shortcut_menu = None;
                self.shortcut_gallery = Some((at, bar));
            }
        }
    }

    /// Pin or unpin from a menu or the gallery, and say what happened in the status bar.
    fn report_a_pin(&mut self, name: &str, bar: Bar, pinned: bool) {
        self.message = Some(match self.pin_the_shortcut(name, bar, pinned) {
            Ok(_) if pinned => format!("Pinned {name} to the {}.", bar.label().to_lowercase()),
            Ok(_) => format!("Unpinned {name} from the {}.", bar.label().to_lowercase()),
            Err(refused) => refused,
        });
    }

    /// What the rail is asked to draw beside its state: the plugins' buttons, the order, and the pins.
    pub(crate) fn rail_input<'a>(
        &'a self,
        plugins: &'a [activity_bar::PluginButton],
    ) -> activity_bar::RailInput<'a> {
        let all = self.shortcuts();
        let on_the_rail =
            |one: &&Shortcut| shortcuts::is_pinned(&self.settings, &one.name, one.home, Bar::Rail);
        activity_bar::RailInput {
            plugins,
            order: &self.settings.rail_order,
            hidden: all
                .iter()
                .filter(|one| one.kind == Kind::Panel && !on_the_rail(one))
                .map(|one| one.name.clone())
                .collect(),
            tools: Tool::ALL
                .into_iter()
                .filter(|tool| {
                    shortcuts::is_pinned(&self.settings, tool.name(), Bar::Top, Bar::Rail)
                })
                .collect(),
        }
    }

    /// Whether a tool is on the top bar.
    pub(crate) fn tool_on_the_top_bar(&self, tool: Tool) -> bool {
        shortcuts::is_pinned(&self.settings, tool.name(), Bar::Top, Bar::Top)
    }

    /// The chips the top bar draws for the panes pinned to it, with the action each one's press is.
    pub(crate) fn top_bar_chips(&self) -> Vec<(shortcut_gallery::Chip, Action)> {
        let buttons = self.plugin_rail_buttons();
        let state = self.rail_state();
        let mut out = Vec::new();
        activity_bar::cards(state, &buttons, |card| {
            if card.kind != Kind::Panel
                || !shortcuts::is_pinned(&self.settings, card.name, card.home, Bar::Top)
            {
                return;
            }
            if let Some((on, action)) = Self::what_a_panel_shortcut_does(state, &buttons, card.name)
            {
                out.push((
                    shortcut_gallery::Chip { name: card.name.to_owned(), mark: card.mark, on },
                    action,
                ));
            }
        });
        out
    }

    /// Whether the pane a panel shortcut stands for is showing, and the action its press is. The same answers the
    /// rail's own button gives, so a chip and a rail button are one control in two places.
    fn what_a_panel_shortcut_does(
        state: activity_bar::RailState,
        plugins: &[activity_bar::PluginButton],
        name: &str,
    ) -> Option<(bool, Action)> {
        use crate::app::actions::{GitAction, RealmAction};
        Some(match name {
            "Project" => (state.explorer_visible, Action::ToggleExplorer),
            "Editing Area" => (state.editor_visible, Action::ToggleEditor),
            "Version Control" => (state.git_open, Action::Git(GitAction::Commit)),
            "Realm" => (state.realm_visible, Action::Realm(RealmAction::Toggle)),
            "Debug tile" => (state.debug_visible, Action::ToggleDebugTile),
            "Run tile" => (state.run_visible, Action::ToggleRunTile),
            "Terminal tile" => (state.terminal_visible, Action::ToggleTerminal),
            other => {
                let button = plugins.iter().find(|button| button.label == other)?;
                let action = match button.opens {
                    activity_bar::Opens::Pane => Action::PluginPane { pane: button.key.clone() },
                    activity_bar::Opens::Tab => Action::PluginTab { tab: button.key.clone() },
                };
                (button.on, action)
            }
        })
    }

    /// The right click menu of a shortcut and the gallery, drawn over everything else.
    pub(crate) fn show_the_shortcut_popups(
        &mut self,
        ui: &mut egui::Ui,
        action: &mut Option<Action>,
    ) {
        if let Some(menu) = self.shortcut_menu.clone() {
            let mut entries = match menu.panel {
                Some(panel) => {
                    let mut rows = actions::panel_menu(&self.menu_state(), panel);
                    rows.push(actions::Entry::Separator);
                    rows
                }
                None => Vec::new(),
            };
            let on_other = self.shortcut_is_pinned(&menu.name, menu.bar.other());
            entries.extend(actions::shortcut_menu(&menu.name, menu.bar, on_other));
            let outcome = context_menu::show(ui, "shortcut", menu.at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if outcome.close {
                self.shortcut_menu = None;
            }
        }
        if let Some((at, bar)) = self.shortcut_gallery {
            let cards = self.gallery_cards(bar);
            let outcome = shortcut_gallery::show(ui.ctx(), at, bar, &cards);
            if let Some(name) = outcome.toggled {
                let pinned = cards.iter().any(|card| card.name == name && card.pinned);
                self.report_a_pin(&name, bar, !pinned);
            }
            if outcome.reset {
                self.run_a_shortcut_action(ShortcutAction::Reset(bar), None);
            }
            if outcome.close {
                self.shortcut_gallery = None;
            }
        }
    }

    /// Every card the gallery for `bar` draws, with whether each is pinned to that bar.
    fn gallery_cards(&self, bar: Bar) -> Vec<GalleryCard> {
        let buttons = self.plugin_rail_buttons();
        let mut cards = Vec::new();
        activity_bar::cards(self.rail_state(), &buttons, |card| {
            cards.push(GalleryCard {
                name: card.name.to_owned(),
                mark: card.mark,
                kind: card.kind,
                pinned: shortcuts::is_pinned(&self.settings, card.name, card.home, bar),
            });
        });
        cards
    }

    /// Open, keep or close a tool's flyout from what its rail button reported, then draw the open one.
    ///
    /// Resting the pointer on a button opens its flyout, and it closes a quarter of a second after the pointer has
    /// left the button, the flyout and the corridor between them. A press opens it and holds it open until a
    /// press outside it or Escape. Nothing closes it while a popup it opened, the configuration dropdown, is open.
    pub(crate) fn show_the_rail_flyout(
        &mut self,
        ui: &mut egui::Ui,
        buttons: &[activity_bar::ToolButton],
        action: &mut Option<Action>,
    ) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|input| input.time);
        for button in buttons {
            let same = self.rail_flyout.is_some_and(|open| open.tool == button.tool);
            if button.clicked {
                let held =
                    self.rail_flyout.is_some_and(|open| open.tool == button.tool && open.held);
                self.rail_flyout = match held {
                    true => None,
                    false => Some(Flyout {
                        tool: button.tool,
                        anchor: button.rect,
                        held: true,
                        left_at: None,
                    }),
                };
            } else if button.hovered && !same && !self.rail_flyout.is_some_and(|open| open.held) {
                self.rail_flyout = Some(Flyout {
                    tool: button.tool,
                    anchor: button.rect,
                    held: false,
                    left_at: None,
                });
            }
        }
        // A tool unpinned from the rail since its flyout opened has no button any more.
        if let Some(open) = self.rail_flyout {
            match buttons.iter().find(|button| button.tool == open.tool) {
                Some(button) => self.rail_flyout = Some(Flyout { anchor: button.rect, ..open }),
                None => self.rail_flyout = None,
            }
        }
        let Some(open) = self.rail_flyout else { return };
        let card = self.draw_the_flyout(&ctx, &open, action);
        let popup_open = egui::Popup::is_any_open(&ctx);
        let with_it = shortcut_gallery::pointer_is_with_it(&ctx, open.anchor, card);
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let pressed_outside =
            ctx.input(|input| input.pointer.primary_pressed()) && !with_it && !popup_open;
        let close = escape
            || pressed_outside
            || match (open.held, with_it || popup_open, open.left_at) {
                (true, _, _) => false,
                (false, true, _) => {
                    self.rail_flyout = Some(Flyout { left_at: None, ..open });
                    false
                }
                (false, false, None) => {
                    self.rail_flyout = Some(Flyout { left_at: Some(now), ..open });
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                        shortcut_gallery::GRACE,
                    ));
                    false
                }
                (false, false, Some(left)) => {
                    if now - left < shortcut_gallery::GRACE {
                        ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                            shortcut_gallery::GRACE - (now - left),
                        ));
                    }
                    now - left >= shortcut_gallery::GRACE
                }
            };
        if close {
            self.rail_flyout = None;
        }
    }

    /// Draw the open flyout with its tool's controls, which are the top bar's own functions, and answer its card.
    fn draw_the_flyout(
        &mut self,
        ctx: &egui::Context,
        open: &Flyout,
        action: &mut Option<Action>,
    ) -> Rect {
        let (text_options, view_modes) = self.tools_that_apply();
        let mark = activity_bar::tool_mark(open.tool);
        match open.tool {
            Tool::TextOptions => {
                // The panel's own width, measured in the interface's face, less the margin the panel's frame took.
                let width = |ui: &egui::Ui| text_tools::panel_layout(ui).1 - 12.0;
                let (commands, card) = shortcut_gallery::tool_flyout(
                    ctx,
                    open,
                    mark,
                    width,
                    |ui| match text_options {
                        true => text_tools::text_options(ui, self.document(), &self.bold_family),
                        false => {
                            shortcut_gallery::cannot_apply(
                                ui,
                                "Text options apply to Markdown and text files.",
                            );
                            Vec::new()
                        }
                    },
                );
                for command in commands {
                    self.document_mut().apply(command);
                }
                card
            }
            Tool::ViewModes => {
                let path = self.document().path().map(std::path::Path::to_path_buf);
                let mode = self.view_mode();
                let width = |_: &egui::Ui| text_tools::track_width() + 8.0;
                let (picked, card) =
                    shortcut_gallery::tool_flyout(ctx, open, mark, width, |ui| match view_modes {
                        true => {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(text_tools::track_width() + 8.0, 42.0),
                                egui::Sense::hover(),
                            );
                            text_tools::view_modes(
                                ui,
                                Pos2::new(rect.left() + 4.0, rect.center().y),
                                path.as_deref(),
                                mode,
                            )
                        }
                        false => {
                            shortcut_gallery::cannot_apply(
                                ui,
                                "View modes apply to Markdown files and diagrams.",
                            );
                            None
                        }
                    });
                if let Some(mode) = picked {
                    self.set_view_mode(mode);
                }
                card
            }
            Tool::RunConfigurations => {
                let state = self.run_widget_state();
                let width = run_widget::width(&state);
                let nothing_to_list = state.rows.is_empty() && state.current_file.is_none();
                let (chosen, card) = shortcut_gallery::tool_flyout(
                    ctx,
                    open,
                    mark,
                    |_| width + 8.0,
                    |ui| {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(width + 8.0, 44.0),
                            egui::Sense::hover(),
                        );
                        let room = Rect::from_min_size(
                            Pos2::new(rect.left() + 4.0, rect.top()),
                            egui::vec2(width, rect.height()),
                        );
                        let chosen = run_widget::show(ui, room, &state);
                        // With nothing to choose from the widget is the play button alone, which says nothing on its own.
                        if nothing_to_list {
                            shortcut_gallery::cannot_apply(
                                ui,
                                "No run configurations yet. Play adds one.",
                            );
                        }
                        chosen
                    },
                );
                if chosen.is_some() {
                    *action = chosen;
                }
                card
            }
        }
    }
}
