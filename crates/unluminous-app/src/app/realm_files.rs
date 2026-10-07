//! The window's side of a project's realm files: which ones there are, which one is open, and making,
//! renaming, copying and deleting them.
//!
//! `task-2202`, §5.7 of `tasks/task-2199-realm-tdd.md`. **One realm is open at a time**, in the panel.
//! Opening another writes the one that is open, then reads the next; a realm's nodes are brought to life by
//! `bring_the_current_view_to_life` on the next frame, exactly as a view used to be.
//!
//! **A realm file that moves or goes away is noticed wherever that happened**, because a person can rename
//! one in the explorer as easily as on the realm bar. `move_path` and `delete_path` both call in here, so the
//! sidecar follows its realm and an open realm that was deleted is replaced by another.
//!
//! **A window that was given no project to remember writes no realm file.** That is the window a test builds,
//! and the rule `restore_project` keeps for every other state file: a test must not write into a project it
//! shares with other tests. Such a window keeps the realms it has made or changed in memory instead
//! (`RealmState::unsaved`), so switching between them, renaming one and deleting one all behave exactly as
//! they do on disk.

use std::path::{Path, PathBuf};

use crate::app::UnluminousApp;
use crate::services::realm::{slashed, store, title_of, Realm, State};

/// How long the list of realm files is believed before it is walked again.
///
/// The list is a walk of the project, which is cheap but not free, and it changes when a person makes or
/// moves a realm. Every change made through Unluminous lists again at once; this bounds how long one made by
/// another program takes to appear.
const LIST_AGAIN_AFTER: std::time::Duration = std::time::Duration::from_secs(3);

impl UnluminousApp {
    /// Whether the Realm plugin is switched on. Off, the panel, its rail button, its menu and its commands
    /// are all absent, the way Agent-Tasks' pane is when that plugin is off. `task-2202`.
    pub fn realm_is_on(&self) -> bool {
        self.plugins.is_on("realm")
    }

    /// The sentence a command or a menu answers with while the Realm plugin is off.
    pub(crate) fn the_realm_is_off(&self) -> String {
        "The Realm plugin is switched off. `plugins enable realm` turns it on.".to_owned()
    }

    /// The project's realm files, walked now, with the open realm and any kept in memory among them.
    pub(crate) fn list_the_realms(&mut self) {
        let root = self.tree.root().to_path_buf();
        let mut files = store::list(&root);
        let more = std::iter::once(self.realm.realm.path.clone())
            .chain(self.realm.unsaved.keys().cloned());
        for path in more.collect::<Vec<_>>() {
            if !files.contains(&path) {
                files.push(path);
            }
        }
        files.sort_by_key(|path| slashed(path).to_lowercase());
        self.realm.counts = files
            .iter()
            .filter_map(|path| match self.realm.unsaved.get(path) {
                Some(kept) => Some((path.clone(), (kept.nodes.len(), kept.edges.len()))),
                None => Some((path.clone(), store::counts(&root, path)?)),
            })
            .collect();
        self.realm.files = files;
        self.realm.listed_at = Some(std::time::Instant::now());
    }

    /// Walk the project again when the list is older than [`LIST_AGAIN_AFTER`].
    pub(crate) fn keep_the_realm_list_current(&mut self) {
        let stale = self.realm.listed_at.map(|at| at.elapsed() > LIST_AGAIN_AFTER).unwrap_or(true);
        if stale {
            self.list_the_realms();
        }
    }

    /// The realms the bar draws: a path and a name each.
    pub(crate) fn realms_for_the_bar(&self) -> Vec<(PathBuf, String)> {
        self.realm.files.iter().map(|path| (path.clone(), title_of(path))).collect()
    }

    /// A realm named the way a person or an agent names one: its path in the project, or its name.
    pub(crate) fn a_realm_called(&self, said: &str) -> Option<PathBuf> {
        let said = said.trim().replace('\\', "/");
        let by_path = Path::new(&said);
        if let Some(found) = self.realm.files.iter().find(|path| path.as_path() == by_path) {
            return Some(found.clone());
        }
        if said.ends_with(".realm") && self.tree.root().join(by_path).is_file() {
            return Some(by_path.to_path_buf());
        }
        self.realm.files.iter().find(|path| title_of(path).eq_ignore_ascii_case(&said)).cloned()
    }

