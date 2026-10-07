//! The bodies of the node kinds `task-2202` added: a picture, a sound, a video, a note, and a node of a kind
//! this Unluminous does not know.
//!
//! Each is drawn the way the six older kinds are, into the node's own layer through `show_a_node_body`, and
//! each answers for itself when there is nothing to show — a file that is missing, a file the realm named
//! outside the project, a file too large to open — with one sentence in the middle of the node rather than an
//! empty rectangle.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Vec2};

use crate::app::UnluminousApp;
use std::path::{Path, PathBuf};

use crate::services::realm::{Fit, Kind, Node, NoteView, State};
use crate::theme::color;
use crate::theme::crisp::CrispPainter;

/// A sentence in the middle of a node, with an accessible name, for a node that has nothing else to show.
pub(crate) fn say_in_a_node(ui: &egui::Ui, body: Rect, id: u64, said: &str) {
    let painter = ui.painter_at(body);
    let galley = painter.layout(
        said.to_owned(),
        FontId::proportional(12.0),
        color::text_dim(),
        (body.width() - 24.0).max(40.0),
    );
    let at =
        Pos2::new(body.center().x - galley.size().x / 2.0, body.center().y - galley.size().y / 2.0);
    painter.galley(at, galley, color::text_dim());
    let response = ui.interact(body, egui::Id::new(("realm-said", id)), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, said));
}

/// Why a node that shows a file has nothing to show, when it has nothing.
pub(crate) fn why_there_is_nothing(node: &Node) -> Option<String> {
    if let Some(refused) = &node.refused {
        return Some(refused.clone());
    }
    let Some(file) = node.state.file() else {
        return Some(format!("This {} node names no file.", node.kind().label().to_lowercase()));
    };
    if !file.is_file() {
        let name =
            file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        return Some(format!("{name} is missing."));
    }
    None
}

/// The rectangle a picture `size` pixels across is drawn in, inside `body`, for `fit`.
///
/// **Contain** is the whole picture as large as fits, centred; **cover** fills the body and lets the rest go
/// past its edges, where the clip cuts it off; **actual** is one pixel a point times the node's own zoom, from
/// the top left corner moved by its scroll.
pub(crate) fn where_a_picture_goes(
    body: Rect,
    size: Vec2,
    fit: Fit,
    zoom: f32,
    scroll: Vec2,
) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return body;
    }
    match fit {
        Fit::Contain | Fit::Cover => {
            let (across, down) = (body.width() / size.x, body.height() / size.y);
            let scale = match fit {
                Fit::Contain => across.min(down),
                _ => across.max(down),
            };
            Rect::from_center_size(body.center(), size * scale)
        }
        Fit::Actual => Rect::from_min_size(body.min - scroll, size * zoom.max(0.01)),
    }
}

