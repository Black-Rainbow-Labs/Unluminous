//! Offering a newer Unluminous, and installing it when a person says so.
//!
//! `task-2063` asks for three things: a check once a day on its own, a toast with `Install & Restart`
//! and `Don't Ask Again` when it finds something, and `Check for Updates` able to install what it found.
//! The check is `services::update`, the download and the hand over are `services::update_install`, and
//! this is the window's half: when to ask, what to offer, and what each button does.

use crate::components::toast::{Act, Kind};
use crate::services::update::{self, Answer, Release};
use crate::services::update_install::{self, Install, Progress};

use crate::app::UnluminousApp;

/// How often the window reads `update-checked.txt` to see whether the daily check is due. The read is
/// one small file, and a minute is more than precise enough for a check that happens once a day.
const LOOK_EVERY: f64 = 60.0;

impl UnluminousApp {
    /// Start the daily check when it is due. Called once a frame; costs a comparison nearly always.
    ///
    /// Only with a settings folder, which is to say only in the released binary: a test has none, and a
    /// test must not reach the network.
    pub(crate) fn ask_on_a_schedule(&mut self, now: f64) {
        if self.settings.update_check != crate::settings::UpdateCheck::Daily {
            return;
        }
        if now < self.update_looked_at + LOOK_EVERY && self.update_looked_at > 0.0 {
            return;
        }
        self.update_looked_at = now.max(f64::MIN_POSITIVE);
        let Some(folder) = self.store.as_ref().map(|store| store.folder().to_path_buf()) else {
            return;
        };
        let seconds = update::now_seconds();
        if !update::is_due(update::last_checked(&folder), seconds) {
            return;
        }
        update::note_checked(&folder, seconds);
        self.check_for_updates_on_its_own();
    }

    /// A check nobody pressed anything for: the daily one, or `update.check = start`.
    ///
    /// It says nothing unless it finds a version that has not been declined, so a failure on a train
    /// with no connection is a line in the status bar rather than a notice somebody has to dismiss.
    pub(crate) fn check_for_updates_on_its_own(&mut self) {
        if self.update.as_ref().is_some_and(|check| check.is_asking()) {
            return;
        }
        self.update_asked_by_a_person = false;
        self.update = Some(crate::services::update::Check::start(self.thread_waker()));
    }

    /// What to do with an answer that has just arrived.
    pub(crate) fn offer_what_the_check_found(&mut self, answer: &Answer) {
        let asked = std::mem::replace(&mut self.update_asked_by_a_person, false);
        match answer {
            Answer::Newer(release) => {
                // `Don't Ask Again` is about the check that runs on its own. A person who asks wants
                // to know, whatever they declined before.
                if !asked && release.version == self.settings.update_skip {
                    return;
                }
                self.offer_the_update(release);
            }
            Answer::Current(_) if asked => self.toasts.say(answer.sentence(), Kind::Done),
            Answer::Failed(_) if asked => self.toasts.say(answer.sentence(), Kind::Problem),
            _ => {}
        }
    }

    /// Raise the notice with the two buttons.
    ///
    /// `Install & Restart` where there is something this Unluminous can install; `Open Download Page`
    /// where there is not, which is Linux, and a build run from `target/release` rather than installed.
    pub(crate) fn offer_the_update(&mut self, release: &Release) {
        let text = format!(
            "Unluminous {} is available. This is {}.{}",
            release.version,
            crate::build_info::VERSION,
            match release.notes.is_empty() {
                true => String::new(),
                false => format!(" {}", release.notes),
            }
        );
        let skip = ("Don't Ask Again".to_owned(), Act::SkipUpdate(release.version.clone()));
        let go = match (update::PLATFORM_FIELD, update_install::installed_at()) {
            (Some(_), Ok(_)) => {
                ("Install & Restart".to_owned(), Act::InstallUpdate(release.version.clone()))
            }
            _ => ("Open Download Page".to_owned(), Act::OpenPage(update::RELEASES_PAGE.to_owned())),
        };
        self.toasts.withdraw_the_offers();
        self.toasts.offer(text, vec![skip, go]);
    }

    /// Carry out a button on a notice.
    pub(crate) fn act_on_a_notice(&mut self, act: Act) {
        match act {
            Act::InstallUpdate(_) => {
                if let Err(problem) = self.install_the_update(true) {
                    self.toasts.say(problem, Kind::Problem);
                }
            }
            Act::SkipUpdate(version) => self.skip_the_update(&version),
            Act::OpenPage(url) => {
                if let Err(problem) = self.open_browser(&url) {
                    self.message = Some(problem);
                }
            }
        }
    }

