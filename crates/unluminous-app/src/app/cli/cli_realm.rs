//! `realm` — the agent's half of the Realm.
//!
//! **Moved here from `app/realm.rs` by `task-1984` §3.6**, which found that file at 4,734 lines and
//! the largest in the crate. Every other area of the catalogue keeps its handlers in this folder;
//! this one did not, and the canvas is the area with the most verbs in it. Nothing about what a
//! command does changed in the move; only where its code lives did, which is the sentence
//! `services::agent_tasks::commands` already carries about its own split.

use super::*;

use egui::{Pos2, Vec2};

use crate::app::actions::RealmAction;
use crate::app::realm::{drives, node_zoom_of, set_node_zoom};
use crate::components::realm::{self as realm_view};
use crate::services::realm::{
    shape, Align, Kind, NodeId, Pipe, ShapeKind, SizeStep, State, VAlign,
};

/// How many points a shape of this kind can be joined at.
fn held_points(shape: ShapeKind) -> usize {
    shape::anchors(shape, egui::Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0))).len()
}

/// The refusal for a shape that is not one, naming the ones there are.
fn no_such_shape(name: &str) -> String {
    let names: Vec<&str> = ShapeKind::ALL.into_iter().map(ShapeKind::name).collect();
    format!("There is no shape called {name}. The shapes are {}.", names.join(", "))
}

/// The `realm` area of `unluminous-cli`, which is the agent's half of the canvas.
///
/// **Every one of these goes through the same functions the pointer does.** `realm add` is what the
/// right click modal calls, `realm connect` is what letting a wire go calls, and `realm remove` is
/// what the close cross calls - which is `UnluminousApp::run_cli`'s own rule, that a thing done by
/// hand and the same thing done by an agent are the same thing rather than two paths that agree
/// today.
///
/// **`--from` is what a connection is for.** A command carrying it is acting *as* that node and is
/// refused when there is no wire from it to the node it names; a command with no `--from` is the
/// window's own agent, which may drive everything. That is the ticket's *"the main agent for the IDE
/// can control every single node"* beside its *"terminal node agents control the things they are
/// connected to"*.
/// The verbs that change what the open realm file says, refused while it is read only.
const CHANGES_THE_FILE: &[&str] = &[
    "add",
    "move",
    "size",
    "title",
    "remove",
    "arrange",
    "connect",
    "disconnect",
    "volume",
    "shape",
];

impl UnluminousApp {
    pub(crate) fn cli_realm(
        &mut self,
        request: &Request,
        verb: &str,
        ctx: &egui::Context,
    ) -> Outcome {
        // The Realm plugin is the switch for the panel and everything on it. `task-2202`.
        if !self.realm_is_on() {
            return no(request, code::REFUSED, self.the_realm_is_off());
        }
        // A realm a newer Unluminous wrote is open for reading only, so a command that would change what
        // the file says is refused with the reason the banner gives. The camera, the chosen node and the
        // players are this machine's, and stay free. `task-2202`.
        if CHANGES_THE_FILE.contains(&verb) {
            if let Some(why) = self.realm.realm.read_only_because() {
                return no(request, code::REFUSED, format!("This realm is read only. {why}"));
            }
        }
        match verb {
            "show" => self.cli_realm_show(request),
            "hide" => self.cli_realm_hide(request),
            "here" => self.cli_realm_here(request),
            "view" => ok(request, "The canvas.", self.realm.realm.as_json()),
            "nodes" => self.cli_realm_nodes(request),
            "manage" => self.cli_realm_manage(request),
            "list" => self.cli_realm_list(request),
            "open" => self.cli_realm_open(request),
            "new" => self.cli_realm_new(request),
            "rename" => self.cli_realm_rename(request),
            "duplicate" => self.cli_realm_duplicate(request),
            "delete" => self.cli_realm_delete(request),
            "close" => self.cli_realm_close(request),
            "files" => self.cli_realm_files(request),
            "import" => self.cli_realm_import(request),
            "info" => self.cli_realm_info(request),
            "add" => self.cli_realm_add(request),
            "move" => self.cli_realm_move(request),
            "size" => self.cli_realm_size(request),
            "title" => self.cli_realm_title(request),
            "remove" => self.cli_realm_remove(request),
            "focus" => self.cli_realm_focus(request),
            "arrange" => self.cli_realm_arrange(request),
            "connect" => self.cli_realm_connect(request),
            "disconnect" => self.cli_realm_disconnect(request),
            "connections" => self.cli_realm_connections(request),
            "camera" => self.cli_realm_camera(request),
            "send" => self.cli_realm_send(request),
            "chat" => self.cli_realm_chat(request),
            "read" => self.cli_realm_read(request),
            "restart" => self.cli_realm_restart(request),
            "font" => self.cli_realm_font(request),
            "zoom" => self.cli_realm_zoom(request),
            "address" => self.cli_realm_address(request),
            "browser" => self.cli_realm_browser(request, ctx),
            "folder" => self.cli_realm_folder(request),
            "editor" => self.cli_realm_editor(request),
            "note" => self.cli_realm_note(request),
            "shape" => self.cli_realm_shape(request),
            "play" => self.cli_realm_transport(request, crate::app::realm_nodes::Transport::Play),
            "pause" => self.cli_realm_transport(request, crate::app::realm_nodes::Transport::Pause),
            "seek" => match request.number("seconds") {
                Some(seconds) => self.cli_realm_transport(
                    request,
                    crate::app::realm_nodes::Transport::Seek(seconds as f32),
                ),
                None => no(request, code::USAGE, "Say how many seconds in."),
            },
            "volume" => match request.number("level") {
                Some(level) if (0.0..=1.0).contains(&level) => self.cli_realm_transport(
                    request,
                    crate::app::realm_nodes::Transport::Volume(level as f32),
                ),
                _ => no(request, code::USAGE, "Say a volume from 0 to 1."),
            },
            _ => unknown(request),
        }
    }