impl UnluminousApp {
    /// A picture node: the file, decoded by `services::picture` and fitted to the node.
    ///
    /// A double click flips between the realm's own fit and one pixel a point, which is what a picture
    /// viewer's double click does; the wheel with the command key zooms it, through `zoom_a_node`.
    pub(crate) fn show_an_image_node(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node,
        body: Rect,
        focused: bool,
    ) {
        let _ = focused;
        let State::Image(image) = &node.state else { return };
        if let Some(why) = why_there_is_nothing(node) {
            say_in_a_node(ui, body, node.id, &why);
            return;
        }
        let file = image.file.clone().expect("why_there_is_nothing said there is a file");
        let too_large = std::fs::metadata(&file)
            .map(|about| about.len() > crate::services::file_kind::SIZE_LIMIT)
            .unwrap_or(false);
        if too_large {
            say_in_a_node(ui, body, node.id, "This picture is larger than Unluminous opens.");
            return;
        }
        let name =
            file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let picture = self.realm.live.picture(node.id, &file);
        if let Some(problem) = &picture.problem {
            say_in_a_node(ui, body, node.id, &format!("{name} could not be read: {problem}"));
            return;
        }
        let size = Vec2::new(picture.size[0] as f32, picture.size[1] as f32);
        let Some(texture) = picture.texture(ui.ctx(), &format!("realm-image-{}", node.id)) else {
            return;
        };
        let rect = where_a_picture_goes(body, size, image.fit, image.zoom, image.scroll);
        let painter = ui.painter_at(body);
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        let response =
            ui.interact(body, egui::Id::new(("realm-image", node.id)), Sense::click_and_drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Image, true, format!("Picture: {name}"))
        });
        let (id, fit) = (node.id, image.fit);
        if response.double_clicked() {
            self.realm.realm.change(id, |state| {
                if let State::Image(image) = state {
                    image.fit = match fit {
                        Fit::Actual => Fit::Contain,
                        _ => Fit::Actual,
                    };
                    image.scroll = Vec2::ZERO;
                }
            });
        } else if fit == Fit::Actual && response.dragged() {
            let by = response.drag_delta();
            self.realm.realm.change(id, |state| {
                if let State::Image(image) = state {
                    image.scroll -= by;
                }
            });
        }
    }

    /// A node of a kind this Unluminous does not know: a dashed frame, the kind's name and what it means.
    ///
    /// It can be moved, resized, wired and deleted like any other, through its header, and it cannot be
    /// opened, because there is nothing here that knows what it holds. §5.2 rule 2.
    pub(crate) fn show_an_unknown_node(&mut self, ui: &mut egui::Ui, node: &Node, body: Rect) {
        let State::Unknown(unknown) = &node.state else { return };
        let painter = ui.painter_at(body);
        let frame = body.shrink(6.0);
        let corners = [
            frame.left_top(),
            frame.right_top(),
            frame.right_bottom(),
            frame.left_bottom(),
            frame.left_top(),
        ];
        painter.extend(egui::Shape::dashed_line(
            &corners,
            egui::Stroke::new(1.2, color::text_faint()),
            6.0,
            4.0,
        ));
        crate::theme::icon::unknown(
            &painter,
            Pos2::new(body.center().x, body.center().y - 22.0),
            color::text_dim(),
        );
        painter.crisp_text(
            Pos2::new(body.center().x, body.center().y + 2.0),
            Align2::CENTER_CENTER,
            &unknown.kind,
            FontId::proportional(13.0),
            color::text_strong(),
        );
        painter.crisp_text(
            Pos2::new(body.center().x, body.center().y + 22.0),
            Align2::CENTER_CENTER,
            "Made by a newer Unluminous. Kept as it was written.",
            FontId::proportional(11.0),
            color::text_dim(),
        );
        let response = ui.interact(body, egui::Id::new(("realm-unknown", node.id)), Sense::hover());
        let kind = unknown.kind.clone();
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Other,
                true,
                format!("Unknown node: {kind}"),
            )
        });
    }

    /// Where a sound was, written to the node's sidecar when it has moved: on pause, and every five seconds
    /// while it plays. §5.4.
    pub(crate) fn note_where_a_player_is(&mut self, node: u64) {
        let (now, playing) = match self.realm.live.a_player(node) {
            Some(player) => (player.position().as_secs_f32(), player.playing()),
            None => match self.realm.live.video_report(node) {
                Some((at, playing, _)) => (at, playing),
                None => return,
            },
        };
        let held = match self.realm.realm.node(node).map(|found| &found.state) {
            Some(State::Audio(audio)) => audio.position,
            Some(State::Video(video)) => video.position,
            _ => return,
        };
        let moved = match playing {
            true => (now - held).abs() >= 5.0,
            false => (now - held).abs() > 0.05,
        };
        if moved {
            self.realm.realm.change(node, |state| match state {
                State::Audio(audio) => audio.position = now,
                State::Video(video) => video.position = now,
                _ => {}
            });
        }
    }

    /// Play, pause, seek or set the volume of a sound node. The transport bar and `realm play`, `pause`,
    /// `seek` and `volume` all come here, so a person and an agent reach the same player. §5.9.
    pub(crate) fn drive_a_sound(&mut self, node: u64, asked: Transport) -> Result<Playing, String> {
        let Some(found) = self.realm.realm.node(node).cloned() else {
            return Err(format!("There is no node {node}."));
        };
        let State::Audio(audio) = &found.state else {
            return match found.kind() {
                Kind::Video => self.drive_a_video(node, asked),
                _ => Err(format!("Node {node} is not a sound or a video.")),
            };
        };
        if let Some(why) = why_there_is_nothing(&found) {
            return Err(why);
        }
        let file = audio.file.clone().expect("why_there_is_nothing said there is a file");
        let player = self.realm.live.player(node, &file, audio.position)?;
        player.set_loop(audio.looping);
        player.set_volume(audio.volume);
        let mut volume = None;
        match asked {
            Transport::Play => player.play(),
            Transport::Pause => player.pause(),
            Transport::Toggle => match player.playing() {
                true => player.pause(),
                false => player.play(),
            },
            Transport::Seek(seconds) => {
                player.seek(std::time::Duration::from_secs_f32(seconds.max(0.0)))
            }
            Transport::Volume(level) => {
                let level = level.clamp(0.0, 1.0);
                player.set_volume(level);
                volume = Some(level);
            }
            Transport::Read => {}
        }
        let answer = Playing {
            playing: player.playing(),
            position: player.position().as_secs_f32(),
            duration: player.duration().as_secs_f32(),
            volume: volume.unwrap_or(audio.volume),
        };
        if let Some(level) = volume {
            self.realm.realm.change(node, |state| {
                if let State::Audio(audio) = state {
                    audio.volume = level;
                }
            });
        }
        // Where it is now goes to the sidecar at once when it stopped or was moved, rather than on the clock.
        if !matches!(asked, Transport::Read) {
            let now = answer.position;
            self.realm.realm.change(node, |state| {
                if let State::Audio(audio) = state {
                    audio.position = now;
                }
            });
        }
        Ok(answer)
    }

    /// A sound node: a transport bar drawn in `egui`, so every sound node on a realm is live at once and every
    /// control on it has a name. §5.3.
    pub(crate) fn show_an_audio_node(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node,
        body: Rect,
        focused: bool,
    ) {
        let _ = focused;
        if let Some(why) = why_there_is_nothing(node) {
            say_in_a_node(ui, body, node.id, &why);
            return;
        }
        let name = self.name_of_a_node(node);
        let state = match self.drive_a_sound(node.id, Transport::Read) {
            Ok(state) => state,
            Err(problem) => {
                say_in_a_node(ui, body, node.id, &problem);
                return;
            }
        };
        if state.playing {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
        let painter = ui.painter_at(body);
        let inner = body.shrink2(Vec2::new(12.0, 8.0));
        painter.crisp_text(
            Pos2::new(inner.left(), inner.top() + 7.0),
            Align2::LEFT_CENTER,
            &name,
            FontId::proportional(11.5),
            color::text_dim(),
        );
        // The play button and the time on one line, the volume at its right hand end.
        let line = Rect::from_min_size(
            Pos2::new(inner.left(), inner.bottom() - 26.0),
            Vec2::new(inner.width(), 26.0),
        );
        let button = Rect::from_min_size(line.min, Vec2::splat(26.0));
        let verb = if state.playing { "Pause" } else { "Play" };
        let pressed =
            ui.interact(button, egui::Id::new(("realm-audio-play", node.id)), Sense::click());
        painter.circle_filled(button.center(), 13.0, color::control());
        match state.playing {
            true => pause_mark(&painter, button.center(), color::text_strong()),
            false => crate::theme::icon::run(&painter, button.center(), color::text_strong()),
        }
        pressed.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{verb} {name}"))
        });
        let time = format!("{} / {}", clock(state.position), clock(state.duration));
        painter.crisp_text(
            Pos2::new(button.right() + 10.0, line.center().y),
            Align2::LEFT_CENTER,
            &time,
            FontId::monospace(11.5),
            color::text(),
        );
        let said = ui.interact(
            Rect::from_min_size(
                Pos2::new(button.right() + 6.0, line.top()),
                Vec2::new(90.0, line.height()),
            ),
            egui::Id::new(("realm-audio-time", node.id)),
            Sense::hover(),
        );
        said.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Other,
                true,
                format!("Time of {name}: {time}"),
            )
        });
        let volume_bar = Rect::from_min_max(
            Pos2::new(line.right() - 80.0, line.top() + 6.0),
            Pos2::new(line.right(), line.bottom() - 6.0),
        );
        let volume = slider(
            ui,
            volume_bar,
            state.volume,
            &format!("Volume of {name}"),
            ("realm-audio-volume", node.id),
        );
        // The seek bar across the whole node, between the name and the controls.
        let seek_bar = Rect::from_min_max(
            Pos2::new(inner.left(), line.top() - 18.0),
            Pos2::new(inner.right(), line.top() - 6.0),
        );
        let fraction = match state.duration > 0.0 {
            true => state.position / state.duration,
            false => 0.0,
        };
        let seek =
            slider(ui, seek_bar, fraction, &format!("Seek {name}"), ("realm-audio-seek", node.id));
        let asked = match (pressed.clicked(), seek, volume) {
            (true, _, _) => Some(Transport::Toggle),
            (_, Some(to), _) => Some(Transport::Seek(to * state.duration)),
            (_, _, Some(level)) => Some(Transport::Volume(level)),
            _ => None,
        };
        if let Some(asked) = asked {
            self.realm.realm.choose(Some(node.id));
            if let Err(problem) = self.drive_a_sound(node.id, asked) {
                self.message = Some(problem);
            }
        }
    }

    /// The page a video node's tab shows. `task-2202`.
    pub(crate) fn video_address(node: u64) -> String {
        format!("unluminous://realm/video/{node}")
    }

    /// Make sure a video node has its page registered and a tab on the one native view, and answer the tab.
    ///
    /// **Registered by node id**, so what the page can load is the one file this node names: see
    /// `services::browser::MediaStore`. A node pointed at another file, or given another volume, registers
    /// again; the tab is kept.
    fn a_video_tab(&mut self, node: u64, autoplay: bool) -> Result<u64, String> {
        if !crate::services::browser::SUPPORTED {
            return Err(
                "Videos play in the window's web view, which is on Windows and macOS.".to_owned()
            );
        }
        let Some(found) = self.realm.realm.node(node).cloned() else {
            return Err(format!("There is no node {node}."));
        };
        let State::Video(video) = &found.state else {
            return Err(format!("Node {node} is not a video."));
        };
        if let Some(why) = why_there_is_nothing(&found) {
            return Err(why);
        }
        let position =
            self.realm.live.video_report(node).map(|(at, ..)| at).unwrap_or(video.position);
        self.browser.media().register(
            node,
            crate::services::browser::MediaPage {
                file: video.file.clone().expect("why_there_is_nothing said there is a file"),
                volume: video.volume,
                looping: video.looping,
                muted: video.muted,
                position,
                autoplay,
            },
        );
        if let Some(tab) = self.realm.live.browser(node).map(|tab| tab.id) {
            return Ok(tab);
        }
        let tab = self.browser.open_tab(crate::services::browser::BrowserLocation::Remote {
            url: Self::video_address(node),
        });
        let id = tab.id;
        self.realm.live.put_a_browser(node, tab);
        Ok(id)
    }

    /// Play, pause, seek or set the volume of a video node.
    ///
    /// **A video that is showing is asked through the page**, with `evaluate_script`, so the browser's own
    /// controls and these commands move one player. One that is not showing is told what to do when it next
    /// loads, and a play makes it the chosen node so it does show: the window has one native web view, and
    /// the chosen video or browser node is the one that has it.
    pub(crate) fn drive_a_video(&mut self, node: u64, asked: Transport) -> Result<Playing, String> {
        let Some(State::Video(video)) =
            self.realm.realm.node(node).map(|found| found.state.clone())
        else {
            return Err(format!("Node {node} is not a video."));
        };
        let reported = self.realm.live.video_report(node);
        let (mut position, mut playing, duration) =
            reported.unwrap_or((video.position, false, 0.0));
        let mut volume = video.volume;
        let script = match asked {
            Transport::Play => Some("document.getElementById('v').play();".to_owned()),
            Transport::Pause => Some("document.getElementById('v').pause();".to_owned()),
            Transport::Toggle => Some(
                "const v=document.getElementById('v');if(v.paused){v.play();}else{v.pause();}"
                    .to_owned(),
            ),
            Transport::Seek(seconds) => {
                position = seconds.max(0.0);
                Some(format!("document.getElementById('v').currentTime={position};"))
            }
            Transport::Volume(level) => {
                volume = level.clamp(0.0, 1.0);
                Some(format!("document.getElementById('v').volume={volume};"))
            }
            Transport::Read => None,
        };
        if volume != video.volume || position != video.position {
            self.realm.realm.change(node, |state| {
                if let State::Video(held) = state {
                    held.volume = volume;
                    held.position = position;
                }
            });
        }
        let Some(script) = script else {
            return Ok(Playing { playing, position, duration, volume });
        };
        let starts =
            matches!(asked, Transport::Play) || (matches!(asked, Transport::Toggle) && !playing);
        let tab = self.a_video_tab(node, starts)?;
        if self.browser.evaluate(tab, &script).is_err() {
            // Not showing, so it is told what to do when it loads, and a play is what shows it.
            playing = starts;
            if starts {
                self.realm.realm.choose(Some(node));
                self.show_a_panel(crate::app::dock::Panel::Realm, true);
            }
        } else if matches!(asked, Transport::Play | Transport::Pause | Transport::Toggle) {
            playing = starts;
        }
        Ok(Playing { playing, position, duration, volume })
    }

    /// A video node: the window's native web view on its page while it is the chosen node, and a
    /// placeholder drawn in `egui` otherwise. §5.4: one native view a window, so only one video or page
    /// plays at a time, and the placeholders are what a screenshot test can see.
    pub(crate) fn show_a_video_node(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node,
        body: Rect,
        focused: bool,
    ) {
        if let Some(why) = why_there_is_nothing(node) {
            say_in_a_node(ui, body, node.id, &why);
            return;
        }
        let name = self.name_of_a_node(node);
        let chosen = self.realm.realm.chosen() == Some(node.id);
        // **A video that is playing keeps playing when it stops being the chosen node.** `task-2207`:
        // *"Video node playback stops when I click outside of the video node."* Its page was placed only
        // while the node was chosen, so a click anywhere else took the one native view away from it and the
        // video stopped with it. It is the page reporting that it plays, and that the view is still on it,
        // that keeps it; another page that is chosen still takes the view, because there is only one.
        let playing = self.realm.live.video_report(node.id).is_some_and(|(_, playing, _)| playing)
            && self
                .realm
                .live
                .browser(node.id)
                .is_some_and(|tab| self.browser.showing() == Some(tab.id));
        if (chosen || playing) && crate::services::browser::SUPPORTED {
            match self.a_video_tab(node.id, false) {
                Ok(tab) => {
                    let camera = self.realm.realm.camera;
                    let mut placement =
                        crate::services::browser::BrowserPlacement::whole(tab, body, focused);
                    let whole = camera.rect_to_screen(self.realm.body.min, body);
                    placement.area = whole;
                    placement.visible = whole.intersect(self.realm.body);
                    placement.zoom = Some(f64::from(camera.zoom));
                    placement.over = self.nodes_in_front_of(node.id);
                    placement.playing = playing;
                    if placement.visible.width() > 1.0 && placement.visible.height() > 1.0 {
                        self.browser_placements.push(placement);
                    }
                    // Underneath the page, in case the engine has not drawn its first frame yet.
                    ui.painter_at(body).rect_filled(body, 0.0, Color32::BLACK);
                    return;
                }
                Err(problem) => {
                    say_in_a_node(ui, body, node.id, &problem);
                    return;
                }
            }
        }
        // The placeholder: a film mark, the file's name, and what a press does.
        let painter = ui.painter_at(body);
        painter.rect_filled(body, 0.0, Color32::from_black_alpha(140));
        let middle = body.center();
        crate::theme::icon::video(
            &painter,
            Pos2::new(middle.x, middle.y - 20.0),
            color::text_dim(),
        );
        painter.crisp_text(
            Pos2::new(middle.x, middle.y + 4.0),
            Align2::CENTER_CENTER,
            &name,
            FontId::proportional(12.0),
            color::text(),
        );
        painter.crisp_text(
            Pos2::new(middle.x, middle.y + 22.0),
            Align2::CENTER_CENTER,
            "Click to play",
            FontId::proportional(11.0),
            color::text_dim(),
        );
        let response = ui.interact(body, egui::Id::new(("realm-video", node.id)), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Show video {name}"))
        });
        if response.clicked() {
            self.realm.realm.choose(Some(node.id));
            self.take_the_keyboard_for_the_realm();
        }
    }

    /// A note node: its Markdown file in the editing area's own editor, in whichever of the three views the
    /// note is set to. `task-2202`, §5.5 of the design.
    ///
    /// **The focus is borrowed** exactly as a File Editor node borrows it, so `files.active()` is the note's
    /// tab while it is drawn, and then [`UnluminousApp::show_a_document_in`] draws it the way a tab in a pane
    /// is drawn. The raw source, the side by side view and the preview are all the editing area's own.
    pub(crate) fn show_a_note_node(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node,
        body: Rect,
        focused: bool,
    ) {
        let State::Note(note) = &node.state else { return };
        if let Some(why) = why_there_is_nothing(node) {
            say_in_a_node(ui, body, node.id, &why);
            return;
        }
        let file = note.file.clone().expect("why_there_is_nothing said there is a file");
        let index = match self.files.tab_in_node(node.id) {
            Some(index) => index,
            None => {
                // A note made since the realm was brought to life, or one whose tab was closed: opened here,
                // without taking the keyboard, because drawing a node is not choosing it.
                let (focus, chosen) = (self.focus, self.realm.realm.chosen());
                let opened = self.open_in_a_realm_node(node.id, &file);
                self.focus = focus;
                self.realm.realm.choose(chosen);
                match (opened, self.files.tab_in_node(node.id)) {
                    (Ok(()), Some(index)) => index,
                    (Err(problem), _) => {
                        say_in_a_node(ui, body, node.id, &problem);
                        return;
                    }
                    (Ok(()), None) => return,
                }
            }
        };
        let was = self.files.focus();
        self.files.show(index);
        self.files.focus_node(node.id);
        self.size_a_nodes_tab(node, index);
        // **The note and its tab agree about how it is shown.** The header's three buttons change both; the
        // title bar's three, \`editor view\` and the View menu change the tab, which is the active one while
        // the note has the keyboard. So a tab whose view moved is the newer answer and goes to the note.
        let showing = crate::components::realm::view_mode_of(note.view);
        let tab_mode = self.files.at(index).view_mode;
        if tab_mode != showing {
            let view = crate::components::realm::note_view_of(tab_mode);
            self.realm.realm.change(node.id, |state| {
                if let State::Note(held) = state {
                    held.view = view;
                }
            });
        }
        let divider = format!("note {}", self.name_of_a_node(node));
        let took = self.show_a_document_in(ui, body, focused, &divider);
        if took {
            self.realm.realm.choose(Some(node.id));
            self.take_the_keyboard_for_the_realm();
        }
        if !focused {
            self.files.restore_focus(was);
        }
    }

    /// Give a node's tab the node's own font size, once a change rather than once a frame.
    ///
    /// The File Editor node's rule, for a note: the editor's font is one setting for the whole window, so a
    /// node that walked it would resize every other tab. `set_base_style` walks every byte of the document,
    /// so it is applied when the size changes and remembered on the tab as `OpenFile::sized_at`.
    pub(crate) fn size_a_nodes_tab(&mut self, node: &Node, index: usize) {
        let wanted = crate::components::realm::editor_font_size_of(node, self.settings.font_size);
        let asked = match (wanted - self.settings.font_size).abs() > 0.01 {
            true => Some(wanted),
            false => None,
        };
        if self.files.at(index).sized_at != asked {
            let change = unluminous_core::StyleChange {
                size: Some(wanted),
                ..self.settings.as_style_change()
            };
            self.files.at_mut(index).document.set_base_style(change);
            self.files.at_mut(index).cached.stale = true;
            self.files.at_mut(index).sized_at = asked;
        }
    }

    /// Open every note on the realm that is showing into its node, at the caret and the scroll it was left
    /// at. Part of bringing a realm to life, beside the File Editor nodes.
    pub(crate) fn open_the_canvass_notes(&mut self) {
        let waiting: Vec<(u64, std::path::PathBuf, usize, f32)> = self
            .realm
            .realm
            .nodes
            .iter()
            .filter_map(|node| match &node.state {
                State::Note(note) if node.refused.is_none() => {
                    Some((node.id, note.file.clone()?, note.caret, note.scroll))
                }
                _ => None,
            })
            .filter(|(node, file, ..)| self.files.tabs_in_node(*node).is_empty() && file.is_file())
            .collect();
        for (node, file, caret, scroll) in waiting {
            // A file already on another node is left there, which is the File Editor node's rule.
            if let Some(open) = self.files.index_of(&file) {
                if self.files.at(open).home.node().is_some_and(|held| held != node) {
                    continue;
                }
            }
            if self.open_in_a_realm_node(node, &file).is_err() {
                continue;
            }
            if let Some(index) = self.files.tab_in_node(node) {
                let end = self.files.at(index).document.text().len_bytes();
                self.files.at_mut(index).document.apply(unluminous_core::Command::PlaceCaret {
                    offset: caret.min(end),
                    extend: false,
                });
                self.files.at_mut(index).scroll = scroll.max(0.0);
            }
        }
    }

    /// Show a note as its source, beside its preview, or as its preview. The header's buttons and
    /// `realm note view` both come here, so the two cannot disagree.
    pub(crate) fn set_a_notes_view(&mut self, node: u64, view: NoteView) -> Result<(), String> {
        match self.realm.realm.node(node).map(|found| found.kind()) {
            Some(Kind::Note) => {}
            Some(_) => return Err(format!("Node {node} is not a note.")),
            None => return Err(format!("There is no node {node}.")),
        }
        self.realm.realm.change(node, |state| {
            if let State::Note(note) = state {
                note.view = view;
            }
        });
        if let Some(index) = self.files.tab_in_node(node) {
            self.files.at_mut(index).view_mode = crate::components::realm::view_mode_of(view);
            self.files.at_mut(index).cached.stale = true;
        }
        Ok(())
    }

    /// Put a node on the realm that shows `file`, a file in this project: a picture, a sound, a video or a
    /// note, by `kind`.
    ///
    /// **A file from outside the project is copied into the realm's own folder first**,
    /// `.realm-files/<realm>/`, and the node names the copy. A realm is shared with the project, and a node
    /// that named a file elsewhere would show something different on every machine, which is why a realm
    /// file naming one is refused when it is read (§5.1). Refusing the file chooser's answer instead meant a
    /// picture picked from Downloads added nothing, with one line in the status bar to say why.
    pub(crate) fn add_a_file_node(
        &mut self,
        kind: Kind,
        file: &Path,
        at: Pos2,
    ) -> Result<u64, String> {
        if let Some(why) = self.realm.realm.read_only_because() {
            return Err(why.to_owned());
        }
        let root = self.tree.root().to_path_buf();
        let absolute = match file.is_absolute() {
            true => file.to_path_buf(),
            false => root.join(file),
        };
        let absolute = match absolute.is_file() && !is_inside(&root, &absolute) {
            true => self.copy_into_the_realm(&root, &absolute)?,
            false => absolute,
        };
        let relative = relative_to(&root, &absolute);
        let checked = crate::services::realm::store::inside(
            &root,
            &crate::services::realm::slashed(&relative),
        )
        .map_err(|why| format!("{} cannot be put on a realm: {why}.", file.display()))?;
        if !checked.is_file() {
            return Err(format!("{} is not a file in this project.", file.display()));
        }
        let id = self.realm.realm.add_node(kind, at, Some(&root));
        self.realm.realm.change(id, |state| state.set_file(checked.clone()));
        if kind == Kind::Note {
            self.open_in_a_realm_node(id, &checked)?;
        }
        self.take_the_keyboard_for_the_realm();
        Ok(id)
    }

    /// Copy a file from outside the project into `.realm-files/<realm>/`, and answer where the copy is.
    ///
    /// A file of that name already there with the same bytes is used as it is, so adding the same picture
    /// twice makes one copy. One with different bytes gets a number after its name rather than being
    /// written over, because a file Unluminous did not write is never overwritten.
    fn copy_into_the_realm(&self, root: &Path, from: &Path) -> Result<PathBuf, String> {
        let folder =
            root.join(crate::services::realm::store::FOLDER).join(self.realm.realm.title());
        std::fs::create_dir_all(&folder)
            .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
        let bytes = std::fs::read(from)
            .map_err(|problem| format!("{} could not be read: {problem}", from.display()))?;
        let name =
            from.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let stem = Path::new(&name)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let extension = Path::new(&name)
            .extension()
            .map(|extension| format!(".{}", extension.to_string_lossy()));
        for count in 1..1000 {
            let candidate = match count {
                1 => folder.join(&name),
                more => {
                    folder.join(format!("{stem} {more}{}", extension.clone().unwrap_or_default()))
                }
            };
            match std::fs::read(&candidate) {
                Ok(there) if there == bytes => return Ok(candidate),
                Ok(_) => continue,
                Err(_) => {
                    crate::services::store::write_atomically(&candidate, &bytes).map_err(
                        |problem| {
                            format!("{} could not be written: {problem}", candidate.display())
                        },
                    )?;
                    return Ok(candidate);
                }
            }
        }
        Err(format!("There are too many files called {name} in {} already.", folder.display()))
    }

    /// Make a note called `name` in `.realm-files/<realm>/` and put a node on it. A file by that name already
    /// there is used rather than written over. §5.5.
    pub(crate) fn new_note(&mut self, name: &str, at: Pos2) -> Result<u64, String> {
        let name = name.trim();
        if name.is_empty() || name.contains(['/', '\\', ':']) {
            return Err(format!("{name:?} cannot be a note's name."));
        }
        let file_name = match name.to_lowercase().ends_with(".md") {
            true => name.to_owned(),
            false => format!("{name}.md"),
        };
        let folder = self
            .tree
            .root()
            .join(crate::services::realm::store::FOLDER)
            .join(self.realm.realm.title());
        let file = folder.join(file_name);
        if !file.exists() {
            std::fs::create_dir_all(&folder)
                .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
            crate::services::store::write_a_source_file(&file, b"")
                .map_err(|problem| format!("{} could not be written: {problem}", file.display()))?;
            self.the_project_changed_on_disk();
        }
        self.add_a_file_node(Kind::Note, &file, at)
    }

    /// Rename the file a note node shows, through `move_path` so a link to it elsewhere in the project follows,
    /// and the node with it. §5.5: the node's header shows the file's name, so renaming the node is renaming
    /// the file.
    pub(crate) fn rename_a_note(
        &mut self,
        node: u64,
        name: &str,
    ) -> Result<std::path::PathBuf, String> {
        let Some(State::Note(note)) = self.realm.realm.node(node).map(|found| found.state.clone())
        else {
            return Err(format!("Node {node} is not a note."));
        };
        let Some(from) = note.file else { return Err("This note names no file.".to_owned()) };
        let name = name.trim();
        if name.is_empty() || name.contains(['/', '\\', ':']) {
            return Err(format!("{name:?} cannot be a note's name."));
        }
        let file_name = match name.to_lowercase().ends_with(".md") {
            true => name.to_owned(),
            false => format!("{name}.md"),
        };
        let to =
            from.parent().map(|folder| folder.join(&file_name)).unwrap_or_else(|| file_name.into());
        if to == from {
            return Ok(to);
        }
        if !self.move_path(&from, &to, true) {
            return Err(self
                .message
                .clone()
                .unwrap_or_else(|| "The note could not be renamed.".to_owned()));
        }
        Ok(to)
    }

    /// A note name no file in this realm's folder has: `Note`, `Note 2` and so on.
    pub(crate) fn an_unused_note_name(&self) -> String {
        let folder = self
            .tree
            .root()
            .join(crate::services::realm::store::FOLDER)
            .join(self.realm.realm.title());
        (1..)
            .map(|number| match number {
                1 => "Note".to_owned(),
                more => format!("Note {more}"),
            })
            .find(|name| !folder.join(format!("{name}.md")).exists())
            .expect("one of the names is free")
    }

    /// Ask for a note's name, which is what `Add Note` does. The answer comes back through the prompt as
    /// [`Self::new_note`].
    pub(crate) fn ask_for_a_note(&mut self, at: Pos2) {
        self.show_a_panel(crate::app::dock::Panel::Realm, true);
        self.prompt = Some(crate::components::prompt_dialog::Prompt::new(
            "New Note",
            "What to call it. It is written as a Markdown file in this realm's own folder under .realm-files.",
            "",
            "Create",
            crate::components::prompt_dialog::Purpose::NewNote(at.x.round() as i32, at.y.round() as i32),
        ));
    }

    /// Ask which file a picture, sound or video node shows, through the platform's own file chooser, which is
    /// what `Add Image`, `Add Audio` and `Add Video` do. `realm add <kind> <path>` is the command that takes the
    /// path instead.
    pub(crate) fn ask_for_a_file_node(&mut self, kind: Kind, at: Pos2) {
        let (label, extensions): (&str, &[&str]) = match kind {
            Kind::Image => ("Pictures", crate::services::file_kind::IMAGE_EXTENSIONS),
            Kind::Audio => ("Sounds", AUDIO_EXTENSIONS),
            Kind::Video => ("Videos", VIDEO_EXTENSIONS),
            _ => return,
        };
        let Some(chosen) = rfd::FileDialog::new()
            .set_title(format!("Choose the file this {} node shows", kind.label().to_lowercase()))
            .set_directory(self.tree.root())
            .add_filter(label, extensions)
            .pick_file()
        else {
            return;
        };
        if let Err(problem) = self.add_a_file_node(kind, &chosen, at) {
            self.message = Some(problem);
        }
    }
}

