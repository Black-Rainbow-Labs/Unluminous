//! The Skills Manager: agent skills from unluminous.com, the person's own, and every one installed on this
//! machine, installed into any coding agent's skills folder for one project or for every project.
//!
//! `tasks/task-2396-skills-manager-tdd.md` is the design. This is the provider behind `ui.provider = skills`: it
//! knows the skills ([`model::Shelf`]), changes the disk through [`ops`], fetches the site's catalogue on a thread
//! of its own, and answers `plugins run skills <verb>` ([`commands`]). `components::skills` draws it and changes
//! nothing itself, so a press and a command are the same call.

pub mod catalogue;
pub mod commands;
pub mod config;
pub mod fetch;
pub mod frontmatter;
pub mod harness;
pub mod model;
pub mod ops;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use catalogue::Catalogue;
pub use config::Configuration;
use harness::{Harness, Places, Scope};
use model::{Shelf, Skill, Source};

use crate::services::plugin_ui::{Answer, Context, Look, Request, UiProvider};

/// How often the skills folders are read again while the plugin is open, because an agent or a person may change
/// one behind it, and a library skill that was edited is copied again into every place it is installed.
pub const LOOK_AGAIN: Duration = Duration::from_secs(2);

/// How long a catalogue read from the site is trusted before showing the pane asks again.
const CATALOGUE_FRESH: Duration = Duration::from_secs(6 * 60 * 60);

/// Whether this window may read the site's catalogue by itself. Only the released binary says so
/// ([`allow_fetching`]); a test's window reads one only from `UNLUMINOUS_SKILLS_CATALOGUE`.
static MAY_FETCH: AtomicBool = AtomicBool::new(false);

/// Let this window read unluminous.com's skills catalogue when the pane is shown. `main` calls it.
pub fn allow_fetching() {
    MAY_FETCH.store(true, Ordering::Relaxed);
}

/// Which skills the pane lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    Installed,
    Mine,
    Updates,
}

impl Filter {
    pub const ALL: [Filter; 4] = [Filter::All, Filter::Installed, Filter::Mine, Filter::Updates];

    pub fn name(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Installed => "installed",
            Filter::Mine => "mine",
            Filter::Updates => "updates",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Installed => "Installed",
            Filter::Mine => "Mine",
            Filter::Updates => "Updates",
        }
    }

    pub fn parse(word: &str) -> Option<Filter> {
        Filter::ALL.into_iter().find(|filter| filter.name() == word.trim().to_lowercase())
    }

    /// Whether `skill` is listed.
    pub fn keeps(self, skill: &Skill) -> bool {
        match self {
            Filter::All => true,
            Filter::Installed => skill.is_installed(),
            Filter::Mine => matches!(skill.source, Source::Mine | Source::Found) || skill.edited,
            Filter::Updates => skill.update,
        }
    }
}

/// The dialogs the pane can have open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    /// Asking what a new skill is called and when it applies.
    NewSkill { name: String, description: String },
    /// Making sure before a skill is deleted.
    Delete { id: String },
}

/// What happens once a skill's files have been fetched.
#[derive(Debug, Clone, PartialEq)]
pub enum After {
    Install { harnesses: Vec<&'static str>, scope: Scope, force: bool },
    Update { discard: bool },
    Edit,
}

/// The Skills Manager.
pub struct Skills {
    open: bool,
    pub project: Option<PathBuf>,
    pub folder: Option<PathBuf>,
    pub places: Places,
    pub config: Configuration,
    pub bundled: Catalogue,
    pub remote: Option<Catalogue>,
    pub shelf: Shelf,
    looked: Option<Instant>,
    checked: Option<Instant>,
    /// The skill whose page the pane shows, or the grid.
    pub viewing: Option<String>,
    pub filter: Filter,
    pub search: String,
    /// Where the pane's buttons install, which starts as Settings says.
    pub scope: Scope,
    pub modal: Option<Modal>,
    /// The last thing a change said, shown under the pane's header.
    pub status: Option<String>,
    /// A fetch going on, in words.
    pub busy: Option<String>,
    outbox: Vec<Request>,
    fetcher: fetch::Fetcher,
    waiting: Vec<(String, After)>,
    /// A catalogue address a unit test gives, in place of `UNLUMINOUS_SKILLS_CATALOGUE`.
    pub catalogue_here: Option<String>,
}

impl std::fmt::Debug for Skills {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Skills")
            .field("open", &self.open)
            .field("project", &self.project)
            .field("skills", &self.shelf.skills.len())
            .finish()
    }
}

