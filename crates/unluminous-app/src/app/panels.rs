//! The panels: which are showing, where each one is, and the dividers and drop bands between them.
//!
//! Since `task-1697` the shape of the window is a value — which edge each panel is docked to, and
//! where in that edge — and `app::dock::regions` is the one function that turns it into rectangles.
//! Everything here either reads that or changes it.
//!
//! Maximising is in here for the same reason: a maximised pane is not a fifth kind of layout, it is
//! the layout there already is with one thing switched on and the rest switched off.

use egui::{Pos2, Rect, Vec2};

use crate::components::splitter;
use crate::settings::Panes;

use crate::app::{dock, space};
use crate::app::{Drag, Focus, Maximise, UnluminousApp};

impl UnluminousApp {
    /// Whether `panel` was showing before a pane was maximised, or is showing now when none is.
    ///
    /// What a project remembers is the arrangement a person chose, and maximising is not one: it is every
    /// other panel put away for as long as one pane fills the window. See [`Maximise`].
    pub(crate) fn was_showing(&self, panel: dock::Panel, now: bool) -> bool {
        match &self.maximised {
            Maximise::Filling { panels, .. } => panels[panel.index()],
            Maximise::No => now,
        }
    }

    /// Show the run tile, or put it away.
    ///
    /// **The bottom of the window holds one tile**, so showing this one puts the other two away —
    /// and its two siblings do the same in the other directions. Every path that shows any of the
    /// three goes through one of them, which is what stops them drifting apart: `terminal show` from
    /// the command line used to leave both `visible`, and the two grids were then drawn into the
    /// same rectangle, one over the other. `task-1687` made the pair a trio, on the same terms.
    pub fn show_the_run_tile(&mut self, showing: bool) {
        self.leave_the_maximised_pane();
        self.run.visible = showing;
        if showing {
            self.put_the_other_tiles_away(dock::Panel::Run);
            self.a_tile_took_the_keyboard(dock::Panel::Run);
        } else if self.focus == Focus::Terminal {
            self.focus = Focus::Editor;
        }
    }

    /// The rectangle a panel has, or **would** have if it were showing.
    ///
    /// The second half is what makes a run or a debug session started while its tile is put away
    /// open its grid at the size it is about to be drawn at, which `run_grid_size` records as not
    /// being a nicety: a pseudoconsole resized while its child is writing its first line loses that
    /// line. The rectangle a hidden panel would have is the layout with it switched on, worked out
    /// by the same function — which is why `dock::regions` takes what is showing rather than reading
    /// the window.
    /// The rectangle the last frame actually gave `panel`, which is `Rect::ZERO` when it was not showing.
    ///
    /// [`Self::panel_area`] answers a different question — where a panel *would* be — because a menu has
    /// to be able to offer to move a panel that is put away. A test asserting that a hidden pane takes no
    /// room wants this one.
    pub fn panel_rect_for_tests(&self, panel: dock::Panel) -> Rect {
        self.panel_rects.of(panel)
    }

    /// Everything below the title bar and right of the rail: the room every panel and the editing area
    /// are laid out inside. What `dock::regions` is given.
    pub fn panes_area(&self) -> Rect {
        self.panes_area
    }

    pub fn panel_area(&self, panel: dock::Panel) -> Rect {
        let rect = self.panel_rects.of(panel);
        if rect.width() > 1.0 && rect.height() > 1.0 {
            return rect;
        }
        if self.panes_area.width() <= 1.0 || self.panes_area.height() <= 1.0 {
            return Rect::ZERO;
        }
        let mut showing = self.panels_showing();
        showing[panel.index()] = true;
        // **Told whether the editing area is showing**, which this used to assume. A panel whose real
        // rectangle is degenerate is asked about again with itself switched on, and answering with the
        // layout of a window that is not on the screen is how a pseudoconsole was opened at the wrong
        // size while the editing area was hidden — which is the fault `task-1684` measured losing a
        // program's first line. `task-1905`.
        dock::regions_with(
            self.panes_area,
            &self.panes.dock,
            showing,
            &self.panes,
            self.editor_visible,
        )
        .of(panel)
    }