    /// `Don't Ask Again`: the daily check stops offering `version`, and a later one is offered again.
    pub(crate) fn skip_the_update(&mut self, version: &str) {
        self.settings.update_skip = version.to_owned();
        self.unsaved_settings = true;
        self.toasts.withdraw_the_offers();
        self.message = Some(format!(
            "Unluminous {version} will not be offered again. A later version will be."
        ));
    }

    /// Download the newer version the last check found, check it, and when `restart` is true install
    /// it and start it again.
    ///
    /// Refused with a sentence when there is nothing to install: no check has found a newer version, or
    /// this Unluminous was not put where it is by an installer.
    pub(crate) fn install_the_update(&mut self, restart: bool) -> Result<String, String> {
        if let Some(install) = self.install.as_ref().filter(|install| install.is_running()) {
            return Ok(install.progress().sentence(&install.version));
        }
        let Some(Answer::Newer(release)) = self.update_answer.clone() else {
            return Err(
                "No newer Unluminous is known. Check for Updates first, from the Unluminous menu."
                    .to_owned(),
            );
        };
        if update::PLATFORM_FIELD.is_none() {
            return Err(format!(
                "There is no installer for this platform. Download it from {}",
                update::RELEASES_PAGE
            ));
        }
        if restart {
            update_install::installed_at()
                .map_err(|problem| format!("Cannot update in place: {problem}."))?;
        }
        self.toasts.withdraw_the_offers();
        let install =
            Install::start(release.version.clone(), release.download, restart, self.thread_waker());
        let said = install.progress().sentence(&install.version);
        self.message = Some(said.clone());
        self.install = Some(install);
        Ok(said)
    }

    /// Take in how far an install has got, and hand over once it is ready. Called once a frame.
    ///
    /// True when something changed and the window has to draw.
    pub(crate) fn take_the_install_progress(&mut self) -> bool {
        let Some(install) = self.install.as_mut() else { return false };
        let was = install.progress().clone();
        let now = install.poll().clone();
        if now == was {
            return false;
        }
        let version = install.version.clone();
        let restart = install.restart;
        self.message = Some(now.sentence(&version));
        match now {
            Progress::Failed(_) => self.toasts.say(now.sentence(&version), Kind::Problem),
            Progress::Ready { installer } if restart => self.hand_over_and_close(&installer),
            Progress::Ready { .. } => self.toasts.say(now.sentence(&version), Kind::Done),
            _ => {}
        }
        true
    }

    /// Start the helper and close the window the ordinary way, so what the project remembers is written.
    fn hand_over_and_close(&mut self, installer: &std::path::Path) {
        let handed = update_install::installed_at()
            .and_then(|relaunch| update_install::hand_over(installer, &relaunch));
        if let Err(problem) = handed {
            self.toasts.say(format!("Could not install the update: {problem}"), Kind::Problem);
            return;
        }
        if !self.may_the_window_close() {
            self.toasts.say(
                "The update is ready, but a file could not be saved, so the window stayed open. It installs when this window closes.",
                Kind::Problem,
            );
            return;
        }
        self.closing = true;
        self.write_settings();
        self.remember_the_project(None);
        if let Some(context) = &self.context {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// What `update status` answers with.
    pub(crate) fn update_status_value(&self) -> serde_json::Value {
        let answer = self.update_answer.as_ref().map(|answer| match answer {
            Answer::Newer(release) => serde_json::json!({
                "state": "newer",
                "version": release.version,
                "notes": release.notes,
                "installable": release.download.is_some(),
            }),
            Answer::Current(version) => {
                serde_json::json!({ "state": "current", "version": version })
            }
            Answer::Failed(problem) => serde_json::json!({ "state": "failed", "problem": problem }),
        });
        let install = self.install.as_ref().map(|install| {
            let progress = install.progress();
            serde_json::json!({
                "version": install.version,
                "state": progress.name(),
                "sentence": progress.sentence(&install.version),
                "restart": install.restart,
                "installer": match progress {
                    Progress::Ready { installer } => Some(installer.display().to_string()),
                    _ => None,
                },
            })
        });
        let offered: Vec<serde_json::Value> = self
            .toasts
            .notices()
            .iter()
            .filter(|notice| notice.kind == Kind::Offer)
            .map(|notice| {
                serde_json::json!({
                    "text": notice.text,
                    "buttons": notice.actions.iter().map(|(label, _)| label.clone()).collect::<Vec<_>>(),
                })
            })
            .collect();
        serde_json::json!({
            "version": crate::build_info::VERSION,
            "check": self.settings.update_check.name(),
            "skipped": match self.settings.update_skip.is_empty() {
                true => None,
                false => Some(self.settings.update_skip.clone()),
            },
            "asking": self.update.as_ref().is_some_and(|check| check.is_asking()),
            "answer": answer,
            "install": install,
            "offered": offered,
            "installedAt": update_install::installed_at().ok().map(|path| path.display().to_string()),
        })
    }
}
