//! What one frame of the window does, in the order it does it.
//!
//! `UnluminousApp::ui` is the list and every method below it is one phase of it. Two things decide
//! where a phase goes and neither is a preference: **egui hands a pointer to the last widget that
//! asked for the point**, so a phase moved a line either way is a behaviour change, and the
//! `frame_trace` labels measure the run of statements between one and the next.
//!
//! [`FramePlaces`] is why the phases can be methods at all. Every rectangle is worked out once, at
//! the top of the frame, and handed on — a phase measuring its own would be a second answer to a
//! question that has one.

use std::path::PathBuf;

use egui::{CornerRadius, Pos2, Rect, Vec2};

use crate::components::about_dialog::{self};
use crate::components::activity_bar;
use crate::components::context_menu;
use crate::components::debug_panel::{self};
use crate::components::explorer;
use crate::components::find_in_files::{self};
use crate::components::go_to_file::{self};
use crate::components::prompt_dialog::{self};
use crate::components::resize_edges;
use crate::components::run_dialog::{self};
use crate::components::run_panel::{self};
use crate::components::run_widget;
use crate::components::settings_dialog::{self};
use crate::components::splitter;
use crate::components::status_bar;
use crate::components::terminal_panel::{self};
use crate::components::text_menu;
use crate::components::text_tools;
use crate::components::title_bar::{self};
use crate::services;
use crate::services::debuggers;
use crate::services::file_kind;
use crate::services::project_state::{self};
use crate::theme::{self, color, size};

use crate::app::actions::{Action, RunAction};
use crate::app::{
    a_modal_has_the_keyboard, git_colour, hold_the_keyboard, split_the_debug,
    text_box_has_the_keyboard, Answer, Confirmation, DebugSplit, Drag, Focus, FramePlaces,
    Maximise, UnluminousApp, ZoomClaim, HEARTBEAT,
};
use crate::app::{actions, dock};

use crate::services::browser::BrowserTab;

impl UnluminousApp {
    /// Draw the whole window. Split out from the `eframe::App` implementation so the screenshot tests can
    /// drive it without a real window.
    ///
    /// A frame is a list of phases and this is the list. Each is a method below, named for what it
    /// does, and each is called exactly where its statements used to be: **egui hands a pointer to the
    /// last widget that asked for the point**, so a phase moved a line either way is a behaviour
    /// change. The comments that say why one phase is after another are here; the ones that explain
    /// what a phase does went with it.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        crate::theme::keep_egui_on_the_theme(ui.ctx());
        // Unluminous's own glyphs are rasterised for the display the window is on, which moves when the window
        // is dragged to a screen of another density. `task-2216`.
        self.renderer.follow_the_display(ui.ctx().pixels_per_point());
        // And the interface's zoom taken back out of the editor's and the terminal's sizes, with every file
        // laid out again from nothing when it moves. `task-2280`. From nothing, because a relayout keeps every
        // paragraph whose text and formatting did not change, and a zoom changes neither: the old advances
        // were kept and the letters drawn on top of each other.
        if self.renderer.follow_the_interface(ui.ctx().zoom_factor()) {
            for file in self.files.iter_mut() {
                file.cached.layout = Default::default();
            }
            self.set_the_font_everywhere();
        }
        crate::services::browser::follow_the_interface(ui.ctx().zoom_factor());
        self.begin_the_frame(ui);
        // Usage statistics: two comparisons on a frame where nothing is due. `task-2271`.
        self.keep_the_telemetry(ui.ctx());
        // A double click at the pace the system's mouse settings accept, given once. `task-2280`.
        if let Some(interval) = self.double_click_interval.take() {
            crate::services::double_click::use_interval(ui.ctx(), interval);
        }
        // Where every modal was left, read back once and written when a drag ends. `task-2280`.
        self.keep_the_modal_places(ui.ctx());
        self.take_what_the_threads_answered(ui);
        // Before any button is drawn, so that on the very first frame the focus is here and not on the
        // first thing in the title bar.
        hold_the_keyboard(ui);
        self.route_the_escape_key(ui);
        // Before anything else reads the frame's input, so a file dropped while the Report an Issue modal is
        // open is attached rather than copied into the folder pane behind it. `task-2276`.
        self.take_the_report_input(ui.ctx());
        self.ask_for_the_next_frame_and_let_the_plugins_catch_up(ui);

        let places = self.lay_the_frame_out(ui);
        // Before the menus are built, so `Recent Projects` lists a project another window has just opened.
        self.keep_the_recent_projects_current();
        // **The menus, built once** (`task-1984` A6). `menu_state` was built at least twice a frame
        // and `menus` once, plus once more inside `action_for_key` for every key press -- and
        // `menu_state` clones the recent list and every plugin's menu tree and asks the run
        // detectors, which read and parse `package.json`. One tree, handed to the bar that draws it
        // and to the shortcuts that search it, so a chord and a click cannot answer differently
        // about the same frame either.
        let menus = actions::menus(&self.menu_state());
        crate::services::frame_trace::phase("menus");
        // What a menu entry, a key chord, a button or a right click asked for. One is run at the end of
        // the frame, after everything that could ask has been drawn.
        let mut action = None;
        self.show_the_title_bar(ui, &places, &menus, &mut action);
        self.show_the_rail(ui, &places, &mut action);
        self.read_the_menu_shortcuts(ui, &menus, &mut action);
        crate::services::frame_trace::phase("chrome");

        self.show_the_explorer(ui, &places);
        crate::services::frame_trace::phase("explorer");

        self.route_the_keys_before_the_panes(ui, &mut action);
        let pane_rects = self.show_the_panes(ui, &places);
        self.show_the_dividers_between_panes(ui, &places, &pane_rects);
        self.show_the_menus_over_the_panes(ui, &mut action);

        self.show_the_contributed_panes(ui);
        crate::services::frame_trace::phase("contributed");
        self.show_the_canvas(ui, &mut action);
        crate::services::frame_trace::phase("canvas");
        // After the canvas as well as after the panes, because since `task-1905` a File Editor node
        // can hold a tab and a Folder node can report a file being carried out of it.
        self.settle_the_drags(ui, &pane_rects);

        self.show_the_tiles(ui, &places, &mut action);
        self.settle_the_maximise_and_the_zoom(ui);
        self.show_the_panel_furniture(ui, &places, &mut action);
        self.read_what_the_programs_said(ui, &mut action);
        self.show_the_status_bar(ui, &places);
        crate::services::frame_trace::phase("status");

        self.take_what_the_workers_answered(ui);
        // The one confirmation, drawn over whatever asked it and before every other modal.
        self.show_the_confirmation(ui.ctx());
        if let Some(chosen) = self.show_git_windows(ui.ctx()) {
            action = Some(chosen);
        }
        // The modals, newest first: the one a person asked for most recently belongs on top of the
        // older ones.
        self.show_the_prompt(ui);
        self.show_the_new_project_dialog(ui);
        self.show_the_background_grid(ui);
        self.show_the_command_palette(ui);
        self.show_go_to_file(ui);
        self.show_find_in_files(ui);
        // The references, the candidate list and the rename, which are one modal wearing three faces.
        self.show_the_references(ui);
        self.show_the_about_box(ui);
        self.show_the_report(ui);
        self.show_the_setup(ui);
        // The two debug modals, beside the other project-wide dialogs and for the same reason: only
        // one is ever open, so where they are drawn among the others decides nothing.
        self.show_the_debug_modals(ui.ctx());
        self.show_the_run_dialog(ui);
        self.show_the_settings_window(ui);
        crate::services::frame_trace::phase("modals");

        // The right click menu a link opened, over every pane and modal that could have drawn the link.
        self.show_the_link_menu(ui.ctx());
        self.show_the_notices(ui, &places);
        self.show_the_resize_grips(ui, &places);
        crate::services::frame_trace::phase("notices");