    /// Which panels are drawn at all, in [`dock::Panel::index`] order.
    ///
    /// The one place the four `visible` flags are gathered, so `regions`, `zones` and every command
    /// that asks where a panel is are looking at the same four booleans.
    pub fn panels_showing(&self) -> [bool; dock::SLOTS] {
        let mut showing = [false; dock::SLOTS];
        showing[dock::Panel::Explorer.index()] = self.explorer_visible;
        showing[dock::Panel::Terminal.index()] = self.terminal.visible;
        showing[dock::Panel::Run.index()] = self.run.visible;
        showing[dock::Panel::Debug.index()] = self.debug_panel.visible;
        showing[dock::Panel::Space.index()] = self.space.visible;
        // And whichever panes the plugins that are switched on are showing. A slot with no plugin in it
        // is never showing, so it takes no room and gets `Rect::ZERO`.
        for (slot, visible) in self.plugin_ui.visible().into_iter().enumerate() {
            showing[dock::Panel::Plugin(slot as u8).index()] = visible;
        }
        showing
    }

    /// Whether a panel is showing.
    pub fn panel_is_showing(&self, panel: dock::Panel) -> bool {
        self.panels_showing()[panel.index()]
    }

    /// Whether any panel at all is showing, which is what makes hiding the editing area safe.
    ///
    /// `task-28`: hiding the editing area with nothing else on the screen would leave a window holding the rail
    /// and a status bar. `Action::ToggleEditor` and `Action::ToggleExplorer` both read this.
    pub fn anything_is_showing_in_the_panes(&self) -> bool {
        self.panels_showing().iter().any(|showing| *showing)
    }

    /// Put away the other tiles **that share this one's side**.
    ///
    /// `task-1683`'s rule, which used to be written out three times as "the bottom of the window
    /// holds one of the three and never two". Its reason was that two character grids in one strip
    /// are two half-sized grids — a reason about a *strip* — so since `task-1697` the rule follows
    /// the strip: put the terminal on the right and it no longer competes with the run tile along
    /// the bottom, and both are showing at once, which is the point of being able to move it. The
    /// explorer is a list rather than a grid and never competes with anything.
    pub(crate) fn put_the_other_tiles_away(&mut self, showing: dock::Panel) {
        let side = self.panes.dock.side_of(showing);
        // Which contributed panes are tiles is what their manifests said, so the question is asked with the
        // surfaces to hand. A plugin pane in the bottom group is a tile like the other three, which means it
        // puts them away **and** they put it away: the rule is about a strip rather than about three panels.
        let tiles = self.plugin_panes_that_are_tiles();
        for panel in self.panes.dock.panels_on(side) {
            if panel == showing || !panel.is_a_tile_given(&tiles) {
                continue;
            }
            match panel {
                dock::Panel::Terminal => self.terminal.visible = false,
                dock::Panel::Run => self.run.visible = false,
                dock::Panel::Debug => self.debug_panel.visible = false,
                dock::Panel::Plugin(slot) => {
                    if let Some(key) = self.plugin_ui.pane_key(slot as usize) {
                        self.show_the_plugin_pane(&key, false);
                    }
                }
                // The explorer is a list and never competes with anything, and neither is the
                // canvas: it holds several grids inside itself deliberately, so the rule about a
                // strip holding one does not apply to it - `task-1904`.
                dock::Panel::Explorer | dock::Panel::Space => {}
            }
        }
    }

    /// Which contributed panes their manifests said are tiles, in slot order.
    ///
    /// A tile is a pane that may not share a strip: two of them in one are two half sized things, which is
    /// the rule that exists about a strip rather than about three particular panels. It used to be read off
    /// `pane.group`, and `task-1949` separated the two — see [`crate::services::plugins::PaneContribution::tile`]
    /// for why a board whose button belongs at the top is still a thing that wants a strip to itself.
    pub(crate) fn plugin_panes_that_are_tiles(&self) -> Vec<bool> {
        (0..self.plugin_ui.pane_count())
            .map(|slot| self.plugin_ui.pane(slot).is_some_and(|pane| pane.tile))
            .collect()
    }

    /// Show the terminal tile, or put it away, opening a shell if there is not one already.
    ///
    /// The second of the three; see [`Self::show_the_run_tile`] for why there is a function at all.
    pub fn show_the_terminal_tile(&mut self, showing: bool) {
        self.leave_the_maximised_pane();
        self.terminal.visible = showing;
        if showing {
            self.put_the_other_tiles_away(dock::Panel::Terminal);
            self.open_terminal_tab();
            self.a_tile_took_the_keyboard(dock::Panel::Terminal);
        } else if self.focus == Focus::Terminal {
            self.focus = Focus::Editor;
        }
    }

