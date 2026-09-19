//! `space` — the agent's half of the Base of Infinite Space.
//!
//! **Moved here from `app/space.rs` by `task-1984` §3.6**, which found that file at 4,734 lines and
//! the largest in the crate. Every other area of the catalogue keeps its handlers in this folder;
//! this one did not, and the canvas is the area with the most verbs in it. Nothing about what a
//! command does changed in the move; only where its code lives did, which is the sentence
//! `services::agent_tasks::commands` already carries about its own split.

use super::*;

use egui::{Pos2, Vec2};

use crate::app::actions::SpaceAction;
use crate::app::space::{drives, node_zoom_of, set_node_zoom};
use crate::components::space::{self as space_view};
use crate::services::space::{Kind, NodeId, Pipe, State};

/// The `space` area of `unluminous-cli`, which is the agent's half of the canvas.
///
/// **Every one of these goes through the same functions the pointer does.** `space add` is what the
/// right click modal calls, `space connect` is what letting a wire go calls, and `space remove` is
/// what the close cross calls - which is `UnluminousApp::run_cli`'s own rule, that a thing done by
/// hand and the same thing done by an agent are the same thing rather than two paths that agree
/// today.
///
/// **`--from` is what a connection is for.** A command carrying it is acting *as* that node and is
/// refused when there is no wire from it to the node it names; a command with no `--from` is the
/// window's own agent, which may drive everything. That is the ticket's *"the main agent for the IDE
/// can control every single node"* beside its *"terminal node agents control the things they are
/// connected to"*.
impl UnluminousApp {
    pub(crate) fn cli_space(
        &mut self,
        request: &Request,
        verb: &str,
        ctx: &egui::Context,
    ) -> Outcome {
        match verb {
            "show" => self.cli_space_show(request),
            "hide" => self.cli_space_hide(request),
            "here" => self.cli_space_here(request),
            "view" => ok(request, "The canvas.", self.space.space.as_json()),
            "list" => self.cli_space_list(request),
            "manage" => self.cli_space_manage(request),
            "views" => self.cli_space_views(request),
            "open-view" => match self.a_named_view(request) {
                Ok(id) => {
                    self.space.space.show_view(id);
                    self.bring_the_current_view_to_life();
                    done(request, format!("Showing {}.", self.space.space.current().name))
                }
                Err(outcome) => *outcome,
            },
            "new-view" => self.cli_space_new_view(request),
            "rename-view" => self.cli_space_rename_view(request),
            "duplicate-view" => self.cli_space_duplicate_view(request),
            "delete-view" => self.cli_space_delete_view(request),
            "add" => self.cli_space_add(request),
            "move" => self.cli_space_move(request),
            "size" => self.cli_space_size(request),
            "title" => self.cli_space_title(request),
            "remove" => self.cli_space_remove(request),
            "focus" => self.cli_space_focus(request),
            "connect" => self.cli_space_connect(request),
            "disconnect" => self.cli_space_disconnect(request),
            "connections" => self.cli_space_connections(request),
            "camera" => self.cli_space_camera(request),
            "send" => self.cli_space_send(request),
            "chat" => self.cli_space_chat(request),
            "read" => self.cli_space_read(request),
            "restart" => self.cli_space_restart(request),
            "font" => self.cli_space_font(request),
            "zoom" => self.cli_space_zoom(request),
            "address" => self.cli_space_address(request),
            "browser" => self.cli_space_browser(request, ctx),
            "folder" => self.cli_space_folder(request),
            "editor" => self.cli_space_editor(request),
            _ => unknown(request),
        }
    }

