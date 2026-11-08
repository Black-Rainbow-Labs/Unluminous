//! The board's supervisor: the agent that works the tickets, the skill file that says how, and what it runs on.
//!
//! `task-2253` made the supervisor: one agent, started in the board's project folder by the same `spawn` a ticket's
//! agent uses, keyed [`SUPERVISOR`] among the tickets' terminals. `task-2556` gave it two things a person can change:
//!
//! - **A skill file.** *"I want a tab to show an editable supervisor skill file so I can modify just like in file
//!   preview pane, and saving it immediately gets applied as the skill the supervisor uses."* The file is
//!   `<plugin folder>/supervisor/SKILL.md` ([`AgentTasks::supervisor_skill_path`]). Until somebody saves it, it is
//!   the `unluminous-supervisor` skill Unluminous ships ([`DEFAULT_SKILL`]). The supervisor is told to follow the
//!   file when it starts ([`agent::supervisor_handoff`]), and a save types [`agent::skill_changed`] into a running
//!   one, which is what "applied at once" means for an agent that already read the old version.
//! - **Its own endpoint, model and effort.** *"We also want the user to be able to configure which model is used
//!   for the supervisor."* `supervisor-endpoint`, `supervisor-model` and `supervisor-effort` in the board's
//!   `settings.conf`, each empty for what the supervisor ran on before it had a setting: the board's endpoint and
//!   that endpoint's model.
//!
//! `tasks/task-2556-supervisor-skill-and-model-tdd.md` is the design. The modal is
//! `components::agent_tasks::supervisor_modal`, and every control on it is a word of `supervisor` in
//! `commands.rs`.

use super::*;

/// The skill the supervisor follows until somebody saves one of their own: the `unluminous-supervisor` skill that
/// ships with Unluminous, which is the ai-service board's `/process-tasks` supervisor written for this board.
///
/// Compiled in rather than read from the Skills Manager's copy, because that copy is installed only when somebody
/// installs it, and the supervisor needs a skill whether or not anybody has.
pub const DEFAULT_SKILL: &str = include_str!("../../../skills/unluminous-supervisor/SKILL.md");

/// How often the skill file's modified time is looked at while the Skill view shows it with nothing unsaved.
const LOOK_AT_THE_FILE: std::time::Duration = std::time::Duration::from_secs(1);

/// The baseline with the line endings a text box keeps: `\n` only. A checkout made on Windows with
/// `core.autocrlf` compiles the file in with `\r\n`, and a draft compared against that would always be unsaved.
pub fn default_skill() -> String {
    DEFAULT_SKILL.replace("\r\n", "\n")
}

/// What the supervisor modal is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SupervisorView {
    /// The supervisor's terminal, the message box and `Send`.
    #[default]
    Terminal,
    /// The supervisor skill file, as source and as rendered Markdown.
    Skill,
    /// What the supervisor is started on: the gear in the modal's header.
    Settings,
}

impl SupervisorView {
    pub const ALL: [SupervisorView; 3] =
        [SupervisorView::Terminal, SupervisorView::Skill, SupervisorView::Settings];

    pub fn name(self) -> &'static str {
        match self {
            SupervisorView::Terminal => "terminal",
            SupervisorView::Skill => "skill",
            SupervisorView::Settings => "settings",
        }
    }

    pub fn parse(name: &str) -> Option<SupervisorView> {
        Self::ALL.into_iter().find(|view| view.name() == name.trim())
    }
}

/// How the Skill view lays the file out: the file preview pane's three arrangements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkillLayout {
    /// The source alone.
    Edit,
    /// The source and the rendered Markdown side by side.
    #[default]
    Split,
    /// The rendered Markdown alone.
    Preview,
}

impl SkillLayout {
    pub const ALL: [SkillLayout; 3] = [SkillLayout::Edit, SkillLayout::Split, SkillLayout::Preview];