    /// Show the debug tile, or put it away.
    ///
    /// The third of the three. It does **not** take the keyboard: the tile holds a list and a tree
    /// rather than a grid a program is being typed into, and taking the keyboard away from the
    /// editor to look at a variable would mean pressing `F8` moved the caret rather than the
    /// program. The stepping keys work wherever the keyboard is, because they are menu entries.
    pub fn show_the_debug_tile(&mut self, showing: bool) {
        self.leave_the_maximised_pane();
        self.debug_panel.visible = showing;
        if showing {
            self.put_the_other_tiles_away(dock::Panel::Debug);
            if self.focus == Focus::Terminal {
                self.focus = Focus::Editor;
            }
        }
    }

    /// Fill the window with one pane, or put back what was showing before one did.
    ///
    /// `pane` is `None` for the editing area. Toggling the pane that is already maximised restores;
    /// toggling a different one maximises that one instead, keeping the same memory of what to restore, so
    /// double clicking your way round the window never loses the arrangement you started from.
    ///
    /// Public because the menu, the command line and a double click on a header all reach it, which is the
    /// one-action-one-place rule `run_action` keeps.
    pub fn toggle_maximised(&mut self, pane: Option<dock::Panel>) {
        // Every `show_*` below ends a maximise first, and this **is** the maximise. See
        // [`Self::settling_the_maximise`].
        self.settling_the_maximise = true;
        self.settle_the_maximise(pane);
        self.settling_the_maximise = false;
    }

    fn settle_the_maximise(&mut self, pane: Option<dock::Panel>) {
        match std::mem::replace(&mut self.maximised, Maximise::No) {
            // The same one again: put everything back where it was.
            Maximise::Filling { pane: was, editor, panels, focus } if was == pane => {
                self.editor_visible = editor;
                for panel in dock::Panel::all(self.plugin_ui.pane_count()) {
                    self.show_a_panel(panel, panels[panel.index()]);
                }
                // With nothing showing at all — which a window can be left in only by putting the last
                // panel away while the editing area was hidden — the editing area comes back, which is the
                // promise `Action::ToggleEditor` already makes.
                if !self.editor_visible && !self.anything_is_showing_in_the_panes() {
                    self.editor_visible = true;
                }
                // **And who had the keyboard.** Putting a panel back takes it: `show_the_terminal_tile`
                // hands the keys to the terminal whenever it is shown, which is right when somebody presses
                // its own button and wrong when a restore happens to bring it back.
                self.focus = focus;
            }
            // A different one, or none: this pane fills the window and the memory is kept.
            was => {
                let (editor, panels, focus) = match was {
                    Maximise::Filling { editor, panels, focus, .. } => (editor, panels, focus),
                    Maximise::No => (self.editor_visible, self.panels_showing(), self.focus),
                };
                for panel in dock::Panel::all(self.plugin_ui.pane_count()) {
                    self.show_a_panel(panel, pane == Some(panel));
                }
                self.editor_visible = pane.is_none();
                self.maximised = Maximise::Filling { pane, editor, panels, focus };
            }
        }
    }

    /// Put the window back the way it was, if a pane is filling it. What `Escape` means.
    ///
    /// Answers whether it did anything, because `Escape` has other meanings and the one that acts has to
    /// be the only one that acts.
    pub fn restore_the_maximised_pane(&mut self) -> bool {
        let Maximise::Filling { pane, .. } = self.maximised else {
            return false;
        };
        self.toggle_maximised(pane);
        true
    }

    /// Which pane is filling the window, for a test and for `panel list`.
    pub fn maximised_pane(&self) -> Option<Option<dock::Panel>> {
        match self.maximised {
            Maximise::Filling { pane, .. } => Some(pane),
            Maximise::No => None,
        }
    }

    /// Show or hide one panel, whichever kind it is.
    ///
    /// The four have their own flags and a contributed pane has a registry, so "put this panel away" was
    /// four `match` arms written out wherever it was needed. One function, so maximising cannot forget a
    /// kind of panel the day a fifth is added — the compiler names it here.
    /// Put the window back before a panel is shown or hidden by anything but the maximise itself.
    ///
    /// **A maximised window is one pane and nothing else**, so a toggle inside it has no arrangement to
    /// change: hiding the maximised pane left a body with nothing in it at all, and showing a second one
    /// left two panes up with the menu still offering `Restore Pane`. The arrangement comes back first and
    /// the toggle then means what it has always meant. Found by the `task-1771` review.
    pub(crate) fn leave_the_maximised_pane(&mut self) {
        if self.maximised != Maximise::No && !self.settling_the_maximise {
            self.restore_the_maximised_pane();
        }
    }