    /// Which node this command came from, and what it may act on.
    ///
    /// **The one command whose answer depends on which process is asking**, which is why it is a command
    /// rather than a paragraph: `UNLUMINOUS_SPACE_NODE` is in the *client's* environment, so the client
    /// reads it and sends it, and the window answers about the node it names.
    ///
    /// `task-1905` is the report — an agent in a terminal node spent nine tool calls and two shell
    /// commands working out what one call could have told it, and then hedged its answer because it had
    /// never established what it was allowed to do. What makes this worth having is not the eight calls
    /// saved: it is that **the answer names the command for each thing it lists**, which `task-1695`
    /// measured as what decides whether a command is used at all rather than `bash`.
    ///
    /// **Outside a node it is an answer rather than a refusal.** The window's own agent runs it too, and a
    /// refusal there would be a refusal about nothing — which is `picture::from_the_clipboard`'s rule.
    fn cli_space_here(&self, request: &Request) -> Outcome {
        let view = self.space.space.current();
        // The client puts what its own environment said here; the window has no way to know.
        let asking = request.number("node").map(|id| id as u64);
        let Some(asking) = asking else {
            return lines(
                request,
                "Not running inside a node.",
                vec![
                    "This command is not running inside a node on the canvas, so it is the window's own"
                        .to_owned(),
                    "agent: every node is reachable and no --from is needed. `space list` is the nodes."
                        .to_owned(),
                ],
                json!({ "node": Value::Null, "inANode": false, "reaches": [], "reachedBy": [] }),
            );
        };
        let Some(found) = view.node(asking) else {
            return no(
                request,
                code::NOT_FOUND,
                format!(
                    "UNLUMINOUS_SPACE_NODE says node {asking}, and there is no such node on {}. The canvas may have moved to another view.",
                    view.name
                ),
            );
        };
        let describe = |id: NodeId| -> Option<Value> {
            let other = view.node(id)?;
            Some(json!({
                "node": id,
                "kind": other.kind().name(),
                "title": self.name_of_a_node(other),
                "command": drives(other.kind(), id, asking),
            }))
        };
        let reaches: Vec<Value> = view.reaches(asking).into_iter().filter_map(describe).collect();
        let reached_by: Vec<Value> =
            view.reached_by(asking).into_iter().filter_map(describe).collect();
        let mut rows = vec![format!(
            "node {asking}  {}  \"{}\"  on view {}",
            found.kind().name(),
            self.name_of_a_node(found),
            view.name,
        )];
        rows.push("wired to:".to_owned());
        match reaches.is_empty() {
            true => rows.push("  nothing".to_owned()),
            false => {
                for one in &reaches {
                    rows.push(format!(
                        "  {:<4} {:<9} {:<22} {}",
                        one["node"],
                        one["kind"].as_str().unwrap_or_default(),
                        one["title"].as_str().unwrap_or_default(),
                        one["command"].as_str().unwrap_or_default(),
                    ));
                }
            }
        }
        rows.push("wired from:".to_owned());
        match reached_by.is_empty() {
            true => rows.push("  nothing".to_owned()),
            false => {
                for one in &reached_by {
                    rows.push(format!(
                        "  {:<4} {:<9} {}",
                        one["node"],
                        one["kind"].as_str().unwrap_or_default(),
                        one["title"].as_str().unwrap_or_default(),
                    ));
                }
            }
        }
        lines(
            request,
            format!("node {asking} on {}", view.name),
            rows,
            json!({
                "node": asking,
                "inANode": true,
                "kind": found.kind().name(),
                "title": self.name_of_a_node(found),
                "view": view.name,
                "reaches": reaches,
                "reachedBy": reached_by,
            }),
        )
    }

    /// The nodes on the view that is showing, one a line.
    fn cli_space_list(&self, request: &Request) -> Outcome {
        let view = self.space.space.current();
        let rows: Vec<String> = view
            .nodes
            .iter()
            .map(|node| {
                let wired = view.reaches(node.id);
                format!(
                    "{}{:<4} {:<9} {:>6},{:<6} {:>4} x {:<4}  {}{}",
                    if view.chosen == Some(node.id) { "*" } else { " " },
                    node.id,
                    node.kind().name(),
                    node.at.x.round(),
                    node.at.y.round(),
                    node.size.x.round(),
                    node.size.y.round(),
                    self.name_of_a_node(node),
                    match wired.is_empty() {
                        true => String::new(),
                        false => format!(
                            "  -> {}",
                            wired.iter().map(u64::to_string).collect::<Vec<_>>().join(", ")
                        ),
                    },
                )
            })
            .collect();
        let message = format!(
            "{} node{} on {}",
            rows.len(),
            if rows.len() == 1 { "" } else { "s" },
            view.name
        );
        lines(request, message, rows, self.space.space.as_json())
    }

    /// Every view the canvas has.
    fn cli_space_views(&self, request: &Request) -> Outcome {
        let current = self.space.space.current_id();
        let rows: Vec<String> = self
            .space
            .space
            .views()
            .iter()
            .map(|view| {
                format!(
                    "{}{:<4} {:<24} {} node{}",
                    if view.id == current { "*" } else { " " },
                    view.id,
                    view.name,
                    view.nodes.len(),
                    if view.nodes.len() == 1 { "" } else { "s" },
                )
            })
            .collect();
        let views: Vec<Value> = self
            .space
            .space
            .views()
            .iter()
            .map(|view| {
                json!({
                    "view": view.id,
                    "name": view.name,
                    "nodes": view.nodes.len(),
                    "connections": view.edges.len(),
                    "showing": view.id == current,
                })
            })
            .collect();
        lines(request, format!("{} views", views.len()), rows, json!({ "views": views }))
    }