    pub fn name(self) -> &'static str {
        match self {
            SkillLayout::Edit => "edit",
            SkillLayout::Split => "split",
            SkillLayout::Preview => "preview",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SkillLayout::Edit => "Edit",
            SkillLayout::Split => "Split",
            SkillLayout::Preview => "Preview",
        }
    }

    pub fn parse(name: &str) -> Option<SkillLayout> {
        Self::ALL.into_iter().find(|layout| layout.name() == name.trim().to_ascii_lowercase())
    }
}

/// What the Skill view holds between frames: the draft being typed, the text last saved, and what the last save
/// did.
///
/// The draft outlives the modal, so a person who closes the modal with a change not yet saved finds it there when
/// they open it again, with `Unsaved` beside the file's name.
#[derive(Debug, Default)]
pub struct SkillEditor {
    /// What is in the source box.
    pub draft: String,
    /// What the file held when it was last read or written.
    pub saved: String,
    /// Whether `draft` and `saved` have been read from the file yet.
    pub loaded: bool,
    pub layout: SkillLayout,
    /// What the last save, revert or reset did, and whether it worked.
    pub said: Option<(String, bool)>,
    /// How far the rendered Markdown is scrolled.
    pub preview_scroll: f32,
    /// The file's modified time when it was last read or written, and when it was last looked at.
    stamp: Option<std::time::SystemTime>,
    looked: Option<std::time::Instant>,
}

impl SkillEditor {
    /// Whether the draft differs from the file.
    pub fn unsaved(&self) -> bool {
        self.loaded && self.draft != self.saved
    }
}

/// The modified time of a file, or `None` when it is not there.
fn modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|metadata| metadata.modified()).ok()
}

impl AgentTasks {
    /// Where the board's supervisor skill file is: `supervisor/SKILL.md` in the plugin's own folder, beside
    /// `settings.conf`.
    ///
    /// A window with no settings folder, which is every window a test builds, keeps it under the temporary folder
    /// in a folder named for this process, so a test never writes into the settings of the person running it.
    pub fn supervisor_skill_path(&self) -> PathBuf {
        let folder = match &self.folder {
            Some(folder) => folder.clone(),
            None => {
                std::env::temp_dir().join(format!("unluminous-agent-tasks-{}", std::process::id()))
            }
        };
        folder.join("supervisor").join("SKILL.md")
    }

    /// What the supervisor skill says now: the file, or the shipped skill when nobody has saved one.
    pub fn read_the_supervisor_skill(&self) -> String {
        std::fs::read_to_string(self.supervisor_skill_path())
            .map(|text| text.replace("\r\n", "\n"))
            .unwrap_or_else(|_| default_skill())
    }

    /// Whether the supervisor follows the skill Unluminous ships, unchanged.
    pub fn supervisor_skill_is_the_default(&self) -> bool {
        self.read_the_supervisor_skill() == default_skill()
    }

    /// Read the skill into the Skill view the first time it is shown.
    pub fn load_the_supervisor_skill(&mut self) {
        if self.supervisor_skill.loaded {
            return;
        }
        let text = self.read_the_supervisor_skill();
        let path = self.supervisor_skill_path();
        let editor = &mut self.supervisor_skill;
        editor.draft = text.clone();
        editor.saved = text;
        editor.loaded = true;
        editor.stamp = modified(&path);
    }

    /// Read the file again when another program changed it and nothing in the draft would be lost.
    ///
    /// `Open in a tab` puts the file in an editor tab, and a file saved there is the skill too. The modified time is
    /// looked at once a second, and only while the draft holds nothing unsaved, so a change typed here is never
    /// replaced by the file underneath it.
    pub fn notice_the_supervisor_skill_file(&mut self) {
        self.load_the_supervisor_skill();
        let now = std::time::Instant::now();
        let due = self
            .supervisor_skill
            .looked
            .is_none_or(|looked| now.duration_since(looked) >= LOOK_AT_THE_FILE);
        if !due || self.supervisor_skill.unsaved() {
            return;
        }
        self.supervisor_skill.looked = Some(now);
        let path = self.supervisor_skill_path();
        let stamp = modified(&path);
        if stamp == self.supervisor_skill.stamp {
            return;
        }
        let text = self.read_the_supervisor_skill();
        let editor = &mut self.supervisor_skill;
        editor.draft = text.clone();
        editor.saved = text;
        editor.stamp = stamp;
        editor.said =
            Some(("read again: the file was changed outside this modal".to_owned(), true));
    }