    /// A tile took the keyboard. `Focus::Terminal` says a grid has it and cannot say which of the three,
    /// so the answer is kept beside it — see [`Self::tile_with_the_keyboard`].
    pub(crate) fn a_tile_took_the_keyboard(&mut self, tile: dock::Panel) {
        self.focus = Focus::Terminal;
        self.tile_with_the_keyboard = tile;
    }

    pub(crate) fn show_a_panel(&mut self, panel: dock::Panel, showing: bool) {
        match panel {
            dock::Panel::Explorer => self.explorer_visible = showing,
            dock::Panel::Terminal => self.show_the_terminal_tile(showing),
            dock::Panel::Run => self.show_the_run_tile(showing),
            dock::Panel::Debug => self.show_the_debug_tile(showing),
            dock::Panel::Space => {
                self.space.visible = showing;
                if !showing {
                    // A gesture is a pointer half way through something, and the pointer is about to
                    // be somewhere else entirely. A `Wiring` left set draws a line from a node nobody
                    // can see to wherever the pointer now is, and a `Panning` left set pans the
                    // canvas on the next drag anywhere. `task-1922`.
                    self.space.gesture = space::Gesture::None;
                    if matches!(self.focus, Focus::Space) {
                        self.focus = Focus::Editor;
                    }
                }
            }
            dock::Panel::Plugin(slot) => {
                if let Some(key) = self.plugin_ui.pane_key(slot as usize) {
                    self.show_the_plugin_pane(&key, showing);
                }
            }
        }
    }

    /// Which panel holds the keyboard, or `None` for the editing area.
    ///
    /// `Focus` is the one value that says who holds it, so it is the one thing asked. Two cases are worth
    /// writing down. The three tiles share `terminal.font.size` and are one strip at a time, so it does not
    /// matter which of them `Focus::Terminal` meant. And a plugin showing as a **tab** is in the editing
    /// area, so it is not a panel and the keys are the editing area's; only a contributed **pane** answers.
    pub(crate) fn the_pane_the_keys_hold(&self) -> Option<dock::Panel> {
        match self.focus {
            Focus::Editor => None,
            Focus::Explorer => Some(dock::Panel::Explorer),
            Focus::Space => Some(dock::Panel::Space),
            Focus::Terminal => Some(self.tile_with_the_keyboard),
            Focus::Plugin => {
                let plugin = self.plugin_with_the_keyboard.as_deref()?;
                (0..self.plugin_ui.pane_count())
                    .filter(|slot| self.plugin_ui.is_visible(*slot))
                    .find(|slot| self.plugin_ui.plugin_of(*slot).as_deref() == Some(plugin))
                    .map(|slot| dock::Panel::Plugin(slot as u8))
            }
        }
    }

    /// Take down what a panel's header reported: that it is in the air, or that it was right clicked.
    ///
    /// Each panel says this as it is drawn and none of them can act on it, because where a panel
    /// lands depends on where every *other* panel ended up — see [`Self::settle_the_panel_drag`].
    pub(crate) fn note_a_panel_grab(
        &mut self,
        panel: dock::Panel,
        grab: crate::components::dock::Grab,
    ) {
        if let Some(at) = grab.carrying {
            self.panel_drag = Drag::carrying(panel, at, grab.dropped);
        }
        if let Some(at) = grab.menu {
            self.panel_menu = Some((at, panel));
        }
        // Two presses on a header fill the window with that panel, and two more put everything back.
        // `task-1771`, and it is the gesture every window manager gives a title bar.
        if grab.twice {
            self.toggle_maximised(Some(panel));
        }
    }