    /// Put a node on the canvas, and give it whatever its kind was told to hold.
    fn cli_space_add(&mut self, request: &Request) -> Outcome {
        let Some(name) = request.text("kind") else {
            return no(request, code::USAGE, "Say which kind of node.");
        };
        let Some(kind) = Kind::from_name(name.trim()) else {
            let names: Vec<&str> = Kind::ALL.into_iter().map(Kind::name).collect();
            return no(
                request,
                code::USAGE,
                format!("There is no node called {name}. Unluminous has {}.", names.join(", ")),
            );
        };
        self.show_a_panel(dock::Panel::Space, true);
        let middle = self.middle_of_the_canvas();
        let at = Pos2::new(
            request.number("x").map(|x| x as f32).unwrap_or(middle.x),
            request.number("y").map(|y| y as f32).unwrap_or(middle.y),
        );
        let node = self.add_a_space_node(kind, at);
        if let (Some(width), Some(height)) = (request.number("width"), request.number("height")) {
            self.space.space.resize_node(node, Vec2::new(width as f32, height as f32));
        }
        if let Some(title) = request.text("title") {
            self.space.space.title_node(node, title.trim());
        }
        // Whatever the kind was told to hold, applied through the same functions the window uses.
        let mut problem: Option<String> = None;
        if let Some(command) = request.text("command") {
            self.space.space.change(node, |state| {
                if let State::Terminal(terminal) = state {
                    terminal.command = command.trim().to_owned();
                }
            });
            if let Err(refusal) = self.start_a_space_terminal(node, false) {
                problem = Some(refusal);
            }
        }
        if let Some(url) = request.text("url") {
            if let Err(refusal) = self.open_a_space_browser(node, url.trim()) {
                problem = Some(refusal);
            }
        }
        if let Some(root) = self.cli_path_argument(request, "root") {
            self.space.space.change(node, |state| {
                if let State::Folder(folder) = state {
                    folder.root = Some(root.clone());
                }
            });
            self.space.live.forget(node);
        }
        if let Some(path) = self.cli_path_argument(request, "path") {
            if let Err(refusal) = self.open_in_a_space_node(node, &path) {
                problem = Some(refusal);
            }
        }
        // **The node is still there when part of what it was given failed**, and the reply says so:
        // a refusal that also took the node away would leave a caller with nothing to correct.
        let made = self.space.space.current().node(node).cloned();
        let where_it_is = made.as_ref().map(|node| node.at).unwrap_or(at);
        let message = match &problem {
            Some(refusal) => format!("Added node {node}, but {refusal}"),
            None => format!("Added {} node {node}.", kind.name()),
        };
        ok(
            request,
            message,
            json!({
                "node": node,
                "kind": kind.name(),
                "x": where_it_is.x,
                "y": where_it_is.y,
                "problem": problem,
            }),
        )
    }

    fn cli_space_move(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        let at = Pos2::new(
            request.number("x").map(|x| x as f32).unwrap_or(found.at.x),
            request.number("y").map(|y| y as f32).unwrap_or(found.at.y),
        );
        self.space.space.move_node(node, at);
        ok(request, format!("Moved node {node}."), json!({ "node": node, "x": at.x, "y": at.y }))
    }

    fn cli_space_size(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        let size = Vec2::new(
            request.number("width").map(|width| width as f32).unwrap_or(found.size.x),
            request.number("height").map(|height| height as f32).unwrap_or(found.size.y),
        );
        self.space.space.resize_node(node, size);
        // What it really came out, because a kind has a smallest size and a caller that asked for
        // less deserves to be told what it got rather than that it worked.
        let now = self.space.space.current().node(node).map(|node| node.size).unwrap_or(size);
        ok(
            request,
            format!("Node {node} is {} x {}.", now.x.round(), now.y.round()),
            json!({ "node": node, "width": now.x, "height": now.y }),
        )
    }