    /// Write `text` as the supervisor skill and apply it: a running supervisor is told to read it again at once,
    /// and one that is not running reads it when it starts.
    ///
    /// What `Save & apply`, `Ctrl+S` in the source and `supervisor skill-set` do.
    pub fn save_the_supervisor_skill(&mut self, text: &str) -> Result<Answer, String> {
        let text = text.replace("\r\n", "\n");
        if text.trim().is_empty() {
            return Err(
                "the supervisor skill cannot be empty: Reset to default puts the shipped one back"
                    .to_owned(),
            );
        }
        let path = self.supervisor_skill_path();
        self.write_the_skill(&path, &text)?;
        let applied = self.tell_the_supervisor_the_skill_changed(&path);
        let message = match &applied {
            Some(Ok(())) => "saved, and the supervisor was told to read it again".to_owned(),
            Some(Err(problem)) => format!("saved, but the supervisor could not be told: {problem}"),
            None => "saved; the supervisor reads it when it starts".to_owned(),
        };
        let editor = &mut self.supervisor_skill;
        editor.draft = text.clone();
        editor.saved = text;
        editor.loaded = true;
        editor.stamp = modified(&path);
        editor.said = Some((message.clone(), !matches!(applied, Some(Err(_)))));
        Ok(Answer::said(message).with(json!({
            "path": path.display().to_string(),
            "applied": matches!(applied, Some(Ok(()))),
            "default": self.supervisor_skill.saved == default_skill(),
        })))
    }

    /// Save what is in the source box.
    pub fn save_the_supervisor_skill_draft(&mut self) -> Result<Answer, String> {
        self.load_the_supervisor_skill();
        let draft = self.supervisor_skill.draft.clone();
        self.save_the_supervisor_skill(&draft)
    }

    /// Put the source box back to what the file holds.
    pub fn revert_the_supervisor_skill_draft(&mut self) {
        self.load_the_supervisor_skill();
        let editor = &mut self.supervisor_skill;
        editor.draft = editor.saved.clone();
        editor.said = Some(("the unsaved changes were thrown away".to_owned(), true));
    }

    /// Put the shipped skill back, saved and applied.
    pub fn reset_the_supervisor_skill(&mut self) -> Result<Answer, String> {
        let answer = self.save_the_supervisor_skill(&default_skill())?;
        let message = answer.message.replacen("saved", "reset to the shipped skill", 1);
        self.supervisor_skill.said = Some((message.clone(), true));
        Ok(Answer { message, ..answer })
    }