    /// Which node this command came from, and what it may act on.
    ///
    /// **The one command whose answer depends on which process is asking**, which is why it is a command
    /// rather than a paragraph: `UNLUMINOUS_REALM_NODE` is in the *client's* environment, so the client
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
    fn cli_realm_here(&self, request: &Request) -> Outcome {
        let view = &self.realm.realm;
        // The client puts what its own environment said here; the window has no way to know.
        let asking = request.number("node").map(|id| id as u64);
        let Some(asking) = asking else {
            return lines(
                request,
                "Not running inside a node.",
                vec![
                    "This command is not running inside a node on the canvas, so it is the window's own"
                        .to_owned(),
                    "agent: every node is reachable and no --from is needed. `realm list` is the nodes."
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
                    "UNLUMINOUS_REALM_NODE says node {asking}, and there is no such node on {}. The canvas may have moved to another view.",
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

    /// The nodes on the realm that is open, one a line.
    fn cli_realm_nodes(&self, request: &Request) -> Outcome {
        let view = &self.realm.realm;
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
            view.title()
        );
        lines(request, message, rows, self.realm.realm.as_json())
    }

    /// Every realm file in the project, the open one marked. `task-2202`.
    fn cli_realm_list(&mut self, request: &Request) -> Outcome {
        self.list_the_realms();
        let open = self.realm.realm.path.clone();
        let mut rows = Vec::new();
        let mut realms = Vec::new();
        for path in &self.realm.files {
            let showing = *path == open;
            let (nodes, connections) = match showing {
                true => (self.realm.realm.nodes.len(), self.realm.realm.edges.len()),
                false => self.realm.counts.get(path).copied().unwrap_or((0, 0)),
            };
            let written = crate::services::realm::slashed(path);
            rows.push(format!(
                "{}{:<40} {} node{}",
                if showing { "*" } else { " " },
                written,
                nodes,
                if nodes == 1 { "" } else { "s" },
            ));
            realms.push(json!({
                "path": written,
                "name": crate::services::realm::title_of(path),
                "nodes": nodes,
                "connections": connections,
                "showing": showing,
            }));
        }
        lines(
            request,
            format!("{} realm{}", realms.len(), if realms.len() == 1 { "" } else { "s" }),
            rows,
            json!({ "current": crate::services::realm::slashed(&open), "realms": realms }),
        )
    }

    /// Put a node on the canvas, and give it whatever its kind was told to hold.
    fn cli_realm_add(&mut self, request: &Request) -> Outcome {
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
        self.show_a_panel(dock::Panel::Realm, true);
        if let Some(why) = self.realm.realm.read_only_because() {
            return no(request, code::REFUSED, why.to_owned());
        }
        let middle = self.middle_of_the_canvas();
        let at = Pos2::new(
            request.number("x").map(|x| x as f32).unwrap_or(middle.x),
            request.number("y").map(|y| y as f32).unwrap_or(middle.y),
        );
        // **A picture, a sound, a video and a note are made around the file they show** (`task-2202`), so the
        // file is the argument rather than something given afterwards.
        let given = request.text("file").or_else(|| request.text("path"));
        let node = match kind {
            Kind::Image | Kind::Audio | Kind::Video => {
                let Some(file) = given else {
                    return no(
                        request,
                        code::USAGE,
                        format!("Say which file in this project the {} node shows.", kind.name()),
                    );
                };
                match self.add_a_file_node(kind, std::path::Path::new(file.trim()), at) {
                    Ok(node) => node,
                    Err(problem) => return no(request, code::REFUSED, problem),
                }
            }
            Kind::Note => {
                let given = given.unwrap_or_default();
                let existing = self.tree.root().join(given.trim());
                let made = match given.trim().to_lowercase().ends_with(".md") && existing.is_file()
                {
                    true => self.add_a_file_node(kind, &existing, at),
                    false => {
                        let name = match given.trim() {
                            "" => self.an_unused_note_name(),
                            named => named.to_owned(),
                        };
                        self.new_note(&name, at)
                    }
                };
                match made {
                    Ok(node) => node,
                    Err(problem) => return no(request, code::REFUSED, problem),
                }
            }
            // **A shape is named as the second argument** and given its words with `--text`. It is chosen and
            // the canvas takes the keyboard, as it is from the dialog, but nobody is typing in it: an agent
            // that wanted to type would say `realm shape <node> edit`. `task-2284`.
            Kind::Shape => {
                let named = given.as_deref().map(str::trim).filter(|name| !name.is_empty());
                let shape = match named {
                    None => ShapeKind::Rectangle,
                    Some(name) => match ShapeKind::from_name(name) {
                        Some(shape) => shape,
                        None => return no(request, code::USAGE, no_such_shape(name)),
                    },
                };
                let node = self.realm.realm.add_shape(shape, at);
                if let Some(text) = request.text("text") {
                    let text = text.replace("\\n", "\n");
                    let _ = self.realm.realm.change_shape(node, |held| held.text = text);
                }
                self.take_the_keyboard_for_the_realm();
                node
            }
            _ => self.add_a_realm_node(kind, at),
        };
        if let (Some(width), Some(height)) = (request.number("width"), request.number("height")) {
            self.realm.realm.resize_node(node, Vec2::new(width as f32, height as f32));
        }
        if let Some(title) = request.text("title") {
            self.realm.realm.title_node(node, title.trim());
        }
        // Whatever the kind was told to hold, applied through the same functions the window uses.
        let mut problem: Option<String> = None;
        if let Some(command) = request.text("command") {
            self.realm.realm.change(node, |state| {
                if let State::Terminal(terminal) = state {
                    terminal.command = command.trim().to_owned();
                }
            });
            if let Err(refusal) = self.start_a_realm_terminal(node, false) {
                problem = Some(refusal);
            }
        }
        if let Some(url) = request.text("url") {
            if let Err(refusal) = self.open_a_realm_browser(node, url.trim()) {
                problem = Some(refusal);
            }
        }
        if let Some(root) = self.cli_path_argument(request, "root") {
            self.realm.realm.change(node, |state| {
                if let State::Folder(folder) = state {
                    folder.root = Some(root.clone());
                }
            });
            self.realm.live.forget(node);
        }
        if kind == Kind::Editor {
            if let Some(path) = self.cli_path_argument(request, "path") {
                if let Err(refusal) = self.open_in_a_realm_node(node, &path) {
                    problem = Some(refusal);
                }
            }
        }
        // **The node is still there when part of what it was given failed**, and the reply says so:
        // a refusal that also took the node away would leave a caller with nothing to correct.
        let made = self.realm.realm.node(node).cloned();
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

    fn cli_realm_move(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(found) = self.realm.realm.node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        let at = Pos2::new(
            request.number("x").map(|x| x as f32).unwrap_or(found.at.x),
            request.number("y").map(|y| y as f32).unwrap_or(found.at.y),
        );
        self.realm.realm.move_node(node, at);
        ok(request, format!("Moved node {node}."), json!({ "node": node, "x": at.x, "y": at.y }))
    }

    fn cli_realm_size(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(found) = self.realm.realm.node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        let size = Vec2::new(
            request.number("width").map(|width| width as f32).unwrap_or(found.size.x),
            request.number("height").map(|height| height as f32).unwrap_or(found.size.y),
        );
        self.realm.realm.resize_node(node, size);
        // What it really came out, because a kind has a smallest size and a caller that asked for
        // less deserves to be told what it got rather than that it worked.
        let now = self.realm.realm.node(node).map(|node| node.size).unwrap_or(size);
        ok(
            request,
            format!("Node {node} is {} x {}.", now.x.round(), now.y.round()),
            json!({ "node": node, "width": now.x, "height": now.y }),
        )
    }

    fn cli_realm_connect(&mut self, request: &Request) -> Outcome {
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
        let from_point = match self.a_named_point(request, "from-point", from) {
            Ok(point) => point,
            Err(outcome) => return *outcome,
        };
        let to_point = match self.a_named_point(request, "to-point", to) {
            Ok(point) => point,
            Err(outcome) => return *outcome,
        };
        // **A shape whose point was not named is joined at the point nearest the other node**, which is what
        // letting a wire go over it does, so the file says which point rather than leaving it to be worked
        // out every time it is drawn.
        let nearest = |this: NodeId, other: NodeId| -> Option<usize> {
            let node = self.realm.realm.node(this)?;
            let held = node.shape()?;
            let toward = self.realm.realm.node(other)?.rect().center();
            Some(shape::nearest_anchor(held.shape, node.rect(), toward))
        };
        let from_point = from_point.or_else(|| nearest(from, to));
        let to_point = to_point.or_else(|| nearest(to, from));
        match self.realm.realm.connect_at(from, from_point, to, to_point, pipe) {
            Ok(edge) => {
                if pipe == Pipe::Lines {
                    self.realm.live.follow_from_here(from);
                }
                let name = |node: NodeId, point: Option<usize>| -> Value {
                    match (self.realm.realm.node(node).and_then(|node| node.shape()), point) {
                        (Some(held), Some(point)) => shape::point_name(held.shape, point).into(),
                        _ => Value::Null,
                    }
                };
                ok(
                    request,
                    format!("Connected {from} to {to}."),
                    json!({
                        "connection": edge,
                        "from": from,
                        "to": to,
                        "pipe": pipe.name(),
                        "fromPoint": name(from, from_point),
                        "toPoint": name(to, to_point),
                    }),
                )
            }
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    /// The point a `--from-point` or `--to-point` names on a node, when it names one.
    ///
    /// Refused when it is given for a node that is not a shape, or names a point the shape does not have,
    /// with the points it does have, because a wire joined somewhere other than where it was asked to be is a
    /// change nobody asked for.
    fn a_named_point(
        &self,
        request: &Request,
        flag: &str,
        node: NodeId,
    ) -> Result<Option<usize>, Box<Outcome>> {
        let Some(named) = request.text(flag) else { return Ok(None) };
        let Some(held) = self.realm.realm.node(node).and_then(|found| found.shape()) else {
            return Err(Box::new(no(
                request,
                code::USAGE,
                format!(
                    "Node {node} is not a shape, so it has no points to name. Leave --{flag} off."
                ),
            )));
        };
        match shape::point_from_name(held.shape, &named) {
            Some(point) => Ok(Some(point)),
            None => {
                let count = held_points(held.shape);
                let names: Vec<String> =
                    (0..count).map(|index| shape::point_name(held.shape, index)).collect();
                Err(Box::new(no(
                    request,
                    code::USAGE,
                    format!(
                        "A {} has the points {}, not {named}.",
                        held.shape.name(),
                        names.join(", ")
                    ),
                )))
            }
        }
    }

    /// `realm shape <node> <command> [value]`: everything a shape's menu, its colour picker and a double click
    /// do, through the same functions. `task-2284`.
    fn cli_realm_shape(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(found) = self.realm.realm.node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        let Some(held) = found.shape().cloned() else {
            return no(
                request,
                code::REFUSED,
                format!("Node {node} is a {} node, not a shape.", found.kind().name()),
            );
        };
        let command = request.text("command").map(|command| command.trim().to_lowercase());
        let value = request.text("value").map(|value| value.trim().to_owned()).unwrap_or_default();
        let done = match command.as_deref().unwrap_or("view") {
            "view" | "points" => Ok(String::new()),
            "text" => {
                let text = value.replace("\\n", "\n");
                self.realm.realm.change_shape(node, |held| held.text = text).map(|()| "Set its words.".to_owned())
            }
            "colour" | "color" => match value.to_lowercase().as_str() {
                "theme" | "none" | "" => self
                    .realm
                    .realm
                    .change_shape(node, |held| held.fill = None)
                    .map(|()| "It follows the theme.".to_owned()),
                written => match unluminous_core::Rgba::parse(written) {
                    Some(colour) => self
                        .realm
                        .realm
                        .change_shape(node, |held| held.fill = Some(colour))
                        .map(|()| format!("Colored it {}.", colour.to_hex())),
                    None => Err(format!("{value} is not a colour. Write #RRGGBB, #RRGGBBAA or theme.")),
                },
            },
            "align" => match Align::from_name(&value) {
                Some(align) => self
                    .realm
                    .realm
                    .change_shape(node, |held| held.align = align)
                    .map(|()| format!("Its words are {}.", align.label().to_lowercase())),
                None => Err(format!("Words are aligned left, center or right, not {value}.")),
            },
            "valign" => match VAlign::from_name(&value) {
                Some(valign) => self
                    .realm
                    .realm
                    .change_shape(node, |held| held.valign = valign)
                    .map(|()| format!("Its words are {}.", valign.label().to_lowercase())),
                None => Err(format!("Words sit at the top, middle or bottom, not {value}.")),
            },
            "size" => match SizeStep::from_name(&value) {
                Some(step) => self
                    .realm
                    .realm
                    .change_shape(node, |held| held.size = step.apply(held.size))
                    .map(|()| "Changed the size of its words.".to_owned()),
                None => Err(format!("A size is 6 to 96 points, larger or smaller, not {value}.")),
            },
            "kind" => match ShapeKind::from_name(&value) {
                Some(shape) => self
                    .realm
                    .realm
                    .change_shape(node, |held| held.shape = shape)
                    .map(|()| format!("It is a {} now.", shape.label().to_lowercase())),
                None => Err(no_such_shape(&value)),
            },
            "edit" => self.start_typing_in_a_shape(node).map(|()| "Typing in it.".to_owned()),
            "done" => {
                if self.realm.typing.is_some_and(|typing| typing.node == node) {
                    self.realm.typing = None;
                }
                Ok("Stopped typing in it.".to_owned())
            }
            "pick" => self.pick_a_shapes_colour(node, &value),
            other => Err(format!(
                "A shape takes view, text, color, align, valign, size, kind, edit, done, pick or points, not {other}."
            )),
        };
        let said = match done {
            Ok(said) => said,
            Err(problem) => {
                let refused = problem.starts_with("This realm is read only")
                    || problem.contains("not a shape");
                return no(request, if refused { code::REFUSED } else { code::USAGE }, problem);
            }
        };
        let value = self.realm.realm.as_json();
        let mut answer = value["nodes"]
            .as_array()
            .and_then(|nodes| nodes.iter().find(|each| each["id"] == node).cloned())
            .unwrap_or(Value::Null);
        if let Some(map) = answer.as_object_mut() {
            map.insert(
                "typing".into(),
                self.realm.typing.is_some_and(|typing| typing.node == node).into(),
            );
            map.insert(
                "picker".into(),
                match self.realm.picking.as_ref().filter(|picking| picking.node == node) {
                    Some(picking) => json!({ "open": true, "colour": picking.colour().to_hex() }),
                    None => json!({ "open": false }),
                },
            );
        }
        let now =
            self.realm.realm.node(node).and_then(|found| found.shape().cloned()).unwrap_or(held);
        let message = match said.is_empty() {
            true => format!(
                "Node {node} is a {} saying {:?}.",
                now.shape.label().to_lowercase(),
                now.text
            ),
            false => said,
        };
        let rows: Vec<String> = match command.as_deref() {
            Some("points") => answer["points"]
                .as_array()
                .map(|points| {
                    points
                        .iter()
                        .map(|point| {
                            format!(
                                "{:<8} {:>8.1},{:<8.1}",
                                point["point"].as_str().unwrap_or_default(),
                                point["x"].as_f64().unwrap_or_default(),
                                point["y"].as_f64().unwrap_or_default(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        match rows.is_empty() {
            true => ok(request, message, answer),
            false => lines(request, message, rows, answer),
        }
    }

    /// `realm shape <node> pick [colour|apply|cancel]`: the colour picker, driven the way a person drives it.
    ///
    /// With nothing after it the picker opens, which is the shape's `Colour...` row. A colour moves the open
    /// picker to it, which recolours the shape while the picker is open and writes nothing, as dragging the
    /// picker does. `apply` and `cancel` are its two buttons.
    fn pick_a_shapes_colour(&mut self, node: NodeId, value: &str) -> Result<String, String> {
        let open = self.realm.picking.as_ref().is_some_and(|picking| picking.node == node);
        match value.to_lowercase().as_str() {
            "" => self.open_the_colour_picker(node).map(|()| "Opened the color picker.".to_owned()),
            "apply" | "cancel" if !open => {
                Err("The color picker is not open on that shape.".to_owned())
            }
            "apply" => {
                self.finish_the_colour_picker(true);
                Ok("Kept the colour.".to_owned())
            }
            "cancel" => {
                self.finish_the_colour_picker(false);
                Ok("Put the color back.".to_owned())
            }
            written => {
                let Some(colour) = unluminous_core::Rgba::parse(written) else {
                    return Err(format!("{written} is not a colour. Write #RRGGBB or #RRGGBBAA."));
                };
                if !open {
                    self.open_the_colour_picker(node)?;
                }
                if let Some(picking) = self.realm.picking.as_mut() {
                    picking.set(colour);
                }
                Ok(format!("The picker is on {}.", colour.to_hex()))
            }
        }
    }

    fn cli_realm_connections(&self, request: &Request) -> Outcome {
        let only = request.number("from").map(|id| id as u64);
        let view = &self.realm.realm;
        let found: Vec<&crate::services::realm::Edge> =
            view.edges.iter().filter(|edge| only.is_none_or(|from| edge.from == from)).collect();
        let point = |node: NodeId, point: Option<usize>| -> Option<String> {
            let held = view.node(node)?.shape()?;
            point.map(|point| shape::point_name(held.shape, point))
        };
        let rows: Vec<String> = found
            .iter()
            .map(|edge| {
                let at = |node, index| {
                    point(node, index).map(|name| format!(".{name}")).unwrap_or_default()
                };
                format!(
                    "{:<4} {:>4}{} -> {:<4}{}  {}",
                    edge.id,
                    edge.from,
                    at(edge.from, edge.from_point),
                    edge.to,
                    at(edge.to, edge.to_point),
                    edge.pipe.name()
                )
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
                    "fromPoint": point(edge.from, edge.from_point),
                    "toPoint": point(edge.to, edge.to_point),
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

    fn cli_realm_camera(&mut self, request: &Request) -> Outcome {
        let body = self.realm.body;
        if request.switch("fit") {
            let bounds = self.realm.realm.bounds();
            self.realm.realm.camera.fit(bounds, body.size(), 32.0);
        }
        let camera = self.realm.realm.camera;
        let at = Pos2::new(
            request.number("x").map(|x| x as f32).unwrap_or(camera.at.x),
            request.number("y").map(|y| y as f32).unwrap_or(camera.at.y),
        );
        let zoom = request.number("zoom").map(|zoom| zoom as f32).unwrap_or(camera.zoom);
        {
            let camera = &mut self.realm.realm.camera;
            camera.at = at;
            // Through `zoom_to` rather than by assignment, so the ladder's ends are kept in one place
            // and a caller that asked for ten gets 2.5 rather than a canvas nobody can read.
            camera.zoom_to(zoom, body.min, body.min);
            camera.at = at;
        }
        // **A command sets the camera outright**, glide and all: `realm camera --zoom 2` answers 2.00 on
        // the frame it lands, because a script that had to wait out an animation to read back what it just
        // set is a script with a race in it. The glide is the pointer's, the wheel's and the keys'.
        // `task-1945`.
        self.realm.glide = None;
        self.realm.realm.touch();
        let camera = self.realm.realm.camera;
        ok(
            request,
            format!(
                "The canvas is at {:.0},{:.0} at {:.2}x.",
                camera.at.x, camera.at.y, camera.zoom
            ),
            json!({ "x": camera.at.x, "y": camera.at.y, "zoom": camera.zoom }),
        )
    }

    fn cli_realm_send(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Terminal) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(text) = request.text("text") else {
            return no(request, code::USAGE, "Say what to type.");
        };
        if !self.realm.live.has_a_terminal(node) {
            return no(request, code::REFUSED, format!("Node {node} has no terminal running."));
        }
        // Remembered as something typed in, so its echo is not piped back out - the rule in
        // `services::realm::pipe`, which a send by hand needs exactly as much as a pipe does.
        self.realm.live.typed_into(node, text.trim_end());
        if let Some(session) = self.realm.live.terminal(node) {
            session.send(format!("{}\r", text.trim_end()).into_bytes());
        }
        done(request, format!("Typed into node {node}."))
    }

    /// What a terminal node is showing, scrollback and all.
    ///
    /// **The one thing an agent could not ask about.** `realm view` answers with a node's command, folder,
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
    fn cli_realm_chat(&mut self, request: &Request) -> Outcome {
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
        let Some(chat) = self.realm.live.chat_mut(node) else {
            return no(
                request,
                code::REFUSED,
                format!(
                    "Node {node} has not been drawn yet, so it has no conversation. Show the canvas and scroll to it - `realm focus {node}` does both."
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

    fn cli_realm_read(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Terminal) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        // Take in whatever the program has written since the last frame, so a read straight after a send is
        // not looking at the screen as it was before the command ran. `cli_terminal_read`'s own rule.
        self.realm.live.catch_up();
        let lines = request.whole("tail");
        let Some(session) = self.realm.live.terminal(node) else {
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
    fn cli_realm_font(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Terminal) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        if request.switch("reset") {
            self.realm.realm.change(node, |state| {
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
            self.realm.realm.change(node, |state| {
                if let State::Terminal(terminal) = state {
                    terminal.font_size = wanted;
                }
            });
        } else if request.switch("bigger") {
            self.step_a_node_font(node, 1);
        } else if request.switch("smaller") {
            self.step_a_node_font(node, -1);
        }
        let found = self.realm.realm.node(node).cloned();
        let size = found
            .as_ref()
            .map(|node| realm_view::font_size_of(node, self.settings.terminal_font_size))
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
    fn cli_realm_zoom(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        if let Some(from) = request.number("from").map(|id| id as u64) {
            if !self.realm.realm.may_reach(from, node) {
                return no(
                    request,
                    code::REFUSED,
                    format!("Node {from} is not connected to node {node}."),
                );
            }
        }
        let Some(found) = self.realm.realm.node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        if request.switch("reset") {
            self.realm.realm.change(node, |state| match state {
                State::Terminal(terminal) => terminal.font_size = 0.0,
                State::Editor(editor) => editor.font_size = 0.0,
                State::Note(note) => note.font_size = 0.0,
                State::Image(image) => image.zoom = 1.0,
                State::Shape(held) => held.size = shape::DEFAULT_SIZE,
                State::Folder(_) | State::Chat(_) | State::Tasks(_) => set_node_zoom(state, 1.0),
                State::Browser(_) | State::Audio(_) | State::Video(_) | State::Unknown(_) => {}
            });
            if found.kind() == Kind::Browser {
                self.realm.live.set_page_zoom(node, 1.0);
                if let Some(tab) = self.realm.live.browser(node).map(|tab| tab.id) {
                    let _ = self.browser.zoom(tab, 1.0);
                }
            }
        } else if let Some(asked) = request.number("factor") {
            let asked = asked as f32;
            if asked <= 0.0 {
                return no(request, code::USAGE, "A zoom is a number above zero.");
            }
            match found.kind() {
                Kind::Terminal | Kind::Editor | Kind::Note | Kind::Shape => {
                    if !(6.0..=96.0).contains(&asked) {
                        return no(request, code::USAGE, "A point size is 6 to 96.");
                    }
                    self.realm.realm.change(node, |state| match state {
                        State::Terminal(terminal) => terminal.font_size = asked,
                        State::Editor(editor) => editor.font_size = asked,
                        State::Note(note) => note.font_size = asked,
                        State::Shape(held) => held.size = asked,
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
                    self.realm.realm.change(node, |state| set_node_zoom(state, asked));
                }
                Kind::Image => {
                    if !(0.02..=40.0).contains(&asked) {
                        return no(request, code::USAGE, "A picture's zoom is 0.02 to 40.");
                    }
                    self.realm.realm.change(node, |state| {
                        if let State::Image(image) = state {
                            image.zoom = asked;
                            image.fit = crate::services::realm::Fit::Actual;
                        }
                    });
                }
                Kind::Audio | Kind::Video | Kind::Unknown => {
                    return no(request, code::REFUSED, "This node has nothing to zoom.");
                }
                Kind::Browser => {
                    if !(0.25..=4.0).contains(&asked) {
                        return no(request, code::USAGE, "A page's zoom is 0.25 to 4.");
                    }
                    self.realm.live.set_page_zoom(node, asked);
                    if let Some(tab) = self.realm.live.browser(node).map(|tab| tab.id) {
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
        let found = self.realm.realm.node(node).cloned();
        let (factor, own) = match found.as_ref().map(|node| &node.state) {
            Some(State::Terminal(terminal)) => (
                realm_view::font_size_of(
                    found.as_ref().expect("it is there"),
                    self.settings.terminal_font_size,
                ),
                terminal.font_size > 0.0,
            ),
            Some(State::Editor(editor)) => (
                realm_view::editor_font_size_of(
                    found.as_ref().expect("it is there"),
                    self.settings.font_size,
                ),
                editor.font_size > 0.0,
            ),
            Some(State::Note(note)) => (
                realm_view::editor_font_size_of(
                    found.as_ref().expect("it is there"),
                    self.settings.font_size,
                ),
                note.font_size > 0.0,
            ),
            Some(State::Image(image)) => (image.zoom, (image.zoom - 1.0).abs() > 0.001),
            Some(State::Shape(held)) => (held.size, (held.size - shape::DEFAULT_SIZE).abs() > 0.01),
            Some(State::Audio(_) | State::Video(_) | State::Unknown(_)) => (1.0, false),
            Some(State::Folder(_) | State::Chat(_) | State::Tasks(_)) => {
                let zoom = node_zoom_of(found.as_ref().expect("it is there"));
                (zoom, (zoom - 1.0).abs() > 0.001)
            }
            Some(State::Browser(_)) => {
                let zoom = self.realm.live.page_zoom_of(node);
                (zoom, (zoom - 1.0).abs() > 0.001)
            }
            None => (1.0, false),
        };
        let walks = match found.as_ref().map(|node| node.kind()) {
            Some(Kind::Terminal) => "terminal.font.size",
            Some(Kind::Editor | Kind::Note) => "appearance.font.size",
            Some(Kind::Image) => "the picture's own zoom",
            Some(Kind::Shape) => "the size of its words",
            Some(Kind::Audio | Kind::Video | Kind::Unknown) => "nothing",
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

    fn cli_realm_browser(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Browser) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(command) = request.text("command") else {
            return no(
                request,
                code::USAGE,
                "Say what to do: go, back, forward, reload, url, shot, pick, pin, unpin or pinned.",
            );
        };
        // Showing one element is part of what the realm is, so it is refused while the realm is read only.
        if matches!(command.trim(), "pin" | "unpin") {
            if let Some(why) = self.realm.realm.read_only_because() {
                return no(request, code::REFUSED, format!("This realm is read only. {why}"));
            }
        }
        match command.trim() {
            // **The element picker and the pin.** `task-2203`.
            "pick" => match self.pick_in_a_realm_browser(node, None) {
                Ok(()) => done(
                    request,
                    format!(
                        "The element picker is open in node {node}. Choose an element in the page, or use `pin --selector` to name one."
                    ),
                ),
                Err(problem) => no(request, code::REFUSED, problem),
            },
            "pin" => {
                let Some(selector) = request.text("selector") else {
                    return no(request, code::USAGE, "Say which element, with --selector <css>.");
                };
                match self.pin_a_realm_browser(node, &selector) {
                    Ok(()) => done(request, format!("Node {node} shows only {}.", selector.trim())),
                    Err(problem) => no(request, code::REFUSED, problem),
                }
            }
            "unpin" => match self.unpin_a_realm_browser(node) {
                Ok(()) => done(request, format!("Node {node} shows the whole page.")),
                Err(problem) => no(request, code::REFUSED, problem),
            },
            "pinned" => {
                let pin = match self.realm.realm.node(node).map(|found| &found.state) {
                    Some(State::Browser(browser)) => browser.pin.clone(),
                    _ => None,
                };
                let showing = self.a_nodes_pin(node).is_some();
                match pin {
                    Some(pin) => ok(
                        request,
                        format!(
                            "Node {node} shows only {} on {}{}.",
                            pin.selector,
                            pin.url,
                            if showing { "" } else { ", which is not the page it is on now" }
                        ),
                        json!({ "node": node, "selector": pin.selector, "url": pin.url, "showing": showing }),
                    ),
                    None => ok(
                        request,
                        format!("Node {node} shows the whole page."),
                        json!({ "node": node, "selector": null, "url": null, "showing": false }),
                    ),
                }
            }
            "go" => {
                let Some(url) = request.text("url") else {
                    return no(request, code::USAGE, "Say where to go, with --url.");
                };
                match self.send_a_realm_browser_to(node, url.trim()) {
                    Ok(()) => done(request, format!("Node {node} is going to {url}.")),
                    Err(problem) => no(request, code::FAILED, problem),
                }
            }
            step @ ("back" | "forward") => {
                let Some(tab) = self.realm.live.browser(node).map(|tab| tab.id) else {
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
                let Some(tab) = self.realm.live.browser(node).map(|tab| tab.id) else {
                    return no(request, code::REFUSED, format!("Node {node} has no page open."));
                };
                self.run_browser_command(tab, crate::services::browser::BrowserCommand::Reload);
                done(request, format!("Node {node} is reloading."))
            }
            "url" => match self.realm.live.browser(node) {
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
                let Some(found) = self.realm.realm.node(node).cloned() else {
                    return no(request, code::NOT_FOUND, format!("There is no node {node}."));
                };
                // The node has to be **showing** to be photographed, because a picture is of the
                // window as the operating system composited it and a native child view is part of
                // that rather than something Unluminous can render on its own.
                self.show_a_panel(dock::Panel::Realm, true);
                self.realm.realm.choose(Some(node));
                self.realm.realm.raise(node);
                let camera = self.realm.realm.camera;
                let area = camera.rect_to_screen(self.realm.body.min, found.rect());
                ctx.request_repaint();
                Outcome::Hold(crate::app::cli::Waiting::Screenshot {
                    path,
                    until: std::time::Instant::now() + std::time::Duration::from_secs(10),
                    settled: std::time::Instant::now() + std::time::Duration::from_millis(250),
                    asked: false,
                    crop: Some(area.intersect(self.realm.body)),
                })
            }
            other => no(
                request,
                code::USAGE,
                format!(
                    "A browser node does go, back, forward, reload, url, shot, pick, pin, unpin or pinned, not {other}."
                ),
            ),
        }
    }

    fn cli_realm_folder(&mut self, request: &Request) -> Outcome {
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
        let Some(found) = self.realm.realm.node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        self.make_sure_a_node_has_a_tree(&found);
        match command.trim() {
            open @ ("expand" | "collapse") => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which folder, with --path.");
                };
                let wanted = open == "expand";
                let Some(tree) = self.realm.live.tree_mut(node) else {
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
                self.realm.live.select_in_tree(node, Some(path.clone()));
                done(request, format!("Node {node} is on {}.", path.display()))
            }
            "open" => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which file, with --path.");
                };
                // **The same rule a double click keeps**: into a wired File Editor node when there is one,
                // and into the editing area when there is not. One function, so the pointer and the agent
                // cannot come to different answers. `task-1905`.
                let wired = self.realm.realm.reaches(node).into_iter().find(|other| {
                    self.realm.realm.node(*other).is_some_and(|found| found.kind() == Kind::Editor)
                });
                match wired {
                    Some(editor) => match self.open_in_a_realm_node(editor, &path) {
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
                let Some(tree) = self.realm.live.tree(node) else {
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

    /// The realm a command named, by its path in the project or by its name. The open one when it named none.
    ///
    /// The error is boxed because `Outcome` carries the whole of `Waiting`, which clippy flags as too
    /// large to return unboxed.
    fn a_named_realm(
        &mut self,
        request: &Request,
        key: &str,
    ) -> Result<std::path::PathBuf, Box<Outcome>> {
        let Some(name) = request.text(key) else {
            return Ok(self.realm.realm.path.clone());
        };
        self.list_the_realms();
        self.a_realm_called(&name).ok_or_else(|| {
            let names: Vec<String> = self
                .realm
                .files
                .iter()
                .map(|path| crate::services::realm::title_of(path))
                .collect();
            Box::new(no(
                request,
                code::NOT_FOUND,
                format!("There is no realm called {name}. This project has {}.", names.join(", ")),
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
                format!("Say which node, with `{argument}` and an id from `realm list`."),
            )));
        };
        match self.realm.realm.node(id).is_some() {
            true => Ok(id),
            false => Err(Box::new(no(
                request,
                code::NOT_FOUND,
                format!("There is no node {id} on {}.", self.realm.realm.name),
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
        let found = self.realm.realm.node(node).ok_or_else(|| {
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
        if self.realm.realm.node(from).is_none() {
            return Err(Box::new(no(
                request,
                code::NOT_FOUND,
                format!("There is no node {from}."),
            )));
        }
        if self.realm.realm.may_reach(from, node) {
            return Ok(node);
        }
        let reaches = self.realm.realm.reaches(from);
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

    /// `show`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_show(&mut self, request: &Request) -> Outcome {
        self.show_a_panel(dock::Panel::Realm, true);
        self.take_the_keyboard_for_the_realm();
        done(request, "The Realm is showing.")
    }

    /// `hide`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_hide(&mut self, request: &Request) -> Outcome {
        self.show_a_panel(dock::Panel::Realm, false);
        done(request, "Put the canvas away.")
    }

    /// `manage`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_manage(&mut self, request: &Request) -> Outcome {
        self.run_a_realm_action(RealmAction::Manage);
        done(request, "The realm manager is open.")
    }

    /// `play`, `pause`, `seek` and `volume`: the same player the node's own controls drive. `task-2202`.
    fn cli_realm_transport(
        &mut self,
        request: &Request,
        asked: crate::app::realm_nodes::Transport,
    ) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        if let Some(from) = request.number("from").map(|id| id as u64) {
            if !self.realm.realm.may_reach(from, node) {
                return no(
                    request,
                    code::REFUSED,
                    format!("Node {from} is not connected to node {node}."),
                );
            }
        }
        match self.drive_a_sound(node, asked) {
            Ok(state) => ok(
                request,
                format!(
                    "Node {node} is {} at {} of {}.",
                    if state.playing { "playing" } else { "paused" },
                    crate::app::realm_nodes::clock(state.position),
                    crate::app::realm_nodes::clock(state.duration),
                ),
                json!({
                    "node": node,
                    "playing": state.playing,
                    "position": state.position,
                    "duration": state.duration,
                    "volume": state.volume,
                }),
            ),
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    /// `note view`: show a note as its source, beside its preview, or as its preview. `task-2202`.
    fn cli_realm_note(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Note) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(view) = request
            .text("view")
            .and_then(|view| crate::services::realm::NoteView::from_name(&view))
        else {
            return no(request, code::USAGE, "Say raw, side or preview.");
        };
        match self.set_a_notes_view(node, view) {
            Ok(()) => ok(
                request,
                format!(
                    "Node {node} shows its {}.",
                    match view {
                        crate::services::realm::NoteView::Raw => "source",
                        crate::services::realm::NoteView::Side => "source beside its preview",
                        crate::services::realm::NoteView::Preview => "preview",
                    }
                ),
                json!({ "node": node, "view": view.name() }),
            ),
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    /// `open`: show a realm file in the panel. `task-2202`.
    fn cli_realm_open(&mut self, request: &Request) -> Outcome {
        let path = match self.a_named_realm(request, "realm") {
            Ok(path) => path,
            Err(outcome) => return *outcome,
        };
        self.show_a_panel(dock::Panel::Realm, true);
        match self.open_a_realm(&path) {
            Ok(()) => ok(
                request,
                format!("Showing {}.", crate::services::realm::slashed(&path)),
                json!({ "path": crate::services::realm::slashed(&path) }),
            ),
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    /// `new`: make a realm in `.realm-files/` and open it. `task-2202`.
    fn cli_realm_new(&mut self, request: &Request) -> Outcome {
        let name = request.text("name").unwrap_or_default();
        self.show_a_panel(dock::Panel::Realm, true);
        match self.new_realm(&name) {
            Ok(path) => ok(
                request,
                format!("Made {}.", crate::services::realm::slashed(&path)),
                json!({
                    "path": crate::services::realm::slashed(&path),
                    "name": crate::services::realm::title_of(&path),
                }),
            ),
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    /// `rename`: rename a realm's file, which its sidecar follows. `task-2202`.
    fn cli_realm_rename(&mut self, request: &Request) -> Outcome {
        let path = match self.a_named_realm(request, "realm") {
            Ok(path) => path,
            Err(outcome) => return *outcome,
        };
        let Some(name) = request.text("name") else {
            return no(request, code::USAGE, "Say what to call it.");
        };
        match self.rename_a_realm(&path, &name) {
            Ok(to) => ok(
                request,
                format!("It is {} now.", crate::services::realm::slashed(&to)),
                json!({ "path": crate::services::realm::slashed(&to) }),
            ),
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    /// `duplicate`: copy a realm under new ids and open the copy. `task-2202`.
    fn cli_realm_duplicate(&mut self, request: &Request) -> Outcome {
        let path = match self.a_named_realm(request, "realm") {
            Ok(path) => path,
            Err(outcome) => return *outcome,
        };
        match self.duplicate_a_realm(&path) {
            Ok(copy) => ok(
                request,
                format!("Copied it to {}.", crate::services::realm::slashed(&copy)),
                json!({ "path": crate::services::realm::slashed(&copy) }),
            ),
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    /// `delete`: delete a realm file the way the explorer deletes one, with its sidecar. `task-2202`.
    ///
    /// **No question is asked**, because the command is the answer to it: an agent that sends `realm delete`
    /// has said so, and the file goes wherever a deleted file goes on this platform rather than nowhere.
    fn cli_realm_delete(&mut self, request: &Request) -> Outcome {
        let path = match self.a_named_realm(request, "realm") {
            Ok(path) => path,
            Err(outcome) => return *outcome,
        };
        let file = self.tree.root().join(&path);
        if !file.is_file() {
            // A realm that was never written has nothing on disk to delete; forgetting it is the whole job.
            self.a_realm_file_is_going(&file);
            self.a_realm_file_went(&file);
            return done(request, "It had never been written, so there was nothing to delete.");
        }
        self.delete_path(&file);
        match file.exists() {
            true => no(request, code::FAILED, self.message.clone().unwrap_or_default()),
            false => done(request, format!("Deleted {}.", crate::services::realm::slashed(&path))),
        }
    }

    /// `close`: take a realm's tab off the bar, leaving its file where it is. `task-2293`.
    fn cli_realm_close(&mut self, request: &Request) -> Outcome {
        let path = match self.a_named_realm(request, "realm") {
            Ok(path) => path,
            Err(outcome) => return *outcome,
        };
        match self.close_a_realm_tab(&path) {
            Ok(()) => ok(
                request,
                format!("Closed the tab of {}.", crate::services::realm::slashed(&path)),
                json!({
                    "closed": crate::services::realm::slashed(&path),
                    "current": crate::services::realm::slashed(&self.realm.realm.path),
                    "tabs": self.open_realm_tabs().iter().map(|tab| crate::services::realm::slashed(tab)).collect::<Vec<_>>(),
                }),
            ),
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    /// `files`: every realm file newest first, which is what the bar's folder button lists, and `--show` or
    /// `--hide` for the list itself. `task-2293`.
    fn cli_realm_files(&mut self, request: &Request) -> Outcome {
        self.list_the_realms();
        if request.switch("show") {
            self.show_a_panel(dock::Panel::Realm, true);
            self.realm.recent = Some(crate::components::realm::recent::State::default());
        }
        if request.switch("hide") {
            self.realm.recent = None;
        }
        let now = std::time::SystemTime::now();
        let rows = self.rows_for_the_realm_files();
        let lines_out: Vec<String> = rows
            .iter()
            .map(|row| {
                let when = row
                    .written
                    .map(|written| {
                        crate::components::realm::recent::ago(
                            now.duration_since(written).unwrap_or_default(),
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "{}{:<40} {:<14} {}",
                    if row.showing {
                        "*"
                    } else if row.tab {
                        "+"
                    } else {
                        " "
                    },
                    crate::services::realm::slashed(&row.id),
                    when,
                    if row.tab { "tab" } else { "" },
                )
            })
            .collect();
        let realms: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "path": crate::services::realm::slashed(&row.id),
                    "name": row.name,
                    "nodes": row.nodes,
                    "tab": row.tab,
                    "showing": row.showing,
                    "written": row.written.and_then(|written| written.duration_since(std::time::UNIX_EPOCH).ok()).map(|since| since.as_secs()),
                })
            })
            .collect();
        lines(
            request,
            format!(
                "{} realm file{}, newest first",
                realms.len(),
                if realms.len() == 1 { "" } else { "s" }
            ),
            lines_out,
            json!({ "realms": realms, "listShowing": self.realm.recent.is_some() }),
        )
    }

    /// `import`: turn `.unluminous/space.conf` into realm files, writing over nothing. `task-2202`.
    fn cli_realm_import(&mut self, request: &Request) -> Outcome {
        let root = self.tree.root().to_path_buf();
        match crate::services::realm::store::import(&root) {
            Ok(imported) => {
                self.realm.imported = true;
                self.list_the_realms();
                self.the_project_changed_on_disk();
                let written: Vec<String> = imported
                    .written
                    .iter()
                    .map(|path| crate::services::realm::slashed(path))
                    .collect();
                let skipped: Vec<String> = imported
                    .skipped
                    .iter()
                    .map(|path| crate::services::realm::slashed(path))
                    .collect();
                let left_alone = match skipped.len() {
                    0 => String::new(),
                    many => format!(", and left {many} that were already there alone"),
                };
                ok(
                    request,
                    format!(
                        "Wrote {} realm file{}{left_alone}. space.conf is left where it is.",
                        written.len(),
                        if written.len() == 1 { "" } else { "s" },
                    ),
                    json!({
                        "written": written,
                        "skipped": skipped,
                        "current": imported.current.map(|path| crate::services::realm::slashed(&path)),
                    }),
                )
            }
            Err(problem) => no(request, code::NOT_FOUND, problem),
        }
    }

    /// `info`: what the open realm's file says about which Unluminous may read and write it. `task-2202`.
    ///
    /// This is how an agent finds out that a realm is read only and why, which the banner tells a person.
    fn cli_realm_info(&self, request: &Request) -> Outcome {
        let realm = &self.realm.realm;
        let access = match (&self.realm.problem, realm.read_only_because()) {
            (Some(problem), _) => format!("unreadable: {problem}"),
            (None, Some(why)) => format!("read only: {why}"),
            (None, None) => "edit".to_owned(),
        };
        let unknown = realm.unknown_kinds();
        let rows = vec![
            format!("path      {}", crate::services::realm::slashed(&realm.path)),
            format!("format    {}", realm.format.format),
            format!("reader    {}", realm.format.reader),
            format!("writer    {}", realm.format.writer),
            format!("needs     {}", realm.format.needs.join(", ")),
            format!("access    {access}"),
            format!("nodes     {}", realm.nodes.len()),
            format!("unknown   {}", unknown.join(", ")),
        ];
        lines(
            request,
            format!("{} is {access}.", crate::services::realm::slashed(&realm.path)),
            rows,
            json!({
                "path": crate::services::realm::slashed(&realm.path),
                "format": realm.format.format,
                "reader": realm.format.reader,
                "writer": realm.format.writer,
                "needs": realm.format.needs,
                "access": access,
                "nodes": realm.nodes.len(),
                "connections": realm.edges.len(),
                "unknownKinds": unknown,
            }),
        )
    }

    /// `title`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_title(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let title = request.text("title").unwrap_or_default();
        self.realm.realm.title_node(node, title.trim());
        done(request, format!("Called node {node} {title}."))
    }

    /// `remove`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_remove(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        self.close_a_realm_node(node);
        done(request, format!("Took node {node} off the canvas."))
    }

    /// `focus`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_focus(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        self.show_a_panel(dock::Panel::Realm, true);
        self.realm.realm.choose(Some(node));
        self.take_the_keyboard_for_the_realm();
        done(request, format!("Node {node} has the keyboard."))
    }

    /// `arrange`: the node menu's `Arrange` rows, through the same `Realm::arrange`. `task-2200`.
    fn cli_realm_arrange(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let word = request.text("how").unwrap_or_default();
        let Some(how) = crate::services::realm::Arrange::from_name(&word) else {
            return no(
                request,
                code::USAGE,
                format!("`{word}` is not a way to arrange a node. Say front, forward, backward or back."),
            );
        };
        let moved = self.realm.realm.arrange(node, how);
        let order: Vec<u64> = self.realm.realm.nodes.iter().map(|node| node.id).collect();
        let said = match moved {
            true => format!("Node {node}: {}.", how.label()),
            false => format!("Node {node} is already there."),
        };
        ok(request, said, serde_json::json!({ "node": node, "moved": moved, "order": order }))
    }

    /// `disconnect`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_disconnect(&mut self, request: &Request) -> Outcome {
        let Some(edge) = request.number("connection").map(|id| id as u64) else {
            return no(request, code::USAGE, "Say which connection, by its id.");
        };
        match self.realm.realm.disconnect(edge) {
            true => done(request, format!("Took connection {edge} away.")),
            false => no(request, code::NOT_FOUND, format!("There is no connection {edge}.")),
        }
    }

    /// `restart`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_restart(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        // **`--running` is the other half of the row on the node's own menu**, which is the rule that a
        // thing done by hand and the same thing done by an agent are the same thing: it goes through
        // `start_what_a_node_was_running`, so the program is typed into the shell the node has rather
        // than replacing it. `task-1907`.
        if request.switch("running") {
            let was = self.realm.chosen();
            self.realm.realm.choose(Some(node));
            self.start_what_a_node_was_running();
            let answer = self.message.clone().unwrap_or_default();
            self.realm.realm.choose(was);
            let started = answer.starts_with("Started ");
            return match started {
                true => done(request, answer),
                false => no(request, code::REFUSED, answer),
            };
        }
        match self.start_a_realm_terminal(node, request.switch("resume")) {
            Ok(()) => done(request, format!("Started node {node} again.")),
            Err(problem) => no(request, code::FAILED, problem),
        }
    }

    /// `address`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_address(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Browser) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(url) = request.text("url") else {
            return no(request, code::USAGE, "Say which address.");
        };
        // Through the same function the field's own Enter reaches, which is `run_cli`'s rule.
        match self.send_a_realm_browser_to(node, url.trim()) {
            Ok(()) => {
                self.realm.realm.change(node, |state| {
                    if let State::Browser(browser) = state {
                        browser.typed = url.trim().to_owned();
                    }
                });
                done(request, format!("Node {node} is going to {}.", url.trim()))
            }
            Err(problem) => no(request, code::FAILED, problem),
        }
    }

    /// `editor`. Split out of [`Self::cli_realm`] by `task-1984` §3.6.
    fn cli_realm_editor(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Editor) {
            Ok(node) => node,
            Err(outcome) => return *outcome,
        };
        let Some(path) = self.cli_path_argument(request, "path") else {
            return no(request, code::USAGE, "Say which file.");
        };
        match self.open_in_a_realm_node(node, &path) {
            Ok(()) => ok(
                request,
                format!("Opened {} in node {node}.", path.display()),
                json!({ "node": node, "path": path.to_string_lossy() }),
            ),
            Err(problem) => no(request, code::FAILED, problem),
        }
    }
}