    /// Write the open realm now, if it needs writing and there is a project to write it in.
    pub(crate) fn write_the_open_realm(&mut self) -> Result<(), String> {
        if !self.remembers_this_project() || !self.realm.realm.is_dirty() {
            return Ok(());
        }
        let root = self.tree.root().to_path_buf();
        store::save(&root, &mut self.realm.realm)?;
        self.realm.realm.written();
        Ok(())
    }

    /// Read the realm at `path`: the copy kept in memory when there is one, or the file.
    fn read_a_realm(&mut self, path: &Path) -> Result<Realm, String> {
        if let Some(kept) = self.realm.unsaved.remove(path) {
            return Ok(kept);
        }
        store::load(self.tree.root(), path)
    }

    /// Show the realm at `path` in the panel, writing the one that is open first.
    ///
    /// Refused, with the reason, when the file cannot be read: the realm that was open stays open rather
    /// than a blank canvas being shown under a name it does not have.
    pub(crate) fn open_a_realm(&mut self, path: &Path) -> Result<(), String> {
        if self.realm.realm.path == path && self.realm.problem.is_none() {
            return Ok(());
        }
        let next = match self.read_a_realm(path) {
            Ok(next) => next,
            Err(problem) => {
                self.message = Some(problem.clone());
                return Err(problem);
            }
        };
        self.replace_the_open_realm(next);
        Ok(())
    }

    /// Put `next` in the panel in place of the realm that is open.
    fn replace_the_open_realm(&mut self, next: Realm) {
        self.leave_the_open_realm();
        self.realm.realm = next;
        self.realm.problem = None;
        self.realm.brought_to_life = None;
        self.realm.glide = None;
        self.list_the_realms();
    }

    /// Everything the open realm needs before another replaces it: where its nodes are written down, the
    /// realm and its sidecar written (or kept in memory, in a window with no project to write in), its
    /// players stopped, and the native web view let go.
    ///
    /// **Terminals keep running**, which is what switching views always did: a dev server in a node on one
    /// realm is still serving when somebody looks at another. Their sessions are keyed by node id, and a
    /// realm copied through Unluminous gets new ids, so the two cannot be confused.
    fn leave_the_open_realm(&mut self) {
        if self.realm.brought_to_life.as_deref() == Some(self.realm.realm.path.as_path()) {
            self.note_where_the_nodes_are_reading();
        }
        if let Err(problem) = self.write_the_open_realm() {
            self.message = Some(problem);
        }
        if !self.remembers_this_project() && self.realm.problem.is_none() {
            let leaving = self.realm.realm.clone();
            self.realm.unsaved.insert(leaving.path.clone(), leaving);
        }
        self.stop_the_players_of_the_open_realm();
        // The web view belongs to the chosen browser or video node, and the next realm has none of them.
        let pages: Vec<u64> = self
            .realm
            .realm
            .nodes
            .iter()
            .filter_map(|node| self.realm.live.browser(node.id).map(|tab| tab.id))
            .collect();
        for tab in pages {
            self.browser.close_tab(tab);
        }
        let nodes: Vec<u64> = self.realm.realm.nodes.iter().map(|node| node.id).collect();
        self.realm.live.forget_the_pages(&nodes);
        for node in &nodes {
            self.browser.media().forget(*node);
        }
    }

    /// A name no listed realm has: `stem`, or `stem 2`, `stem 3` and so on.
    fn an_unused_realm_name(&self, stem: &str) -> String {
        let taken = |name: &str| {
            self.realm.files.iter().any(|path| title_of(path).eq_ignore_ascii_case(name))
        };
        (1..)
            .map(|number| match number {
                1 => stem.to_owned(),
                more => format!("{stem} {more}"),
            })
            .find(|name| !taken(name))
            .expect("one of the names is free")
    }