    /// One divider a panel, along the edge that faces the editing area.
    ///
    /// A left column's is on its right and a right column's on its left, so in both cases the edge a
    /// person reaches for is the one between the panel and the document; the sign of the drag follows
    /// from the side, in this one place. A **strip** along the top or the bottom has one depth rather
    /// than one a panel, so its divider runs the whole way across and moves every panel in it
    /// together, with a divider between each pair of columns for their widths.
    pub(crate) fn show_the_panel_dividers(&mut self, ui: &mut egui::Ui, panes: Rect) {
        let showing = self.panels_showing();
        for side in dock::Side::ALL {
            let here: Vec<dock::Panel> = self
                .panes
                .dock
                .panels_on(side)
                .into_iter()
                .filter(|panel| showing[panel.index()])
                .filter(|panel| self.panel_rects.of(*panel).width() > 0.0)
                .collect();
            let Some(first) = here.first().copied() else {
                continue;
            };
            if side.is_a_column() {
                for panel in here {
                    let rect = self.panel_rects.of(panel);
                    let x = match side {
                        dock::Side::Right => rect.left(),
                        _ => rect.right(),
                    };
                    let sign = match side {
                        dock::Side::Right => -1.0,
                        _ => 1.0,
                    };
                    let line = Rect::from_min_size(
                        Pos2::new(x, rect.top()),
                        Vec2::new(1.0, rect.height()),
                    );
                    let drag = splitter::show(ui, line, panel.name(), splitter::Axis::Upright);
                    self.act_on_a_panel_divider(panel, drag, sign, panes);
                }
                continue;
            }
            // A strip: the outer divider is the whole strip's, and it moves every panel in it.
            let strip = here
                .iter()
                .map(|panel| self.panel_rects.of(*panel))
                .fold(self.panel_rects.of(first), |whole, rect| whole.union(rect));
            let (y, sign) = match side {
                dock::Side::Top => (strip.bottom(), 1.0),
                _ => (strip.top(), -1.0),
            };
            let line =
                Rect::from_min_size(Pos2::new(strip.left(), y), Vec2::new(strip.width(), 1.0));
            let drag = splitter::show(ui, line, first.name(), splitter::Axis::Flat);
            for panel in here.iter().copied() {
                self.act_on_a_panel_divider(panel, drag, sign, panes);
            }
            // And one between each pair of columns, for the width of the one on its left.
            for panel in here.iter().copied().take(here.len().saturating_sub(1)) {
                let rect = self.panel_rects.of(panel);
                let line = Rect::from_min_size(
                    Pos2::new(rect.right(), rect.top()),
                    Vec2::new(1.0, rect.height()),
                );
                let id = format!("{} width", panel.name());
                let drag = splitter::show(ui, line, &id, splitter::Axis::Upright);
                if drag.delta != 0.0 {
                    let room = (panes.width() - dock::EDITOR_MIN_WIDTH).max(1.0);
                    let width = (self.panes.width_of(panel) + drag.delta).clamp(
                        self.panes.min_width_of(panel),
                        self.panes.max_width_of(panel).min(room),
                    );
                    self.panes.set_width_of(panel, width);
                    self.unsaved_settings = true;
                }
                if drag.reset {
                    self.panes.set_width_of(panel, Panes::new().width_of(panel));
                    self.unsaved_settings = true;
                }
            }
        }
    }

    /// What a drag on one panel's divider is worth, in whichever measurement its side reads.
    fn act_on_a_panel_divider(
        &mut self,
        panel: dock::Panel,
        drag: splitter::Drag,
        sign: f32,
        panes: Rect,
    ) {
        if drag.delta != 0.0 {
            // **Which of the two paths depends on whether the stored sizes are the drawn ones**, not on
            // whether the editing area is showing. See `the_sizes_are_being_shared`: `dock::share_the_depth`
            // scales a side down whenever the panels ask for more room than there is, and that happens with
            // the editing area showing too — which is what `task-1907` reports and what the gate on
            // `editor_visible` missed.
            match !self.editor_visible || self.the_sizes_are_being_shared(panel) {
                true => self.move_a_divider_by_sharing(panel, drag.delta * sign),
                false => {
                    let room = match self.panes.dock.side_of(panel).is_a_column() {
                        true => panes.width() - dock::EDITOR_MIN_WIDTH,
                        false => panes.height() - dock::EDITOR_MIN_HEIGHT,
                    };
                    self.panes.resize(panel, drag.delta * sign, room);
                }
            }
            self.unsaved_settings = true;
        }
        if drag.reset {
            self.panes.reset_size_of(panel);
            self.unsaved_settings = true;
        }
    }

