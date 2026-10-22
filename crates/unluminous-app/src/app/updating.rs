//! Offering a newer Unluminous, and installing it when a person says so.
//!
//! `task-2063` asks for three things: a check once a day on its own, a toast with `Install & Restart`
//! and `Don't Ask Again` when it finds something, and `Check for Updates` able to install what it found.
//! The check is `services::update`, the download and the hand over are `services::update_install`, and
//! this is the window's half: when to ask, what to offer, and what each button does.

use std::time::{Duration, Instant};

use crate::components::toast::{Act, Kind};
use crate::services::update::{self, Answer, Channel, Release};
use crate::services::update_install::{self, Install, Progress};

use crate::app::UnluminousApp;

/// How often the window reads `update-checked.txt` to see whether the daily check is due. The read is
/// one small file, and a minute is more than precise enough for a check that happens once a day.
const LOOK_EVERY: f64 = 60.0;

/// How long the channels' version lists are kept before opening the menu asks for them again.
/// `task-2328`. A release is cut at most every few minutes, and a menu opened twice in a minute should
/// not send two requests.
pub(crate) const LISTS_KEPT: Duration = Duration::from_secs(10 * 60);

/// The name of the submenu whose opening fetches the lists. `actions::unluminous_menu` draws it.
pub(crate) const UPDATES_SUBMENU: &str = "Check for Updates";

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
        self.update = Some(crate::services::update::Check::start(
            self.settings.update_channel,
            self.thread_waker(),
        ));
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
        // The version is the sentence under the title and the release notes are the details under it, which
        // the notice draws in a well of their own (`task-2313`).
        let text = format!(
            "Unluminous {} is ready to install. You have {}.{}",
            release.version,
            crate::build_info::VERSION,
            match release.notes.trim().is_empty() {
                true => String::new(),
                false => format!("\n{}", release.notes.trim()),
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
            Act::RunSetup => self.act_on_the_setup_notice(true),
            Act::DeclineSetup => self.act_on_the_setup_notice(false),
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
        // What a toast says is not said again in the status bar (`task-2313`), so the bar carries only the
        // steps along the way.
        match now {
            Progress::Failed(_) => self.toasts.say(now.sentence(&version), Kind::Problem),
            Progress::Ready { installer } if restart => self.hand_over_and_close(&installer),
            Progress::Ready { .. } => self.toasts.say(now.sentence(&version), Kind::Done),
            _ => self.message = Some(now.sentence(&version)),
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

    /// Fetch both channels' version lists, unless a fetch is already running. `task-2328`.
    pub(crate) fn fetch_the_version_lists(&mut self) {
        if self.update_listing.as_ref().is_some_and(|listing| listing.is_asking()) {
            return;
        }
        self.update_versions_asked_at = Some(Instant::now());
        self.update_listing = Some(update::Listing::start(update::TIMEOUT, self.thread_waker()));
    }

    /// Fetch the lists when the Check for Updates submenu is open and they are older than
    /// [`LISTS_KEPT`]. Called once a frame; costs a lookup in egui's memory nearly always.
    ///
    /// `controls::submenu` writes down which submenus it drew open, because the menu is drawn inside a
    /// popup and nothing else learns that it opened. The bar along the top of the screen on macOS is
    /// AppKit's and says nothing about a submenu opening, so there the lists come with every check and
    /// with `Load Versions`.
    pub(crate) fn fetch_the_lists_when_the_menu_opens(&mut self, ctx: &egui::Context) {
        let open = crate::components::controls::submenus_drawn_open(ctx);
        if !open.iter().any(|name| name == UPDATES_SUBMENU) {
            return;
        }
        let stale = self.update_versions_asked_at.is_none_or(|asked| asked.elapsed() > LISTS_KEPT);
        if stale {
            self.fetch_the_version_lists();
        }
    }

    /// Take in the lists, if they have arrived, and install a version that was asked for before they
    /// did. True when something changed and the window has to draw.
    pub(crate) fn take_the_version_lists(&mut self) -> bool {
        let Some(versions) = self.update_listing.as_mut().and_then(|listing| listing.poll()) else {
            return false;
        };
        self.update_versions = Some(versions);
        if let Some((channel, version)) = self.install_when_listed.take() {
            match self.install_a_version(channel, version, true) {
                Ok(said) => self.message = Some(said),
                Err(problem) => self.toasts.say(problem, Kind::Problem),
            }
        }
        true
    }

    /// Install a version from one channel and start it again: the menu's `Latest` and version rows, and
    /// `update install <version>`. `version` is `None` for the newest on the channel. `task-2328`.
    ///
    /// The release comes out of the channel's list, with its installer's size and SHA-256, so it is checked
    /// before anything runs exactly as `Install & Restart` is. A list that has not arrived yet is fetched
    /// and the install starts when it lands, so `Latest` works before the submenu has ever been opened.
    pub(crate) fn install_a_version(
        &mut self,
        channel: Channel,
        version: Option<String>,
        restart: bool,
    ) -> Result<String, String> {
        if let Some(install) = self.install.as_ref().filter(|install| install.is_running()) {
            return Ok(install.progress().sentence(&install.version));
        }
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
        let version = version.map(|named| named.trim().trim_start_matches('v').to_owned());
        let Some(versions) = self.update_versions.clone() else {
            return Ok(self.install_once_listed(channel, version));
        };
        let release = match &version {
            Some(named) => versions.find(channel, named).cloned(),
            None => versions.newest(channel).cloned(),
        };
        let Some(release) = release else {
            return Err(match (versions.of(channel), &version) {
                (Err(problem), _) => {
                    format!("Could not read the {} versions: {problem}", channel.name())
                }
                (Ok(_), Some(named)) => format!(
                    "Unluminous {named} is not one of the {} versions unluminous.com lists.",
                    channel.name()
                ),
                (Ok(_), None) => format!("unluminous.com lists no {} version.", channel.name()),
            });
        };
        if restart && release.version == crate::build_info::VERSION {
            return Err(format!("Unluminous {} is the version running now.", release.version));
        }
        let Some(download) = release.download.clone() else {
            return Err(format!(
                "unluminous.com names no installer for this platform in {}.",
                release.version
            ));
        };
        self.toasts.withdraw_the_offers();
        let install =
            Install::start(release.version.clone(), Some(download), restart, self.thread_waker());
        let said = install.progress().sentence(&install.version);
        self.message = Some(said.clone());
        self.install = Some(install);
        Ok(said)
    }

    /// Remember a version to install, and fetch the lists it is in.
    fn install_once_listed(&mut self, channel: Channel, version: Option<String>) -> String {
        let what = match &version {
            Some(named) => format!("Unluminous {named}"),
            None => "the newest version".to_owned(),
        };
        let said = format!("Finding {what} on the {} channel...", channel.name());
        self.install_when_listed = Some((channel, version));
        self.fetch_the_version_lists();
        self.message = Some(said.clone());
        said
    }

    /// What the menu shows for each channel: the versions, or why there are none yet.
    pub(crate) fn update_menu(&self) -> crate::app::actions::UpdateMenu {
        use crate::app::actions::ChannelRows;
        let loading = self.update_listing.as_ref().is_some_and(|listing| listing.is_asking());
        let rows = |channel: Channel| match (&self.update_versions, loading) {
            (Some(versions), _) => match versions.of(channel) {
                Ok(found) => ChannelRows::Listed(
                    found
                        .iter()
                        .take(update::SHOWN)
                        .map(|release| release.version.clone())
                        .collect(),
                ),
                Err(problem) => ChannelRows::Failed(problem.clone()),
            },
            (None, true) => ChannelRows::Loading,
            (None, false) => ChannelRows::NotLoaded,
        };
        crate::app::actions::UpdateMenu {
            stable: rows(Channel::Stable),
            nightly: rows(Channel::Nightly),
            following: self.settings.update_channel,
        }
    }

    /// The versions both channels list, for `update versions` and `update status`.
    pub(crate) fn update_versions_value(&self) -> serde_json::Value {
        let listed = |channel: Channel| match self.update_versions.as_ref().map(|v| v.of(channel)) {
            Some(Ok(found)) => serde_json::json!({
                "versions": found.iter().take(update::SHOWN).map(|release| serde_json::json!({
                    "version": release.version,
                    "notes": release.notes,
                    "installable": release.download.is_some(),
                    "running": release.version == crate::build_info::VERSION,
                })).collect::<Vec<_>>(),
            }),
            Some(Err(problem)) => serde_json::json!({ "problem": problem }),
            None => serde_json::json!({ "versions": null }),
        };
        serde_json::json!({
            "running": crate::build_info::VERSION,
            "following": self.settings.update_channel.name(),
            "loading": self.update_listing.as_ref().is_some_and(|listing| listing.is_asking()),
            "stable": listed(Channel::Stable),
            "nightly": listed(Channel::Nightly),
        })
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
            "channel": self.settings.update_channel.name(),
            "channels": self.update_versions_value(),
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