    /// Make a realm called `name` in `.realm-files/`, and open it.
    pub(crate) fn new_realm(&mut self, name: &str) -> Result<PathBuf, String> {
        self.list_the_realms();
        let name = match name.trim() {
            "" => self.an_unused_realm_name("Realm"),
            given => given.to_owned(),
        };
        let root = self.tree.root().to_path_buf();
        if !self.remembers_this_project() {
            let path = Path::new(store::FOLDER).join(format!("{name}.{}", store::EXTENSION));
            if self.realm.files.contains(&path) {
                return Err(format!("There is a realm called {name} already."));
            }
            let mut made = Realm::new(&path, &name);
            made.touch();
            self.replace_the_open_realm(made);
            return Ok(path);
        }
        let path = store::create(&root, &name)?;
        self.the_project_changed_on_disk();
        self.open_a_realm(&path)?;
        Ok(path)
    }

    /// Rename the realm at `path` to `name`, through `move_path`, so its sidecar and the open realm follow.
    pub(crate) fn rename_a_realm(&mut self, path: &Path, name: &str) -> Result<PathBuf, String> {
        let name = name.trim();
        if name.is_empty() || name.contains(['/', '\\', ':']) {
            return Err(format!("{name:?} cannot be a realm's name."));
        }
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let to = folder.join(format!("{name}.{}", store::EXTENSION));
        if to == path {
            return Ok(to);
        }
        if self.realm.files.contains(&to) {
            return Err(format!("There is a realm called {name} already."));
        }
        if !self.remembers_this_project() {
            if self.realm.realm.path == path {
                self.realm.realm.path = to.clone();
                self.realm.realm.name = name.to_owned();
                self.realm.brought_to_life = Some(to.clone());
            } else if let Some(mut kept) = self.realm.unsaved.remove(path) {
                kept.path = to.clone();
                kept.name = name.to_owned();
                self.realm.unsaved.insert(to.clone(), kept);
            }
            self.list_the_realms();
            return Ok(to);
        }
        let root = self.tree.root().to_path_buf();
        // Written first, so what moves is what is on the screen.
        if self.realm.realm.path == path {
            self.write_the_open_realm()?;
        }
        if !self.move_path(&root.join(path), &root.join(&to), false) {
            return Err(self
                .message
                .clone()
                .unwrap_or_else(|| "The realm could not be renamed.".to_owned()));
        }
        // `realm.name` follows the file, so a realm read on its own still says what it is.
        if self.realm.realm.path == to {
            self.realm.realm.name = name.to_owned();
            self.realm.realm.touch();
            self.write_the_open_realm()?;
        } else if let Ok(mut moved) = store::load(&root, &to) {
            if moved.editable() {
                moved.name = name.to_owned();
                moved.touch();
                store::save(&root, &mut moved)?;
            }
        }
        self.list_the_realms();
        Ok(to)
    }

    /// Copy the realm at `path` under new ids to `<name> copy.realm`, and open the copy.
    pub(crate) fn duplicate_a_realm(&mut self, path: &Path) -> Result<PathBuf, String> {
        if self.realm.realm.path == path {
            self.note_where_the_nodes_are_reading();
            self.write_the_open_realm()?;
        }
        let source = match self.realm.realm.path == path {
            true => self.realm.realm.clone(),
            false => match self.realm.unsaved.get(path) {
                Some(kept) => kept.clone(),
                None => store::load(self.tree.root(), path)?,
            },
        };
        if !self.remembers_this_project() {
            let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let name = (1..)
                .map(|number| match number {
                    1 => format!("{} copy", source.title()),
                    more => format!("{} copy {more}", source.title()),
                })
                .find(|name| {
                    !self.realm.files.contains(&folder.join(format!("{name}.{}", store::EXTENSION)))
                })
                .expect("one of the names is free");
            let to = folder.join(format!("{name}.{}", store::EXTENSION));
            let copy = source.duplicated(&to, &name);
            self.replace_the_open_realm(copy);
            return Ok(to);
        }
        let root = self.tree.root().to_path_buf();
        let copy = store::duplicate(&root, &source)?;
        self.the_project_changed_on_disk();
        self.open_a_realm(&copy)?;
        Ok(copy)
    }