    /// Whether the panels on `panel`'s axis are being drawn at the sizes they asked for.
    ///
    /// **The question `act_on_a_panel_divider` has to ask before it adds a drag to a stored number.**
    /// `dock::share_the_depth` scales a side down whenever the two strips together want more room than there
    /// is, so a panel's stored height is then a *share* rather than a size — and adding ten points of pointer
    /// to a share that is already against its clamp moves the divider not at all. Measured on a 670 point
    /// window with the canvas alone along the bottom: stored 560, drawn 550, and a drag up of 120 points
    /// bought ten. With the Agent-Tasks board above it, the first drag up bought nothing and the next bought
    /// 2.4 points of 120.
    ///
    /// `task-1771` found this with the editing area hidden and fixed it there, gated on `editor_visible`; the
    /// scaling happens with the editing area showing too, which is what `task-1907` reports. Comparing the
    /// two numbers is the honest form of the question.
    ///
    /// **Read off the rectangles the frame really drew** rather than by running `share_the_depth` again here,
    /// which is `follow_the_open_file`'s rule: a second computation is a second place for the two to
    /// disagree. A panel that is not showing has no rectangle and is skipped.
    fn the_sizes_are_being_shared(&self, panel: dock::Panel) -> bool {
        let side = self.panes.dock.side_of(panel);
        let column = side.is_a_column();
        let showing = self.panels_showing();
        dock::Panel::all(self.plugin_ui.pane_count())
            .into_iter()
            .filter(|one| showing[one.index()])
            .filter(|one| self.panes.dock.side_of(*one).is_a_column() == column)
            .any(|one| {
                let rect = self.panel_rects.of(one);
                let (drawn, asked) = match column {
                    true => (rect.width(), self.panes.width_of(one)),
                    false => (rect.height(), self.panes.height_of(one)),
                };
                // **Smaller than it asked for, not merely different from it**, and that asymmetry is the whole
                // of the question. `share_the_depth` only ever *scales down*, so a panel drawn short of what it
                // asked for is a panel being shared. A panel drawn **larger** is `lay_a_strip_out` equalising a
                // strip — every panel in a strip is drawn at the deepest one's depth, because one that wanted
                // less would leave a hole — and reading that as sharing sent an ordinary drag down the wrong
                // path and rewrote the shorter panel's stored height. The Codex Sol review of `task-1907` found
                // it, and an absolute difference is what made it possible.
                drawn > 0.0 && asked - drawn > 0.5
            })
    }

