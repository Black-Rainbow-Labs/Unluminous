//! The window's half of telemetry: starting it, the once a second look at what is showing, the setting
//! changing, and the closing request. `task-2271`; `services::telemetry` is the rest.
//!
//! **Only the released binary starts it.** `main.rs` calls [`UnluminousApp::allow_telemetry`] after the
//! settings are read, the way it calls `use_the_system_clipboard`. A window a test builds never does, so
//! its permission stays [`Permission::Test`] and nothing it does is counted or sent, whatever its settings
//! say.
//!
//! **The setting is followed every frame, not hooked.** `telemetry.enabled` can change from the Settings
//! page, from `settings set` and from a settings file read again, and three places that each had to
//! remember to start or stop telemetry is a list whose next entry would be the one that forgot. One
//! comparison a frame answers all three.

use std::time::Duration;

use serde_json::{json, Value};

use crate::app::UnluminousApp;
use crate::services::telemetry::{self, payload, sender, Opening, Permission, Telemetry};

/// The longest closing the window waits for its last request to be posted. The request is on disk
/// before the wait starts, so running out costs the next start a request rather than losing it.
const CLOSING_WAIT: Duration = Duration::from_millis(600);

/// Whether telemetry may run in this window, and the running telemetry when it does.
pub struct TelemetryState {
    pub(crate) permission: Permission,
    /// Whether Unluminous started on the last session rather than on a named folder.
    launched_from_the_desktop: bool,
    pub(crate) running: Option<Telemetry>,
}

impl Default for TelemetryState {
    fn default() -> Self {
        TelemetryState {
            permission: Permission::Test,
            launched_from_the_desktop: false,
            running: None,
        }
    }
}

impl UnluminousApp {
    /// Let telemetry run in this window while `telemetry.enabled` is on. Only the released binary calls it.
    ///
    /// @param background - whether the window was started with `--background`
    /// @param from_the_desktop - whether no folder was named, so the last session was reopened
    pub fn allow_telemetry(&mut self, background: bool, from_the_desktop: bool) {
        let environment = std::env::var("UNLUMINOUS_TELEMETRY").ok();
        self.telemetry.permission = Permission::for_the_binary(background, environment.as_deref());
        self.telemetry.launched_from_the_desktop = from_the_desktop;
    }

    /// Start or stop telemetry when the setting moved, advance its clocks, look at what is showing once a
    /// second, and send when a request is due. Two comparisons on a frame where nothing is due.
    ///
    /// @param ctx - the window's context, for its time, its focus and this frame's input
    pub(crate) fn keep_the_telemetry(&mut self, ctx: &egui::Context) {
        self.start_or_stop_the_telemetry();
        if self.telemetry.running.is_none() {
            return;
        }
        let (now, focused, input) = ctx.input(|input| {
            (input.time, input.viewport().focused.unwrap_or(false), !input.events.is_empty())
        });
        let Some(running) = self.telemetry.running.as_mut() else {
            return;
        };
        if running.tick(now, focused, input) {
            let (panels, tab) = self.what_is_showing_for_the_telemetry();
            if let Some(running) = self.telemetry.running.as_mut() {
                running.sample(now, focused, &panels, tab.as_deref());
            }
        }
        if let Some(running) = self.telemetry.running.as_mut() {
            if running.due(now) {
                running.send(now);
            }
        }
    }

    /// Follow `telemetry.enabled`: start telemetry the frame it is on and allowed, and stop it, throwing
    /// away what was counted, the frame it is turned off.
    fn start_or_stop_the_telemetry(&mut self) {
        let wanted =
            self.settings.telemetry_enabled && self.telemetry.permission == Permission::Allowed;
        match (wanted, self.telemetry.running.is_some()) {
            (true, false) => self.start_the_telemetry(),
            (false, true) => {
                self.telemetry.running = None;
                telemetry::set_on(false);
            }
            _ => {}
        }
    }