/// Whether `path` is inside the project at `root`, asked of the folders as they really are on the disk so a
/// link or a differently spelled path to the same place counts as inside.
/// `path` relative to the project, worked out on the folders as the disk spells them when the two paths as
/// written do not share a beginning, which is a project opened through a link.
fn relative_to(root: &Path, path: &Path) -> PathBuf {
    if let Ok(inside) = path.strip_prefix(root) {
        return inside.to_path_buf();
    }
    match (std::fs::canonicalize(root), std::fs::canonicalize(path)) {
        (Ok(root), Ok(path)) => path.strip_prefix(&root).map(Path::to_path_buf).unwrap_or(path),
        _ => path.to_path_buf(),
    }
}

fn is_inside(root: &Path, path: &Path) -> bool {
    match (std::fs::canonicalize(root), std::fs::canonicalize(path)) {
        (Ok(root), Ok(path)) => path.starts_with(root),
        _ => path.starts_with(root),
    }
}

/// The sounds an audio node plays, by extension. Every one of them is decoded by `rodio` with no system
/// library behind it.
pub(crate) const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "ogg", "oga", "m4a", "aac"];

/// The videos a video node plays, by extension: the ones WebView2 and WKWebView both play.
pub(crate) const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "webm"];

/// The kind of node a file from the project makes, by its extension: a picture, a sound or a video.
/// `None` for anything else, which a File Editor node opens. A Markdown file dropped on the canvas is a
/// File Editor node, as it always was; a note is made with `realm add note`.
pub(crate) fn kind_for_a_file(path: &Path) -> Option<Kind> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    if crate::services::file_kind::IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return Some(Kind::Image);
    }
    if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        return Some(Kind::Audio);
    }
    if VIDEO_EXTENSIONS.contains(&extension.as_str()) {
        return Some(Kind::Video);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_contained_covered_or_drawn_at_its_own_size() {
        let body = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(200.0, 100.0));
        let square = Vec2::new(64.0, 64.0);
        let contained = where_a_picture_goes(body, square, Fit::Contain, 1.0, Vec2::ZERO);
        assert_eq!(contained.size(), Vec2::new(100.0, 100.0), "as large as fits");
        assert_eq!(contained.center(), body.center());
        let covered = where_a_picture_goes(body, square, Fit::Cover, 1.0, Vec2::ZERO);
        assert_eq!(covered.size(), Vec2::new(200.0, 200.0), "the node filled");
        let actual = where_a_picture_goes(body, square, Fit::Actual, 2.0, Vec2::new(10.0, 0.0));
        assert_eq!(actual, Rect::from_min_size(Pos2::new(-10.0, 0.0), Vec2::new(128.0, 128.0)));
    }
}