    /// Ask before deleting the realm at `path`. The answer goes through the explorer's own delete, which
    /// puts the file wherever a deleted file goes on this platform; [`Self::a_realm_file_went`] does the rest.
    pub(crate) fn ask_before_deleting_a_realm(&mut self, path: &Path) {
        if !self.remembers_this_project() || !self.tree.root().join(path).is_file() {
            // Nothing on disk to ask about: a realm kept in memory is forgotten.
            let file = self.tree.root().join(path);
            self.a_realm_file_is_going(&file);
            self.a_realm_file_went(&file);
            return;
        }
        let file = self.tree.root().join(path);
        self.ask_before_deleting(&file);
    }

    /// A path that is about to be deleted, from anywhere. Everything running behind the nodes of every realm
    /// under it is stopped first, which is what deleting a view always did: a terminal on a realm that no
    /// longer exists is a program nobody can reach.
    pub(crate) fn a_realm_file_is_going(&mut self, going: &Path) {
        let root = self.tree.root().to_path_buf();
        let relative = crate::services::project_state::relative(&root, going);
        let mut nodes: Vec<u64> = Vec::new();
        for path in self.realm.files.clone() {
            if !path.starts_with(&relative) {
                continue;
            }
            if self.realm.realm.path == path {
                nodes.extend(self.realm.realm.nodes.iter().map(|node| node.id));
            } else if let Some(kept) = self.realm.unsaved.get(&path) {
                nodes.extend(kept.nodes.iter().map(|node| node.id));
            } else if let Ok(read) = store::load(&root, &path) {
                nodes.extend(read.nodes.iter().map(|node| node.id));
            }
        }
        self.forget_these_nodes(&nodes);
    }

    /// Close every tab on these nodes and stop everything behind them.
    ///
    /// Highest tab index first, because closing a tab moves every later one down — see `close_a_realm_node`.
    pub(crate) fn forget_these_nodes(&mut self, nodes: &[u64]) {
        let mut on_them: Vec<usize> =
            nodes.iter().flat_map(|node| self.files.tabs_in_node(*node)).collect();
        on_them.sort_unstable_by(|left, right| right.cmp(left));
        for index in on_them {
            self.close_tab(index);
        }
        for node in nodes {
            if let Some(tab) = self.realm.live.browser(*node).map(|tab| tab.id) {
                self.browser.close_tab(tab);
            }
        }
        self.realm.live.forget_all(nodes);
    }

    /// A path that was deleted, from anywhere. When it was a realm, or held one, its sidecar goes with it,
    /// and the open realm is replaced when it was among them.
    pub(crate) fn a_realm_file_went(&mut self, gone: &Path) {
        let root = self.tree.root().to_path_buf();
        let relative = crate::services::project_state::relative(&root, gone);
        self.close_the_realm_tabs_under(&relative);
        let was_open = self.realm.realm.path.starts_with(&relative);
        for path in self.realm.files.clone() {
            if path.starts_with(&relative) {
                self.realm.unsaved.remove(&path);
                if self.remembers_this_project() {
                    let _ = std::fs::remove_file(store::sidecar_path(&root, &path));
                }
            }
        }
        if was_open {
            // Nothing left to write: the file is gone, and writing it would put it back.
            self.realm.realm.written();
            let gone_path = self.realm.realm.path.clone();
            self.realm.files.retain(|path| *path != gone_path);
            self.list_the_realms();
            self.realm.files.retain(|path| *path != gone_path);
            let next = self.realm.files.first().cloned();
            let opened = next.map(|next| self.read_a_realm(&next));
            let next = match opened {
                Some(Ok(next)) => next,
                _ => Realm::default(),
            };
            // The deleted realm is not stashed or written on the way out: it is gone.
            self.realm.realm = next;
            self.realm.problem = None;
            self.realm.brought_to_life = None;
        }
        self.list_the_realms();
    }