    fn cli_space_connect(&mut self, request: &Request) -> Outcome {
        let from = match self.a_named_node(request, "from") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let to = match self.a_named_node(request, "to") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let pipe = match request.text("pipe").as_deref().map(str::trim) {
            None => Pipe::Off,
            Some(name) => match Pipe::from_name(name) {
                Some(pipe) => pipe,
                None => {
                    return no(
                        request,
                        code::USAGE,
                        format!("A connection carries `lines` or `off`, not {name}."),
                    )
                }
            },
        };
        match self.space.space.connect(from, to, pipe) {
            Ok(edge) => {
                if pipe == Pipe::Lines {
                    self.space.live.follow_from_here(from);
                }
                ok(
                    request,
                    format!("Connected {from} to {to}."),
                    json!({ "connection": edge, "from": from, "to": to, "pipe": pipe.name() }),
                )
            }
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    fn cli_space_connections(&self, request: &Request) -> Outcome {
        let only = request.number("from").map(|id| id as u64);
        let view = self.space.space.current();
        let found: Vec<&crate::services::space::Edge> =
            view.edges.iter().filter(|edge| only.is_none_or(|from| edge.from == from)).collect();
        let rows: Vec<String> = found
            .iter()
            .map(|edge| {
                format!("{:<4} {:>4} -> {:<4}  {}", edge.id, edge.from, edge.to, edge.pipe.name())
            })
            .collect();
        let values: Vec<Value> = found
            .iter()
            .map(|edge| {
                json!({
                    "connection": edge.id,
                    "from": edge.from,
                    "to": edge.to,
                    "pipe": edge.pipe.name(),
                })
            })
            .collect();
        lines(
            request,
            format!("{} connection{}", values.len(), if values.len() == 1 { "" } else { "s" }),
            rows,
            json!({ "connections": values }),
        )
    }

    fn cli_space_camera(&mut self, request: &Request) -> Outcome {
        let body = self.space.body;
        if request.switch("fit") {
            let bounds = self.space.space.current().bounds();
            self.space.space.current_mut().camera.fit(bounds, body.size(), 32.0);
        }
        let camera = self.space.space.current().camera;
        let at = Pos2::new(
            request.number("x").map(|x| x as f32).unwrap_or(camera.at.x),
            request.number("y").map(|y| y as f32).unwrap_or(camera.at.y),
        );
        let zoom = request.number("zoom").map(|zoom| zoom as f32).unwrap_or(camera.zoom);
        {
            let camera = &mut self.space.space.current_mut().camera;
            camera.at = at;
            // Through `zoom_to` rather than by assignment, so the ladder's ends are kept in one place
            // and a caller that asked for ten gets 2.5 rather than a canvas nobody can read.
            camera.zoom_to(zoom, body.min, body.min);
            camera.at = at;
        }
        // **A command sets the camera outright**, glide and all: `space camera --zoom 2` answers 2.00 on
        // the frame it lands, because a script that had to wait out an animation to read back what it just
        // set is a script with a race in it. The glide is the pointer's, the wheel's and the keys'.
        // `task-1945`.
        self.space.glide = None;
        self.space.space.touch();
        let camera = self.space.space.current().camera;
        ok(
            request,
            format!(
                "The canvas is at {:.0},{:.0} at {:.2}x.",
                camera.at.x, camera.at.y, camera.zoom
            ),
            json!({ "x": camera.at.x, "y": camera.at.y, "zoom": camera.zoom }),
        )
    }

    fn cli_space_send(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Terminal) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(text) = request.text("text") else {
            return no(request, code::USAGE, "Say what to type.");
        };
        if !self.space.live.has_a_terminal(node) {
            return no(request, code::REFUSED, format!("Node {node} has no terminal running."));
        }
        // Remembered as something typed in, so its echo is not piped back out - the rule in
        // `services::space::pipe`, which a send by hand needs exactly as much as a pipe does.
        self.space.live.typed_into(node, text.trim_end());
        if let Some(session) = self.space.live.terminal(node) {
            session.send(format!("{}\r", text.trim_end()).into_bytes());
        }
        done(request, format!("Typed into node {node}."))
    }

    /// What a terminal node is showing, scrollback and all.
    ///
    /// **The one thing an agent could not ask about.** `space view` answers with a node's command, folder,
    /// size and session id, and `terminal read` reads the terminal *panel* — so until `task-1912` there was no
    /// way at all to read a terminal node, which is the rule this repository opens with turned on its head. It
    /// is also why every measurement in that ticket had to be a photograph of a window.
    ///
    /// **The scrollback as well as the screen**, which is `run output`'s own distinction and is what a
    /// restored node needs: the commands somebody wants to see again are very often above the fold.
    /// Drive one chat node's own conversation, through the same function the pane's commands go through.
    ///
    /// **The verbs are not written out here.** `AgentChat::command` is `UiProvider::command`, which is what
    /// `plugins run agent-chat` already calls and what `CLAUDE.md` calls the one path a change goes down —
    /// so a verb added to the chat is a verb a node has the day it is added, and the two cannot answer
    /// differently. What this command adds is *whose* conversation: the pane has one and each node has one
    /// of its own.
    ///
    /// A node that has never been drawn has no chat behind it yet, because a chat is opened lazily the
    /// first time its node is drawn — see `make_sure_a_node_has_a_chat`. That is a refusal naming the
    /// reason rather than a chat built here, so a canvas nobody has looked at still costs nothing.
    fn cli_space_chat(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Chat) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(verb) = request.text("verb") else {
            return no(request, code::USAGE, "Say what to do: state, send, new, stop, last, view.");
        };
        let verb = verb.trim().to_owned();
        let words: Vec<String> = match request.text("words") {
            Some(words) if !words.trim().is_empty() => vec![words.trim().to_owned()],
            _ => Vec::new(),
        };
        let Some(chat) = self.space.live.chat_mut(node) else {
            return no(
                request,
                code::REFUSED,
                format!(
                    "Node {node} has not been drawn yet, so it has no conversation. Show the canvas and scroll to it - `space focus {node}` does both."
                ),
            );
        };
        match crate::services::plugin_ui::UiProvider::command(chat, &verb, &words) {
            Ok(answer) => {
                // The conversation a `new` or an `open` moved to is written down at once, for the reason
                // `make_sure_a_node_has_a_chat` writes the first one down: the reading pass that would
                // otherwise catch it only runs for a project the window remembers.
                self.note_which_conversation_a_node_is_on(node);
                let mut value = answer.value;
                if let Some(map) = value.as_object_mut() {
                    map.insert("node".into(), json!(node));
                }
                ok(request, answer.message, value)
            }
            Err(problem) => no(request, code::FAILED, problem),
        }
    }