/// What a sound or a video node is asked to do. `task-2202`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Transport {
    Play,
    Pause,
    Toggle,
    /// To this many seconds in.
    Seek(f32),
    /// From 0 to 1.
    Volume(f32),
    /// Nothing; answer where it is.
    Read,
}

/// Where a sound or a video is, which is what the transport bar draws and `realm play` answers with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Playing {
    pub playing: bool,
    /// Seconds in.
    pub position: f32,
    /// Seconds long.
    pub duration: f32,
    pub volume: f32,
}

/// Seconds as a clock reads them: `1:05`, or `1:02:05` past an hour.
pub(crate) fn clock(seconds: f32) -> String {
    let whole = seconds.max(0.0).floor() as u64;
    match whole >= 3600 {
        true => format!("{}:{:02}:{:02}", whole / 3600, (whole / 60) % 60, whole % 60),
        false => format!("{}:{:02}", whole / 60, whole % 60),
    }
}

/// Two bars, which is what a pause button is.
fn pause_mark(painter: &egui::Painter, centre: Pos2, colour: Color32) {
    for offset in [-2.6, 2.6] {
        painter.rect_filled(
            Rect::from_center_size(Pos2::new(centre.x + offset, centre.y), Vec2::new(2.6, 10.0)),
            1.0,
            colour,
        );
    }
}