    /// A path that moved, from anywhere: a realm's sidecar follows it, the open realm's path follows it,
    /// and every node on the open realm that names a file that moved names it where it is now.
    pub(crate) fn a_realm_file_moved(&mut self, from: &Path, to: &Path) {
        let root = self.tree.root().to_path_buf();
        let (from_relative, to_relative) = (
            crate::services::project_state::relative(&root, from),
            crate::services::project_state::relative(&root, to),
        );
        self.move_the_realm_tabs(&from_relative, &to_relative);
        for path in self.realm.files.clone() {
            if let Ok(rest) = path.strip_prefix(&from_relative) {
                let moved = to_relative.join(rest);
                store::move_the_sidecar(&root, &path, &moved);
                if self.realm.realm.path == path {
                    self.realm.realm.path = moved.clone();
                    self.realm.brought_to_life = Some(moved);
                }
            }
        }
        let renamed: Vec<(u64, PathBuf)> = self
            .realm
            .realm
            .nodes
            .iter()
            .filter_map(|node| {
                let file = node.state.file()?;
                let rest = file.strip_prefix(from).ok()?;
                Some((node.id, to.join(rest)))
            })
            .collect();
        for (node, file) in renamed {
            self.realm.realm.change(node, |state| state.set_file(file));
        }
        self.list_the_realms();
    }

    /// Read the realm a project was left on, making the realm files out of `space.conf` the first time.
    ///
    /// Called from `restore_project`, which only the released binary and a test that asks for it call, so a
    /// window nobody gave a project neither reads nor writes one.
    pub(crate) fn restore_the_realm(&mut self) {
        let root = self.tree.root().to_path_buf();
        let state = crate::services::project_state::load(&root);
        let mut current = state.realm_current.clone();
        self.realm.imported = state.realm_imported;
        if !self.realm.imported {
            let legacy = crate::services::project_state::folder(&root).join(store::LEGACY_FILE);
            if legacy.is_file() {
                match store::import(&root) {
                    Ok(imported) => {
                        if current.is_none() {
                            current = imported.current.clone();
                        }
                        self.message = Some(format!(
                            "Made {} realm file{} from the canvas in .unluminous/space.conf, which is left where it is.",
                            imported.written.len(),
                            if imported.written.len() == 1 { "" } else { "s" },
                        ));
                    }
                    Err(problem) => self.message = Some(problem),
                }
            }
            self.realm.imported = true;
        }
        let files = store::list(&root);
        let chosen = current
            .filter(|path| root.join(path).is_file())
            .or_else(|| files.iter().find(|path| root.join(path).is_file()).cloned());
        self.realm.brought_to_life = None;
        self.realm.unsaved.clear();
        self.realm.problem = None;
        match chosen {
            Some(path) => match store::load(&root, &path) {
                Ok(realm) => self.realm.realm = realm,
                Err(problem) => {
                    // The realm keeps its path so the bar names it, and it is read only so nothing is written
                    // over the file that could not be read.
                    self.realm.realm = Realm::new(&path, &title_of(&path));
                    self.realm.realm.access =
                        crate::services::realm::Access::ReadOnly(problem.clone());
                    self.realm.problem = Some(problem);
                }
            },
            None => self.realm.realm = Realm::default(),
        }
        self.list_the_realms();
    }

    /// Write a realm that has never been written, the first time the panel shows it. §5.7: *"a project with
    /// no space.conf and no realm files gets .realm-files/main.realm created the first time the panel shows,
    /// so there is always a current realm."*
    pub(crate) fn make_sure_the_open_realm_is_on_disk(&mut self) {
        if !self.remembers_this_project()
            || self.realm.problem.is_some()
            || !self.realm.realm.editable()
        {
            return;
        }
        if self.tree.root().join(&self.realm.realm.path).exists() {
            return;
        }
        self.realm.realm.touch();
        match self.write_the_open_realm() {
            Ok(()) => {
                self.the_project_changed_on_disk();
                self.list_the_realms();
            }
            Err(problem) => self.message = Some(problem),
        }
    }