    /// Write the skill file, making its folder.
    fn write_the_skill(&self, path: &std::path::Path, text: &str) -> Result<(), String> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)
                .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
        }
        crate::services::store::write_atomically(path, text.as_bytes())
            .map_err(|problem| format!("{} could not be written: {problem}", path.display()))
    }

    /// Type [`agent::skill_changed`] into the supervisor. `None` when it is not running.
    fn tell_the_supervisor_the_skill_changed(
        &mut self,
        path: &std::path::Path,
    ) -> Option<Result<(), String>> {
        if !self.supervisor_is_running() {
            return None;
        }
        Some(self.send_to_the_supervisor(&agent::skill_changed(path)).map(|_| ()))
    }

    /// The skill file the supervisor is told to follow, written with the shipped skill first when nobody has saved
    /// one, so the path in its handoff always names a file that is there.
    pub fn ensure_the_supervisor_skill_file(&self) -> Result<PathBuf, String> {
        let path = self.supervisor_skill_path();
        if !path.exists() {
            self.write_the_skill(&path, &default_skill())?;
        }
        Ok(path)
    }

    /// The kind and the endpoint the supervisor is started on: `supervisor-endpoint` when it names one that runs an
    /// agent, and otherwise the endpoint a new ticket is given to.
    pub fn supervisor_agent(&self) -> (Assignee, Option<String>) {
        if let Some(named) = self.configuration.supervisor_endpoint.as_deref() {
            if let Ok(found) = self.assignee_named(named) {
                if found.0.is_an_agent() {
                    return found;
                }
            }
        }
        self.default_assignee()
    }

    /// The models the supervisor's `Model` field offers: its agent's, and whatever the setting already names.
    pub fn supervisor_models(&self) -> Vec<String> {
        let (agent, _) = self.supervisor_agent();
        agent::models_for(agent, self.configuration.supervisor_model.as_deref())
    }

    /// Set the supervisor's endpoint by name, or back to the board's endpoint with `None`, `""` or `default`.
    pub fn set_the_supervisor_endpoint(&mut self, name: Option<&str>) -> Result<Answer, String> {
        let name = name.map(str::trim).filter(|name| !name.is_empty() && *name != "default");
        let mut changed = self.configuration.clone();
        changed.supervisor_endpoint = match name {
            None => None,
            Some(name) => {
                let (kind, endpoint) = self.assignee_named(name)?;
                if !kind.is_an_agent() {
                    return Err("the supervisor is an agent, so it cannot be a person".to_owned());
                }
                Some(endpoint.unwrap_or_else(|| kind.name().to_owned()))
            }
        };
        // A model chosen for the old agent is not a model the new one has.
        if changed.supervisor_endpoint != self.configuration.supervisor_endpoint {
            changed.supervisor_model = None;
        }
        self.change_the_configuration(changed)?;
        Ok(self.supervisor_setting_said("endpoint"))
    }

    /// Set the supervisor's model, or back to its endpoint's own with `None`, `""` or `default`.
    pub fn set_the_supervisor_model(&mut self, model: Option<&str>) -> Result<Answer, String> {
        let model = model.map(str::trim).filter(|model| !model.is_empty() && *model != "default");
        let mut changed = self.configuration.clone();
        changed.supervisor_model = model.map(str::to_owned);
        self.change_the_configuration(changed)?;
        Ok(self.supervisor_setting_said("model"))
    }

    /// Set the supervisor's effort, one of [`EFFORTS`], or back to the agent's own with `None`, `""` or `default`.
    pub fn set_the_supervisor_effort(&mut self, effort: Option<&str>) -> Result<Answer, String> {
        let effort =
            effort.map(str::trim).filter(|effort| !effort.is_empty() && *effort != "default");
        if let Some(level) = effort {
            if !EFFORTS.contains(&level) {
                return Err(format!("`{level}` is not an effort: {}", EFFORTS.join(", ")));
            }
        }
        let mut changed = self.configuration.clone();
        changed.supervisor_effort = effort.map(str::to_owned);
        self.change_the_configuration(changed)?;
        Ok(self.supervisor_setting_said("effort"))
    }

    /// What the supervisor is started on, as the command line reads it.
    pub fn supervisor_settings_json(&self) -> serde_json::Value {
        let (agent, endpoint) = self.supervisor_agent();
        json!({
            "endpoint": endpoint.clone().unwrap_or_else(|| agent.name().to_owned()),
            "endpointSetting": self.configuration.supervisor_endpoint,
            "agent": agent.name(),
            "model": self.configuration.supervisor_model,
            "effort": self.configuration.supervisor_effort,
        })
    }

    /// One setting, said after it was changed, with all three in the answer.
    fn supervisor_setting_said(&self, which: &str) -> Answer {
        let settings = self.supervisor_settings_json();
        let value = match which {
            "endpoint" => settings["endpoint"].as_str().map(str::to_owned),
            other => settings[other].as_str().map(str::to_owned),
        };
        let said = match (value, self.supervisor_is_running()) {
            (Some(value), true) => {
                format!("the supervisor's {which} is {value}; restart it to use it")
            }
            (Some(value), false) => format!("the supervisor's {which} is {value}"),
            (None, true) => {
                format!("the supervisor's {which} is its default; restart it to use it")
            }
            (None, false) => format!("the supervisor's {which} is its default"),
        };
        Answer::said(said).with(settings)
    }

    /// Start the board's supervisor: one agent that works the tickets the way its skill file says.
    ///
    /// `task-2253`: *"We are missing a chat with supervisor button on agent tasks. when I click that, a modal should
    /// open that has the terminal window, along with a message input and send button."* It is started in the board's
    /// project folder exactly the way a ticket's agent is ([`Self::spawn`]), on [`Self::supervisor_agent`] with the
    /// supervisor's own model and effort, and told the board's commands and its skill file by
    /// [`agent::supervisor_handoff`].
    pub fn start_the_supervisor(&mut self) -> Result<Answer, String> {
        if self.supervisor_is_running() {
            return Ok(Answer::said("the supervisor is already running"));
        }
        let (agent, endpoint) = self.supervisor_agent();
        if !agent.is_an_agent() {
            return Err(
                "the board's agent is a person, so there is no supervisor to start: choose an AI endpoint in Settings -> Agent-Tasks"
                    .to_owned(),
            );
        }
        let folder = self
            .configuration
            .project
            .clone()
            .or_else(|| self.project.clone())
            .ok_or("there is no project folder to start the supervisor in: open a folder, or set the board's project in Settings -> Agent-Tasks")?;
        let skill = self.ensure_the_supervisor_skill_file()?;
        let plan = self.supervisor_plan(agent, endpoint.as_deref());
        let launch = agent::launch(&plan)?;
        self.terminals.retain(|terminal| terminal.task_id != SUPERVISOR);
        self.spawn(
            SUPERVISOR,
            &plan.session,
            &launch,
            &folder,
            agent::supervisor_handoff(agent, &skill),
        )?;
        self.message = format!("the supervisor is {} in {}", agent.name(), folder.display());
        Ok(Answer::said(self.message.clone()).with(json!({
            "agent": agent.name(),
            "endpoint": endpoint,
            "model": plan.model,
            "effort": plan.effort,
            "session": plan.session,
            "folder": folder.display().to_string(),
            "skill": skill.display().to_string(),
            "command": launch.line(),
        })))
    }

    /// The plan the supervisor is launched with: its own model and effort, and the endpoint's program and model where
    /// it names none.
    fn supervisor_plan(&self, agent: Assignee, endpoint: Option<&str>) -> agent::Plan {
        let chosen = self.endpoint_for(agent, endpoint);
        let model = self.configuration.supervisor_model.clone().or_else(|| {
            chosen.as_ref().map(|one| one.model.trim().to_owned()).filter(|model| !model.is_empty())
        });
        agent::Plan {
            agent,
            session: new_session_id(),
            model,
            effort: self.configuration.supervisor_effort.clone(),
            resuming: false,
            command: self.configuration.command_for(agent),
            mode: chosen
                .as_ref()
                .map(|one| one.mode.trim().to_owned())
                .filter(|mode| !mode.is_empty()),
            extra: chosen.as_ref().map(|one| one.args.clone()).unwrap_or_default(),
            program: chosen
                .map(|one| one.command.trim().to_owned())
                .filter(|program| !program.is_empty()),
        }
    }

    /// Stop the supervisor and start it again, so a new endpoint, model or effort is used.
    pub fn restart_the_supervisor(&mut self) -> Result<Answer, String> {
        self.stop_the_supervisor();
        let answer = self.start_the_supervisor()?;
        let message = format!("restarted: {}", answer.message);
        Ok(Answer { message, ..answer })
    }

    /// Whether the supervisor's agent is running.
    pub fn supervisor_is_running(&self) -> bool {
        self.terminal_for(SUPERVISOR).is_some_and(|terminal| terminal.session.is_running())
    }

    /// Type a line into the supervisor, which is what the modal's `Send` does. Queued behind its prompt when it
    /// has only just started, the way a ticket's handoff is.
    pub fn send_to_the_supervisor(&mut self, line: &str) -> Result<Answer, String> {
        let line = line.trim();
        if line.is_empty() {
            return Err("there is nothing to send".to_owned());
        }
        let terminal = self
            .terminals
            .iter_mut()
            .find(|terminal| terminal.task_id == SUPERVISOR && terminal.session.is_running())
            .ok_or("the supervisor is not running: start it first")?;
        let queued = terminal.queue(line);
        Ok(Answer::said(match queued {
            true => "sent; the supervisor will read it once its prompt is ready",
            false => "sent to the supervisor",
        })
        .with(json!({ "queued": queued })))
    }

    /// Close the supervisor's terminal, which ends its agent. The tickets it started keep their own agents.
    pub fn stop_the_supervisor(&mut self) -> Answer {
        let had = self.terminals.iter().any(|terminal| terminal.task_id == SUPERVISOR);
        self.terminals.retain(|terminal| terminal.task_id != SUPERVISOR);
        Answer::said(match had {
            true => "the supervisor was stopped",
            false => "the supervisor was not running",
        })
    }

    /// What the supervisor has written, the last `lines` of it, for the command line.
    pub fn supervisor_screen(&self, lines: usize) -> Option<String> {
        let terminal = self.terminal_for(SUPERVISOR)?;
        let text = terminal.session.written_text(Some(lines));
        Some(text)
    }

    /// The modal's `Send`: what is in its box goes to the supervisor and the box is emptied.
    pub fn send_the_supervisor_draft(&mut self) -> Result<String, String> {
        let draft = self.supervisor_draft.clone();
        let answer = self.send_to_the_supervisor(&draft)?;
        self.supervisor_draft.clear();
        Ok(answer.message)
    }
}

