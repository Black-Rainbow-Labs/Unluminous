//! The first time Unluminous starts on a machine, it finishes setting itself up (`task-2278`).
//!
//! On Windows the installer runs `unluminous-cli setup`. On macOS there is no installer: the archive is
//! an application dragged into `/Applications`, so nothing runs before the window does. So the window
//! runs the same `setup` itself, on a thread, the first time it starts on a machine, which is a settings
//! folder with no settings file in it. It runs with the login shell's environment, because launchd gives
//! an application a `PATH` that finds none of the agents, and the toast says which agents were connected.
//! Linux takes the same path. Only the released binary asks for it, from `main`.

use crate::components::toast::Kind;

use crate::app::UnluminousApp;

impl UnluminousApp {
    /// Start `setup` on a thread when this is the first start on a machine that has no installer to do
    /// it. Called from `main` and by nothing else, so a test's window never edits an agent's settings.
    pub fn set_up_on_the_first_start(&mut self) {
        if !self.fresh_install || cfg!(windows) || self.first_setup.is_some() {
            return;
        }
        let waker = self.thread_waker();
        let (send, receive) = std::sync::mpsc::channel();
        self.first_setup = Some(receive);
        std::thread::spawn(move || {
            let machine = unluminous_cli::setup::Machine::with_variables(
                crate::services::login_shell::for_a_child(),
            );
            let report =
                unluminous_cli::setup::run(&unluminous_cli::setup::Options::default(), &machine);
            unluminous_cli::setup::write_the_log(&report, &machine);
            let _ = send.send(report);
            waker();
        });
    }

    /// Take in what the first start's `setup` did, and say so. Called once a frame; true when it
    /// arrived and the window has to draw.
    pub(crate) fn take_the_first_setup(&mut self) -> bool {
        let Some(receive) = self.first_setup.as_ref() else { return false };
        let Ok(report) = receive.try_recv() else { return false };
        self.first_setup = None;
        let (text, kind) = sentence(&report);
        self.toasts.say(text, kind);
        true
    }
}

/// What the toast says about a first start's `setup`.
pub(crate) fn sentence(report: &unluminous_cli::setup::Report) -> (String, Kind) {
    use unluminous_cli::setup::State;
    let titles: Vec<&str> = report
        .steps
        .iter()
        .filter(|step| step.state == State::Done)
        .filter_map(|step| step.detail["title"].as_str())
        .collect();
    let failed: Vec<&str> = report
        .steps
        .iter()
        .filter(|step| step.state == State::Failed)
        .map(|step| step.message.as_str())
        .collect();
    if !failed.is_empty() {
        return (
            format!("Setting Unluminous up did not finish: {}", failed.join(" ")),
            Kind::Problem,
        );
    }
    let path = report
        .step("path")
        .filter(|step| step.state == State::Done)
        .map(|_| " `unluminous-cli` is on the path for terminals opened from now on.");
    let text = match titles.is_empty() {
        true => format!(
            "Unluminous is set up. No coding agent needed connecting.{}",
            path.unwrap_or_default()
        ),
        false => format!(
            "Connected {} to Unluminous. Restart {} to use its tools.{}",
            titles.join(", "),
            match titles.len() {
                1 => "it",
                _ => "them",
            },
            path.unwrap_or_default()
        ),
    };
    (text, Kind::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use unluminous_cli::setup::{Report, State, Step};

    fn step(name: &str, state: State, title: &str) -> Step {
        Step {
            step: name.to_owned(),
            state,
            message: format!("{name} said"),
            detail: json!({ "title": title }),
        }
    }

    #[test]
    fn the_toast_names_the_agents_that_were_connected() {
        let report = Report {
            steps: vec![
                step("claude", State::Done, "Claude Code"),
                step("codex", State::Skipped, "Codex"),
                step("opencode", State::Done, "OpenCode"),
                step("pi", State::Done, "Pi"),
            ],
        };
        let (text, kind) = sentence(&report);
        assert_eq!(kind, Kind::Done);
        assert_eq!(
            text,
            "Connected Claude Code, OpenCode, Pi to Unluminous. Restart them to use its tools."
        );
    }

    #[test]
    fn a_step_that_failed_is_what_the_toast_says() {
        let report = Report { steps: vec![step("server", State::Failed, "")] };
        let (text, kind) = sentence(&report);
        assert_eq!(kind, Kind::Problem);
        assert!(text.contains("server said"), "{text}");
    }
}