    /// The line across the top of the canvas saying why it cannot be changed, when it cannot.
    ///
    /// §5.2 rule 4: a realm a newer Unluminous wrote opens for reading only, and somebody dragging a node
    /// that will not move has to be told why. A realm whose file could not be read says that instead.
    pub(crate) fn show_the_realm_banner(&self, ui: &egui::Ui, body: egui::Rect) {
        use crate::theme::crisp::CrispPainter;
        let Some(why) = self.realm.problem.as_deref().or(self.realm.realm.read_only_because())
        else {
            return;
        };
        let strip = egui::Rect::from_min_size(body.min, egui::Vec2::new(body.width(), 24.0));
        let painter = ui.painter_at(body);
        painter.rect_filled(strip, 0.0, crate::theme::color::toolbar());
        painter.line_segment(
            [strip.left_bottom(), strip.right_bottom()],
            egui::Stroke::new(1.0, crate::theme::color::divider()),
        );
        painter.crisp_text(
            strip.left_center() + egui::vec2(10.0, 0.0),
            egui::Align2::LEFT_CENTER,
            why,
            egui::FontId::proportional(11.5),
            crate::theme::color::text_strong(),
        );
        let response = ui.interact(strip, ui.id().with("realm-banner"), egui::Sense::hover());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("Read only: {why}"))
        });
    }

    /// Pause every sound and video on the open realm. Nothing plays on a realm nobody is looking at.
    pub(crate) fn stop_the_players_of_the_open_realm(&mut self) {
        let playing: Vec<u64> = self
            .realm
            .realm
            .nodes
            .iter()
            .filter(|node| matches!(node.state, State::Audio(_) | State::Video(_)))
            .map(|node| node.id)
            .collect();
        for node in playing {
            self.realm.live.pause_a_player(node);
        }
    }
}

/// What a realm tab's key starts with. The rest is the realm's path, relative to the project and written
/// with `/`, so `open-files.txt`'s neighbour `plugin-tabs.txt` brings the tab back on the realm it showed.
pub(crate) const REALM_TAB: &str = "realm:";

/// The key of the tab that shows the realm at `path`.
pub(crate) fn realm_tab_key(path: &Path) -> String {
    format!("{REALM_TAB}{}", slashed(path))
}

/// The realm a tab's key names, when it is a realm tab's key.
pub(crate) fn realm_of_a_tab_key(key: &str) -> Option<PathBuf> {
    key.strip_prefix(REALM_TAB).map(PathBuf::from)
}

/// A realm drawn in a tab of the editing area, rather than in the panel.
///
/// **A realm tab is a plugin tab of the `realm` plugin**, so everything the window already refuses a tab a
/// plugin draws is refused it too: it is not saved, it has no gutter, no preview, no text tools and no git.
/// Its key names the realm, so it comes back on the realm it showed. One realm is open at a time, so the tab
/// draws the open realm and opens its own when it is shown, and while a realm tab is showing the panel says
/// so rather than drawing the same nodes a second time.
impl UnluminousApp {
    /// Open the realm at `path` in a tab of the editing area, or show the tab already open on it.
    pub(crate) fn open_a_realm_in_a_tab(&mut self, path: &Path) -> Result<(), String> {
        if !self.realm_is_on() {
            return Err(self.the_realm_is_off());
        }
        self.open_a_realm(path)?;
        let open = self.realm.realm.path.clone();
        self.files.open_plugin_tab(
            unluminous_core::Document::new(),
            crate::app::files::PluginTab {
                key: realm_tab_key(&open),
                plugin: "realm".to_owned(),
                label: title_of(&open),
            },
        );
        self.realm.tab_drawn = None;
        self.focus = crate::app::Focus::Editor;
        Ok(())
    }

    /// Whether a realm tab is the tab showing in one of the editing area's panes.
    pub(crate) fn a_realm_tab_is_showing(&self) -> bool {
        self.editor_visible
            && (0..self.files.pane_count()).any(|pane| {
                self.files.showing_in(pane).is_some_and(|index| {
                    self.files.at(index).plugin.as_ref().is_some_and(|tab| tab.plugin == "realm")
                })
            })
    }