#[cfg(test)]
mod tests_task_2556 {
    use super::*;

    /// A board whose plugin folder is a fresh temporary folder, so its skill file and its settings are its own.
    fn board_in(name: &str) -> (AgentTasks, PathBuf) {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-task-2556-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");
        let board = AgentTasks { folder: Some(folder.clone()), ..AgentTasks::default() };
        (board, folder)
    }

    /// Before anybody saves, the supervisor follows the skill Unluminous ships, and no file has been written.
    #[test]
    fn the_skill_is_the_shipped_one_until_somebody_saves() {
        let (board, folder) = board_in("baseline");
        assert_eq!(board.supervisor_skill_path(), folder.join("supervisor").join("SKILL.md"));
        assert!(!board.supervisor_skill_path().exists());
        assert_eq!(board.read_the_supervisor_skill(), default_skill());
        assert!(board.supervisor_skill_is_the_default());
        assert!(default_skill().contains("name: unluminous-supervisor"));
        assert!(!default_skill().contains('\r'));
    }

    /// A save writes the file, a second board on the same folder reads it, and reset puts the shipped skill back.
    #[test]
    fn a_saved_skill_is_the_file_and_reset_puts_the_shipped_one_back() {
        let (mut board, folder) = board_in("save");
        board.load_the_supervisor_skill();
        board.supervisor_skill.draft = "# Work at most 3 tickets at a time\n".to_owned();
        assert!(board.supervisor_skill.unsaved());
        let answer = board.save_the_supervisor_skill_draft().expect("saved");
        assert!(answer.message.contains("reads it when it starts"), "{}", answer.message);
        assert!(!board.supervisor_skill.unsaved());
        let again = AgentTasks { folder: Some(folder.clone()), ..AgentTasks::default() };
        assert_eq!(again.read_the_supervisor_skill(), "# Work at most 3 tickets at a time\n");
        assert!(!again.supervisor_skill_is_the_default());
        board.reset_the_supervisor_skill().expect("reset");
        assert!(board.supervisor_skill_is_the_default());
        assert_eq!(board.supervisor_skill.draft, default_skill());
        assert!(board.save_the_supervisor_skill("  \n").is_err(), "an empty skill is refused");
    }