        // What a notebook's keys, toolbar or cell buttons asked for, when nothing else asked first.
        if action.is_none() {
            action = self.notebook_wanted.take();
        }
        self.notebook_wanted = None;
        self.send_the_notebook_input();
        if let Some(chosen) = action {
            self.run_action(chosen, ui.ctx());
        }
        // **The page follows the node it was drawn in, in the same frame.** See `BrowserHost::follow`.
        if !self.browser_placements.is_empty() {
            let occluders = self.occluding_rects(ui.ctx());
            // Before `follow` cuts a new popup out of the page, so the picture is of the whole page. `task-2277`.
            self.stand_a_picture_under_what_covers_the_page(ui.ctx(), &occluders);
            self.browser.follow(&self.browser_placements, &occluders);
        }
        crate::services::frame_trace::phase("action");
        // The title bar's and the rail's `rux` canvases that were not drawn this frame are given back.
        crate::components::chrome_kit::end_frame(ui.ctx());
        // Last, once every popup has been drawn: glass under the ones egui framed itself (`task-2281`).
        crate::theme::glass::glaze_the_popups(ui.ctx());
        crate::components::designer::kit::end_frame(ui.ctx());
        self.end_the_frame(ui);
    }

    /// Everything a frame does before it has decided where anything goes.
    ///
    /// The palette, the context and the repository are each looked for once and then never again; the
    /// zoom claim and the panel drag are frame locals and are cleared here, at the one point that is
    /// before every reader of them.
    fn begin_the_frame(&mut self, ui: &mut egui::Ui) {
        crate::services::frame_trace::begin();
        self.receive_browser_events();
        self.browser_placements.clear();
        // **A menu belongs to the tab it was opened over** (`task-2255`), so a change of tab closes it.
        let showing = (self.files.active_index(), self.files.focused_pane());
        if self.menus_were_for != Some(showing) {
            if self.menus_were_for.is_some() {
                crate::components::controls::close_every_menu(ui.ctx());
            }
            self.menus_were_for = Some(showing);
        }
        // The dividers of the frame before, forgotten here beside the other frame locals and for the
        // same reason: the window's resize grips are cut against this frame's, and a stale rectangle
        // would leave a piece of the window's edge dead after the divider had moved away from it.
        crate::components::splitter::forget_last_frames_dividers(ui.ctx());
        if self
            .files
            .iter()
            .any(|file| file.browser.as_ref().and_then(|tab| tab.location.source_path()).is_some())
        {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
        if !self.themed {
            // Only the palette and spacing. The fonts are installed in `prepare`, before the first frame.
            theme::apply(ui.ctx());
            self.themed = true;
        }
        if self.context.is_none() {
            self.context = Some(ui.ctx().clone());
        }
        // Looked for once, on the first frame, rather than in `new`: a window built by a test has no
        // context to wake and no business starting a thread, and this is the first point at which
        // there is one.
        if !self.git_looked {
            self.open_repository();
        }
        // **A zoom is the modal's while a modal is open.** Every pane claims the gesture by finding the pointer
        // inside itself, and a modal covers them without being one of them - so a wheel turned over a ticket
        // would have zoomed whatever pane happened to be underneath it. Taken before anything can claim it,
        // which is the same lock the claim already is, and since `task-2198` spent on the modal itself.
        self.zoom = ZoomClaim::Nobody;
        if a_modal_has_the_keyboard(ui.ctx()) {
            self.zoom_the_open_modal(ui);
            self.zoom = ZoomClaim::Taken;
        }
        // Frame local, like the tab drag: every panel that is drawn says whether it is in the air,
        // and `settle_the_panel_drag` reads the answer once they all have. Cleared here rather than
        // beside the tab drag because the explorer is drawn before that point.
        self.panel_drag = Drag::Nothing;
    }

    /// What the command line, git, the debug adapter and the two indexers have said since the last
    /// frame, and what the disk has done underneath them.
    ///
    /// All of it before anything is drawn, so that what a command asked for is in the frame about to be
    /// painted and therefore in the next screenshot. Each step carries its own `frame_trace` label.
    fn take_what_the_threads_answered(&mut self, ui: &mut egui::Ui) {
        // Before anything is drawn, so that what a command asked for is in the frame about to be
        // painted and therefore in the next screenshot.
        self.pump_control(ui.ctx());
        crate::services::frame_trace::phase("control");
        // After the commands and before any key is read, so the view with the keyboard holds the document
        // that the keys go into. `task-2320`.
        self.keep_the_twins_in_step();
        self.open_what_the_system_handed_over(ui.ctx());
        self.ask_git_about_the_open_file();
        crate::services::frame_trace::phase("git");
        // Everything the adapter has said since the last frame, taken beside git's own replies
        // because it is the same kind of thing: a thread has answered and the window has to draw it.
        self.take_the_debug_replies(ui.ctx());
        crate::services::frame_trace::phase("debug");
        // What every notebook's kernel has said, and the next queued cell sent. `task-2220`.
        self.hear_from_the_kernels();
        self.take_the_kernelspecs();
        crate::services::frame_trace::phase("kernels");
        self.colour_the_open_file();
        crate::services::frame_trace::phase("colour");
        // The project's definitions, from the code index (`task-2231` §5.2). Opened from the second
        // frame on, never the first: nothing the first frame does not need happens before the window
        // is shown, and the index reads the project on threads of its own.
        if ui.ctx().cumulative_pass_nr() > 0 {
            self.keep_the_symbol_index_fresh();
            // The language servers start on the second frame as well, and every reply they sent since
            // the last frame is read here. Nothing in a frame waits on one. `task-2231` §5.3.
            self.keep_the_servers_running();
            // The showing tab's structure, read again on a frame nobody typed in. `app::gather`.
            if ui.input(|input| input.events.is_empty()) {
                self.refresh_the_tab_structure_when_idle();
            }
            crate::services::frame_trace::phase("servers");
        }
        // What a person chose from the completion list, written once the window has settled after
        // the choice and never on the keystroke. `task-2231` §6.3.
        let now = ui.input(|input| input.time);
        self.completion_stats.write_when_settled(now, crate::app::WINDOW_SETTLE);
        crate::services::frame_trace::phase("index");
        // Before the explorer is drawn, so a file another program has just made is in the tree on
        // this frame rather than the next one.
        let focused = ui.ctx().input(|input| input.viewport().focused.unwrap_or(false));
        self.notice_what_changed_on_disk(focused);
        crate::services::frame_trace::phase("watch");
        // Where the window is, so the project can be opened here again next time — `task-1693`.
        self.note_where_the_window_is(ui.ctx());
        crate::services::frame_trace::phase("geometry");
        // Before the explorer is drawn, so the folders it needs are already open on this frame.
        self.follow_the_open_file();
        crate::services::frame_trace::phase("follow");
    }

    /// The two things `Escape` means before any pane reads the frame's keys.
    ///
    /// The newest notice first and a maximised pane second, because a notice is the newer thing on the
    /// screen and `Escape` everywhere in Unluminous puts away the most recent thing first. Both consume the
    /// press, so nothing drawn later reads it a second time.
    fn route_the_escape_key(&mut self, ui: &mut egui::Ui) {
        // Escape dismisses the newest notice, and it is asked **before** the maximised pane below,
        // because a notice is the newer thing on the screen and Escape everywhere in Unluminous puts away
        // the most recent thing first. One at a time rather than all of them: somebody with three
        // failures should read three. Consumed, so nothing drawn later reads the same press.
        if !self.toasts.is_empty()
            && !a_modal_has_the_keyboard(ui.ctx())
            && !text_box_has_the_keyboard(ui.ctx())
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.toasts.dismiss_the_newest();
        }
        // Escape puts a maximised pane back, which is the other half of `task-1771`'s double click. Here,
        // before anything reads the frame's keys, and **consumed** rather than merely read: Escape already
        // means "give the keyboard back" to the explorer and "leave the terminal", and a key that did two
        // things at once would be worse than one that did neither. Nothing while a modal or a text box has
        // the keys, because there Escape is theirs.
        if self.maximised != Maximise::No
            && !a_modal_has_the_keyboard(ui.ctx())
            && !text_box_has_the_keyboard(ui.ctx())
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.restore_the_maximised_pane();
        }
    }

    /// Everything a frame does that is not drawing, for a window the operating system says is not on the screen.
    ///
    /// `task-2253`: *"On my mac i have the latest unluminous but the agents keep reporting the window isn't
    /// rendering ... by the last try it had not drawn a frame for 733 seconds, and every command to it timed
    /// out without running."* eframe 0.36 runs **no UI pass at all** for a window that is minimised or occluded,
    /// and on macOS a window behind other windows or on another Space is occluded. It calls `App::logic`
    /// instead, and every command, every reply from a thread and every plugin's turn was taken inside the UI
    /// pass, so a covered window answered nothing and its board never read a new ticket. Activating it uncovered
    /// it, which is why that was the one thing that ever recovered one. Reproduced on Windows by minimising a
    /// window, which takes the same path: `_agent_output/task-2253-hidden-window/repro.ps1`.
    ///
    /// So this runs the parts of a frame that answer somebody: the command line, the debug adapter, the
    /// kernels, and the plugins, with the heartbeat asked for so it goes on being called. A command that needs a
    /// frame drawn, such as `window screenshot`, is held and answered when the window is next drawn or when its
    /// own deadline passes, as a held command always is.
    pub(crate) fn keep_working_while_hidden(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(HEARTBEAT);
        crate::services::wake::a_frame_was_drawn();
        self.pump_control(ctx);
        self.keep_the_twins_in_step();
        self.take_the_debug_replies(ctx);
        self.hear_from_the_kernels();
        self.let_the_plugins_catch_up(ctx);
        self.tick_the_plugins();
    }

    /// Ask to be woken again, say that a frame was drawn, and give every open plugin its turn.
    ///
    /// The heartbeat and the plugins are together because they are the same kind of thing: work that has
    /// to happen on every frame whatever else the frame turns out to do.
    fn ask_for_the_next_frame_and_let_the_plugins_catch_up(&mut self, ui: &mut egui::Ui) {
        // Always ask to be woken again. See `HEARTBEAT`.
        ui.ctx().request_repaint_after(HEARTBEAT);
        // Once a frame and nowhere else, so a worker thread can tell an idle window from a stopped one
        // without anybody having to remember to report it. `services::wake` says what it is for.
        crate::services::wake::a_frame_was_drawn();
        // Every open plugin gets a turn, twice over. Once a frame it is asked to catch up with whatever it
        // owns that is not drawing — for Agent-Tasks that is reading its terminals and sending a queued
        // handoff, which must not wait for somebody to look at the board. And once every `PLUGIN_TICK` it
        // gets the clock, which is the watchdog.
        self.let_the_plugins_catch_up(ui.ctx());
        self.tick_the_plugins();
        crate::services::frame_trace::phase("plugins");
    }

    /// Paint the window's own ground and work out where every part of it goes.
    ///
    /// The one place a rectangle is decided. Everything after this is handed the answer, which is what
    /// stops two phases measuring the same thing and disagreeing.
    fn lay_the_frame_out(&mut self, ui: &mut egui::Ui) -> FramePlaces {
        let full = ui.max_rect();

        // **The picture behind the window, when there is one** — `task-2004`. It goes between the
        // rounded rectangle and the ground, so the opacity slider goes on meaning exactly what it meant:
        // at 100% the panes hide it and below that it shows through, which is the relationship the
        // desktop has with it now. A window with no picture chosen paints nothing here at all and the
        // desktop shows through as it always did.
        self.paint_the_background(ui, full);
        // **Under glass the window is one sheet of glass** (`task-2281`): the picture bent at its rounded
        // edge, under the window's own smoke.
        if theme::is_glass() {
            theme::glass::paint_the_window(ui.painter(), full, f32::from(size::WINDOW_CORNER));
        }
        // The window is one painted surface with rounded corners, because it has no operating system
        // title bar. Everything else is drawn on top of it.
        ui.painter().rect_filled(
            full,
            CornerRadius::same(size::WINDOW_CORNER),
            theme::faded(color::editor(), self.settings.window_opacity()),
        );

        let title_rect = Rect::from_min_size(full.min, Vec2::new(full.width(), size::TITLE_BAR));
        // The text tools live at the right hand end of the title bar rather than in a strip of their
        // own. How much room they want depends on the open file, and the title bar leaves exactly that
        // much clear — but the bar's own height never changes, so switching from a `.md` file to a
        // `.rs` one no longer moves the tabs and the editing area up and down by forty four points.
        // **Nothing at all unless the tab holds a document.** A browser tab, a picture tab and a plugin
        // tab are each a `Document` with no path, which `file_kind` reads as an unsaved prose file — so
        // the `F` button and the three view modes were drawn over the Agent-Tasks board and over a page,
        // offering the Markdown parser's reading of a document there is none of. That is the
        // absent-control rule: a control that can never apply to what is showing is not drawn, and a
        // board and a web page each have no font, no bold and no preview. `task-2009`, and
        // `UnluminousApp::formatting_applies_here` is where the pair of questions lives.
        let tools_width = match self.files.active().is_a_document() {
            true => text_tools::width(self.document().path()),
            false => 0.0,
        };
        // The run widget takes the right hand end and the text tools sit in front of it, so the play
        // and the bug are in the same place whatever file is open — `task-1693`. How much room each
        // wants is worked out first, because the tools have to know where the run widget starts.
        let run_state = self.run_widget_state();
        let run_width = run_widget::width(&run_state);
        let tools_rect =
            title_bar::tools_rect(title_rect, self.menu_placement, tools_width, run_width);
        let run_rect = title_bar::run_rect(title_rect, self.menu_placement, run_width);
        let status_rect = Rect::from_min_size(
            Pos2::new(full.left(), full.bottom() - size::STATUS_BAR),
            Vec2::new(full.width(), size::STATUS_BAR),
        );
        let body = Rect::from_min_max(
            Pos2::new(full.left(), title_rect.bottom()),
            Pos2::new(full.right(), status_rect.top()),
        );

        // The rail of pane buttons takes the far left of the body, the whole way down, so the terminal
        // button sits at the bottom left corner of the window as `task-1658`'s capture shows it.
        let rail_rect = Rect::from_min_size(body.min, Vec2::new(size::ACTIVITY_BAR, body.height()));
        let panes = Rect::from_min_max(Pos2::new(rail_rect.right(), body.top()), body.max);

        // Where every panel goes. Since `task-1697` this is not written out here: the shape of the
        // window is a value — which edge each panel is docked to, and where in that edge — and
        // `app::dock::regions` is the one function that turns it into rectangles. The default value
        // gives exactly the arithmetic that used to be spelled out in this spot, and there is a test
        // that says so.
        let showing = self.panels_showing();
        let placed =
            dock::regions_with(panes, &self.panes.dock, showing, &self.panes, self.editor_visible);
        self.panes_area = panes;
        self.panel_rects = placed;
        // **Under glass every pane is a panel of glass floating on the window** (`task-2281`), drawn the way a
        // card is, here, before any of them draws, so each is drawn on its own panel. The title bar, the
        // status bar and the rail sit on the window's own glass with no panel of their own.
        if theme::is_glass() {
            for (index, room) in placed.panels.iter().enumerate() {
                if room.width() > 0.0 && room.height() > 0.0 {
                    theme::glass::paint_a_panel(ui, ("pane", index), *room);
                }
            }
            if self.editor_visible && placed.editor.is_positive() {
                theme::glass::paint_a_panel(ui, "editing-area", placed.editor);
            }
        }
        let explorer_rect = placed.of(dock::Panel::Explorer);
        let terminal_rect = theme::glass::tile_room(placed.of(dock::Panel::Terminal));
        let run_rect_tile = theme::glass::tile_room(placed.of(dock::Panel::Run));
        let debug_rect = theme::glass::tile_room(placed.of(dock::Panel::Debug));
        let editing_area = placed.editor;

        // Where the run and debug tiles are, recorded **whether they are showing or not**, so that a
        // run or a session started while its tile is put away is still opened at the size it will be
        // drawn at. See `run_grid_size` for what that is worth. The rectangle a hidden tile *would*
        // have is the layout with it switched on, worked out by the same function — which is the
        // whole reason that function takes `showing` rather than reading the window.
        self.run.tile = self.panel_area(dock::Panel::Run);
        self.debug_panel.tile = self.panel_area(dock::Panel::Debug);

        FramePlaces {
            full,
            title_rect,
            tools_rect,
            run_rect,
            tools_width,
            run_width,
            run_state,
            status_rect,
            rail_rect,
            panes,
            explorer_rect,
            terminal_rect,
            run_rect_tile,
            debug_rect,
            editing_area,
        }
    }

    /// The title bar, and the three controls drawn over its right hand end.
    ///
    /// The bar takes drags over the room between the menus and the buttons to move the window, so the
    /// text tools, the run widget and the branch widget are added **after** it: a control added earlier
    /// would sit underneath that and never be pressed.
    fn show_the_title_bar(
        &mut self,
        ui: &mut egui::Ui,
        places: &FramePlaces,
        menus: &[actions::Menu],
        action: &mut Option<Action>,
    ) {
        let title_rect = places.title_rect;
        let (tools_rect, run_rect) = (places.tools_rect, places.run_rect);
        let (tools_width, run_width) = (places.tools_width, places.run_width);
        let run_state = &places.run_state;

        // The project tabs, a pill a project in this window with the one showing raised, where the project's
        // name and the branch widget used to be (`task-2322`). The branch is in the status bar.
        let tabs = self.project_tabs();
        let names: Vec<String> = tabs.tabs.iter().map(|tab| tab.name.clone()).collect();

        // The title bar.
        let outcome = title_bar::show(
            ui,
            title_rect,
            &names,
            tabs.showing,
            self.settings.window_opacity(),
            self.menu_placement,
            menus,
            tools_width,
            run_width,
        );
        if outcome.tabs.plus_rect != Rect::NOTHING {
            self.project_plus_rect = Some(outcome.tabs.plus_rect);
        }
        if let Some(index) = outcome.tabs.show {
            self.ask_the_window(crate::app::project_tabs::Ask::Show(index));
        }
        if let Some((at, index)) = outcome.tabs.menu {
            self.project_tab_menu = Some((at, index));
        }
        if outcome.tabs.plus {
            self.open_the_project_dropdown();
        }
        if outcome.close {
            // Every modified tab is written first, and the window does not go when one of them could
            // not be (`task-1984` A2). `may_the_window_close` is the one function the three ways of
            // closing ask, so none of them can be the one that forgets. A window of several project tabs
            // asks every tab (`task-2322`).
            self.close_the_window(ui.ctx());
        }
        if outcome.minimise {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        if outcome.dragged {
            let now = ui.input(|input| input.time);
            self.unlatch.asked(now);
        }
        if outcome.toggle_maximise {
            let maximised = ui.input(|input| input.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximised));
        }
        // The green button on macOS. Full screen there is a space of its own, which is what the
        // button does in every other application on that platform, and it is a different thing from
        // the maximise a double click on the bar asks for — so the two are two commands rather than
        // one, and a window can be maximised and not full screen.
        if outcome.toggle_fullscreen {
            let fullscreen = ui.input(|input| input.viewport().fullscreen.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
        }
        if let Some(chosen) = outcome.action {
            *action = Some(chosen);
        }

        // The macOS menu bar, which is built once and rebuilt when what it holds changes.
        if let Some(native) = self.native_menu.as_mut() {
            // **No key equivalents while a chord is being recorded** on the Keymap page (`task-2286`):
            // AppKit hands a key equivalent to the menu before the window sees it, so pressing `Cmd+S` to
            // bind it would save instead.
            match self.settings_window.keymap.recording.is_some() {
                true => native.refresh(&crate::services::keymap::without_chords(menus)),
                false => native.refresh(menus),
            }
            if let Some(chosen) = native.poll() {
                *action = Some(chosen);
            }
        }

        // The text tools, drawn over the right hand end of the title bar. After the bar rather than
        // before it, because the bar takes drags over the room between the menus and the buttons to move
        // the window, and a control added earlier would sit underneath that and never be pressed.
        if tools_width > 0.0 {
            let tools_outcome = {
                let mut tools_ui = ui.new_child(egui::UiBuilder::new().max_rect(tools_rect));
                text_tools::show(
                    &mut tools_ui,
                    tools_rect,
                    self.document(),
                    &self.bold_family,
                    self.view_mode(),
                )
            };
            for command in tools_outcome.commands {
                self.document_mut().apply(command);
            }
            if let Some(mode) = tools_outcome.view_mode {
                self.set_view_mode(mode);
            }
        }

        // The run widget, drawn over the title bar after the bar for the reason the text tools are:
        // the bar takes drags over the room between the menus and the buttons to move the window,
        // and a control added earlier would sit underneath that and never be pressed.
        if run_width > 0.0 {
            let chosen = {
                let mut run_ui = ui.new_child(egui::UiBuilder::new().max_rect(run_rect));
                run_widget::show(&mut run_ui, run_rect, run_state)
            };
            if let Some(chosen) = chosen {
                *action = Some(chosen);
            }
        }
    }

    /// The rail of pane buttons down the far left, and the buttons the plugins contributed to it.
    fn show_the_rail(
        &mut self,
        ui: &mut egui::Ui,
        places: &FramePlaces,
        action: &mut Option<Action>,
    ) {
        let rail_rect = places.rail_rect;
        // The rail of pane buttons down the far left.
        {
            let state = activity_bar::RailState {
                explorer_visible: self.explorer_visible,
                editor_visible: self.editor_visible,
                git_open: self.git.as_ref().is_some_and(|git| git.panel.open),
                in_repository: self.repository_controls_apply(),
                terminal_visible: self.terminal.visible,
                realm_visible: self.realm.visible,
                realm_available: self.realm_is_on(),
                run_visible: self.run.visible,
                debug_visible: self.debug_panel.visible,
            };
            let opacity = self.settings.window_opacity();
            // One button per pane the plugins that are switched on contributed, in the group each
            // manifest named. Built here rather than inside the rail, because what is contributed is the
            // window's knowledge and the rail draws what it is given.
            let plugin_buttons: Vec<activity_bar::PluginButton> = (0..self.plugin_ui.pane_count())
                .filter(|slot| self.plugin_ui.applies(*slot))
                .filter_map(|slot| {
                    let pane = self.plugin_ui.pane(slot)?;
                    Some(activity_bar::PluginButton {
                        // **`<label> pane`, because a plugin names its menu after itself.** Every plugin
                        // that draws puts its own name in the menu bar, so a rail button called
                        // `Agent-Tasks` is the second control of that name in the window and a test asking
                        // for one finds two — which is what `no_two_controls_in_the_rail_share_a_name`
                        // catches. `Terminal tile`, `Run tile` and `Version Control` are the same rule
                        // already applied to Unluminous's own three, for the same reason.
                        label: format!("{} pane", pane.label),
                        icon: pane.icon.clone(),
                        on: self.plugin_ui.is_visible(slot),
                        bottom: pane.group == crate::services::plugins::RailGroup::Bottom,
                        key: self.plugin_ui.pane_key(slot)?,
                        slot,
                        opens: activity_bar::Opens::Pane,
                    })
                })
                .collect();
            // And one per tab a plugin contributed, after the panes, so the board's button lands under
            // the chat pane's — which is where `task-28` asks for it. A tab had no button at all before
            // and could only be reached from the `Plugins` menu.
            let tab_buttons: Vec<activity_bar::PluginButton> = self
                .plugin_ui
                .surfaces()
                .tabs
                .iter()
                .map(|surface| {
                    let key = surface.key(&surface.what.id);
                    activity_bar::PluginButton {
                        // **The tab's own label, and the manifest is what makes it distinct.**
                        // `<label> tab` was chosen when this button was added, to avoid two controls
                        // called `Agent-Tasks` — the plugin's menu and this. `task-1848` reports the
                        // result as unreadable: `Database` and `Database tab` are a distinction nobody
                        // can act on. So the tab says what it *is* — `tab.label = Query Console` — and
                        // the collision goes away because the two names are genuinely different things.
                        // `no_two_controls_in_the_rail_share_a_name` is what keeps that true.
                        label: surface.what.label.clone(),
                        icon: surface.what.icon.clone(),
                        // Lit when that tab is the one showing, which is what the pill means for every
                        // other button in the rail.
                        on: self.files.active().plugin.as_ref().is_some_and(|open| open.key == key),
                        bottom: false,
                        key,
                        slot: 0,
                        opens: activity_bar::Opens::Tab,
                    }
                })
                .collect();
            let plugin_buttons: Vec<activity_bar::PluginButton> =
                plugin_buttons.into_iter().chain(tab_buttons).collect();
            let rail = {
                let mut rail_ui = ui.new_child(egui::UiBuilder::new().max_rect(rail_rect));
                activity_bar::show_with(&mut rail_ui, rail_rect, state, opacity, &plugin_buttons)
            };
            if let Some(chosen) = rail.chosen {
                *action = Some(chosen);
            }
            // A right click on a rail button is the panel's own menu, which is the one way to move a
            // panel that has been put away — `task-1697`.
            if let Some((at, panel)) = rail.menu {
                self.panel_menu = Some((at, panel));
            }
        }
    }

    /// The key chords belonging to the menus.
    ///
    /// Read here rather than in the editing area, because they work whether or not the editing area has
    /// the keyboard, and because in preview mode there is no editing area taking key presses at all. On
    /// macOS these never arrive: the menu bar takes them first and sends an action instead.
    fn read_the_menu_shortcuts(
        &mut self,
        ui: &mut egui::Ui,
        menus: &[actions::Menu],
        action: &mut Option<Action>,
    ) {
        // The shortcuts belonging to the menus. Read here rather than in the editing area, because they work
        // whether or not the editing area has the keyboard, and because in preview mode there is no editing
        // area taking key presses at all. On macOS these never arrive, because the menu bar takes them
        // first and sends an action instead.
        // **Not while a chord is being recorded** on the Keymap page, or pressing `Ctrl+S` to bind it would
        // also save. `task-2286`.
        if action.is_none() && self.settings_window.keymap.recording.is_none() {
            // While one of the window's text boxes has the keyboard, undo, redo and select all
            // belong to that box rather than to the document, and it already does all three itself.
            // The rest of the menu is untouched, so control and S in the filter box still saves.
            let in_a_text_box = text_box_has_the_keyboard(ui.ctx());
            // **`Ctrl+[` and `Ctrl+]` are the terminal's while a terminal has the keyboard.** In a
            // terminal the first is `Escape` and the second is how a person detaches from `claude`,
            // and the menu's use of the pair for going back and forward (`task-2063`) must not take
            // either away from a program.
            let in_a_terminal = self.a_terminal_has_the_keyboard();
            // **The press an entry acted on is taken out of the frame** (`task-2286`), except while a terminal
            // has the keyboard, where a program has always been sent the chord as well. Left in, a chord
            // the keymap gives to a menu entry would also reach whatever reads that key itself: with the
            // reference editor's keymap preset `Ctrl+R` is `Replace`, and in a Markdown file the editing area would also
            // have aligned the paragraph to the right.
            *action = ui.ctx().input_mut(|input| {
                let mut found = None;
                input.events.retain(|event| {
                    let egui::Event::Key { key, pressed: true, modifiers, .. } = event else {
                        return true;
                    };
                    if found.is_some() {
                        return true;
                    }
                    let Some(chosen) = actions::action_for_key(menus, *key, modifiers) else {
                        return true;
                    };
                    if (in_a_text_box || in_a_terminal) && chosen.edits_the_document() {
                        return true;
                    }
                    let a_bracket = matches!(key, egui::Key::OpenBracket | egui::Key::CloseBracket);
                    if in_a_terminal && a_bracket {
                        return true;
                    }
                    found = Some(chosen);
                    in_a_terminal
                });
                found
            });
        }
    }

    /// The explorer, and everything a row in it reported.
    fn show_the_explorer(&mut self, ui: &mut egui::Ui, places: &FramePlaces) {
        let explorer_rect = places.explorer_rect;
        // **A rectangle with no room in it is not drawn**, which is the guard `show_the_plugin_panes` and
        // `show_the_realm` already have and which the explorer was the one panel without: it was drawn
        // into nothing, registering a filter box and row interactions at no size at all. One threshold
        // and one rule for all three panels rather than three. `task-1905`.
        if self.explorer_visible && explorer_rect.width() > 1.0 && explorer_rect.height() > 1.0 {
            let explorer_outcome = {
                // The file the tab is about, which for a rendered tab holding a local HTML file is
                // that file rather than its empty document's `None`. See `follow_the_open_file`.
                let open = self.files.active().file_on_disk().map(std::path::Path::to_path_buf);
                let unsaved = self.document().is_modified();
                // True for the two frames after the file that is showing changed, which is when the
                // list scrolls to it. Counted down rather than left on, because it is a one shot: a
                // person who closed the folder holding the open file closed it deliberately.
                let reveal = self.reveal_in_explorer > 0;
                self.reveal_in_explorer = self.reveal_in_explorer.saturating_sub(1);
                // The same one shot for the explorer's own cursor, which the arrow keys move
                // without opening anything, so nothing else would scroll to it.
                let reveal_selected = self.reveal_selection > 0;
                self.reveal_selection = self.reveal_selection.saturating_sub(1);
                let selected = self.selected.clone();
                let chosen = self.chosen.clone();
                let outside_drag = self.files_from_another_program(ui.ctx());
                if self.reveal_selection > 0 {
                    ui.ctx().request_repaint();
                }
                if self.reveal_in_explorer > 0 {
                    // The second frame has to actually happen, and an idle window draws nothing.
                    ui.ctx().request_repaint();
                }
                // Worked out for every row before the explorer is drawn, because decoding an icon
                // needs the context mutably and the explorer already has the window borrowed.
                //
                // **Once per set of rows rather than once a frame** (`task-1984` A5). It is a `Vec`
                // of every visible row's path, a `HashMap` keyed on a clone of each, and a plugin
                // icon looked up a row -- all of it rebuilt twice a second on a project where nothing
                // had changed. The key is what really decides the answer: which rows there are (the
                // tree's revision and the filter) and what git says about them. It is the shape
                // `symbols::Hover` already uses.
                let key = (
                    self.tree.revision(),
                    self.filter.clone(),
                    self.git.as_ref().map(|git| git.reads()).unwrap_or_default(),
                );
                if self.explorer_decorations.as_ref().map(|(was, _)| was) != Some(&key) {
                    let rows: Vec<PathBuf> = if self.filter.trim().is_empty() {
                        self.tree.rows().iter().map(|row| row.entry.path.clone()).collect()
                    } else {
                        self.tree
                            .matching(&self.filter)
                            .iter()
                            .map(|path| path.to_path_buf())
                            .collect()
                    };
                    // A map rather than a list. It used to be searched for each row as the row was
                    // drawn, comparing paths, so a project with four hundred rows open did a hundred
                    // and sixty thousand path comparisons every frame.
                    let built: std::collections::HashMap<PathBuf, explorer::Decoration> = rows
                        .into_iter()
                        .map(|path| {
                            let icon = self.plugin_icon(ui.ctx(), Some(&path));
                            let tint = self
                                .git
                                .as_ref()
                                .and_then(|git| git.state_of(&path))
                                .map(git_colour);
                            (path, explorer::Decoration { tint, icon })
                        })
                        .collect();
                    self.explorer_decorations = Some((key, built));
                }
                // **Borrowed rather than cloned**: cloning the map back out would be the allocation
                // this cache exists to remove. It is a field of `self` and `filter` is another, which
                // is what lets the closure hold one while `explorer::show` holds the other.
                let decorations = self.explorer_decorations.as_ref().map(|(_, built)| built);
                let decorate = move |path: &std::path::Path| -> explorer::Decoration {
                    decorations.and_then(|built| built.get(path)).cloned().unwrap_or_default()
                };
                let mut explorer_ui = ui.new_child(egui::UiBuilder::new().max_rect(explorer_rect));
                explorer::show(
                    &mut explorer_ui,
                    explorer_rect,
                    &self.tree,
                    &mut self.filter,
                    explorer::View {
                        current: open.as_deref(),
                        selected: selected.as_deref(),
                        chosen: &chosen,
                        outside_drag,
                        keyboard: self.focus == Focus::Explorer,
                        unsaved,
                        reveal,
                        reveal_selected,
                        opacity: self.settings.window_opacity(),
                        zoom: self.panes.zoom_of(dock::Panel::Explorer),
                        scroll_to: self.explorer_scroll_to.take(),
                        host: explorer::Host::Panel,
                    },
                    &decorate,
                )
            };
            if let Some(path) = explorer_outcome.select {
                self.pick_in_the_explorer(path, explorer_outcome.pick);
            }
            if let Some(folder) = &explorer_outcome.dropped_into {
                self.take_the_dropped_files(ui.ctx(), folder);
            }
            if explorer_outcome.focus {
                self.focus = Focus::Explorer;
            }
            if let Some(path) = explorer_outcome.toggle {
                self.tree.toggle(&path);
            }
            // A single click opens the file and leaves the keyboard here, which is VS Code's own
            // behaviour and is what makes `Down` `Down` `Down` a way to look through a folder. A
            // double click is somebody going to the editor, so the keyboard goes with them.
            // Each is a jump `Navigate Back` comes back from (`task-2063`).
            if let Some(path) = explorer_outcome.open {
                self.note_a_jump();
                let _ = self.open_path(&path);
            }
            if let Some(path) = explorer_outcome.open_permanently {
                self.note_a_jump();
                let _ = self.open_path_permanently(&path);
                self.focus = Focus::Editor;
            }
            // **A row carried out of the panel may be meant for the canvas**, which this list cannot know
            // about. Reported here in the window's own points, which is what the panel draws in, and
            // settled once every node has been drawn — see [`Self::settle_the_file_drag`]. A drop the list
            // itself claimed is a move on disk and is not offered twice.
            if explorer_outcome.moved.is_none() {
                if let Some((path, at, dropped)) = explorer_outcome.carrying {
                    self.file_drag = Drag::carrying(path, at, dropped);
                }
            }
            if let Some((source, folder)) = explorer_outcome.moved {
                // Every chosen row goes when the one carried is among them (`task-2194`), each by the
                // same move a single row takes, so each still takes the code that names it along.
                // One that is already in the folder, or that the folder is inside, stays where it is.
                for source in self.choice_including(&source) {
                    if source.parent() == Some(folder.as_path()) || folder.starts_with(&source) {
                        continue;
                    }
                    let name = source
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let target = folder.join(name);
                    self.move_path(&source, &target, true);
                }
                self.chosen.clear();
            }
            if explorer_outcome.hide {
                self.explorer_visible = false;
            }
            self.dragging_a_row = explorer_outcome.dragging;
            // Where the list was left, so a zoom can put the row under the pointer back under it on the
            // next frame. See `keep_the_place_through_a_panels_zoom`.
            self.explorer_scroll = explorer_outcome.scroll;
            self.zoom_over_a_panel(ui, dock::Panel::Explorer, explorer_rect);
            self.note_a_panel_grab(dock::Panel::Explorer, explorer_outcome.grab);
            if let Some((at, path, directory)) = explorer_outcome.context_menu {
                let aimed = match explorer_outcome.menu_over_empty_space {
                    true => actions::Aim::AtEmptySpace,
                    false => actions::Aim::AtARow,
                };
                self.explorer_menu = Some((at, path, directory, aimed));
            }
        }
    }

    /// The five readers that take a key out of the frame's input before any pane sees it.
    ///
    /// That is the one-frame ordering `Find in Files` and `Go to File` already rely on: a key one of
    /// these takes never reaches `editor_view::handle_input`. Everything else flows through untouched.
    fn route_the_keys_before_the_panes(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        // The completion popup's five keys, taken out of the frame's input **before** any pane reads
        // it, which is the one-frame ordering `Find in Files` and `Go to File` already rely on: a
        // key the popup takes never reaches `editor_view::handle_input`. Everything else flows
        // through untouched.
        self.route_the_completion_keys(ui);
        // And the value tooltip's one key, in the same place and for the same reason: `Escape` there
        // means "put the popup away" and must not also reach the editing area behind it.
        self.route_the_value_tooltip_keys(ui);
        // The explorer's own keys, for the same reason and in the same place: they are read before
        // any pane is drawn, and only while the explorer has the keyboard, so `Delete` can never
        // mean two things at once.
        if let Some(chosen) = self.route_the_explorer_keys(ui) {
            *action = Some(chosen);
        }
        // And the canvas's, in the same place and for the same reason - `task-1904`. A folder node
        // has a cursor of its own, and `Escape` there means "give the keyboard back" exactly as it
        // does in the explorer and in the terminal.
        if let Some(chosen) = self.route_the_realm_keys(ui) {
            *action = Some(chosen);
        }
        // And `Alt+Up` and `Alt+Down`, the one WP4 chord pair with no menu entry — so the menu's own
        // watcher cannot deliver them and they have to be read somewhere. Here, for the reason every
        // other reader in this function is here: a key taken before the panes are drawn never reaches
        // `editor_view::handle_input`, where a bare `ArrowUp` moves the caret, so one press cannot
        // both move the line and move the caret off it.
        if let Some(chosen) = self.route_the_line_move_keys(ui) {
            *action = Some(chosen);
        }
        // And the copy, when what is selected is in a preview rather than in a document. Before the
        // panes for the same reason again: in the side-by-side view the source is drawn first and
        // would otherwise take the event and copy its own selection instead.
        self.route_the_preview_copy(ui);
        // And a design tab's own, for the same reason. `task-2254`.
        self.route_the_designers_keys(ui);
    }

    /// The panes: a strip of tabs and an editing area each, left to right. One pane is the ordinary
    /// case and takes the same path as any other number.
    ///
    /// The loop **borrows the focus**: `files.active()` answers with the pane being drawn for as long as
    /// it is being drawn, which is what `show_editor` and `show_preview` already mean by it, so nothing
    /// had to have a pane index threaded through it. Two things must not follow the borrowed focus and
    /// are passed in instead — the keyboard, or every pane would take the same key presses and draw a
    /// caret, and `editor_area`, which the status bar reads on the frame after. See
    /// `tasks/task-1664-split-view-tdd.md` section 6.
    ///
    /// Answers with where each pane was drawn, which the dividers and the tab drag both need.
    fn show_the_panes(&mut self, ui: &mut egui::Ui, places: &FramePlaces) -> Vec<Rect> {
        let editing_area = places.editing_area;
        let pane_rects = self.pane_rects(editing_area);
        let had_the_keyboard = self.files.focused_pane();
        let mut keyboard = had_the_keyboard;
        self.tab_strips.clear();
        self.node_tab_strips.clear();
        self.tab_drag = Drag::Nothing;
        self.pane_drag = Drag::Nothing;
        // Rebuilt by the pane that has the keyboard, which is the only thing that can know it.
        self.completion_anchor = None;
        // A close can remove a pane and renumber the ones after it, so it is done once the loop has
        // finished rather than underneath itself.
        let mut close: Option<usize> = None;
        // **Nothing is drawn when the editing area is hidden.** `task-28`. Drawing it into `Rect::ZERO` would
        // still register a tab strip, a gutter and an editing surface per pane in the widget tree, all of them
        // nowhere, and one of them would still be taking the keyboard.
        // Under glass the editing area has no ground of its own beyond its glass panel (`task-2281`): a
        // second fill here was a dark square inside the rounded panel, and Jason asked for it to go.
        if self.editor_visible {
            for (pane, rect) in pane_rects.iter().copied().enumerate() {
                self.files.focus_pane(pane);
                if self.show_pane(ui, pane, pane == had_the_keyboard, rect, &mut close) {
                    keyboard = pane;
                }
            }
        }
        self.files.focus_pane(keyboard);
        // A File Editor node on a realm drawn in a tab borrows the keyboard from inside the pane loop, so
        // the line above has just taken it back. While the realm holds the keyboard the chosen node keeps it,
        // which is what `take_the_keyboard_for_the_realm` gave it. `task-2202`.
        if matches!(self.focus, Focus::Realm) && self.a_realm_tab_is_showing() {
            if let Some(node) = self.realm.chosen() {
                if self.files.tab_in_node(node).is_some() {
                    self.files.focus_node(node);
                }
            }
        }
        crate::services::frame_trace::phase("panes");
        if let Some(index) = close {
            self.close_tab(index);
        }
        // A link a `Ctrl/Cmd+Click` in a preview asked for. After the loop, because opening it makes a
        // browser tab the active one and the rest of a preview's drawing reads the active tab's cache.
        self.settle_the_link_click();
        // The completion popup, drawn from the geometry the pane with the keyboard recorded. After
        // the loop for the reason the tab drag is settled after it: this is the first moment
        // anything knows where that pane's caret ended up, and one popup drawn here can never be
        // underneath a divider or drawn twice in a split view.
        self.show_the_completion(ui);
        // The signature line, from the same caret. `task-2231` §6.8.
        self.show_the_signature(ui);
        // The value tooltip, drawn from the geometry the pane recorded, after the loop for exactly
        // the reason above. `task-1696`.
        self.show_the_value_tooltip(ui);
        pane_rects
    }

    /// The dividers between the panes, added after every pane for the reason
    /// `components::splitter` records: the editing area takes drags over the whole of its rectangle, so
    /// a divider added earlier sits underneath one and never sees the pointer.
    fn show_the_dividers_between_panes(
        &mut self,
        ui: &mut egui::Ui,
        places: &FramePlaces,
        pane_rects: &[Rect],
    ) {
        let editing_area = places.editing_area;
        // The dividers between the panes, added after every pane for the reason
        // `components::splitter` records: the editing area takes drags over the whole of its
        // rectangle, so a divider added earlier sits underneath one and never sees the pointer.
        let dividers = pane_rects.len().saturating_sub(1);
        for (pane, rect) in pane_rects.iter().enumerate().take(dividers) {
            if !self.editor_visible {
                break;
            }
            let edge = Rect::from_min_size(
                Pos2::new(rect.right(), rect.top()),
                Vec2::new(1.0, rect.height()),
            );
            let name = format!("pane-{pane}");
            let drag = splitter::show(ui, edge, &name, splitter::Axis::Upright);
            if drag.delta != 0.0 && editing_area.width() > 1.0 {
                let smallest = (size::EDITOR_PANE_MIN / editing_area.width()).min(0.45);
                self.files.move_divider(pane, drag.delta / editing_area.width(), smallest);
            }
            if drag.reset {
                self.files.reset_pane_widths();
            }
        }
    }

    /// The four right click menus that belong to what has just been drawn: the explorer's, a tab's,
    /// the gutter's and the editing area's.
    ///
    /// Each is added after the thing it was opened on, so it sits over it rather than under it.
    fn show_the_menus_over_the_panes(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        // The panels' own dividers are added once all four have been drawn — see the call to
        // `show_the_panel_dividers` below the debug tile, and `components::splitter` for why a
        // divider can never be added before the pane it belongs to.

        // The explorer's own menu, drawn after the explorer and the editing area so it sits over
        // both rather than under either.
        if let Some((at, path, directory, aimed)) = self.explorer_menu.clone() {
            let mut entries = actions::explorer_menu_with_git(
                &self.menu_state(),
                &path,
                directory,
                self.something_to_paste(),
                aimed,
            );
            // Every right click menu shows the chord the keymap put its entries on. `task-2286`.
            crate::services::keymap::apply_entries(&mut entries, &self.keymap);
            let outcome = context_menu::show(ui, "explorer", at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if outcome.close {
                self.explorer_menu = None;
            }
        }

        // A project tab's own menu and the plus's dropdown. `task-2322`.
        self.show_the_project_tab_menus(ui, action);

        // A tab's own menu, drawn after the panes so it sits over them rather than under one.
        if let Some((at, _)) = self.tab_menu {
            let mut entries = actions::tab_menu(&self.menu_state());
            crate::services::keymap::apply_entries(&mut entries, &self.keymap);
            let outcome = context_menu::show(ui, "tab", at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if outcome.close {
                self.tab_menu = None;
            }
        }

        // The empty part of a tab strip's own menu. `task-2270`.
        if let Some(at) = self.strip_menu {
            let mut entries = actions::tab_strip_menu();
            crate::services::keymap::apply_entries(&mut entries, &self.keymap);
            let outcome = context_menu::show(ui, "tab-strip", at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if outcome.close {
                self.strip_menu = None;
            }
        }

        // A notebook cell's own menu, from the three dots on the cell. `task-2220`.
        if let Some(at) = self.notebook_menu {
            let entries = crate::app::notebook_actions::cell_menu();
            let outcome = context_menu::show(ui, "notebook-cell", at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if outcome.close {
                self.notebook_menu = None;
            }
        }

        // The gutter's own menu, drawn after the editing area so it sits over it rather than under.
        if let Some(at) = self.gutter_menu {
            let mut entries = actions::gutter_menu(&self.menu_state());
            crate::services::keymap::apply_entries(&mut entries, &self.keymap);
            let outcome = context_menu::show(ui, "gutter", at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if outcome.close {
                self.gutter_menu = None;
            }
        }

        // The Markdown preview's own menu: Copy and Select All, about what the preview has selected.
        if let Some(at) = self.preview_menu {
            let mut entries = actions::preview_menu(self.preview_holds_the_selection());
            crate::services::keymap::apply_entries(&mut entries, &self.keymap);
            let outcome = context_menu::show(ui, "preview", at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if outcome.close {
                self.preview_menu = None;
            }
        }

        // The editing area's own menu, which is where a passage is marked. Drawn after the editing
        // area for the same reason the other two are: it has to sit over what it was opened on.
        if let Some(menu) = self.text_menu.clone() {
            let state = self.menu_state();
            let mut above = actions::text_menu(&state);
            crate::services::keymap::apply_entries(&mut above, &self.keymap);
            let mut below = actions::clear_highlight_menu(&state);
            // The ticket's own two rows, under the marks they are about: `Collapse All But
            // Highlighted` is worth something only to somebody who has just marked a passage, and
            // this is where they are already pointing.
            let mut folding = actions::folding_here_menu(&state);
            crate::services::keymap::apply_entries(&mut folding, &self.keymap);
            if !folding.is_empty() {
                below.push(actions::Entry::Separator);
                below.extend(folding);
            }
            let last = self.last_highlight;
            let outcome = text_menu::show(ui, &menu, &above, &below, state.has_selection, last);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if let Some(color) = outcome.highlight {
                self.highlight_selection(color);
            }
            if let Some(wheel) = outcome.wheel {
                if let Some(menu) = self.text_menu.as_mut() {
                    menu.wheel = wheel;
                }
                if let Some(color) = wheel {
                    self.last_highlight = color;
                }
            }
            if outcome.close {
                self.text_menu = None;
            }
        }
    }

    /// The panes the plugins contributed, each in the rectangle `dock::regions` gave its slot, and
    /// whatever modal one of them has open.
    fn show_the_contributed_panes(&mut self, ui: &mut egui::Ui) {
        // The panes the plugins contributed, each in the rectangle `dock::regions` gave its slot.
        //
        // Drawn before the terminal for no reason but order on the page; they are laid out by the same
        // arithmetic as every other panel and cannot overlap one. A provider that failed to open draws
        // the reason rather than nothing, which is what every honest miss in Unluminous does.
        for (plugin, request) in self.show_the_plugin_panes(ui) {
            self.act_on_a_plugin_request(&plugin, request, ui.ctx());
        }
        if std::mem::take(&mut self.plugin_wants_a_repaint) {
            ui.ctx().request_repaint();
        }
        if let Some(text) = self.plugin_wants_copied.take() {
            ui.ctx().copy_text(text);
        }
        // Whatever modal a plugin has open, drawn after the panes and from the context, which is where every
        // other modal in Unluminous is drawn: `components::modal` places it, drags it and resizes it, and it has to
        // be above the panes including the plugin's own.
        self.show_the_plugin_modals(ui);
    }

    /// The Realm, its modal, its manager and its menu - `task-1904`.
    fn show_the_canvas(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        // The Realm - `task-1904`. Drawn where the plugin panes are for the same
        // reason: it is laid out by the same arithmetic as every other panel and cannot overlap one.
        if self.realm.visible {
            self.show_the_realm(ui);
        }
        self.show_the_realm_modal(ui);
        self.show_the_realm_manager(ui);
        self.show_the_realm_files(ui);
        if let Some(chosen) = self.show_the_realm_menu(ui) {
            *action = Some(chosen);
        }
        // A shape's colour picker, after the menu that opened it. `task-2284`.
        self.show_the_colour_picker(ui);
    }

    /// Where a tab and a file being carried by the pointer would land, and where they did.
    ///
    /// After the panes **and** after the canvas, which is the earliest moment anything knows where every
    /// place that can take a drop has been drawn.
    fn settle_the_drags(&mut self, ui: &mut egui::Ui, pane_rects: &[Rect]) {
        // **Where a tab being carried would land, and where it did. After the canvas as well as after the
        // panes.** Its own rule is that a tab picked up in one place is dropped in another as often as not,
        // so it is settled once everything that can hold a tab has been drawn — and since `task-1905` a
        // File Editor **node** can hold one. Settled where it used to be, between the panes and the canvas,
        // `node_tab_strips` was always empty when it was read: dragging a pane's tab onto a node had no
        // target, and a drag reported *by* a node was cleared before the next frame could act on it. The
        // Codex Sol review of `task-1905` found it.
        self.settle_the_tab_drag(ui, pane_rects);
        // And a whole pane carried by its strip, which can only land among the other panes. `task-2280`.
        self.settle_the_pane_drag(ui, pane_rects);
        // And where a **file** being carried out of a list would land, which is the same question about the
        // same canvas and is settled in the same place. `task-1914`.
        self.settle_the_file_drag(ui);
    }

    /// The three character grids: the terminal, the run tile and the debug tile.
    ///
    /// On one side they are never showing at the same time; on different sides they are, which is what
    /// moving one is for.
    fn show_the_tiles(
        &mut self,
        ui: &mut egui::Ui,
        places: &FramePlaces,
        action: &mut Option<Action>,
    ) {
        let (terminal_rect, run_rect_tile) = (places.terminal_rect, places.run_rect_tile);
        let debug_rect = places.debug_rect;
        // The terminal.
        if self.terminal.visible {
            let panel_outcome = {
                let mut panel_ui = ui.new_child(egui::UiBuilder::new().max_rect(terminal_rect));
                panel_ui.set_clip_rect(terminal_rect);
                let font_size = self.settings.terminal_font_size;
                terminal_panel::show(
                    &mut panel_ui,
                    terminal_rect,
                    &mut self.terminal,
                    &self.renderer,
                    font_size,
                    self.settings.window_opacity(),
                )
            };
            self.zoom_over_a_panel(ui, dock::Panel::Terminal, terminal_rect);
            self.note_a_panel_grab(dock::Panel::Terminal, panel_outcome.grab);
            if panel_outcome.take_focus {
                self.a_tile_took_the_keyboard(dock::Panel::Terminal);
            }
            if let Some(text) = panel_outcome.copy {
                ui.ctx().copy_text(text);
            }
            if panel_outcome.new_tab {
                self.new_terminal_tab();
                self.a_tile_took_the_keyboard(dock::Panel::Terminal);
            }
            if panel_outcome.hide {
                self.terminal.visible = false;
                self.focus = Focus::Editor;
            }
            if let Some((index, at)) = panel_outcome.menu {
                self.terminal_menu = Some((at, index));
            }
            // A shell that has stopped, from `exit` or otherwise, closes its tab, and the tile goes with the
            // last of them. A tile that never had a tab is left showing, because that is the one that has a
            // reason to give: the message says why the shell would not start.
            let had_tabs = !self.terminal.tabs.is_empty();
            self.terminal.tabs.pump();
            if had_tabs && self.terminal.tabs.is_empty() {
                self.terminal.visible = false;
                self.focus = Focus::Editor;
            }
        }
        // The run tile. On the same side as the terminal it is never showing at the same time; on
        // another side it is, which is what moving it is for.
        if self.run.visible {
            let panel_outcome = {
                let mut panel_ui = ui.new_child(egui::UiBuilder::new().max_rect(run_rect_tile));
                panel_ui.set_clip_rect(run_rect_tile);
                let font_size = self.settings.terminal_font_size;
                run_panel::show(
                    &mut panel_ui,
                    run_rect_tile,
                    &mut self.run,
                    &self.renderer,
                    font_size,
                    self.settings.window_opacity(),
                )
            };
            self.zoom_over_a_panel(ui, dock::Panel::Run, run_rect_tile);
            self.note_a_panel_grab(dock::Panel::Run, panel_outcome.grab);
            if panel_outcome.take_focus {
                self.a_tile_took_the_keyboard(dock::Panel::Run);
                // Clicking a run's tab is choosing it, so the widget and the tile agree about what
                // `Run` with no name means.
                if let Some(run) = self.run.active() {
                    self.run_selected = Some(run.name().to_owned());
                }
            }
            if let Some(text) = panel_outcome.copy {
                ui.ctx().copy_text(text);
            }
            if panel_outcome.stop {
                self.message = self.run.active().map(|run| format!("Stopping {}", run.name()));
            }
            if panel_outcome.rerun {
                let name = self.run.active().map(|run| run.name().to_owned());
                if let Some(name) = name {
                    *action = Some(Action::Run(RunAction::Rerun(Some(name))));
                }
            }
            if panel_outcome.hide {
                self.show_the_run_tile(false);
            }
        }
        // The debug tile, the third of the three.
        if self.debug_panel.visible {
            // Worked out before the tile is drawn, because it is what the tile says when there is no
            // session and because the search behind it is a cache the window owns.
            let idle = self.debug_idle();
            let outcome = {
                let mut panel_ui = ui.new_child(egui::UiBuilder::new().max_rect(debug_rect));
                panel_ui.set_clip_rect(debug_rect);
                let opacity = self.settings.window_opacity();
                // The panel and the session are borrowed apart, which is what lets a component take
                // its own state mutably and what it draws immutably — the shape every component in
                // Unluminous has.
                let DebugSplit { panel, debug } = split_the_debug(self);
                debug_panel::show(&mut panel_ui, debug_rect, panel, debug, &idle, opacity)
            };
            self.zoom_over_a_panel(ui, dock::Panel::Debug, debug_rect);
            self.note_a_panel_grab(dock::Panel::Debug, outcome.grab);
            self.act_on_the_debug_tile(outcome, ui.ctx());
        }
    }

    /// Two presses on a tab strip, and a zoom gesture nobody was pointing at.
    ///
    /// Both after every panel has been drawn: the first changes what is showing and the pane loop is
    /// what reads that, and the second belongs to the pane being typed into only once every pane has had
    /// its chance to claim it.
    fn settle_the_maximise_and_the_zoom(&mut self, ui: &mut egui::Ui) {
        // Two presses on the empty part of a tab strip. After every pane has been drawn, because it changes
        // what is showing and the pane loop is what reads that.
        if std::mem::take(&mut self.maximise_wanted) {
            self.toggle_maximised(None);
        }

        // A gesture nobody was pointing at belongs to the pane being typed into. Here rather than in the
        // pane loop, because a pane earlier in the row must not take a gesture aimed at one later in it,
        // and which pane the pointer is over is not known until they are all drawn. **After every panel**
        // and not merely after the editing area, which is `task-1771`: the explorer, the tiles and a
        // plugin's pane all claim a gesture the pointer is over now, and settling this in the middle of
        // the frame gave the editing area a wheel turned over the terminal.
        if self.zoom == ZoomClaim::OfferedToTheKeyboard {
            self.zoom = ZoomClaim::Taken;
            self.zoom_the_text(ui, 0.0);
        }
    }

    /// One divider a panel, the blue bands a panel being carried would land in, and a panel's own
    /// right click menu.
    ///
    /// Added once every panel has been drawn, because a panel takes drags over the whole of its
    /// rectangle and a divider overlaps its edge.
    fn show_the_panel_furniture(
        &mut self,
        ui: &mut egui::Ui,
        places: &FramePlaces,
        action: &mut Option<Action>,
    ) {
        let panes = places.panes;
        // One divider a panel, along the edge that faces the editing area. Added once every panel
        // has been drawn, because a panel takes drags over the whole of its rectangle and a divider
        // overlaps its edge: a widget added earlier sits underneath one and never sees the pointer.
        // That is what `components::splitter` has always recorded, applied to four panels rather
        // than to the explorer and whichever tile happened to be up.
        self.show_the_panel_dividers(ui, panes);
        // And where a panel being carried would land, drawn over everything else in the body because
        // it is about to replace some of it. After the dividers, so a band is never drawn under one.
        self.show_the_drop_zones(ui, panes);
        self.settle_the_panel_drag(ui.ctx(), panes);

        // Every canvas whose surface was not drawn this frame gives its texture back. After every panel
        // and before the menus, which is the last moment anything could have drawn decoration.
        self.canvases.tidy();

        // A panel's own menu, drawn after every panel so it sits over them rather than under one.
        if let Some((at, panel)) = self.panel_menu {
            let entries = actions::panel_menu(&self.menu_state(), panel);
            let outcome = context_menu::show(ui, "panel", at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if outcome.close {
                self.panel_menu = None;
            }
        }
    }

    /// What every program has said since the last frame, the terminal tab's own menu, and the report
    /// a program asked for when the keyboard arrives or leaves.
    ///
    /// Outside the `visible` test on purpose: a program that is running has to be read whether or not
    /// anybody is looking at it, or its output would arrive in a rush the moment the tile came back up.
    fn read_what_the_programs_said(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        // What every program has said since the last frame, and the hard kill that follows a polite
        // stop nobody answered. Outside the `visible` test on purpose: a program that is running
        // has to be read whether or not anybody is looking at it, or its output would arrive in a
        // rush the moment the tile came back up.
        if self.run.settle() {
            ui.ctx().request_repaint();
        }
        if let Some(left) = self.run.stopping_in() {
            // An idle window draws nothing, and the grace has to actually run out — so the window
            // is woken once, when it does, rather than kept awake for the whole two seconds.
            ui.ctx().request_repaint_after(left);
        }
        self.run.focused = self.focus == Focus::Terminal && self.run.visible;

        // A terminal tab's own menu, drawn after the tile so it sits over it rather than under.
        if let Some((at, _)) = self.terminal_menu {
            let entries = actions::terminal_tab_menu();
            let outcome = context_menu::show(ui, "terminal-tab", at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            }
            if outcome.close {
                self.terminal_menu = None;
            }
        }
        self.terminal.focused = self.focus == Focus::Terminal;

        // A program that asked to be told when the terminal gains or loses the keyboard is told. `claude`
        // asks, and it is how it knows to stop drawing a cursor of its own.
        if self.focus != self.last_focus {
            let gained = self.focus == Focus::Terminal;
            if let Some(session) = self.terminal.tabs.active() {
                if session.wants_focus_reports() {
                    session.send(unluminous_terminal::keys::focus(gained));
                }
            }
            self.last_focus = self.focus;
        }
    }

    /// The status bar. A picture has no caret and no font, so it says how big it is and how far it
    /// is zoomed instead.
    fn show_the_status_bar(&mut self, ui: &mut egui::Ui, places: &FramePlaces) {
        let status_rect = places.status_rect;
        // The status bar. A picture has no caret and no font, so it says how big it is and how far it
        // is zoomed instead.
        let style = self.document().active_style();
        let branch = self.git.as_ref().and_then(|git| git.status_label());
        crate::services::frame_trace::phase("tiles");
        let picture = self.editor_area.size();
        // **A caret and a font belong to a document, and three kinds of tab have neither.** A picture
        // already said so; a browser tab and a plugin tab did not, so a page reported
        // `Plain text · Ln 1, Col 1 · Helvetica · 16 pt` — a caret in a document nobody is editing and
        // a font nothing is set in. `task-2009`, and it is the same question the title bar's tools ask.
        // `line_ending_label` has answered `None` for all three since it was written, which is the
        // shape followed here.
        let (position, detail) = match self.files.active().picture.as_ref() {
            Some(picture_in_the_tab) => (None, picture_in_the_tab.description(picture)),
            None if !self.files.active().is_a_document() => (None, String::new()),
            None => (
                Some(self.caret_position()),
                format!("{} \u{00B7} {:.0} pt", style.family, style.size),
            ),
        };
        // What the tab holds. A rendered tab says what the page really is — the HTML file it opened,
        // or `Web page` for an address — rather than the `Plain text` its empty document reads as.
        let kind = if self.files.active().is_browser() {
            match self.files.active().file_on_disk() {
                Some(path) => file_kind::kind_name(Some(path)),
                None => "Web page",
            }
        } else if self.files.active().is_a_plugin() {
            "Plugin"
        } else if self.files.active().unrenderable.is_some() {
            // A file Unluminous cannot show, which is not plain text whatever its empty document reads as.
            "Not shown"
        } else {
            file_kind::kind_name(self.document().path())
        };
        let encoding = self.line_ending_label();
        // The language server answering for this file and what it is doing, beside the language, so
        // a list missing its members while rust-analyzer is still indexing explains itself.
        // `task-2231` §6.9. The full reason a server was not found or stopped is in
        // `status --section servers`; the footer says it in two words.
        let kind = match self.showing_server_state() {
            Some((label, state)) => format!("{kind} \u{00B7} {label} {}", footer_words(&state)),
            None => kind.to_owned(),
        };
        let kind = kind.as_str();
        let outcome = status_bar::show(
            ui,
            status_rect,
            &status_bar::Status {
                name: &self.file_name(),
                unsaved: self.document().is_modified(),
                kind,
                encoding: encoding.as_deref(),
                position,
                detail: &detail,
                message: self
                    .git
                    .as_ref()
                    .and_then(|git| git.message.as_deref())
                    .or(self.message.as_deref()),
                git: branch.as_deref(),
            },
            self.settings.window_opacity(),
        );
        // Under glass the status bar is a panel of its own (`task-2281`), so it needs no line above it.
        if !theme::is_glass() {
            title_bar::divider(
                ui.painter(),
                Pos2::new(status_rect.left(), status_rect.top()),
                Pos2::new(status_rect.right(), status_rect.top()),
            );
        }
        // The flag is the menu entry's twin, so it goes through `run_action` like a press of the entry.
        if outcome.reported {
            self.run_action(Action::ReportIssue, ui.ctx());
        }
        // The branch opens the list of local branches, and a row is a `GitAction::Switch` like the one
        // `Git -> Branches...` and `unluminous-cli git switch` send. `task-2325`.
        if let Some(button) = outcome.branch {
            let chosen =
                crate::components::branch_widget::menu_above(ui, &button, || self.branch_state());
            if let Some(action) = chosen {
                self.run_action(action, ui.ctx());
            }
        }
    }

    /// Anything git and the update check have answered since the last frame.
    fn take_what_the_workers_answered(&mut self, ui: &mut egui::Ui) {
        // Anything git has answered since the last frame.
        if let Some(git) = self.git.as_mut() {
            if git.take_replies(&mut self.files) {
                ui.ctx().request_repaint();
            }
        }
        // And whether there is a newer Unluminous, if somebody asked. `task-1804` §6.
        if self.take_the_update_answer() {
            ui.ctx().request_repaint();
        }
        // And both channels' version lists, for the Check for Updates submenu, fetched when it opens.
        // `task-2328`.
        self.fetch_the_lists_when_the_menu_opens(ui.ctx());
        if self.take_the_version_lists() {
            ui.ctx().request_repaint();
        }
        // The daily check, when it is due, and how far an install has got. `task-2063`.
        let now = ui.input(|input| input.time);
        self.ask_on_a_schedule(now);
        if self.take_the_install_progress() {
            ui.ctx().request_repaint();
        }
        // Whether setup is finished, as the window starts, and what a setup run did. `task-2301`.
        self.decide_whether_to_open_the_setup();
        if self.take_the_setup_news() {
            ui.ctx().request_repaint();
        }
        // What speech to text heard, for the prompt that is listening. `task-2290`.
        self.follow_the_voice(ui.ctx());
    }

    /// The text prompt, drawn before the Settings window because a prompt opened from a menu belongs
    /// over the window rather than over the settings.
    fn show_the_prompt(&mut self, ui: &mut egui::Ui) {
        // The text prompt, drawn before the Settings window because a prompt opened from a menu
        // belongs over the window rather than over the settings.
        if let Some(mut prompt) = self.prompt.take() {
            let outcome = prompt_dialog::show(ui.ctx(), &mut prompt);
            if outcome.confirmed {
                self.run_prompt(prompt);
            } else if !outcome.cancelled {
                self.prompt = Some(prompt);
            }
        }
    }

    /// `File -> Create Project...`, beside the prompt and for the same reason.
    ///
    /// The dialog decides nothing: it says what was pressed, and `make_the_project` is what makes the
    /// folder — which is the split `show_the_prompt` already keeps and is what lets `project new` reach
    /// the same code with no dialog at all.
    fn show_the_new_project_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(mut project) = self.new_project.take() else {
            return;
        };
        let outcome = crate::components::new_project_dialog::show(ui.ctx(), &mut project);
        if outcome.browse {
            // The platform's own folder picker, started at wherever the field is pointing now.
            let start = std::path::PathBuf::from(project.location.trim());
            let start = match start.is_dir() {
                true => start,
                false => self.tree.root().to_path_buf(),
            };
            if let Some(chosen) = rfd::FileDialog::new()
                .set_title("Project location")
                .set_directory(&start)
                .pick_folder()
            {
                project.location = unluminous_terminal::paths::plain(&chosen).display().to_string();
                project.problem = None;
            }
        }
        if outcome.cancelled {
            return;
        }
        if outcome.create {
            match self.make_the_project(&project.folder(), project.git) {
                Ok(()) => return,
                Err(problem) => project.problem = Some(problem),
            }
        }
        self.new_project = Some(project);
    }

    /// Make every missing background thumbnail on a thread, once at a time, and ask for a frame when they are
    /// made (`task-2293`). The grid asks when it finds one missing, and `main` asks as the window starts.
    ///
    /// @param folder - the backgrounds folder
    /// @param ctx - the window's context, asked for a frame when the thumbnails are there
    pub(crate) fn make_the_background_thumbnails(
        &self,
        folder: std::path::PathBuf,
        ctx: egui::Context,
    ) {
        use std::sync::atomic::Ordering;
        if self.background_thumbnails_making.swap(true, Ordering::AcqRel) {
            return;
        }
        let making = self.background_thumbnails_making.clone();
        crate::services::backgrounds::thumbnails::make_the_missing_later(folder, move || {
            making.store(false, Ordering::Release);
            ctx.request_repaint();
        });
    }

    /// Make the thumbnails for every background already in the folder, as the released window starts.
    pub fn make_the_missing_background_thumbnails(&self, ctx: &egui::Context) {
        if let Some(folder) = self.backgrounds_folder() {
            self.make_the_background_thumbnails(folder.to_owned(), ctx.clone());
        }
    }

    /// The Background grid, beside the other modals. `task-2004`.
    ///
    /// **Everything it reports is acted on here and takes effect at once**, which needs no machinery at
    /// all: `appearance.background.image` is what `paint_the_background` reads, and the window paints
    /// every frame. The grid itself decides nothing, which is every component in Unluminous.
    fn show_the_background_grid(&mut self, ui: &mut egui::Ui) {
        let Some(mut problem) = self.background_grid.take() else {
            // Nothing is open, so nothing is being looked at: the thumbnails go.
            self.background_thumbnails.clear();
            return;
        };
        let names = self.background_names();
        let folder = self.backgrounds_folder().map(|folder| folder.to_owned());
        // **Decoded before the dialog is drawn rather than inside it**, so the closure it is handed only
        // reads a map. **And from the picture's thumbnail** (`task-2293`), which is 480 pixels wide and decodes
        // in a few milliseconds, where the picture itself is 4K and took the window's thread a large part of a
        // second each. A picture with no thumbnail yet gets one made on a thread, and its cell is empty until then.
        let mut missing = false;
        for name in &names {
            if let (false, Some(folder)) = (self.background_thumbnails.contains_key(name), &folder)
            {
                use crate::services::backgrounds::thumbnails;
                if !thumbnails::is_current(folder, name) {
                    missing = true;
                    continue;
                }
                let decoded = crate::services::picture::decode(&thumbnails::path_of(folder, name))
                    .ok()
                    .map(|image| {
                        crate::services::picture::upload(
                            ui.ctx(),
                            format!("unluminous-background-cell-{name}"),
                            image,
                            egui::TextureOptions::LINEAR,
                        )
                    });
                self.background_thumbnails.insert(name.clone(), decoded);
            }
        }
        if let (true, Some(folder)) = (missing, &folder) {
            self.make_the_background_thumbnails(folder.clone(), ui.ctx().clone());
        }
        let thumbnails = &self.background_thumbnails;
        let texture = |name: &str| thumbnails.get(name).cloned().flatten();
        let outcome = crate::components::background_dialog::show(
            ui.ctx(),
            crate::components::background_dialog::Look {
                names: &names,
                chosen: &self.settings.background_image,
                texture: &texture,
                opacity: self.settings.opacity,
                problem: problem.as_deref(),
            },
        );
        if let Some(chosen) = outcome.chosen {
            self.settings.background_image = chosen;
            // A picture chosen is a picture meant to be seen, so it turns the background on (`task-2293`).
            self.settings.background_enabled = true;
            self.unsaved_settings = true;
            problem = None;
        }
        if let (true, None) = (outcome.add, &folder) {
            problem = Some(Self::NO_BACKGROUNDS_FOLDER.to_owned());
        }
        if let (true, Some(start)) = (outcome.add, &folder) {
            if let Some(chosen) = rfd::FileDialog::new()
                .set_title("Choose a background")
                .add_filter(
                    "Pictures",
                    &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"],
                )
                .set_directory(start)
                .pick_file()
            {
                match crate::services::backgrounds::add_into(start, &chosen) {
                    // Chosen as well as added, because somebody who picked a picture meant to use it.
                    Ok(name) => {
                        self.settings.background_image = name;
                        self.settings.background_enabled = true;
                        self.unsaved_settings = true;
                        problem = None;
                    }
                    Err(said) => problem = Some(said),
                }
            }
        }
        if let (Some(name), Some(folder)) = (outcome.remove, &folder) {
            match crate::services::backgrounds::remove_from(folder, &name) {
                Ok(()) => {
                    self.background_thumbnails.remove(&name);
                    // The one that was showing has gone, so the window goes back to the desktop rather
                    // than to a name that is no longer there.
                    if self.settings.background_image == name {
                        self.settings.background_image.clear();
                        self.unsaved_settings = true;
                    }
                    problem = None;
                }
                Err(said) => problem = Some(said),
            }
        }
        match outcome.closed {
            true => self.background_thumbnails.clear(),
            false => self.background_grid = Some(problem),
        }
    }

    /// `Find Action`: the palette that finds a menu entry by name and runs it. `task-1922` WP4.
    ///
    /// Drawn beside `Go to File`, because it is the same kind of thing asked about a different list.
    /// What it chose is run **here** rather than being put in `action`, because `action` is one
    /// action a frame and a palette row can be a `PluginCommand` that opens another modal.
    fn show_the_command_palette(&mut self, ui: &mut egui::Ui) {
        if let Some(mut palette) = self.palette.take() {
            palette.refresh();
            let outcome = crate::components::command_palette::show(ui.ctx(), &mut palette);
            if !outcome.close {
                self.palette = Some(palette);
            }
            if let Some(command) = outcome.refused {
                self.run_a_palette_row(command, ui.ctx());
            }
            if let Some(command) = outcome.run {
                self.run_a_palette_row(command, ui.ctx());
            }
        }
    }

    /// `Go to File`.
    fn show_go_to_file(&mut self, ui: &mut egui::Ui) {
        // `Go to File`, drawn after the prompt for the same reason the prompt is drawn after the git
        // windows: the newest thing a person asked for belongs on top of the older ones.
        if let Some(mut finder) = self.go_to_file.take() {
            finder.refresh(self.tree.root(), self.tree.all_files());
            let outcome = go_to_file::show(ui.ctx(), &mut finder);
            if let Some(path) = outcome.open {
                // A tab of its own, not the transient one: choosing a file out of a list of file
                // names is not glancing at it.
                self.note_a_jump();
                if self.open_path_permanently(&path).is_ok() {
                    self.focus = Focus::Editor;
                }
            }
            if !outcome.close {
                self.go_to_file = Some(finder);
            }
        }
    }

    /// `Find in Files`, which is drawn beside `Go to File` because they are the same kind of thing: a
    /// question about the project, asked over the top of it.
    fn show_find_in_files(&mut self, ui: &mut egui::Ui) {
        // `Find in Files`, which is drawn beside `Go to File` because they are the same kind of
        // thing: a question about the project, asked over the top of it.
        if let Some(mut find) = self.find_in_files.take() {
            find.pump(self.tree.all_files());
            let outcome = find_in_files::show(ui.ctx(), &mut find, self.panes.find_split);
            if outcome.drag != 0.0 {
                // The divider is dragged in points and the split is a fraction, because the modal
                // can be resized: a fraction keeps the two panes in proportion when it is.
                let height = outcome.panes_height.max(1.0);
                self.panes.find_split = (self.panes.find_split + outcome.drag / height)
                    .clamp(find_in_files::SPLIT_MIN, find_in_files::SPLIT_MAX);
                self.unsaved_settings = true;
            }
            if outcome.reset_split {
                self.panes.find_split = find_in_files::SPLIT;
                self.unsaved_settings = true;
            }
            if outcome.replace_all {
                // Taken before the replacement, because applying it changes every file the search
                // read and the hits are then about a project that has moved.
                let hits = find.hits().to_vec();
                let (needle, match_case) = (find.query.trim().to_owned(), find.match_case);
                let with = find.replacement.clone();
                let report = self.replace_across_the_project(&hits, &needle, match_case, &with);
                self.message = Some(report.sentence(&with));
                // The search is asked again, so the list shows what is there now rather than the
                // matches that have just been replaced.
                find.search_again();
            }
            if let Some((path, range)) = outcome.open {
                self.note_a_jump();
                self.open_the_match(&path, range);
            }
            if !outcome.close {
                self.find_in_files = Some(find);
            }
        }
    }

    /// The About box.
    ///
    /// Only one modal is open at a time - `Action::About` shuts whatever was - so where it is drawn
    /// among the others decides nothing.
    fn show_the_about_box(&mut self, ui: &mut egui::Ui) {
        // The About box. Only one modal is open at a time — `Action::About` shuts whatever was —
        // so where it is drawn among the others decides nothing; it is here because it is the same
        // kind of thing as the two above, a small window over the project rather than about a file.
        if let Some(about) = self.about.take() {
            // What a check found, read fresh every frame: a check started from the box itself
            // answers while the box is still open, and the line changes under it. `task-1804` §6.
            let installable =
                matches!(&self.update_answer, Some(crate::services::update::Answer::Newer(_)))
                    && crate::services::update::PLATFORM_FIELD.is_some()
                    && crate::services::update_install::installed_at().is_ok()
                    && self.install.as_ref().is_none_or(|install| !install.is_running());
            let showing = about.clone().with_update(self.update_line()).installable(installable);
            let outcome = about_dialog::show(ui.ctx(), &showing);
            if outcome.check {
                self.check_for_updates();
            }
            if outcome.install {
                if let Err(problem) = self.install_the_update(true) {
                    self.toasts.say(problem, crate::components::toast::Kind::Problem);
                }
            }
            if !outcome.close {
                self.about = Some(about);
            }
        }
        // The microphone or speech model dialog the microphone button opens when dictation cannot start.
        self.show_the_voice_dialog(ui.ctx());
    }

    /// The `Run Configurations` modal, drawn beside the other project-wide dialogs.
    fn show_the_run_dialog(&mut self, ui: &mut egui::Ui) {
        // The `Run Configurations` modal, drawn beside the other project-wide dialogs.
        {
            let running: Vec<String> = self
                .run
                .runs()
                .iter()
                .filter(|run| run.is_running())
                .map(|run| run.name().to_owned())
                .collect();
            let mut dialog = std::mem::take(&mut self.run_dialog);
            dialog.root = self.tree.root().to_path_buf();
            let outcome =
                run_dialog::show(ui.ctx(), &mut dialog, &mut self.run_configurations, &running);
            self.run_dialog = dialog;
            if outcome.browse_directory {
                self.choose_a_run_directory();
            }
            if outcome.changed {
                self.unsaved_run_configurations = true;
            }
            if let Some(name) = outcome.remove {
                self.run_configurations.remove(&name);
                if self.run_selected.as_deref() == Some(name.as_str()) {
                    self.run_selected = None;
                }
                self.unsaved_run_configurations = true;
            }
            if let Some(name) = outcome.confirm_removal {
                // The same furniture the git dialogs use, because silently killing a server
                // somebody is watching is worse than one extra click.
                self.confirmation = Some(Confirmation {
                    title: "Remove".to_owned(),
                    note: format!("{name} is running. Removing it stops the program first."),
                    button: "Remove".to_owned(),
                    answer: Answer::RemoveRun(name),
                });
            }
            if outcome.closed {
                self.unsaved_run_configurations = true;
            }
        }
    }

    /// Open the folder picker for the chosen run configuration's `Directory`, starting at the folder
    /// the field names now (or the project when that is not a folder), and write the answer into it.
    fn choose_a_run_directory(&mut self) {
        let Some(name) = self.run_dialog.chosen.clone() else {
            return;
        };
        let root = self.tree.root().to_path_buf();
        let current = self
            .run_configurations
            .find(&name)
            .map(|(_, configuration)| configuration.working_directory(&root))
            .filter(|folder| folder.is_dir())
            .unwrap_or_else(|| root.clone());
        if let Some(chosen) = (self.folder_picker)("Run configuration directory", &current) {
            self.set_run_directory(&name, &chosen);
        }
    }

    /// Point a run configuration's `Directory` at a chosen folder, stored the way the field always
    /// is: relative to the project when the folder is inside it. The one place that happens, so the
    /// folder button and `run set --directory` cannot come apart.
    pub fn set_run_directory(&mut self, name: &str, chosen: &std::path::Path) {
        let text = services::run_configurations::directory_for_choice(self.tree.root(), chosen);
        if let Some(configuration) = self.run_configurations.find_mut(name) {
            configuration.directory = text;
            self.unsaved_run_configurations = true;
        }
    }

    /// The Settings window, drawn last because it is a modal and sits over everything.
    ///
    /// Every borrow in here is a **field of `self` named on its own**, which is what lets the page
    /// closure hold `plugin_ui` mutably while the dialog holds `settings` and `settings_window`.
    fn show_the_settings_window(&mut self, ui: &mut egui::Ui) {
        // **Nothing below this line happens while the window is shut** (`task-1984` A1).
        // `settings_dialog::show` returns on its own second line for a closed window, and everything
        // between here and the call was run first: a clone of the whole `Settings`, a copy of every
        // installed font family's name, a clone of every plugin's icon into two vectors, and
        // `unluminous_cli_program`, which asks the operating system where this program is and then
        // asks the disk whether that is a file. Twice a second for the life of a window nobody has
        // opened the Settings on, which is the disk and the allocator both, and is `task-1805`'s two
        // rules in one function.
        if !self.settings_window.open {
            return;
        }
        let before = self.settings.clone();
        let project = self.folder_name().unwrap_or_default();
        let families: Vec<String> = self.renderer.families().to_vec();
        // Worked out before the window is drawn, because decoding an icon needs the context and
        // the settings window already has the plugins borrowed.
        let plugin_icons: Vec<(String, Option<egui::TextureHandle>)> = self
            .plugins
            .all()
            .iter()
            .map(|plugin| (plugin.id.clone(), plugin.icon.clone()))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(id, bytes)| {
                let texture = bytes.and_then(|bytes| self.icons.texture(ui.ctx(), &id, &bytes));
                (id, texture)
            })
            .collect();
        let icon_for = |id: &str| -> Option<egui::TextureHandle> {
            plugin_icons.iter().find(|(known, _)| known == id).and_then(|(_, icon)| icon.clone())
        };
        let store_folder = self.store.as_ref().map(|store| store.folder().to_path_buf());
        let on_disk = |id: &str| -> bool {
            store_folder
                .as_ref()
                .is_some_and(|folder| folder.join("plugins").join(id).join("plugin.conf").is_file())
        };
        // What each plugin that contributed a page calls it, in slot order, from its manifest.
        let page_names: Vec<String> = self
            .plugin_ui
            .surfaces()
            .pages
            .iter()
            .map(|surface| surface.what.name.clone())
            .collect();
        // And the mark each is listed with, which is the plugin's own rail mark when it has a pane.
        let page_marks: Vec<String> = self
            .plugin_ui
            .surfaces()
            .pages
            .iter()
            .map(|page| {
                self.plugin_ui
                    .surfaces()
                    .panes
                    .iter()
                    .find(|pane| pane.plugin == page.plugin)
                    .map(|pane| pane.what.icon.clone())
                    .unwrap_or_default()
            })
            .collect();
        // Taken apart before the call so the closure below can borrow `plugin_ui` while the dialog holds
        // `settings` and `settings_window`. Two disjoint fields of one value, which the compiler allows
        // only when they are named separately.
        // Asked only while the Voice page is the one showing, because it reads the disk and the registry.
        let voice_facts = match self.settings_window.page {
            crate::settings::Page::Voice => {
                self.keep_the_microphone_list_fresh();
                self.voice_facts()
            }
            _ => crate::components::voice_page::VoiceFacts::default(),
        };
        let page_look = crate::services::plugin_ui::Look::of(&self.settings, &self.renderer);
        let plugins_ui = &mut self.plugin_ui;
        let mut page_asked: Vec<(String, crate::services::plugin_ui::Request)> = Vec::new();
        let unluminous_cli_program = unluminous_cli::mcp::install::unluminous_cli_program();
        let settings_context = settings_dialog::SettingsContext {
            families: &families,
            project: &project,
            plugins: &self.plugins,
            // What the MCP page's status line reads. A window that never opened an endpoint — every
            // window a test builds — reads as off, which is what it is.
            mcp_running: self
                .mcp
                .as_ref()
                .map(|hosted| hosted.state())
                .unwrap_or(&services::mcp::State::Off),
            unluminous_cli: &unluminous_cli_program,
            installed_on_disk: &on_disk,
            icon_for: &icon_for,
            plugin_pages: &page_names,
            plugin_page_marks: &page_marks,
            voice: &voice_facts,
        };
        let settings_outcome = settings_dialog::show(
            ui.ctx(),
            &mut self.settings_window,
            &mut self.settings,
            settings_context,
            &mut self.endpoints,
            &mut |page_ui, slot, area| {
                // The contributed page, drawn inside the modal. `plugin_ui` and `settings` are separate
                // fields, so this can borrow one while the dialog holds the other.
                let plugin = plugins_ui
                    .surfaces()
                    .pages
                    .get(slot)
                    .map(|surface| (surface.plugin.clone(), surface.provider.clone()));
                let Some((plugin, provider)) = plugin else {
                    return;
                };
                match plugins_ui.opened(&plugin, &provider) {
                    Ok(_) => {
                        let mut inner = page_ui.new_child(egui::UiBuilder::new().max_rect(area));
                        // Intersected rather than assigned: `Ui::set_clip_rect` assigns, so writing a
                        // component's own rectangle over it throws away whatever the caller had cut it
                        // to. `components::explorer` has recorded that since `task-1905`, and the
                        // dialog is a caller that cuts it now -- it clips the page to the body so a
                        // page taller than the body runs under the footer rather than over it.
                        inner.set_clip_rect(page_ui.clip_rect().intersect(area));
                        if let Some(opened) = plugins_ui.provider(&plugin) {
                            let wanted = opened.settings(&mut inner, &page_look);
                            page_asked.extend(
                                wanted.into_iter().map(|request| (plugin.clone(), request)),
                            );
                        }
                    }
                    Err(problem) => page_asked.push((
                        plugin.clone(),
                        crate::services::plugin_ui::Request::Message(problem),
                    )),
                }
            },
        );
        for (plugin, request) in std::mem::take(&mut page_asked) {
            self.act_on_a_plugin_request(&plugin, request, ui.ctx());
        }
        let page_changed = settings_outcome.page == settings_dialog::PageOutcome::Changed;
        match settings_outcome.page {
            settings_dialog::PageOutcome::Install(id) => self.install_plugin(&id),
            settings_dialog::PageOutcome::Uninstall(id) => self.uninstall_plugin(&id),
            settings_dialog::PageOutcome::SetEnabled(id, on) => self.set_plugin_enabled(&id, on),
            // Over the Settings window rather than in place of it, which is what every modal opened
            // from another one does: closing the grid puts Appearance back where it was.
            settings_dialog::PageOutcome::OpenBackgrounds => self.background_grid = Some(None),
            settings_dialog::PageOutcome::EndpointsChanged => self.the_endpoints_changed(),
            settings_dialog::PageOutcome::Voice(ask) => self.voice_page_asked(ask),
            settings_dialog::PageOutcome::Nothing | settings_dialog::PageOutcome::Changed => {}
        }
        if page_changed || self.settings != before {
            self.apply_settings(&before);
        }
    }

    /// The picture `appearance.background.image` names, drawn inside the window's rounded corners.
    ///
    /// **Scaled to cover and clipped to the corners.** A picture that fitted would leave bands of
    /// nothing down two sides, and the window is drawn on a transparent ground so there is no colour to
    /// put there — see `services::backgrounds::cover`. The corners are cut by painting it as a rounded
    /// rectangle with the picture as its brush, cut at the seams between the strips the picture was
    /// uploaded in (`services::backgrounds::Wallpaper`).
    ///
    /// Nothing at all when no picture is chosen, or when the one named is not there: the desktop shows
    /// through, which is what `appearance.background.image` says by being empty and what a picture
    /// somebody deleted by hand should fall back to.
    ///
    /// **And it is what glass bends** (`task-2281`): the picture's strips and where they are drawn are
    /// handed to `rux` for the page, every frame, so a raised surface in a glass theme samples the
    /// picture exactly where it is behind it. Whatever floats above the page starts each frame with
    /// nothing of its own; a modal hands over its photograph of the window when it is drawn.
    fn paint_the_background(&mut self, ui: &mut egui::Ui, full: Rect) {
        let ctx = ui.ctx().clone();
        rux::render::set_backdrop(&ctx, rux::render::Plane::Above, None);
        let Some(picture) = self.wallpaper.texture(
            &ctx,
            self.backgrounds.as_deref(),
            self.settings.background_picture(),
        ) else {
            rux::render::set_backdrop(&ctx, rux::render::Plane::Page, None);
            theme::glass::set_frosted(&ctx, None);
            return;
        };
        let taken = crate::services::backgrounds::cover(picture.size_vec2(), full.size());
        picture.paint(ui.painter(), full, CornerRadius::same(size::WINDOW_CORNER), taken);
        let backdrop = match crate::theme::is_glass() {
            true => Some(rux::render::Backdrop {
                bands: picture.bands(),
                rect: full,
                uv: taken,
                opacity: 1.0,
            }),
            false => None,
        };
        rux::render::set_backdrop(&ctx, rux::render::Plane::Page, backdrop);
        theme::glass::set_frosted(
            &ctx,
            crate::theme::is_glass().then(|| (picture.frosted(), taken)),
        );
    }

    /// The notices, over every pane and under a modal.
    ///
    /// Before the resize grips so a cross near the window's corner is still pressable: the grips take
    /// the outermost few points and are added after everything, so anything that must be clickable there
    /// goes first.
    fn show_the_notices(&mut self, ui: &mut egui::Ui, places: &FramePlaces) {
        let full = places.full;
        // The notices, over every pane and under a modal. Before the resize grips so a cross near the
        // window's corner is still pressable — the grips take the outermost few points and are added
        // after everything, so anything that must be clickable there goes first.
        //
        // The stale ones are forgotten here rather than on a timer: this runs once a frame, and a window
        // that is drawing is a window somebody is looking at. A `Problem` is never dropped by this — only
        // a person takes one of those away, which is `components::toast`'s whole point.
        self.toasts.forget_the_stale_ones();
        let state = self
            .toast_rux
            .take()
            .unwrap_or_else(|| crate::theme::rux_state(crate::theme::rux_theme()));
        let pressed = crate::components::toast::show(ui, full, &self.toasts, &state);
        state.end_frame();
        self.toast_rux = Some(state);
        if let Some(dismissed) = pressed {
            match dismissed {
                crate::components::toast::Pressed::Dismissed(index) => self.toasts.dismiss(index),
                crate::components::toast::Pressed::Copied(index) => {
                    if let Some(notice) = self.toasts.notices().get(index) {
                        ui.ctx().copy_text(notice.text.clone());
                    }
                }
                crate::components::toast::Pressed::Acted(index, act) => {
                    self.toasts.dismiss(index);
                    self.act_on_a_notice(act);
                }
            }
        }
    }

    /// The eight places the window itself is resized from, added last so they sit over every pane.
    ///
    /// The editing area, the explorer and the status bar all take drags over the whole of their
    /// rectangles, and a grip added earlier would never see a pointer.
    fn show_the_resize_grips(&mut self, ui: &mut egui::Ui, places: &FramePlaces) {
        let full = places.full;
        // The eight places the window itself is resized from, added last so they sit over every pane:
        // the editing area, the explorer and the status bar all take drags over the whole of their
        // rectangles, and a grip added earlier would never see a pointer. See `components::resize_edges`
        // for why they exist at all, which is that Unluminous's window has no operating system frame.
        // Nothing is added at all while the window is maximised, which is Unluminous's rule for a
        // control that cannot apply and — the part that matters — is what stops a request the window
        // manager will refuse ever being sent. `components::resize_edges` records what one of those
        // costs: it wedges every later move and resize as well.
        let maximized = ui.ctx().input(|input| input.viewport().maximized.unwrap_or(false));
        // **The grips give up the points every pane divider wants**, which is the other half of
        // `task-2062`: a divider that reaches the window's edge was under a grip, and both set the same
        // double headed cursor, so the arrows appeared and the drag went to the window rather than to the
        // divider. Read here because this is after every pane and every panel has been drawn, which is
        // the earliest moment all of them are known — the ordering `settle_the_panel_drag` keeps.
        let dividers = crate::components::splitter::dividers_drawn_this_frame(ui.ctx());
        // **None of egui's grips once Windows answers the hit test itself** (`task-2063`): a press on
        // an edge is then a press on the window's frame, which never reaches egui, and the grips would
        // only be eight widgets nothing can press. The dividers are handed to the hit test instead, so
        // it gives up the same points the grips do. See `services::windows_resize`.
        crate::services::windows_resize::keep_clear(&dividers, ui.ctx().pixels_per_point());
        let direction = match self.native_resize {
            true => None,
            false => resize_edges::show(ui, full, maximized, &dividers),
        };
        // **And nothing is asked for while a page holds the operating system's keyboard**, which is the
        // same rule read against the other case the window manager throws a request away in.
        // `egui-winit` already refuses to forward `StartDrag` unless `Window::has_focus()`, and `winit`
        // answers that with `is_active && is_focused` — so a native child that has taken `SetFocus`,
        // which is what a browser node's page does, makes it false. `BeginResize` is **not** behind that
        // check upstream, so it reaches `handle_os_dragging`, which latches a flag that only
        // `WM_EXITSIZEMOVE` clears and returns early from every later move and resize for the life of
        // the process. `task-1945`.
        //
        // **Asked of the browser host rather than of `winit`** (`task-2004`). The two are the same answer
        // in the case this guard exists for and different answers everywhere else: a window merely in the
        // background reports no focus too, and there every grip was dead for no reason. And the page has
        // already been told to give the keyboard back by the time a grip is pressed — see
        // `settle_the_native_views_before_the_pass` — but `winit` cannot know that until it has had its
        // `WM_SETFOCUS`, which is a frame later than the press this drag started on.
        let page_has_it = self.browser.page_holds_the_keyboard();
        // A drag on the window's own edge ends when the button comes up, and the next one starts again
        // from wherever the window is then.
        if !ui.input(|input| input.pointer.primary_down()) {
            self.edge_drag = None;
        }
        if let Some(gesture) = resize_edges::ask_for_it(direction, page_has_it) {
            // **Written down as well as sent**, because `BeginResize` goes to the window manager and
            // nothing inside this process can watch a window change size. What a test — and
            // `status --section window` — can read back is what the window *asked for*, which is the one
            // thing this file decides. It is the last request rather than this frame's, for the same
            // reason: a drag is asserted on some frame after the one it started on.
            self.last_resize_asked = Some(gesture.direction());
            match gesture {
                resize_edges::Gesture::Begin(direction) => {
                    // Watched, so a request that starts no size loop cannot leave `winit`'s drag flag
                    // stuck. See `services::windows_resize::Unlatch`.
                    let now = ui.input(|input| input.time);
                    self.unlatch.asked(now);
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
                // **macOS has nothing to hand the drag to, so the window moves its own edge.**
                // `winit`'s `drag_resize_window` is `NotSupported` there for all eight directions, so
                // every grip on this platform sent a request that had never resized anything — see
                // `components::resize_edges`. The two commands used are the ones
                // `unluminous-cli window size` and `window position` already drive this window with,
                // rather than a second mechanism, and the position goes **first**: a window whose top
                // left is about to move is a window whose size and origin change together, and asking
                // for the size first draws one frame with the far edge in the wrong place.
                resize_edges::Gesture::Move { direction, by } => {
                    self.move_the_windows_own_edge(ui.ctx(), direction, by);
                }
            }
        }
    }

    /// Move one edge of the window by `by`, for the platform with no window manager drag to hand a
    /// resize to.
    ///
    /// The **position** is the outer rectangle's and the **size** the inner one's, which is the pair the
    /// `ViewportBuilder` takes back and the pair `note_where_the_window_is` writes down.
    ///
    /// **Where egui has reported no position, the size is still changed.** Only a west or a north drag
    /// needs to know where the window is — those move it as well as resize it — so a window whose
    /// position is unknown can still be made larger or smaller from its right or bottom edge. Returning
    /// early on the whole thing instead left every grip dead in a window that had not been told its own
    /// position yet, which is what `egui_kittest` is and is where this was caught.
    fn move_the_windows_own_edge(
        &mut self,
        ctx: &egui::Context,
        direction: egui::viewport::ResizeDirection,
        by: egui::Vec2,
    ) {
        // Where the pointer is on the screen, which does not move when the window does. See
        // `resize_edges::EdgeDrag` for what measuring inside the window cost.
        let on_screen = crate::services::system_files::pointer_on_screen();
        let drag = self.edge_drag.unwrap_or_else(|| {
            let (position, inner) = ctx.input(|input| {
                let viewport = input.viewport();
                (
                    viewport.outer_rect.map(|outer| outer.min),
                    viewport.inner_rect.map(|inner| inner.size()),
                )
            });
            // The size the window is now. `content_rect` is what the frame was drawn into and is the
            // honest fallback: it is what `unluminous-cli window size` reports and reads back.
            let was = inner.unwrap_or_else(|| ctx.content_rect().size());
            resize_edges::EdgeDrag::begin(position, was, on_screen)
        });
        let step = drag.step(direction, by, on_screen, crate::app::SMALLEST_WINDOW);
        // Position first, for the reason the caller gives. The position is only sent where it was
        // really known when the drag began: sent from a guessed origin, a west drag would fling the
        // window to the top left of the screen.
        if let Some(at) = step.position {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(at));
        }
        if let Some(size) = step.size {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
        // Drawn again until the button comes up, because the pointer can carry on across the screen
        // while staying put inside a window that is moving with it, and nothing else would ask.
        ctx.request_repaint();
        self.edge_drag = Some(step.drag);
    }

    /// What is written down at the end of a frame, and the two things that wait for the second one.
    ///
    /// Everything here is on the same terms as the settings: once the pointer is up, and only when
    /// something has actually changed.
    fn end_the_frame(&mut self, ui: &mut egui::Ui) {
        // What is open in this project is written down for next time, on the same terms as the
        // settings: once the pointer is up, and only when something has actually changed.
        //
        // **On every frame, deliberately** (`task-1984` A4). What made this expensive was
        // `Session::folder`, which it asks for every terminal tab and which reached the kernel and
        // then the disk; that is asked on a clock now, inside the session. Putting *this* on a clock
        // as well was tried and taken out: `a_split_project_opens_split_again` and everything like it
        // reads the file on the frame after the change, and a person who closes a window half a
        // second after splitting a pane should not lose the split. What a project remembers is
        // written when it changes, which is what the paragraph above says.
        // The time this frame was drawn, which is what decides whether a change that is only where the
        // window is has settled. See `UnluminousApp::window_still_since`.
        let now = ui.input(|input| input.time);
        self.remember_the_project(Some(now));
        crate::services::frame_trace::phase("remember");
        // And the canvas, which says for itself whether anything on it changed - `task-1904`. Written
        // at the end of a frame on which something moved rather than on every frame, or dragging a
        // node would write a file sixty times a second.
        //
        // **And not while the canvas is still moving** (`task-2218`). Panning and zooming change the
        // camera on every frame of the gesture, and the camera is in the realm's sidecar, so a drag
        // across the canvas wrote the sidecar on every frame: 5.8 ms of an 11 ms frame. The camera is
        // written once the pointer is up and the zoom has finished gliding.
        let settled = !ui.input(|input| input.pointer.any_down());
        if settled && self.realm.glide.is_none() {
            self.write_the_realm_if_it_changed(now);
        }
        // And what is marked in its files, on exactly the same terms.
        self.remember_the_marks(settled);
        // And where it stops, on the same terms again.
        self.remember_the_breakpoints(settled);
        crate::services::frame_trace::phase("marks");

        // Settings are written once the pointer is up, so that dragging a divider or a slider writes the
        // file once at the end rather than on every frame of the drag.
        // And not while a zoom gesture is still taking steps, which changes the font size on every notch
        // of the wheel with the pointer up - `task-2218`.
        if self.unsaved_settings && settled && !self.a_zoom_is_still_arriving(now) {
            self.write_settings();
        }
        // And the project's run configurations, on exactly the same terms: typing into a field in
        // the dialog would otherwise write the file on every keystroke.
        if settled {
            self.remember_the_run_configurations();
        }
        // The shells this project was left with, on the **second** frame. Not the first: eframe
        // keeps the window hidden until it has painted once, so work at the end of the first frame
        // is still blank desktop and would give back nothing of what moving it here is for. The
        // window is asked to draw again at once, so the terminal tile is empty for one frame rather
        // than until `HEARTBEAT` next wakes it half a second later.
        if !self.terminals_to_restore.is_empty() {
            match self.frames {
                0 => ui.ctx().request_repaint(),
                _ => self.start_the_restored_terminals(),
            }
        }
        // And the canvas's own, on the same frame and for the same reason - `task-1904`. A terminal
        // node comes back as a fresh shell running the same command in the same folder, which is what
        // `project_state` already promises about the terminal tile, and a browser node comes back on
        // the address it was left on. Both are asked for once, on the second frame, because starting
        // a pseudoconsole before the window is shown is a fifth of the time before anything appears.
        if self.frames == 1 && self.remembers_this_project() {
            self.bring_the_current_view_to_life();
        }
        self.frames += 1;
        crate::services::frame_trace::phase("rest");
        crate::services::frame_trace::end();
    }

    /// What the run widget in the title bar needs to know to draw itself.
    ///
    /// Worked out here rather than in the widget for the reason every component in Unluminous decides
    /// nothing: the widget draws what it is handed and reports what was pressed.
    fn run_widget_state(&self) -> run_widget::WidgetState {
        let rows = self.run_rows();
        let running = self
            .run_selected
            .as_deref()
            .and_then(|name| self.run.index_of(name))
            .and_then(|at| self.run.at(at))
            .is_some_and(run_panel::Run::is_running);
        // `Run Current File` names the file it would run, so the row says what pressing it will do.
        let current_file = self.run_file_template().and_then(|_| {
            self.document()
                .path()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().to_string())
        });
        // The bug button is there when the configuration the play button would start resolves to a
        // debugger — asked of the thing the button acts on rather than of whichever tab is focused,
        // so it does not come and go as tabs are switched. `task-1692` §4.
        let debuggable = self
            .configuration_named(None)
            .or_else(|| self.suggestions().into_iter().next())
            .and_then(|configuration| {
                let adapter = self.adapter_for(&configuration, None)?;
                debuggers::can_debug(&adapter, &configuration.command).then_some(())
            })
            .is_some()
            || self.debug_applies_here();
        run_widget::WidgetState {
            selected: self.run_selected.clone(),
            rows,
            running,
            debuggable,
            current_file,
        }
    }

    /// Remember where the window is, which is the other half of "in the same location and state".
    ///
    /// Read from egui once a frame rather than reported by whatever moved the window, which is
    /// `follow_the_open_file`'s rule: a list of the places that have to say "I moved it" is a list
    /// whose next entry will be the one that forgot, and there are four of them here — the title
    /// bar's drag, the resize grips, the platform's own snap, and `unluminous-cli window position`.
    ///
    /// The **position** comes from the outer rectangle and the **size** from the inner one, because
    /// those are the two the `ViewportBuilder` takes back. A maximised window records its geometry
    /// as it is, so that a window restored from maximised is the size it was before — but it is the
    /// `maximised` flag that decides how it opens.
    fn note_where_the_window_is(&mut self, ctx: &egui::Context) {
        // **In the system's logical points rather than egui's** (`task-2280`). egui's points are those times
        // the interface zoom, and the window is built before any zoom is set, so a geometry kept in egui's
        // points would open the window smaller by the zoom on every start. Asked before `input` rather than
        // inside it, because both take the context's lock.
        let zoom = ctx.zoom_factor();
        let place = ctx.input(|input| {
            let viewport = input.viewport();
            let outer = viewport.outer_rect?;
            let inner = viewport.inner_rect.unwrap_or(outer);
            Some(project_state::WindowPlace {
                x: outer.min.x * zoom,
                y: outer.min.y * zoom,
                width: inner.width() * zoom,
                height: inner.height() * zoom,
                maximised: viewport.maximized.unwrap_or(false),
            })
        });
        // A maximised window's own geometry is the whole screen, and remembering that as the size to
        // restore to would mean a window that could never be made small again. So the size is kept
        // as it was before it was maximised, and only the flag moves.
        match (place, self.window_place) {
            (Some(now), Some(before)) if now.maximised && !before.maximised => {
                self.window_place = Some(project_state::WindowPlace { maximised: true, ..before });
            }
            (Some(now), _) if now.is_sensible() => self.window_place = Some(now),
            _ => {}
        }
    }

    /// Settle the native child views, before the egui pass rather than inside it.
    ///
    /// Creating a WebView2 controller blocks in a nested Windows message pump, and a nested pump run
    /// from inside the pass never came back: the first browser tab worked, the second one hung the
    /// window for good — no frame was ever drawn again, though the thread was still dispatching
    /// messages. This hook runs before the pass begins, so there is no pass for a dispatched message
    /// to re-enter. It uses the placements the last frame drew, which is where the views already are,
    /// and a frame that changes them draws before the next one is reconciled.
    pub(crate) fn settle_the_native_views_before_the_pass(
        &mut self,
        ctx: &egui::Context,
        raw_input: &mut egui::RawInput,
    ) {
        // **Input asked for down the command line, before the pass rather than inside it.**
        // `InputState::pointer` is derived during `begin_pass`, so an event pushed into
        // `ctx.input_mut().events` half way through a frame reaches anything reading the event list and
        // nothing reading the pointer — which is every widget, because they all ask `Response::clicked`.
        // One step a frame, which is what makes a press and a release a click rather than a flicker.
        // See `services::input`.
        if let Some(events) = self.input.next_frame() {
            raw_input.events.extend(events);
        }
        if !self.input.is_empty() {
            // Another step is waiting and nothing else will ask for the frame it needs.
            ctx.request_repaint();
        }
        // Which browser pages are told they are talking to, before any view is made. `task-2255`.
        self.browser.set_agent(self.settings.browser_agent);
        self.browser.set_links_open_new_tabs(self.settings.browser_links.opens_new_tabs());
        let on_the_canvas = self.realm.live.browsers().count();
        if self.files.iter().all(|file| file.browser.is_none())
            && on_the_canvas == 0
            && self.design_tabs.is_empty()
            && !self.browser.has_views()
        {
            return;
        }
        // **The canvas's browser nodes are in the same list**, because a window has one native child
        // view and it is pointed at whichever tab is showing - a node's page and a tab's page are two
        // claims on the same one. `task-1904`.
        let tabs: Vec<BrowserTab> = self
            .files
            .iter()
            .filter_map(|file| file.browser.clone())
            .chain(self.realm.live.browsers().cloned())
            .chain(self.design_tabs.values().cloned())
            .collect();
        let occluders = self.occluding_rects(ctx);
        let mut placements = self.browser_placements.clone();
        // **A press that landed on none of the pages hands the operating system's keyboard back, and it
        // stays back until one lands on a page again.** `placement.focused` says only that this node is
        // the chosen one, and pressing the title bar, a resize grip, the menu bar or the rail changes
        // which node is chosen not at all — so the page kept the keyboard through every one of them and
        // `winit` went on saying the window had none. See
        // `services::browser::the_page_was_the_last_thing_pressed`, and `task-2004`.
        self.page_was_pressed = crate::services::browser::the_page_was_the_last_thing_pressed(
            &raw_input.events,
            &placements,
            &occluders,
            self.page_was_pressed,
        );
        // **A press inside a page reaches the page and not egui**, because the page is a native child window
        // the operating system gives the click to directly. So no press egui saw ever landed on a page, and
        // the line above answered "not the page" on every frame: the frame that the page's own `SetFocus`
        // woke handed the keyboard straight back to the window, and `Ctrl+V` in a field on the page went to
        // Unluminous instead (`task-2255`: *"I can't seem to paste into fields in the browser tab."*). The
        // platform is asked instead: the page holds the keyboard and the pointer is over it, so somebody
        // clicked into it, and the tab or node it belongs to becomes the one with the keyboard.
        let mut clicked_into = None;
        if !self.page_was_pressed {
            if let Some(id) = self.browser.the_page_clicked_into() {
                self.page_was_pressed = true;
                clicked_into = Some(id);
            }
        }
        if let Some(id) = clicked_into {
            // egui never saw the click, so nothing it has open knows it was clicked outside of.
            crate::components::controls::close_every_menu(ctx);
            for placement in &mut placements {
                placement.focused = placement.id == id;
            }
            self.give_the_keyboard_to_the_page(id);
        }
        if !self.page_was_pressed {
            for placement in &mut placements {
                placement.focused = false;
            }
        }
        // **A press egui saw on a page gives the view to that page** (`task-2263`), whichever pane held the keyboard
        // before it. The live page takes its own clicks, so this is a press on a page showing its picture. The page is
        // given the keyboard too, as a click into a live page is, so on the next frame its pane is the focused one and
        // the view does not go back to the pane that held the keyboard before.
        if let Some(id) =
            crate::services::browser::the_page_pressed(&raw_input.events, &placements, &occluders)
        {
            for placement in &mut placements {
                placement.focused = placement.id == id;
            }
            self.give_the_keyboard_to_the_page(id);
        }
        // **`Escape` puts the element picker away** when the keys are Unluminous's rather than the page's, which
        // they are when the picker was opened from the toolbar's button. `task-2203`.
        let escape = raw_input.events.iter().any(|event| {
            matches!(event, egui::Event::Key { key: egui::Key::Escape, pressed: true, .. })
        });
        if escape && self.browser.picking().is_some() {
            self.browser.cancel_picking();
        }
        // **A picture of the page the view is about to leave**, taken before the view is sent anywhere else, so
        // switching back to that tab shows its page at once while the page loads again. `task-2255`. Only while a
        // window keeps to one view: with a view a page, nothing leaves a page (`task-2270`).
        let leaving = self.browser.showing().filter(|_| self.browser.one_view());
        let going_to =
            crate::services::browser::going_to(&placements, leaving).map(|placement| placement.id);
        if let (Some(leaving), Some(going_to)) = (leaving, going_to) {
            // A design's page as well as a tab's, so the pane it leaves can show its picture. `task-2263`. **Only a
            // page that has finished loading**: while the view is still loading a page it shows the one before, and
            // a picture of that, kept under this page's id, put another page's content in the Designer's well.
            let settled = tabs.iter().any(|tab| tab.id == leaving && !tab.loading);
            if leaving != going_to
                && settled
                && (self
                    .files
                    .iter()
                    .any(|file| file.browser.as_ref().is_some_and(|tab| tab.id == leaving))
                    || self.design_tabs.values().any(|tab| tab.id == leaving))
            {
                if let Some(picture) = self.browser.photograph(leaving) {
                    let texture = crate::services::picture::upload(
                        ctx,
                        "pane-page-still".to_owned(),
                        picture,
                        egui::TextureOptions::LINEAR,
                    );
                    self.page_stills.keep(leaving, texture);
                }
            }
        }
        // A design's links stay in the design, whatever `browser.links` says. `task-2270`. So do a browser
        // node's: `browser.links` is about the browser tabs in the editing area. `task-2325`.
        let links_stay: Vec<u64> = self
            .design_tabs
            .values()
            .map(|tab| tab.id)
            .chain(self.realm.live.browsers().map(|tab| tab.id))
            .collect();
        let settled =
            self.browser.reconcile(&tabs, &placements, &occluders, ctx.clone(), &links_stay);
        for id in &settled.pointed_at {
            self.page_stills.switched_to(*id);
        }
        // The picker's answer is asked for on a frame, so while it is open the window keeps drawing.
        // The same for a page the Designer keeps a script in: its answers are asked for on a frame. `task-2254`.
        if self.browser.picking().is_some()
            || self.pick_when_showing.is_some()
            || self.browser.watching().is_some_and(|id| self.browser.is_live(id))
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        // **What a page's own command can be asked about.** A page is a native child view, so nothing
        // Unluminous photographs holds one and no state in the window said whether it was drawing —
        // which is why `task-2009`'s blanked page had to be found by looking at the screen.
        self.page_is_covered = settled.covered;
        for id in settled.pointed_at {
            self.change_browser_tab(id, |tab| tab.pointed_at());
            // **A browser node's own zoom is applied again when the one view arrives at its tab**, and
            // since `task-2004` it is applied by the placement rather than here. A window has one native
            // child, so zooming a node whose page is not the one rendering could not reach the engine at
            // the time — the factor is remembered on the node, and `show_a_browser_node` puts it on the
            // node's placement every frame. The Codex Sol review of `task-1905` found that it never was
            // spent at all; the fix was a second call that computed the same product a second time, and
            // `NativeView::zoom` is forgotten when the view is pointed somewhere else, so the placement's
            // answer is believed on the first frame at the new tab.
        }
        for (id, problem) in settled.problems {
            self.change_browser_tab(id, |tab| tab.problem = Some(problem.clone()));
            self.message = Some(problem);
        }
    }
}

/// A language server's state in the two or three words the footer has room for.
///
/// @param state - what the server is doing
fn footer_words(state: &unluminous_lsp::ServerState) -> String {
    match state {
        unluminous_lsp::ServerState::Indexing { percent: Some(p), .. } => format!("indexing {p}%"),
        unluminous_lsp::ServerState::Indexing { percent: None, .. } => "indexing".to_owned(),
        unluminous_lsp::ServerState::Failed(_) => "stopped".to_owned(),
        unluminous_lsp::ServerState::Absent(_) => "not found".to_owned(),
        other => other.describe(),
    }
}
