//! `panel` -- which edge of the window each of the four core panels is docked to, and how big it
//! is. `dock::regions`, `task-1697`'s own arithmetic, is where the layout really lives; this is
//! just asking it questions and writing the answers into `settings::Panes`.

use super::*;

impl UnluminousApp {
    /// `panel` — which edge of the window each panel is docked to — `task-1697`.
    ///
    /// The command line half of the drag. It goes through `dock_the_panel`, which is the one place a
    /// panel moves, so a panel moved with the pointer and one moved from a script end up in exactly
    /// the same state.
    pub(crate) fn cli_panel(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "list" => self.cli_panel_list(request),
            "dock" => self.cli_panel_dock(request),
            "size" => self.cli_panel_size(request),
            // `task-1771`: every pane is zoomable with the wheel, and everything a person can do an agent
            // can do too. The two shapes a zoom takes are both here, so a caller does not have to know
            // which kind of panel it is asking about — a tile answers with its font size in points and
            // everything else with its multiplier, and both are the number that really decides how big the
            // pane is drawn.
            "zoom" => self.cli_panel_zoom(request),
            "reset" => self.cli_panel_reset(request),
            _ => no(
                request,
                code::UNKNOWN_COMMAND,
                format!("There is no command called {}.", request.command),
            ),
        }
    }

    /// The panel a `panel` command names: one of Unluminous's own four, or a contributed pane.
    ///
    /// `plugins pane agent-chat/chat --side right` is how a plugin's pane is spoken about everywhere else,
    /// and `panel zoom plugin-2` would make somebody count slots. Both are accepted; the slot names are
    /// what a window with no plugins in it still answers to.
    ///
    /// **One resolver rather than two.** `task-1771` gave `panel zoom` a resolver that took a pane and
    /// left `dock` and `size` with one that did not, and `task-1794` is what that cost: `panel size
    /// agent-chat/chat --width 1150` answered "There is no panel by that name" while the same pane
    /// zoomed happily, so the chat pane in the product video had to be widened by editing
    /// `settings.conf` between restarts. Two answers to "which panel is this" were one too many.
    fn cli_panel_named(&self, request: &Request) -> Option<dock::Panel> {
        let name = request.text("panel")?.trim().to_owned();
        if let Some(panel) = dock::Panel::from_name(&name) {
            return Some(panel);
        }
        self.plugin_ui.slot_of(&name).map(|slot| dock::Panel::Plugin(slot as u8))
    }

    /// Set a panel's zoom outright, which is what `panel zoom` does and what a test drives.
    ///
    /// A tile has no multiplier — its size is the terminal's font — so a factor there is read as a
    /// multiple of the size it starts at and lands on the nearest size the Settings window offers. That
    /// keeps one number saying how big a terminal is, which is the rule `step_the_zoom_of` follows.
    pub fn set_the_zoom_of(&mut self, panel: dock::Panel, factor: f32) {
        match panel.is_a_tile() {
            true => {
                let wanted = settings::Settings::new().terminal_font_size * factor;
                let nearest = settings::TERMINAL_FONT_SIZES
                    .iter()
                    .copied()
                    .min_by(|a, b| (a - wanted).abs().total_cmp(&(b - wanted).abs()))
                    .unwrap_or(wanted);
                self.settings.terminal_font_size = nearest;
            }
            false => {
                let was = self.panes.zoom_of(panel);
                self.panes.set_zoom_of(panel, factor);
                let now = self.panes.zoom_of(panel);
                self.keep_the_place_through_a_panels_zoom(panel, now / was, None);
            }
        }
        self.unsaved_settings = true;
    }

    /// The refusal that names them all, which is what a caller who guessed wrong needs.
    fn cli_no_such_panel(&self, request: &Request) -> Outcome {
        // The contributed panes are named too, because a refusal that lists only the built-in four
        // is what told the `task-1794` shoot that a plugin's pane could not be sized at all.
        let mut names: Vec<String> =
            dock::Panel::ALL.iter().map(|panel| panel.name().to_owned()).collect();
        names.extend(self.plugin_ui.pane_keys());
        no(
            request,
            code::NOT_FOUND,
            format!("There is no panel by that name. Unluminous has {}.", names.join(", ")),
        )
    }

    /// The name a contributed pane answers to, which is its `<plugin id>/<pane id>`.
    fn panel_wire_name(&self, panel: dock::Panel) -> String {
        match panel.plugin_slot() {
            Some(slot) => self
                .plugin_ui
                .pane_keys()
                .get(slot)
                .cloned()
                .unwrap_or_else(|| panel.name().to_owned()),
            None => panel.name().to_owned(),
        }
    }

    /// What a `panel` reply calls this panel in its sentence.
    ///
    /// `Panel::label` is a `&'static str` and so cannot name a contributed pane — its own comment says
    /// it is the fallback for a slot with no plugin in it. The sentence was therefore answering "Plugin
    /// pane is on the left" for every one of them, while the payload beside it named the pane properly.
    /// With two panes contributed that sentence is unactionable: it is the same wording whichever one
    /// was asked about. So a pane is called what its own header and the rail's tooltip call it, which is
    /// the manifest's `label` — `task-1794`, whose whole subject is a command that reported something
    /// other than what was true.
    fn panel_label(&self, panel: dock::Panel) -> String {
        match panel.plugin_slot() {
            Some(slot) => self
                .plugin_ui
                .pane(slot)
                .map(|pane| pane.label.clone())
                .unwrap_or_else(|| panel.label().to_owned()),
            None => panel.label().to_owned(),
        }
    }

    /// `list`. Split out of [`Self::cli_panel`] by `task-1984` §3.6.
    fn cli_panel_list(&mut self, request: &Request) -> Outcome {
        // Unluminous's own four. A contributed pane is listed by `plugins list` and moved by
        // `plugins pane`, which is the split `the_board_contributes_a_tab_and_no_pane`
        // states — `task-1794` widened what a pane can be *asked to do* rather than what
        // this listing is of, and the refusal from `cli_no_such_panel` is what makes a
        // pane's name discoverable from here.
        let rows: Vec<String> = dock::Panel::ALL
            .into_iter()
            .map(|panel| {
                let rect = self.panel_rects.of(panel);
                format!(
                    "{}{:<9} {:<7} {}  {:>4} x {:<4}  {}",
                    if self.panel_is_showing(panel) { "*" } else { " " },
                    panel.name(),
                    self.panes.dock.side_of(panel).name(),
                    self.panes.dock.order_of(panel),
                    self.panes.width_of(panel).round(),
                    self.panes.height_of(panel).round(),
                    match self.panel_is_showing(panel) {
                        true => format!(
                            "at {:.0},{:.0} {:.0} x {:.0}",
                            rect.left(),
                            rect.top(),
                            rect.width(),
                            rect.height()
                        ),
                        false => "not showing".to_owned(),
                    },
                )
            })
            .collect();
        let panels: Vec<Value> = dock::Panel::ALL
            .into_iter()
            .map(|panel| {
                let rect = self.panel_rects.of(panel);
                json!({
                    // The name a caller can ask this panel by. For a contributed pane that
                    // is its `<plugin id>/<pane id>`, not `plugin-2`, which would make an
                    // agent count slots to use what it had just been told — `task-1794`.
                    "panel": self.panel_wire_name(panel),
                    "label": self.panel_label(panel),
                    "side": self.panes.dock.side_of(panel).name(),
                    "position": self.panes.dock.order_of(panel),
                    "showing": self.panel_is_showing(panel),
                    "width": self.panes.width_of(panel),
                    "height": self.panes.height_of(panel),
                    "area": {
                        "x": rect.left(),
                        "y": rect.top(),
                        "width": rect.width(),
                        "height": rect.height(),
                    },
                })
            })
            .collect();
        let editor = self.panel_rects.editor;
        lines(
            request,
            format!(
                "{} panels, {} showing",
                dock::Panel::ALL.len(),
                dock::Panel::ALL.iter().filter(|panel| self.panel_is_showing(**panel)).count()
            ),
            rows,
            json!({
                "panels": panels,
                "editor": {
                    "x": editor.left(),
                    "y": editor.top(),
                    "width": editor.width(),
                    "height": editor.height(),
                },
            }),
        )
    }

    /// `dock`. Split out of [`Self::cli_panel`] by `task-1984` §3.6.
    fn cli_panel_dock(&mut self, request: &Request) -> Outcome {
        // A contributed pane is named here the way `panel zoom` already names one, and the
        // way `plugins pane --side` does. `task-1794`: every built-in panel's size and side
        // were settable and a plugin's pane's were not, which is the pane rule with a gap in
        // it — and the gap was found by a person having to edit `settings.conf` between
        // restarts to widen the chat pane for a video.
        let Some(panel) = self.cli_panel_named(request) else {
            return self.cli_no_such_panel(request);
        };
        let Some(side) = request.text("side").and_then(|side| dock::Side::from_name(side.trim()))
        else {
            return no(
                request,
                code::USAGE,
                "Say which edge to put it on: left, right, top or bottom.",
            );
        };
        let position = request.number("position").map(|at| at.max(0.0) as usize);
        self.dock_the_panel(panel, side, position);
        ok(
            request,
            format!("{} is on the {}", self.panel_label(panel), side.name()),
            json!({
                "panel": self.panel_wire_name(panel),
                "side": side.name(),
                "position": self.panes.dock.order_of(panel),
                "showing": self.panel_is_showing(panel),
            }),
        )
    }

    /// `size`. Split out of [`Self::cli_panel`] by `task-1984` §3.6.
    fn cli_panel_size(&mut self, request: &Request) -> Outcome {
        let Some(panel) = self.cli_panel_named(request) else {
            return self.cli_no_such_panel(request);
        };
        if let Some(width) = request.number("width") {
            let width = (width as f32)
                .clamp(self.panes.min_width_of(panel), self.panes.max_width_of(panel));
            self.panes.set_width_of(panel, width);
            self.unsaved_settings = true;
        }
        if let Some(height) = request.number("height") {
            let height = (height as f32).max(self.panes.min_height_of(panel));
            self.panes.set_height_of(panel, height);
            self.unsaved_settings = true;
        }
        ok(
            request,
            format!(
                "{} is {:.0} points wide and {:.0} tall",
                self.panel_label(panel),
                self.panes.width_of(panel),
                self.panes.height_of(panel)
            ),
            json!({
                "panel": self.panel_wire_name(panel),
                "width": self.panes.width_of(panel),
                "height": self.panes.height_of(panel),
                "side": self.panes.dock.side_of(panel).name(),
            }),
        )
    }

    /// `zoom`. Split out of [`Self::cli_panel`] by `task-1984` §3.6.
    fn cli_panel_zoom(&mut self, request: &Request) -> Outcome {
        let Some(panel) = self.cli_panel_named(request) else {
            return self.cli_no_such_panel(request);
        };
        let asked = request.text("factor").map(|said| said.trim().to_owned());
        match asked.as_deref() {
            None | Some("") => {}
            Some("reset") => self.reset_the_zoom_of(panel),
            // **Refused rather than clamped**, because the catalogue, the reference and the
            // refusal all name a range: `panel zoom explorer 100` answered "explorer is at 3.00x",
            // which is a command that did not do what it was asked and said it had. Found by the
            // `task-1771` review.
            Some(said) => match said.parse::<f32>() {
                Ok(factor)
                    if factor.is_finite()
                        && (settings::MIN_ZOOM..=settings::MAX_ZOOM).contains(&factor) =>
                {
                    self.set_the_zoom_of(panel, factor);
                }
                _ => return no(
                    request,
                    code::USAGE,
                    format!(
                        "`{said}` is not a zoom: say a number between {:.1} and {:.1}, or `reset`.",
                        settings::MIN_ZOOM,
                        settings::MAX_ZOOM
                    ),
                ),
            },
        }
        let tile = panel.is_a_tile();
        let said = match tile {
            true => format!(
                "{} is a character grid at {:.0} points",
                self.panel_label(panel),
                self.settings.terminal_font_size
            ),
            false => {
                format!("{} is at {:.2}x", self.panel_label(panel), self.panes.zoom_of(panel))
            }
        };
        ok(
            request,
            said,
            json!({
                "panel": self.panel_wire_name(panel),
                // A tile has no multiplier of its own, so it answers with the one thing that
                // decides its size, and says which of the two this is.
                "kind": if tile { "font size" } else { "zoom" },
                "zoom": self.panes.zoom_of(panel),
                "font_size": self.settings.terminal_font_size,
            }),
        )
    }

    /// `reset`. Split out of [`Self::cli_panel`] by `task-1984` §3.6.
    fn cli_panel_reset(&mut self, request: &Request) -> Outcome {
        self.reset_the_panel_layout();
        ok(
            request,
            "The panels are back where they started",
            json!({
                "panels": dock::Panel::ALL
                    .into_iter()
                    .map(|panel| json!({
                        "panel": panel.name(),
                        "side": self.panes.dock.side_of(panel).name(),
                    }))
                    .collect::<Vec<_>>(),
            }),
        )
    }
}