    /// Revert throws the draft away and keeps the file.
    #[test]
    fn revert_puts_the_draft_back_to_the_file() {
        let (mut board, _) = board_in("revert");
        board.load_the_supervisor_skill();
        board.supervisor_skill.draft.push_str("\nOnly ever one ticket.\n");
        board.revert_the_supervisor_skill_draft();
        assert_eq!(board.supervisor_skill.draft, default_skill());
        assert!(!board.supervisor_skill_path().exists(), "revert writes nothing");
    }

    /// A file changed by another program replaces a draft with nothing unsaved in it, and never one that has.
    #[test]
    fn a_file_changed_elsewhere_is_read_again_unless_the_draft_has_changes() {
        let (mut board, _) = board_in("elsewhere");
        board.save_the_supervisor_skill("first\n").expect("saved");
        let path = board.supervisor_skill_path();
        std::fs::write(&path, "second\n").expect("written");
        // A modified time a second later, whatever the file system's resolution.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(later))
            .expect("touched");
        board.notice_the_supervisor_skill_file();
        assert_eq!(board.supervisor_skill.draft, "second\n");
        board.supervisor_skill.draft = "typed here\n".to_owned();
        std::fs::write(&path, "third\n").expect("written");
        board.supervisor_skill.looked = None;
        board.notice_the_supervisor_skill_file();
        assert_eq!(
            board.supervisor_skill.draft, "typed here\n",
            "an unsaved draft is never replaced"
        );
    }