    /// Move a divider by taking what one side gains off the side facing it.
    ///
    /// **The room is shared out in proportion whenever the panels ask for more than there is**, which
    /// `dock::fill_the_depth` and `dock::share_the_depth` do so that nothing leaves a hole. That is right for the
    /// layout and wrong for a drag: a panel's stored measurement is then a *share* rather than a size, so ten
    /// points of pointer became three points of movement, less the wider it got — and it stopped altogether
    /// at what [`crate::settings::PANEL_MAX_WIDTH`] used to be. `task-1771` reports that as the Agent-Chat
    /// pane having a maximum width; the cap is gone, and this is the other half.
    ///
    /// Two steps, and the first is what makes the second exact. **Write the rendered measurements back**, so
    /// the stored numbers add up to the room and the proportional share is the identity. Then **take what
    /// this side gains off the side facing it**, so they go on adding up to the room and the divider lands
    /// under the pointer rather than a fraction of the way towards it.
    ///
    /// **A column's side is the sum of its panels and a strip's side is the greatest of them**, which is
    /// `dock::regions`' own rule, and it is what decides who moves. Growing a column's side by `by` means
    /// growing the one panel that was dragged; growing a strip's side means growing **every** panel on it,
    /// because they share one depth — a strip whose other panel still asked for the old depth did not move
    /// at all, which is the case the `task-1771` review found.
    ///
    /// With nothing on the far side there is nothing to take from and a side cannot grow: it already has
    /// everything, which is what its divider sitting against the edge of the window says.
    fn move_a_divider_by_sharing(&mut self, panel: dock::Panel, by: f32) {
        let side = self.panes.dock.side_of(panel);
        let column = side.is_a_column();
        let showing = self.panels_showing();
        let contributed = self.plugin_ui.pane_count();
        let measured = |panes: &Panes, one: dock::Panel| match column {
            true => panes.width_of(one),
            false => panes.height_of(one),
        };
        let smallest = |panes: &Panes, one: dock::Panel| match column {
            true => panes.min_width_of(one),
            false => panes.min_height_of(one),
        };
        let set = |panes: &mut Panes, one: dock::Panel, value: f32| match column {
            true => panes.set_width_of(one, value),
            false => panes.set_height_of(one, value),
        };
        // Every panel on this axis, laid out as it is on the screen right now.
        let here: Vec<dock::Panel> = dock::Panel::all(contributed)
            .into_iter()
            .filter(|one| showing[one.index()])
            .filter(|one| self.panes.dock.side_of(*one).is_a_column() == column)
            .collect();
        for one in here.iter().copied() {
            let rect = self.panel_rects.of(one);
            let drawn = match column {
                true => rect.width(),
                false => rect.height(),
            };
            let least = smallest(&self.panes, one);
            if drawn > 0.0 {
                set(&mut self.panes, one, drawn.max(least));
            }
        }
        let mine: Vec<dock::Panel> =
            here.iter().copied().filter(|one| self.panes.dock.side_of(*one) == side).collect();
        let facing: Vec<dock::Panel> = here
            .into_iter()
            .filter(|one| self.panes.dock.side_of(*one) == side.opposite())
            .collect();
        // How deep each side is, and how far each can be squeezed — by its own rule.
        let depth = |panes: &Panes, of: &[dock::Panel]| -> f32 {
            match column {
                true => of.iter().map(|one| measured(panes, *one)).sum(),
                false => of.iter().map(|one| measured(panes, *one)).fold(0.0_f32, f32::max),
            }
        };
        let floor = |panes: &Panes, of: &[dock::Panel]| -> f32 {
            match column {
                true => of.iter().map(|one| smallest(panes, *one)).sum(),
                false => of.iter().map(|one| smallest(panes, *one)).fold(0.0_f32, f32::max),
            }
        };
        let givable = (depth(&self.panes, &facing) - floor(&self.panes, &facing)).max(0.0);
        let sparable = (depth(&self.panes, &mine) - floor(&self.panes, &mine)).max(0.0);
        // **And the editing area gives room too, which is what it is between the two sides for.**
        // Without this, a strip whose facing side has no panels on it could not grow at all — `givable` was
        // zero, the drag was clamped to nothing, and the divider did not move however far the pointer went.
        // That is `task-1907`'s report with the canvas alone along the bottom: measured, a drag of 120 points
        // bought ten. What the editing area has spare is whatever it is drawn at above its own minimum, and
        // it is asked for the same measurement the panels are.
        let from_the_editor = match self.editor_visible {
            true => {
                // The rectangle the frame really gave it, for `the_sizes_are_being_shared`'s own reason:
                // one answer read back rather than a second computation that could disagree.
                let editor = self.panel_rects.editor;
                let (drawn, least) = match column {
                    true => (editor.width(), dock::EDITOR_MIN_WIDTH),
                    false => (editor.height(), dock::EDITOR_MIN_HEIGHT),
                };
                (drawn - least).max(0.0)
            }
            false => 0.0,
        };
        let by = by.clamp(-sparable, givable + from_the_editor);
        if by == 0.0 {
            return;
        }
        match column {
            // One panel moves and its side's total moves with it, because a column's side is a sum.
            true => {
                let was = measured(&self.panes, panel);
                set(&mut self.panes, panel, was + by);
            }
            // Every panel moves, because a strip's side is one depth and the greatest of them is it.
            false => {
                for one in mine {
                    let was = measured(&self.panes, one);
                    let least = smallest(&self.panes, one);
                    set(&mut self.panes, one, (was + by).max(least));
                }
            }
        }
        // And the far side gives it up: in proportion to what each panel has to give for a column, and
        // together for a strip, which is the same rule read the other way round.
        for one in facing {
            let spare = (measured(&self.panes, one) - smallest(&self.panes, one)).max(0.0);
            let share = match column {
                true if givable > 0.0 => by * (spare / givable),
                true => 0.0,
                false => by.min(spare),
            };
            let now = measured(&self.panes, one);
            set(&mut self.panes, one, now - share);
        }
    }