/// A bar that sets a value from 0 to 1 by pressing or dragging along it, with an accessible name. Answers the
/// new value while it is being moved.
///
/// Drawn rather than `egui::Slider`, whose label is drawn beside it and whose look is egui's own rather than
/// the window's.
fn slider(
    ui: &mut egui::Ui,
    bar: Rect,
    value: f32,
    name: &str,
    salt: impl std::hash::Hash + std::fmt::Debug,
) -> Option<f32> {
    let response = ui.interact(bar.expand(4.0), egui::Id::new(salt), Sense::click_and_drag());
    let painter = ui.painter_at(bar.expand(6.0));
    let track = Rect::from_center_size(bar.center(), Vec2::new(bar.width(), 4.0));
    painter.rect_filled(track, 2.0, color::control());
    let value = value.clamp(0.0, 1.0);
    let filled =
        Rect::from_min_max(track.min, Pos2::new(track.left() + track.width() * value, track.max.y));
    painter.rect_filled(filled, 2.0, color::accent());
    painter.circle_filled(Pos2::new(filled.right(), track.center().y), 5.0, color::text_strong());
    response.widget_info(|| egui::WidgetInfo::slider(true, f64::from(value), name));
    if response.clicked() || response.dragged() {
        let at = response.interact_pointer_pos()?;
        return Some(((at.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0));
    }
    None
}