    /// The handoff names the file and says the file wins; the line typed after a save names it too.
    #[test]
    fn the_supervisor_is_told_to_follow_the_file() {
        let path = std::path::Path::new("/board/supervisor/SKILL.md");
        let handoff = agent::supervisor_handoff(Assignee::Claude, path);
        assert!(handoff.contains("/board/supervisor/SKILL.md"), "{handoff}");
        assert!(handoff.contains("the file wins"), "{handoff}");
        assert!(
            !handoff.contains("one at a time"),
            "how many at once is the skill's to say: {handoff}"
        );
        assert!(!handoff.contains('\n'), "one line, because it is typed at a prompt");
        let changed = agent::skill_changed(path);
        assert!(
            changed.contains("/board/supervisor/SKILL.md") && changed.contains("Read it again")
        );
        assert!(agent::supervisor_handoff(Assignee::Human, path).is_empty());
    }

    /// The supervisor's endpoint, model and effort are written to `settings.conf` and read back, and an endpoint that
    /// is not set, or names a person, falls back to the board's own.
    #[test]
    fn the_supervisor_settings_round_trip_and_fall_back_to_the_board() {
        let (mut board, folder) = board_in("settings");
        assert_eq!(board.supervisor_agent(), board.default_assignee());
        board.set_the_supervisor_endpoint(Some("codex")).expect("an endpoint");
        board.set_the_supervisor_model(Some("gpt-5.5")).expect("a model");
        board.set_the_supervisor_effort(Some("high")).expect("an effort");
        assert!(board.set_the_supervisor_effort(Some("enormous")).is_err());
        assert!(board.set_the_supervisor_endpoint(Some("human")).is_err());
        let read = Configuration::read(&folder);
        assert_eq!(read.supervisor_endpoint.as_deref(), Some("codex"));
        assert_eq!(read.supervisor_model.as_deref(), Some("gpt-5.5"));
        assert_eq!(read.supervisor_effort.as_deref(), Some("high"));
        assert_eq!(board.supervisor_agent().0, Assignee::Codex);
        let plan = board.supervisor_plan(Assignee::Codex, None);
        assert_eq!(plan.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(plan.effort.as_deref(), Some("high"));
        // A new endpoint drops a model chosen for the old one.
        board.set_the_supervisor_endpoint(Some("default")).expect("back to the board's");
        assert_eq!(board.configuration.supervisor_model, None);
        assert_eq!(board.supervisor_agent(), board.default_assignee());
    }
}