    /// The four places a panel in the air can be let go, and the one it would land in.
    ///
    /// The strong rectangle is **the layout**, not a picture of it: `dock::regions` run over the
    /// arrangement as it would be after the drop, so the preview and the drop are one function
    /// applied to one value and cannot come apart.
    pub(crate) fn show_the_drop_zones(&mut self, ui: &mut egui::Ui, panes: Rect) {
        let Some((panel, at, _)) = self.panel_drag.in_the_air() else {
            return;
        };
        let showing = self.panels_showing();
        // **Every one of these three asks about the layout that is really on the screen**, which means
        // each is told whether the editing area is showing. They read `dock::regions`, which means "with
        // the editing area showing", so with it hidden the bands, the answer and the strong rectangle
        // were all worked out against a window nobody was looking at. `task-1905`.
        let editor = self.editor_visible;
        let bands = dock::zones(panes, &self.panes.dock, showing, &self.panes, editor);
        let aimed = dock::target(panes, &self.panes.dock, showing, &self.panes, panel, at, editor);
        let landing = match aimed {
            Some((side, position)) => {
                let after = self.panes.dock.with(panel, side, Some(position));
                dock::regions_with(panes, &after, showing, &self.panes, editor).of(panel)
            }
            None => Rect::ZERO,
        };
        crate::components::dock::zones(ui, &bands, aimed.map(|(side, _)| side), landing, panel);
    }

    /// Where the panel that was let go actually landed.
    ///
    /// After every panel has been drawn, which is the earliest moment anything knows where all of
    /// them are — `settle_the_tab_drag`'s shape, and it exists for the same reason.
    pub(crate) fn settle_the_panel_drag(&mut self, ctx: &egui::Context, panes: Rect) {
        let Some((panel, at, dropped)) = self.panel_drag.in_the_air() else {
            return;
        };
        if !dropped {
            return;
        }
        self.panel_drag = Drag::Nothing;
        let showing = self.panels_showing();
        // Let go over the document rather than over an edge, nothing happens: a drag can be thought
        // better of, which is what the explorer's row drag and the tab drag both already promise.
        if let Some((side, position)) = dock::target(
            panes,
            &self.panes.dock,
            showing,
            &self.panes,
            panel,
            at,
            self.editor_visible,
        ) {
            self.dock_the_panel(panel, side, Some(position));
        }
        ctx.request_repaint();
    }

    /// Move a panel to an edge of the window.
    ///
    /// The one place a panel moves, which is what the drag, the panel's own menu and `unluminous-cli
    /// panel dock` all go through — `run_action`'s rule applied to a fifth thing.
    pub fn dock_the_panel(
        &mut self,
        panel: dock::Panel,
        side: dock::Side,
        position: Option<usize>,
    ) {
        let before = self.panes.dock;
        self.panes.dock.dock(panel, side, position);
        if self.panes.dock == before {
            return;
        }
        self.unsaved_settings = true;
        // A tile arriving on a side puts the other tiles there away, for the reason two grids never
        // shared the bottom of the window: two in one strip are two half-sized grids.
        if panel.is_a_tile() && self.panel_is_showing(panel) {
            self.put_the_other_tiles_away(panel);
        }
        self.message = Some(format!("{} is on the {}", panel.label(), side.name()));
    }

    /// Put every panel back where it started, which is what `Reset Panel Layout` means.
    pub fn reset_the_panel_layout(&mut self) {
        self.panes.dock.reset();
        for panel in dock::Panel::ALL {
            self.panes.reset_size_of(panel);
        }
        // And the other measurement of each, because `reset_size_of` sets the one its side reads and
        // this is meant to put the whole arrangement back rather than half of it.
        let fresh = Panes::new();
        self.panes.explorer_width = fresh.explorer_width;
        self.panes.explorer_height = fresh.explorer_height;
        self.panes.terminal_width = fresh.terminal_width;
        self.panes.terminal_height = fresh.terminal_height;
        self.panes.run_width = fresh.run_width;
        self.panes.run_height = fresh.run_height;
        self.panes.debug_width = fresh.debug_width;
        self.panes.debug_height = fresh.debug_height;
        // And the panes the plugins contribute, which a fresh `Layout` has none of. Without this the
        // reset does not move them, it **loses** them: the dock stops believing they exist and they
        // are drawn nowhere, while the rail, the settings file and every command go on saying they
        // are showing. `task-1794`. They go back to their manifests' side and size, because that is
        // what "back where they started" means for a pane nobody dragged.
        self.place_the_plugin_panes(false);
        self.unsaved_settings = true;
        self.message = Some("The panels are back where they started".to_owned());
    }
}