    fn cli_space_read(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Terminal) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        // Take in whatever the program has written since the last frame, so a read straight after a send is
        // not looking at the screen as it was before the command ran. `cli_terminal_read`'s own rule.
        self.space.live.catch_up();
        let lines = request.whole("tail");
        let Some(session) = self.space.live.terminal(node) else {
            return no(request, code::REFUSED, format!("Node {node} has no terminal running."));
        };
        let text = session.written_text(lines);
        ok(request, String::new(), json!({ "node": node, "text": text }))
    }

    /// How big a terminal node's letters are.
    ///
    /// Through [`UnluminousApp::step_a_node_font`] and the node's own state, which is what the two
    /// buttons on its header press - one path, so a size set from the command line and one set by
    /// hand are the same size.
    fn cli_space_font(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Terminal) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        if request.switch("reset") {
            self.space.space.change(node, |state| {
                if let State::Terminal(terminal) = state {
                    terminal.font_size = 0.0;
                }
            });
        } else if let Some(size) = request.number("size") {
            let wanted = size as f32;
            // A range rather than the list `TERMINAL_FONT_SIZES` offers, because that list is what
            // the **buttons** walk and not what the setting allows: `terminal.font.size` is a number
            // somebody types, and this machine's is 34. A node that could not be given the size its
            // own terminal is set in would be a node that refused the only size that was wanted.
            if !(6.0..=96.0).contains(&wanted) {
                return no(request, code::USAGE, "A terminal is set in 6 to 96 point.");
            }
            self.space.space.change(node, |state| {
                if let State::Terminal(terminal) = state {
                    terminal.font_size = wanted;
                }
            });
        } else if request.switch("bigger") {
            self.step_a_node_font(node, 1);
        } else if request.switch("smaller") {
            self.step_a_node_font(node, -1);
        }
        let found = self.space.space.current().node(node).cloned();
        let size = found
            .as_ref()
            .map(|node| space_view::font_size_of(node, self.settings.terminal_font_size))
            .unwrap_or_default();
        let own = matches!(&found.map(|node| node.state), Some(State::Terminal(terminal)) if terminal.font_size > 0.0);
        ok(
            request,
            format!("Node {node} is set in {size:.0} point."),
            json!({ "node": node, "size": size, "itsOwn": own }),
        )
    }

    /// How big one node draws what it holds.
    ///
    /// Through [`UnluminousApp::zoom_a_node`], which is what the modifier wheel over a node presses — one
    /// path, so a size set from the command line and one set by hand are the same size.
    fn cli_space_zoom(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        if let Some(from) = request.number("from").map(|id| id as u64) {
            if !self.space.space.may_reach(from, node) {
                return no(
                    request,
                    code::REFUSED,
                    format!("Node {from} is not connected to node {node}."),
                );
            }
        }
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        if request.switch("reset") {
            self.space.space.change(node, |state| match state {
                State::Terminal(terminal) => terminal.font_size = 0.0,
                State::Editor(editor) => editor.font_size = 0.0,
                State::Folder(_) | State::Chat(_) | State::Tasks(_) => set_node_zoom(state, 1.0),
                State::Browser(_) => {}
            });
            if found.kind() == Kind::Browser {
                self.space.live.set_page_zoom(node, 1.0);
                if let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) {
                    let _ = self.browser.zoom(tab, 1.0);
                }
            }
        } else if let Some(asked) = request.number("factor") {
            let asked = asked as f32;
            if asked <= 0.0 {
                return no(request, code::USAGE, "A zoom is a number above zero.");
            }
            match found.kind() {
                Kind::Terminal | Kind::Editor => {
                    if !(6.0..=96.0).contains(&asked) {
                        return no(request, code::USAGE, "A point size is 6 to 96.");
                    }
                    self.space.space.change(node, |state| match state {
                        State::Terminal(terminal) => terminal.font_size = asked,
                        State::Editor(editor) => editor.font_size = asked,
                        _ => {}
                    });
                    if let Some(index) = self.files.tab_in_node(node) {
                        self.files.at_mut(index).cached.stale = true;
                    }
                }
                Kind::Folder | Kind::Chat | Kind::Tasks => {
                    if !(0.25..=4.0).contains(&asked) {
                        return no(request, code::USAGE, "This node's zoom is 0.25 to 4.");
                    }
                    self.space.space.change(node, |state| set_node_zoom(state, asked));
                }
                Kind::Browser => {
                    if !(0.25..=4.0).contains(&asked) {
                        return no(request, code::USAGE, "A page's zoom is 0.25 to 4.");
                    }
                    self.space.live.set_page_zoom(node, asked);
                    if let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) {
                        if let Err(problem) = self.browser.zoom(tab, f64::from(asked)) {
                            self.message = Some(problem);
                        }
                    }
                }
            }
        } else if request.switch("bigger") {
            self.zoom_a_node(node, 1);
        } else if request.switch("smaller") {
            self.zoom_a_node(node, -1);
        }
        let found = self.space.space.current().node(node).cloned();
        let (factor, own) = match found.as_ref().map(|node| &node.state) {
            Some(State::Terminal(terminal)) => (
                space_view::font_size_of(
                    found.as_ref().expect("it is there"),
                    self.settings.terminal_font_size,
                ),
                terminal.font_size > 0.0,
            ),
            Some(State::Editor(editor)) => (
                space_view::editor_font_size_of(
                    found.as_ref().expect("it is there"),
                    self.settings.font_size,
                ),
                editor.font_size > 0.0,
            ),
            Some(State::Folder(_) | State::Chat(_) | State::Tasks(_)) => {
                let zoom = node_zoom_of(found.as_ref().expect("it is there"));
                (zoom, (zoom - 1.0).abs() > 0.001)
            }
            Some(State::Browser(_)) => {
                let zoom = self.space.live.page_zoom_of(node);
                (zoom, (zoom - 1.0).abs() > 0.001)
            }
            None => (1.0, false),
        };
        let walks = match found.as_ref().map(|node| node.kind()) {
            Some(Kind::Terminal) => "terminal.font.size",
            Some(Kind::Editor) => "appearance.font.size",
            Some(Kind::Folder) => "a multiplier over its rows",
            Some(Kind::Chat | Kind::Tasks) => "a multiplier over everything it draws",
            Some(Kind::Browser) => "the page's own zoom",
            None => "",
        };
        ok(
            request,
            format!("Node {node} is drawn at {factor:.2}."),
            json!({ "node": node, "factor": factor, "itsOwn": own, "walks": walks }),
        )
    }

    fn cli_space_browser(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Browser) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(command) = request.text("command") else {
            return no(
                request,
                code::USAGE,
                "Say what to do: go, back, forward, reload, url or shot.",
            );
        };
        match command.trim() {
            "go" => {
                let Some(url) = request.text("url") else {
                    return no(request, code::USAGE, "Say where to go, with --url.");
                };
                match self.send_a_space_browser_to(node, url.trim()) {
                    Ok(()) => done(request, format!("Node {node} is going to {url}.")),
                    Err(problem) => no(request, code::FAILED, problem),
                }
            }
            step @ ("back" | "forward") => {
                let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) else {
                    return no(request, code::REFUSED, format!("Node {node} has no page open."));
                };
                let command = match step {
                    "back" => crate::services::browser::BrowserCommand::Back,
                    _ => crate::services::browser::BrowserCommand::Forward,
                };
                self.run_browser_command(tab, command);
                done(request, format!("Node {node} went {step}."))
            }
            "reload" => {
                let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) else {
                    return no(request, code::REFUSED, format!("Node {node} has no page open."));
                };
                self.run_browser_command(tab, crate::services::browser::BrowserCommand::Reload);
                done(request, format!("Node {node} is reloading."))
            }
            "url" => match self.space.live.browser(node) {
                Some(tab) => ok(
                    request,
                    tab.current_url().to_owned(),
                    json!({ "node": node, "url": tab.current_url(), "loading": tab.loading }),
                ),
                None => no(request, code::REFUSED, format!("Node {node} has no page open.")),
            },
            "shot" => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(
                        request,
                        code::USAGE,
                        "Say where to write the picture, with --path.",
                    );
                };
                let Some(found) = self.space.space.current().node(node).cloned() else {
                    return no(request, code::NOT_FOUND, format!("There is no node {node}."));
                };
                // The node has to be **showing** to be photographed, because a picture is of the
                // window as the operating system composited it and a native child view is part of
                // that rather than something Unluminous can render on its own.
                self.show_a_panel(dock::Panel::Space, true);
                self.space.space.choose(Some(node));
                self.space.space.raise(node);
                let camera = self.space.space.current().camera;
                let area = camera.rect_to_screen(self.space.body.min, found.rect());
                ctx.request_repaint();
                Outcome::Hold(crate::app::cli::Waiting::Screenshot {
                    path,
                    until: std::time::Instant::now() + std::time::Duration::from_secs(10),
                    settled: std::time::Instant::now() + std::time::Duration::from_millis(250),
                    asked: false,
                    crop: Some(area.intersect(self.space.body)),
                })
            }
            other => no(
                request,
                code::USAGE,
                format!("A browser node does go, back, forward, reload, url or shot, not {other}."),
            ),
        }
    }

    fn cli_space_folder(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Folder) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(command) = request.text("command") else {
            return no(
                request,
                code::USAGE,
                "Say what to do: expand, collapse, select, open, root or rows.",
            );
        };
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        self.make_sure_a_node_has_a_tree(&found);
        match command.trim() {
            open @ ("expand" | "collapse") => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which folder, with --path.");
                };
                let wanted = open == "expand";
                let Some(tree) = self.space.live.tree_mut(node) else {
                    return no(request, code::FAILED, "That node has no tree.");
                };
                let showing = tree.find(&path).map(|entry| entry.expanded).unwrap_or(false);
                if showing != wanted {
                    tree.toggle(&path);
                }
                self.remember_a_folder_nodes_open_folders(node);
                done(
                    request,
                    format!(
                        "{} {} in node {node}.",
                        if wanted { "Opened" } else { "Shut" },
                        path.display()
                    ),
                )
            }
            "select" => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which row, with --path.");
                };
                self.space.live.select_in_tree(node, Some(path.clone()));
                done(request, format!("Node {node} is on {}.", path.display()))
            }
            "open" => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which file, with --path.");
                };
                // **The same rule a double click keeps**: into a wired File Editor node when there is one,
                // and into the editing area when there is not. One function, so the pointer and the agent
                // cannot come to different answers. `task-1905`.
                let wired = self.space.space.current().reaches(node).into_iter().find(|other| {
                    self.space
                        .space
                        .current()
                        .node(*other)
                        .is_some_and(|found| found.kind() == Kind::Editor)
                });
                match wired {
                    Some(editor) => match self.open_in_a_space_node(editor, &path) {
                        Ok(()) => ok(
                            request,
                            format!("Opened {} in node {editor}.", path.display()),
                            json!({ "node": editor, "path": path.to_string_lossy() }),
                        ),
                        Err(problem) => no(request, code::FAILED, problem),
                    },
                    None => match self.open_path_permanently(&path) {
                        Ok(()) => ok(
                            request,
                            format!("Opened {} in the editing area.", path.display()),
                            json!({ "node": Value::Null, "path": path.to_string_lossy() }),
                        ),
                        Err(problem) => no(request, code::FAILED, problem),
                    },
                }
            }
            "root" => {
                let Some(root) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which folder, with --path.");
                };
                // Through the one function the `Choose Folder...` dialog calls, so a folder chosen by
                // hand and one named here are the same change.
                self.point_a_folder_node_at(node, &root);
                done(request, format!("Node {node} is showing {}.", root.display()))
            }
            "rows" => {
                let Some(tree) = self.space.live.tree(node) else {
                    return no(request, code::FAILED, "That node has no tree.");
                };
                let rows: Vec<String> = tree
                    .rows()
                    .iter()
                    .map(|row| {
                        format!(
                            "{}{}{}",
                            "  ".repeat(row.depth),
                            row.entry
                                .path
                                .file_name()
                                .map(|name| name.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            if row.entry.is_directory { "/" } else { "" },
                        )
                    })
                    .collect();
                let paths: Vec<Value> = tree
                    .rows()
                    .iter()
                    .map(|row| {
                        json!({
                            "path": row.entry.path.to_string_lossy(),
                            "directory": row.entry.is_directory,
                            "expanded": row.entry.expanded,
                            "depth": row.depth,
                        })
                    })
                    .collect();
                lines(request, format!("{} rows", paths.len()), rows, json!({ "rows": paths }))
            }
            other => no(
                request,
                code::USAGE,
                format!(
                    "A folder node does expand, collapse, select, open, root or rows, not {other}."
                ),
            ),
        }
    }

    // ------------------------------------------------------------------------------- naming things

    /// The view a command named, by its name or by its id.
    ///
    /// The error is boxed because `Outcome` carries the whole of `Waiting`, which clippy flags as too
    /// large to return unboxed.
    fn a_named_view(&self, request: &Request) -> Result<u64, Box<Outcome>> {
        let Some(name) = request.text("view") else {
            return Err(Box::new(no(
                request,
                code::USAGE,
                "Say which view, by its name or its id.",
            )));
        };
        self.space.space.view_named(name.trim()).ok_or_else(|| {
            let names: Vec<&str> =
                self.space.space.views().iter().map(|view| view.name.as_str()).collect();
            Box::new(no(
                request,
                code::NOT_FOUND,
                format!("There is no view called {name}. This canvas has {}.", names.join(", ")),
            ))
        })
    }

    /// The node a command named, by its id.
    ///
    /// The error is boxed for the same reason `a_named_view`'s is.
    fn a_named_node(&self, request: &Request, argument: &str) -> Result<NodeId, Box<Outcome>> {
        let Some(id) = request.number(argument).map(|id| id as u64) else {
            return Err(Box::new(no(
                request,
                code::USAGE,
                format!("Say which node, with `{argument}` and an id from `space list`."),
            )));
        };
        match self.space.space.current().node(id).is_some() {
            true => Ok(id),
            false => Err(Box::new(no(
                request,
                code::NOT_FOUND,
                format!("There is no node {id} on {}.", self.space.space.current().name),
            ))),
        }
    }

    /// The node a command named, checked against what the node asking is wired to.
    ///
    /// **This is the whole of the permission model.** A command carrying `--from` is acting as that
    /// node; one without it is the window's own agent. A node that is not wired to its target is
    /// refused with what it *is* wired to, so an agent that guessed is told what it may reach rather
    /// than left to guess again.
    ///
    /// The error is boxed for the same reason `a_named_view`'s is.
    fn a_reachable_node(
        &self,
        request: &Request,
        argument: &str,
        wanted: Kind,
    ) -> Result<NodeId, Box<Outcome>> {
        let node = self.a_named_node(request, argument)?;
        let found = self.space.space.current().node(node).ok_or_else(|| {
            Box::new(no(request, code::NOT_FOUND, format!("There is no node {node}.")))
        })?;
        if found.kind() != wanted {
            return Err(Box::new(no(
                request,
                code::REFUSED,
                format!(
                    "Node {node} is a {} node, not a {} one.",
                    found.kind().name(),
                    wanted.name()
                ),
            )));
        }
        let Some(from) = request.number("from").map(|id| id as u64) else {
            return Ok(node);
        };
        if self.space.space.current().node(from).is_none() {
            return Err(Box::new(no(
                request,
                code::NOT_FOUND,
                format!("There is no node {from}."),
            )));
        }
        if self.space.space.may_reach(from, node) {
            return Ok(node);
        }
        let reaches = self.space.space.current().reaches(from);
        Err(Box::new(no(
            request,
            code::REFUSED,
            match reaches.is_empty() {
                true => format!("Node {from} is not connected to anything."),
                false => format!(
                    "Node {from} is not connected to node {node}. It is connected to {}.",
                    reaches.iter().map(u64::to_string).collect::<Vec<_>>().join(", ")
                ),
            },
        )))
    }

    /// `show`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_show(&mut self, request: &Request) -> Outcome {
        self.show_a_panel(dock::Panel::Space, true);
        self.take_the_keyboard_for_the_space();
        done(request, "The Base of Infinite Space is showing.")
    }

    /// `hide`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_hide(&mut self, request: &Request) -> Outcome {
        self.show_a_panel(dock::Panel::Space, false);
        done(request, "Put the canvas away.")
    }

    /// `manage`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_manage(&mut self, request: &Request) -> Outcome {
        self.run_a_space_action(SpaceAction::Manage);
        done(request, "The space manager is open.")
    }

    /// `new-view`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_new_view(&mut self, request: &Request) -> Outcome {
        let name = request.text("name").unwrap_or_else(|| "View".to_owned());
        let id = self.space.space.add_view(&name);
        self.space.space.show_view(id);
        ok(
            request,
            format!("Made {}.", self.space.space.current().name),
            json!({ "view": id, "name": self.space.space.current().name }),
        )
    }

    /// `rename-view`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_rename_view(&mut self, request: &Request) -> Outcome {
        let id = match self.a_named_view(request) {
            Ok(id) => id,
            Err(outcome) => return *outcome,
        };
        let Some(name) = request.text("name") else {
            return no(request, code::USAGE, "Say what to call it.");
        };
        self.space.space.rename_view(id, &name);
        let now = self.space.space.view(id).map(|view| view.name.clone()).unwrap_or_default();
        ok(request, format!("Called it {now}."), json!({ "view": id, "name": now }))
    }

    /// `duplicate-view`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_duplicate_view(&mut self, request: &Request) -> Outcome {
        let id = match self.a_named_view(request) {
            Ok(id) => id,
            Err(outcome) => return *outcome,
        };
        match self.space.space.duplicate_view(id) {
            Some(copy) => {
                self.space.space.show_view(copy);
                self.bring_the_current_view_to_life();
                ok(
                    request,
                    format!("Copied it to {}.", self.space.space.current().name),
                    json!({ "view": copy }),
                )
            }
            None => no(request, code::NOT_FOUND, "There is no such view."),
        }
    }

    /// `delete-view`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_delete_view(&mut self, request: &Request) -> Outcome {
        let id = match self.a_named_view(request) {
            Ok(id) => id,
            Err(outcome) => return *outcome,
        };
        if self.space.space.views().len() < 2 {
            return no(
                request,
                code::REFUSED,
                "A canvas always has one view, so the last one cannot be deleted.",
            );
        }
        self.delete_a_space_view(id);
        done(request, "Deleted it.")
    }

    /// `title`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_title(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let title = request.text("title").unwrap_or_default();
        self.space.space.title_node(node, title.trim());
        done(request, format!("Called node {node} {title}."))
    }

    /// `remove`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_remove(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        self.close_a_space_node(node);
        done(request, format!("Took node {node} off the canvas."))
    }

    /// `focus`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_focus(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        self.show_a_panel(dock::Panel::Space, true);
        self.space.space.choose(Some(node));
        self.space.space.raise(node);
        self.take_the_keyboard_for_the_space();
        done(request, format!("Node {node} has the keyboard."))
    }

    /// `disconnect`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_disconnect(&mut self, request: &Request) -> Outcome {
        let Some(edge) = request.number("connection").map(|id| id as u64) else {
            return no(request, code::USAGE, "Say which connection, by its id.");
        };
        match self.space.space.disconnect(edge) {
            true => done(request, format!("Took connection {edge} away.")),
            false => no(request, code::NOT_FOUND, format!("There is no connection {edge}.")),
        }
    }

    /// `restart`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_restart(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        // **`--running` is the other half of the row on the node's own menu**, which is the rule that a
        // thing done by hand and the same thing done by an agent are the same thing: it goes through
        // `start_what_a_node_was_running`, so the program is typed into the shell the node has rather
        // than replacing it. `task-1907`.
        if request.switch("running") {
            let was = self.space.chosen();
            self.space.space.choose(Some(node));
            self.start_what_a_node_was_running();
            let answer = self.message.clone().unwrap_or_default();
            self.space.space.choose(was);
            let started = answer.starts_with("Started ");
            return match started {
                true => done(request, answer),
                false => no(request, code::REFUSED, answer),
            };
        }
        match self.start_a_space_terminal(node, request.switch("resume")) {
            Ok(()) => done(request, format!("Started node {node} again.")),
            Err(problem) => no(request, code::FAILED, problem),
        }
    }

    /// `address`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_address(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Browser) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(url) = request.text("url") else {
            return no(request, code::USAGE, "Say which address.");
        };
        // Through the same function the field's own Enter reaches, which is `run_cli`'s rule.
        match self.send_a_space_browser_to(node, url.trim()) {
            Ok(()) => {
                self.space.space.change(node, |state| {
                    if let State::Browser(browser) = state {
                        browser.typed = url.trim().to_owned();
                    }
                });
                done(request, format!("Node {node} is going to {}.", url.trim()))
            }
            Err(problem) => no(request, code::FAILED, problem),
        }
    }

    /// `editor`. Split out of [`Self::cli_space`] by `task-1984` §3.6.
    fn cli_space_editor(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Editor) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(path) = self.cli_path_argument(request, "path") else {
            return no(request, code::USAGE, "Say which file.");
        };
        match self.open_in_a_space_node(node, &path) {
            Ok(()) => ok(
                request,
                format!("Opened {} in node {node}.", path.display()),
                json!({ "node": node, "path": path.to_string_lossy() }),
            ),
            Err(problem) => no(request, code::FAILED, problem),
        }
    }
}