    /// Make the client id, read whether the last run crashed, start the sender, and start counting.
    fn start_the_telemetry(&mut self) {
        let Some(settings_folder) = self.store.as_ref().map(|store| store.folder().to_path_buf())
        else {
            return;
        };
        let folder = telemetry::folder_in(&settings_folder);
        let (client_id, first_run) = telemetry::client_id(&folder);
        let crashed = telemetry::crashed_since_last_start(&settings_folder, &folder);
        let debug = telemetry::debug_from_the_environment();
        let sender = sender::Sender::start(sender::Target::for_the_binary(folder, debug));
        let opening = Opening {
            launch: if self.telemetry.launched_from_the_desktop { "desktop" } else { "folder" },
            theme: self.theme_for_the_telemetry(),
            first_run,
            crashed,
        };
        let context = payload::Context::for_this_window(client_id, debug);
        self.telemetry.running =
            Some(Telemetry::new(sender, context, opening, telemetry::every_from_the_environment()));
        telemetry::set_on(true);
    }

    /// The theme's id when Unluminous ships it, and `other` when it is a plugin somebody added.
    fn theme_for_the_telemetry(&self) -> String {
        let theme =
            if self.settings.theme.is_empty() { "unluminous/dark" } else { &self.settings.theme };
        let plugin = theme.split('/').next().unwrap_or_default();
        match telemetry::is_bundled(plugin) {
            true => telemetry::clean_name(theme),
            false => "other".to_owned(),
        }
    }

    /// The panels showing, the editing area included, and the kind of tab showing in it.
    fn what_is_showing_for_the_telemetry(&self) -> (Vec<String>, Option<String>) {
        let showing = self.panels_showing();
        let mut panels: Vec<String> = crate::app::dock::Panel::ALL
            .iter()
            .filter(|panel| showing[panel.index()])
            .map(|panel| panel.name().to_owned())
            .collect();
        // A pane is named `<plugin>/<pane>`, and a plugin Unluminous did not ship is a folder name somebody chose.
        panels.extend(self.showing_plugin_panes().into_iter().map(
            |key| match telemetry::is_bundled(key.split('/').next().unwrap_or_default()) {
                true => key,
                false => "plugin:other".to_owned(),
            },
        ));
        let tab = self.editor_visible.then(|| self.kind_of_tab_for_the_telemetry());
        if self.editor_visible {
            panels.push("editor".to_owned());
        }
        (panels, tab)
    }

    /// What kind of tab is showing: a language from `file_kind::kind_name`, or what the tab holds when it
    /// is not a document.
    fn kind_of_tab_for_the_telemetry(&self) -> String {
        let file = self.files.active();
        if let Some(plugin) = &file.plugin {
            return format!("plugin:{}", plugin.plugin);
        }
        if file.browser.is_some() {
            return "browser".to_owned();
        }
        if file.picture.is_some() {
            return "picture".to_owned();
        }
        if file.notebook.is_some() {
            return "notebook".to_owned();
        }
        crate::services::file_kind::kind_name(file.document.path()).to_owned()
    }

    /// Send the closing request, waiting at most [`CLOSING_WAIT`]. Called from `on_exit`.
    pub(crate) fn close_the_telemetry(&mut self) {
        if let Some(mut running) = self.telemetry.running.take() {
            let now = running.last_frame();
            running.close(now, CLOSING_WAIT);
            telemetry::set_on(false);
        }
    }

    /// What `settings telemetry` answers.
    pub(crate) fn telemetry_status(&self) -> Value {
        let mut status = json!({
            "enabled": self.settings.telemetry_enabled,
            "permission": self.telemetry.permission.name(),
            "running": self.telemetry.running.is_some(),
            "measurementId": telemetry::MEASUREMENT_ID,
        });
        if let Some(running) = &self.telemetry.running {
            if let (Value::Object(into), Value::Object(from)) = (&mut status, running.status()) {
                into.extend(from);
            }
        }
        status
    }

    /// Send what is waiting now rather than at the next twenty minutes, for `settings telemetry --flush`.
    pub(crate) fn flush_the_telemetry(&mut self) -> bool {
        match self.telemetry.running.as_mut() {
            Some(running) => {
                let now = running.last_frame();
                running.send(now);
                true
            }
            None => false,
        }
    }
}