impl Default for Skills {
    fn default() -> Self {
        Self::new()
    }
}

impl Skills {
    pub fn new() -> Self {
        let bundled = catalogue::bundled();
        Self {
            open: false,
            project: None,
            folder: None,
            places: Places::here(),
            config: Configuration::default(),
            bundled,
            remote: None,
            shelf: Shelf { skills: Vec::new() },
            looked: None,
            checked: None,
            viewing: None,
            filter: Filter::All,
            search: String::new(),
            scope: Scope::Project,
            modal: None,
            status: None,
            busy: None,
            outbox: Vec::new(),
            fetcher: fetch::Fetcher::default(),
            waiting: Vec::new(),
            catalogue_here: None,
        }
    }

    /// Where an install goes now: the chosen scope, or everyone when no project is open.
    pub fn scope_now(&self) -> Scope {
        match (self.scope, self.project.is_some()) {
            (Scope::Project, false) => Scope::Global,
            (scope, _) => scope,
        }
    }

    pub fn here(&self) -> ops::Here<'_> {
        ops::Here {
            places: &self.places,
            project: self.project.as_deref(),
            folder: self.folder.as_deref(),
        }
    }

    /// Read every skill from the disk again, and copy an edited library skill into the places it is installed.
    pub fn refresh(&mut self) {
        self.shelf = Shelf::read(&model::Sources {
            places: &self.places,
            project: self.project.as_deref(),
            library: self.folder.as_deref(),
            bundled: &self.bundled,
            remote: self.remote.as_ref(),
            extra: &self.config.folders,
        });
        let mut wrote = 0;
        for skill in &self.shelf.skills {
            wrote += ops::keep_in_step(skill).unwrap_or(0);
        }
        if wrote > 0 {
            self.shelf = Shelf::read(&model::Sources {
                places: &self.places,
                project: self.project.as_deref(),
                library: self.folder.as_deref(),
                bundled: &self.bundled,
                remote: self.remote.as_ref(),
                extra: &self.config.folders,
            });
        }
        self.looked = Some(Instant::now());
    }

    pub fn skill(&self, id: &str) -> Result<&Skill, String> {
        self.shelf
            .get(id)
            .ok_or_else(|| format!("There is no skill called {id}. `list` names them all."))
    }

    /// The skills the pane lists for the filter and the search, in their headings' order.
    pub fn listed(&self) -> Vec<&Skill> {
        let words: Vec<String> =
            self.search.to_lowercase().split_whitespace().map(str::to_owned).collect();
        self.shelf
            .skills
            .iter()
            .filter(|skill| self.filter.keeps(skill))
            .filter(|skill| words.iter().all(|word| text_of(skill).contains(word.as_str())))
            .collect()
    }

    /// The harnesses named, or the ones Settings chooses when the list is empty.
    pub fn harnesses_for(&self, named: &[String]) -> Result<Vec<&'static Harness>, String> {
        if named.iter().any(|word| word == "all") {
            return Ok(harness::HARNESSES.iter().collect());
        }
        if named.is_empty() {
            return Ok(self.config.chosen_harnesses(&self.places));
        }
        named
            .iter()
            .map(|word| {
                harness::find(word).ok_or_else(|| {
                    let known: Vec<&str> =
                        harness::HARNESSES.iter().map(|harness| harness.id).collect();
                    format!(
                        "{word} is not an agent Unluminous knows. It knows {}.",
                        known.join(", ")
                    )
                })
            })
            .collect()
    }

    /// Install a skill, fetching its files from the site first when the binary holds only part of it.
    pub fn install(
        &mut self,
        id: &str,
        harnesses: &[&'static Harness],
        scope: Scope,
        force: bool,
    ) -> Result<String, String> {
        let skill = self.skill(id)?.clone();
        if skill.partial && skill.library.is_none() {
            let after = After::Install {
                harnesses: harnesses.iter().map(|harness| harness.id).collect(),
                scope,
                force,
            };
            return self.fetch_the_files(&skill, after);
        }
        let report = ops::install(&self.here(), &skill, harnesses, scope, force)?;
        self.refresh();
        Ok(report.said())
    }

    pub fn remove(
        &mut self,
        id: &str,
        harnesses: &[&'static Harness],
        scope: Scope,
        force: bool,
    ) -> Result<String, String> {
        let skill = self.skill(id)?.clone();
        let report = ops::remove(&self.here(), &skill, harnesses, scope, force)?;
        self.refresh();
        Ok(report.said())
    }

    /// Open a skill's `SKILL.md` in a tab, writing what shipped into the library first.
    pub fn edit(&mut self, id: &str) -> Result<String, String> {
        let skill = self.skill(id)?.clone();
        if skill.partial && skill.library.is_none() {
            return self.fetch_the_files(&skill, After::Edit);
        }
        let file = ops::file_to_edit(&self.here(), &skill)?;
        self.outbox.push(Request::OpenFile(file.clone()));
        self.refresh();
        Ok(format!("Opened {}. Saving it updates every place {id} is installed.", file.display()))
    }

    pub fn new_skill(
        &mut self,
        name: &str,
        description: &str,
    ) -> Result<(String, PathBuf), String> {
        // What is on the disk now, not two seconds ago: a skill deleted behind the pane frees its name.
        self.refresh();
        let taken: Vec<String> = self.shelf.skills.iter().map(|skill| skill.id.clone()).collect();
        let file = ops::new_skill(&self.here(), name, description, &taken)?;
        self.outbox.push(Request::OpenFile(file.clone()));
        self.refresh();
        let id = frontmatter::tidy_name(name);
        self.viewing = Some(id.clone());
        Ok((id, file))
    }

    /// Take the site's newer files for a skill.
    pub fn update(&mut self, id: &str, discard: bool) -> Result<String, String> {
        let skill = self.skill(id)?.clone();
        if !skill.update {
            return Ok(format!("{id} is up to date."));
        }
        if skill.edited && !discard {
            return Err(format!(
                "{id} has edits of yours; `update {id} --discard` takes the new one."
            ));
        }
        self.fetch_the_files(&skill, After::Update { discard })
    }

    /// Ask for a skill's files from the site, and do `after` with them when they come.
    fn fetch_the_files(&mut self, skill: &Skill, after: After) -> Result<String, String> {
        let entry = self
            .remote
            .as_ref()
            .and_then(|remote| remote.entry(&skill.id).cloned())
            .or_else(|| (!skill.entry.files.is_empty()).then(|| skill.entry.clone()))
            .ok_or_else(|| format!("{} has to be fetched from unluminous.com, and the catalogue has not been read; run `check` first.", skill.id))?;
        let address = self
            .catalogue_address()
            .ok_or_else(|| "This window does not fetch from unluminous.com.".to_owned())?;
        let base = fetch::base_of(&address, self.remote.as_ref());
        self.fetcher.files(&base, entry, self.wake_for_the_fetcher());
        self.waiting.push((skill.id.clone(), after));
        self.busy = Some(format!("Fetching {} from unluminous.com", skill.id));
        Ok(format!(
            "Fetching {} from unluminous.com; `show {}` says when it is done.",
            skill.id, skill.id
        ))
    }

    /// Where the catalogue is read from: `UNLUMINOUS_SKILLS_CATALOGUE` in a test, Settings in the released binary,
    /// nowhere in a window that may not fetch.
    pub fn catalogue_address(&self) -> Option<String> {
        if let Some(named) = &self.catalogue_here {
            return Some(named.clone());
        }
        if let Some(named) =
            std::env::var_os("UNLUMINOUS_SKILLS_CATALOGUE").filter(|value| !value.is_empty())
        {
            return Some(named.to_string_lossy().into_owned());
        }
        MAY_FETCH.load(Ordering::Relaxed).then(|| self.config.catalogue.clone())
    }

    /// Ask the site for its catalogue now.
    pub fn check(&mut self) -> Result<String, String> {
        let address = self
            .catalogue_address()
            .ok_or_else(|| "This window does not fetch from unluminous.com.".to_owned())?;
        self.fetcher.catalogue(&address, self.wake_for_the_fetcher());
        self.checked = Some(Instant::now());
        self.busy = Some("Checking unluminous.com for new skills".into());
        Ok(format!("Checking {address}; `list updates` says what is new."))
    }

    /// Check when the pane is shown and the last check is old, which is the only time anything is fetched by
    /// itself.
    pub fn shown(&mut self) {
        let stale = self.checked.is_none_or(|at| at.elapsed() > CATALOGUE_FRESH);
        if stale && self.catalogue_address().is_some() {
            let _ = self.check();
        }
    }

    fn wake_for_the_fetcher(&self) -> Option<std::sync::Arc<dyn Fn() + Send + Sync>> {
        self.fetcher.wake.clone()
    }

    /// Take in what the fetcher brought back.
    fn take_what_was_fetched(&mut self) -> bool {
        let done = self.fetcher.take();
        if done.is_empty() {
            return false;
        }
        for result in done {
            match result {
                fetch::Done::Catalogue(Ok(catalogue)) => {
                    let updates = catalogue.skills.len();
                    self.remote = Some(catalogue);
                    self.busy = None;
                    self.refresh();
                    let new = self.shelf.skills.iter().filter(|skill| skill.update).count();
                    self.status = Some(match new {
                        0 => format!(
                            "unluminous.com lists {updates} skills; everything here is up to date."
                        ),
                        1 => "One skill has an update.".into(),
                        many => format!("{many} skills have updates."),
                    });
                }
                fetch::Done::Catalogue(Err(why)) => {
                    self.busy = None;
                    self.status = Some(format!("Could not read the skills catalogue: {why}"));
                }
                fetch::Done::Files { id, origin, files } => self.files_arrived(&id, &origin, files),
            }
        }
        true
    }

    /// A skill's files arrived: put them in the library and do what was waiting for them.
    fn files_arrived(&mut self, id: &str, origin: &str, files: Result<catalogue::Files, String>) {
        self.busy = None;
        let afters: Vec<After> = self
            .waiting
            .iter()
            .filter(|(waiting, _)| waiting == id)
            .map(|(_, after)| after.clone())
            .collect();
        self.waiting.retain(|(waiting, _)| waiting != id);
        let files = match files {
            Ok(files) => files,
            Err(why) => {
                self.status = Some(format!("Could not fetch {id}: {why}"));
                return;
            }
        };
        let discard = afters.iter().any(|after| matches!(after, After::Update { discard: true }));
        let said = match self.skill(id).cloned() {
            Ok(skill) => ops::take(&self.here(), &skill, &files, origin, discard),
            Err(why) => Err(why),
        };
        self.refresh();
        let mut lines = vec![said.unwrap_or_else(|why| why)];
        for after in afters {
            let result = match after {
                After::Install { harnesses, scope, force } => {
                    let harnesses: Vec<&'static Harness> =
                        harnesses.iter().filter_map(|id| harness::find(id)).collect();
                    self.install(id, &harnesses, scope, force)
                }
                After::Edit => self.edit(id),
                After::Update { .. } => continue,
            };
            lines.push(result.unwrap_or_else(|why| why));
        }
        self.status = Some(lines.join(" "));
    }

    /// The person's picture for a skill: from the binary, or fetched from the site into the plugin's folder.
    pub fn picture_bytes(&self, skill: &Skill) -> Option<Vec<u8>> {
        if let Some(bytes) = catalogue::bundled_file(&skill.entry.image) {
            return Some(bytes.to_vec());
        }
        let folder = self.folder.as_ref()?;
        std::fs::read(folder.join("images").join(format!("{}.jpg", skill.id))).ok()
    }

    /// Look at the disk again when it is time.
    fn look_again_when_due(&mut self) {
        if self.looked.is_none_or(|at| at.elapsed() >= LOOK_AGAIN) {
            self.refresh();
        }
    }

    /// Say something in the pane and the status bar.
    pub fn say(&mut self, words: impl Into<String>) {
        let words = words.into();
        self.status = Some(words.clone());
        self.outbox.push(Request::Message(words));
    }

    pub fn push(&mut self, request: Request) {
        self.outbox.push(request);
    }
}

/// The home folder a window with no settings folder installs into: one for each project, under the temporary
/// folder, so two test windows on two projects never see each other's installs.
pub fn home_for_a_window_with_no_settings(project: Option<&Path>) -> PathBuf {
    let name = project
        .map(|project| {
            catalogue::hash(&std::iter::once((project.display().to_string(), Vec::new())).collect())
        })
        .unwrap_or_default();
    std::env::temp_dir().join("unluminous-skills-home").join(&name[..name.len().min(16)])
}

/// Every word a skill can be searched by, lower case.
fn text_of(skill: &Skill) -> String {
    [
        skill.id.as_str(),
        skill.entry.name.as_str(),
        skill.entry.author.as_str(),
        skill.entry.summary.as_str(),
        skill.entry.category.as_str(),
        &skill.entry.tags.join(" "),
    ]
    .join(" ")
    .to_lowercase()
}

impl UiProvider for Skills {
    fn id(&self) -> &'static str {
        "skills"
    }

    fn open(&mut self, context: &Context) -> Result<(), String> {
        self.project = context.project.clone();
        self.folder = context.folder.clone();
        self.fetcher.wake = context.wake.clone();
        // A window with no settings folder is a test's, and must not read or write the person's own agents'
        // folders: it is given a home of its own under the temporary folder, which `harnesses` names.
        if context.folder.is_none() && std::env::var_os("UNLUMINOUS_HOME").is_none() {
            let home = home_for_a_window_with_no_settings(context.project.as_deref());
            self.places = Places::at(&home);
            self.folder = Some(home.join(".unluminous-skills"));
        }
        if let Some(folder) = &self.folder {
            let (config, refused) = Configuration::read(folder);
            self.config = config;
            if !refused.is_empty() {
                self.status = Some(refused.join(" "));
            }
        }
        self.scope = self.config.scope;
        self.open = true;
        self.refresh();
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn draws_chrome(&self) -> bool {
        true
    }

    fn pane(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        self.shown();
        crate::components::skills::show(self, ui, look)
    }

    fn settings(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::skills::settings_page(self, ui, look)
    }

    fn modal(&mut self, ctx: &egui::Context, look: &Look<'_>) -> (Vec<Request>, bool) {
        crate::components::skills::modal(self, ctx, look)
    }

    fn command(&mut self, command: &str, arguments: &[String]) -> Result<Answer, String> {
        commands::run(self, command, arguments)
    }

    fn commands(&self) -> Vec<(&'static str, &'static str)> {
        unluminous_cli::plugins::SKILLS.to_vec()
    }

    fn view(&self) -> serde_json::Value {
        commands::view_value(self)
    }

    fn badge(&self) -> Option<String> {
        match self.shelf.skills.iter().filter(|skill| skill.update).count() {
            0 => None,
            many => Some(many.to_string()),
        }
    }

    fn catch_up(&mut self) -> bool {
        let fetched = self.take_what_was_fetched();
        self.look_again_when_due();
        fetched
    }

    fn asking(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.outbox)
    }

    fn showing(&mut self, project: Option<&Path>, _file: Option<&Path>) {
        if let Some(project) = project {
            if self.project.as_deref() != Some(project) {
                self.project = Some(project.to_path_buf());
                self.refresh();
            }
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn close(&mut self) {
        self.open = false;
    }
}