    /// Whether the canvas is drawn this frame, in the panel or in a tab.
    pub(crate) fn the_canvas_is_drawn(&self) -> bool {
        self.realm.visible || self.a_realm_tab_is_showing()
    }

    /// Draw a realm tab into `area`.
    ///
    /// **The tab follows the canvas while it is the one showing, and the canvas follows the tab when it is
    /// shown.** A realm chosen on the realm bar inside the tab, or with `realm open`, is the tab's realm
    /// from then on, and the tab is renamed; a realm tab that has just been shown opens its own realm. Only
    /// one realm tab draws the canvas in a frame, because there is one canvas: a second one says where the
    /// realm is showing.
    pub(crate) fn show_a_realm_tab(
        &mut self,
        ui: &mut egui::Ui,
        area: egui::Rect,
        tab: &crate::app::files::PluginTab,
    ) -> bool {
        ui.painter().rect_filled(area, 0, crate::theme::color::editor());
        let Some(path) = realm_of_a_tab_key(&tab.key) else { return false };
        let pass = ui.ctx().cumulative_pass_nr();
        if self
            .realm
            .tab_drawn
            .as_ref()
            .is_some_and(|(key, drawn)| *drawn == pass && *key != tab.key)
        {
            self.say_in_a_realm_tab(ui, area, "This realm is showing in another pane.");
            return false;
        }
        let same_tab = self.realm.tab_drawn.as_ref().is_some_and(|(key, _)| *key == tab.key);
        if self.realm.realm.path != path && !same_tab {
            if let Err(problem) = self.open_a_realm(&path) {
                self.say_in_a_realm_tab(ui, area, &problem);
                return false;
            }
        }
        self.show_the_realm_canvas(ui, area);
        self.follow_the_canvas_in_the_tab(&tab.key);
        self.realm.tab_drawn = Some((realm_tab_key(&self.realm.realm.path), pass));
        false
    }

    /// Name the realm tab after the realm the canvas is on, when the two have come apart.
    fn follow_the_canvas_in_the_tab(&mut self, key: &str) {
        let open = self.realm.realm.path.clone();
        let wanted = realm_tab_key(&open);
        if key == wanted {
            return;
        }
        if let Some(index) = self.files.index_of_plugin_tab(key) {
            if let Some(tab) = self.files.at_mut(index).plugin.as_mut() {
                tab.key = wanted;
                tab.label = title_of(&open);
            }
        }
    }

    /// One sentence in the middle of a realm tab, for a tab that cannot draw the canvas.
    fn say_in_a_realm_tab(&self, ui: &egui::Ui, area: egui::Rect, said: &str) {
        crate::app::realm_nodes::say_in_a_node(ui, area, u64::MAX - 1, said);
    }

    /// Close every realm tab on a realm under `gone`, which was deleted.
    pub(crate) fn close_the_realm_tabs_under(&mut self, gone: &Path) {
        let going: Vec<usize> = (0..self.files.len())
            .filter(|index| {
                self.files.at(*index).plugin.as_ref().is_some_and(|tab| {
                    realm_of_a_tab_key(&tab.key).is_some_and(|path| path.starts_with(gone))
                })
            })
            .collect();
        for index in going.into_iter().rev() {
            self.close_tab(index);
        }
    }

    /// Point every realm tab on a realm under `from` at the same realm under `to`, which is where it moved.
    pub(crate) fn move_the_realm_tabs(&mut self, from: &Path, to: &Path) {
        for index in 0..self.files.len() {
            let Some(tab) = self.files.at_mut(index).plugin.as_mut() else { continue };
            let Some(path) = realm_of_a_tab_key(&tab.key) else { continue };
            if let Ok(rest) = path.strip_prefix(from) {
                let moved = to.join(rest);
                tab.key = realm_tab_key(&moved);
                tab.label = title_of(&moved);
            }
        }
    }
}
