//! Reporting an issue from inside the window: the draft, the screenshot, the attachments and the send
//! (`task-2276`).
//!
//! `services::issue_report` writes the issue and gathers the files, and `components::report_dialog` draws
//! the modal. This is the window's half: when the picture of the window is taken, what a dropped file or a
//! pasted picture becomes, what `Open on GitHub` does, and the `report` commands an agent drives it with.
//!
//! **The draft outlives the modal.** Closing the modal by accident, or to look at something in the window,
//! keeps everything typed and attached, and the flag brings it back as it was. Only `Discard`, `Done` after
//! a send, and `report close --discard` throw it away.
//!
//! **Only the released binary reaches outside the process.** `report_reaches_out` is set by
//! `use_the_system_clipboard`, which `main.rs` calls and a test does not, so a window a test builds never
//! opens a browser and never writes over what the person running it copied. It still writes the issue and
//! gathers the files, so everything a test can ask about the send is true.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use unluminous_cli::protocol::{code, Request};

use crate::app::cli::{no, ok, Outcome, Waiting};
use crate::app::UnluminousApp;
use crate::components::report_dialog::{self, Sent, Showing};
use crate::services::issue_report::{self, Attachment, Issue, Kind, Report, System};

/// What a screenshot the report asked for carries, so the command line's own screenshot and the modal
/// backdrop's do not take it for theirs.
pub struct ReportShot;

/// Whether a screenshot event was asked for by the report.
pub fn is_a_report_shot(user_data: &egui::UserData) -> bool {
    user_data.data.as_ref().is_some_and(|data| data.downcast_ref::<ReportShot>().is_some())
}

/// How long a picture of the window is waited for before it is given up on. A window a test builds never
/// answers one.
const SHOT_WAIT: f64 = 1.5;

/// How big a thumbnail is made, in pixels along its longer side.
const THUMBNAIL: u32 = 320;

/// A report being written, whether or not the modal is showing.
pub struct ReportDraft {
    pub report: Report,
    /// Whether the modal is on the screen.
    pub showing: bool,
    /// Where this report's files are gathered.
    pub folder: PathBuf,
    pub sent: Option<SentReport>,
    /// A picture of the window is wanted, and is asked for on the next frame the modal would be drawn.
    pub shot_wanted: bool,
    /// When a picture was asked for, in egui's seconds, while one is on its way.
    pub shot_asked: Option<f64>,
    /// How many pictures of the window have been attached, which `report screenshot` waits on.
    pub shots: usize,
    pub thumbnails: HashMap<PathBuf, egui::TextureHandle>,
    /// What went wrong last, said under the attachments.
    pub problem: Option<String>,
    /// Put the keyboard in the title on the next frame.
    pub focus_title: bool,
}

/// What was sent: the issue, the files that wait to be pasted, and whether the browser and the clipboard
/// were really reached.
pub struct SentReport {
    pub issue: Issue,
    pub files: Vec<PathBuf>,
    pub opened: bool,
    pub on_the_clipboard: bool,
}

impl ReportDraft {
    fn new() -> Self {
        ReportDraft {
            report: Report::default(),
            showing: false,
            folder: issue_report::new_folder(),
            sent: None,
            shot_wanted: false,
            shot_asked: None,
            shots: 0,
            thumbnails: HashMap::new(),
            problem: None,
            focus_title: false,
        }
    }

    /// Attach `path`, saying why in `problem` when it cannot be.
    fn attach(&mut self, path: &Path) {
        match Attachment::of(path) {
            Ok(attachment) => {
                self.report.attach(attachment);
                self.problem = None;
            }
            Err(problem) => self.problem = Some(problem),
        }
    }
}

impl UnluminousApp {
    /// Open the modal, starting a draft when there is none and photographing the window when the draft has
    /// no picture of it yet. Every other modal is closed first, which is what opening any of them does.
    pub(crate) fn open_the_report(&mut self) {
        self.close_every_modal();
        let draft = self.report.get_or_insert_with(ReportDraft::new);
        draft.showing = true;
        draft.focus_title = true;
        if draft.sent.is_none() && draft.shots == 0 {
            draft.shot_wanted = true;
        }
    }

    /// The draft as the modal and the command line see it: closed, keeping it unless `discard`.
    pub(crate) fn close_the_report(&mut self, discard: bool) {
        let Some(draft) = self.report.as_mut() else { return };
        draft.showing = false;
        if discard {
            // Nothing was sent, so nothing points at the files gathered for it: they were this window's to
            // write and are this window's to remove. A sent report's files are left, because what was put on
            // the clipboard names them and may not have been pasted yet.
            if draft.sent.is_none() && draft.folder.starts_with(std::env::temp_dir()) {
                let _ = std::fs::remove_dir_all(&draft.folder);
            }
            self.report = None;
        }
    }

    /// What goes into the issue about this build and this machine.
    fn report_system(&self) -> System {
        let plugins = self
            .plugins
            .all()
            .iter()
            .filter(|plugin| plugin.enabled)
            .map(|plugin| plugin.name.clone())
            .collect();
        System::read(&crate::theme::active().name, plugins)
    }

    fn settings_folder(&self) -> Option<PathBuf> {
        self.store.as_ref().map(|store| store.folder().to_path_buf())
    }

    /// Files dropped on the window and a picture pasted, while the modal is showing and has not been sent.
    /// Called before anything else reads the frame's input, so a file dropped on the window while the
    /// modal is open is attached rather than copied into the folder pane behind it.
    pub(crate) fn take_the_report_input(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.report.as_mut() else { return };
        if !draft.showing || draft.sent.is_some() {
            return;
        }
        let dropped: Vec<PathBuf> = ctx.input_mut(|input| {
            let paths =
                input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).collect();
            input.raw.dropped_files.clear();
            paths
        });
        for path in dropped {
            draft.attach(&path);
        }
        let pasted =
            ctx.input(|input| crate::components::agent_chat::is_a_paste_chord(&input.events));
        if pasted && !crate::app::text_box_has_the_keyboard(ctx) && self.report_reaches_out {
            match issue_report::picture_from_the_clipboard(&draft.folder) {
                Ok(Some(path)) => draft.attach(&path),
                Ok(None) => {}
                Err(problem) => draft.problem = Some(problem),
            }
        }
    }

    /// Ask for a picture of the window when one is wanted, take it when it arrives, and draw the modal. The
    /// modal is left out of the frame the picture is asked on, so the picture is of the window and not of
    /// the modal over it.
    pub(crate) fn show_the_report(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|input| input.time);
        self.take_the_report_shot(&ctx, now);
        let Some(draft) = self.report.as_mut() else { return };
        if !draft.showing {
            return;
        }
        if draft.shot_wanted {
            draft.shot_wanted = false;
            draft.shot_asked = Some(now);
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                ReportShot,
            )));
            ctx.request_repaint();
            return;
        }
        make_the_thumbnails(&ctx, draft);
        let system = self.report_system();
        let logs = issue_report::logs(self.settings_folder().as_deref(), issue_report::LOG_LINES);
        let logs_line = logs.as_ref().map(|logs| {
            format!("The last {} lines of the crash log go in the issue.", logs.lines().count())
        });
        let system_line = format!(
            "{}. Written into the issue as a table, with the theme and the plugins.",
            system.line()
        );
        let Some(draft) = self.report.as_mut() else { return };
        let sent = draft.sent.as_ref().map(|sent| Sent {
            address: &sent.issue.address,
            title: &sent.issue.title,
            body: &sent.issue.body,
            files: &sent.files,
            on_the_clipboard: sent.on_the_clipboard,
            opened: sent.opened,
        });
        let showing = Showing {
            system: &system_line,
            logs: logs_line.as_deref(),
            thumbnails: &draft.thumbnails,
            problem: draft.problem.as_deref(),
            sent,
            focus_title: draft.focus_title,
        };
        let outcome = report_dialog::show(&ctx, &mut draft.report, &showing);
        draft.focus_title = false;
        self.act_on_the_report(&ctx, outcome);
    }

    /// The picture of the window, when it has arrived: written into the report's folder and attached.
    fn take_the_report_shot(&mut self, ctx: &egui::Context, now: f64) {
        let Some(draft) = self.report.as_mut() else { return };
        let Some(asked) = draft.shot_asked else { return };
        let picture = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, user_data, .. } if is_a_report_shot(user_data) => {
                    Some(image.clone())
                }
                _ => None,
            })
        });
        match picture {
            Some(picture) => {
                draft.shot_asked = None;
                let path = issue_report::free_name(&draft.folder, "window");
                match issue_report::write_picture(&picture, &path) {
                    Ok(()) => {
                        draft.attach(&path);
                        draft.shots += 1;
                    }
                    Err(problem) => draft.problem = Some(problem),
                }
            }
            None if now - asked > SHOT_WAIT => draft.shot_asked = None,
            None => ctx.request_repaint(),
        }
    }

    /// Carry out what was pressed in the modal.
    fn act_on_the_report(&mut self, ctx: &egui::Context, outcome: report_dialog::Outcome) {
        if outcome.discard {
            self.close_the_report(true);
            return;
        }
        if outcome.close {
            self.close_the_report(false);
            return;
        }
        let reaches_out = self.report_reaches_out;
        let Some(draft) = self.report.as_mut() else { return };
        if let Some(name) = outcome.detach {
            draft.report.detach(&name);
        }
        if outcome.screenshot {
            draft.shot_wanted = true;
            ctx.request_repaint();
        }
        if outcome.add_files && reaches_out {
            if let Some(paths) = rfd::FileDialog::new().set_title("Attach files").pick_files() {
                for path in paths {
                    draft.attach(&path);
                }
            }
        }
        if let Some(sent) = draft.sent.as_ref() {
            if outcome.copy_again && reaches_out {
                crate::services::system_files::put_files_on_the_clipboard(&sent.files);
            }
            if outcome.show_files && reaches_out {
                if let Some(first) = sent.files.first() {
                    crate::services::launcher::reveal(first);
                }
            }
            if outcome.open_again && reaches_out {
                let _ = crate::services::links::open_in_the_default_browser(&sent.issue.address);
            }
        }
        if outcome.send {
            if let Err(problem) = self.send_the_report(true) {
                if let Some(draft) = self.report.as_mut() {
                    draft.problem = Some(problem);
                }
            }
        }
    }

    /// `Open on GitHub`: write the issue, gather the files, put them on the clipboard and open the issue in
    /// the browser. `open` false gathers and writes and reaches nothing, which is `report send --no-browser`.
    pub(crate) fn send_the_report(&mut self, open: bool) -> Result<(), String> {
        let system = self.report_system();
        let settings = self.settings_folder();
        let reaches_out = self.report_reaches_out && open;
        let Some(draft) = self.report.as_mut() else {
            return Err("There is no report. `report open` starts one.".to_owned());
        };
        if !draft.report.can_be_sent() {
            return Err("A report needs a title before it can be sent.".to_owned());
        }
        let logs = match draft.report.include_logs {
            true => issue_report::logs(settings.as_deref(), issue_report::LOG_LINES),
            false => None,
        };
        let system = draft.report.include_system.then_some(system);
        let issue = issue_report::issue(&draft.report, system.as_ref(), logs.as_deref());
        // Gathered into a folder of their own inside the report's, so a file attached from elsewhere can never
        // be copied over a picture the window took, which lives in the report's folder itself.
        let files = issue_report::prepare(
            &draft.report,
            &issue,
            logs.as_deref(),
            &draft.folder.join("issue"),
        )?;
        let on_the_clipboard = reaches_out
            && !files.is_empty()
            && crate::services::system_files::put_files_on_the_clipboard(&files);
        let opened = match reaches_out {
            true => match crate::services::links::open_in_the_default_browser(&issue.address) {
                Ok(()) => true,
                Err(problem) => {
                    draft.problem = Some(problem);
                    false
                }
            },
            false => false,
        };
        draft.sent = Some(SentReport { issue, files, opened, on_the_clipboard });
        Ok(())
    }

    /// The draft as data, for `report status` and the replies of the other `report` commands.
    fn report_status(&self) -> Value {
        let Some(draft) = self.report.as_ref() else {
            return json!({ "draft": false, "showing": false });
        };
        let report = &draft.report;
        let attachments: Vec<Value> = report
            .attachments
            .iter()
            .map(|attachment| {
                json!({
                    "name": attachment.name,
                    "path": attachment.path.to_string_lossy(),
                    "bytes": attachment.bytes,
                    "picture": attachment.picture,
                })
            })
            .collect();
        let sent = draft.sent.as_ref().map(|sent| {
            json!({
                "address": sent.issue.address,
                "title": sent.issue.title,
                "body": sent.issue.body,
                "files": sent.files.iter().map(|file| file.to_string_lossy()).collect::<Vec<_>>(),
                "opened": sent.opened,
                "onTheClipboard": sent.on_the_clipboard,
                "logsCut": sent.issue.logs_cut,
                "descriptionCut": sent.issue.description_cut,
            })
        });
        json!({
            "draft": true,
            "showing": draft.showing,
            "kind": report.kind.name(),
            "title": report.title,
            "description": report.description,
            "attachments": attachments,
            "includeLogs": report.include_logs,
            "includeSystem": report.include_system,
            "canBeSent": report.can_be_sent(),
            "folder": draft.folder.to_string_lossy(),
            "screenshots": draft.shots,
            "problem": draft.problem,
            "sent": sent,
            "repository": issue_report::REPOSITORY,
        })
    }

    /// The `report` commands.
    pub(crate) fn cli_report(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "open" => {
                self.open_the_report();
                if let Err(refusal) = self.set_the_report_from(request) {
                    return no(request, code::USAGE, refusal);
                }
                ok(request, "The Report an Issue modal is open.", self.report_status())
            }
            "status" => {
                let said = match &self.report {
                    None => "There is no report.".to_owned(),
                    Some(draft) if draft.sent.is_some() => "The report has been sent.".to_owned(),
                    Some(draft) => format!(
                        "A {} report with {} attachment(s); the modal is {}.",
                        draft.report.kind.word().to_lowercase(),
                        draft.report.attachments.len(),
                        match draft.showing {
                            true => "showing",
                            false => "closed",
                        }
                    ),
                };
                ok(request, said, self.report_status())
            }
            "set" => {
                if self.report.is_none() {
                    self.report = Some(ReportDraft::new());
                }
                match self.set_the_report_from(request) {
                    Ok(()) => ok(request, "The report was changed.", self.report_status()),
                    Err(refusal) => no(request, code::USAGE, refusal),
                }
            }
            "attach" => {
                let Some(said) = request.text("path") else {
                    return no(request, code::USAGE, "report attach needs a path.");
                };
                let path = self.cli_path(&said);
                let draft = self.report.get_or_insert_with(ReportDraft::new);
                match Attachment::of(&path) {
                    Ok(attachment) => {
                        draft.report.attach(attachment);
                        ok(request, format!("Attached {}.", path.display()), self.report_status())
                    }
                    Err(problem) => no(request, code::NOT_FOUND, problem),
                }
            }
            "detach" => {
                let name = request.text("name").unwrap_or_default();
                match self.report.as_mut().is_some_and(|draft| draft.report.detach(&name)) {
                    true => ok(request, format!("Took {name} off."), self.report_status()),
                    false => {
                        no(request, code::NOT_FOUND, format!("No attachment is called {name}."))
                    }
                }
            }
            "screenshot" => {
                let draft = self.report.get_or_insert_with(ReportDraft::new);
                if !draft.showing {
                    return no(
                        request,
                        code::NOT_APPLICABLE,
                        "The Report an Issue modal is not open; `report open` opens it and photographs the window.",
                    );
                }
                draft.shot_wanted = true;
                let count = draft.shots;
                Outcome::Hold(Waiting::ReportShot {
                    count,
                    until: std::time::Instant::now() + std::time::Duration::from_secs(10),
                })
            }
            "send" => {
                let open = !request.switch("no-browser");
                match self.send_the_report(open) {
                    Ok(()) => {
                        let status = self.report_status();
                        let address =
                            status["sent"]["address"].as_str().unwrap_or_default().to_owned();
                        ok(request, format!("The issue is ready at {address}"), status)
                    }
                    Err(refusal) => no(request, code::NOT_APPLICABLE, refusal),
                }
            }
            "close" => {
                self.close_the_report(request.switch("discard"));
                ok(request, "The Report an Issue modal is closed.", self.report_status())
            }
            other => no(request, code::USAGE, format!("report has no command called {other}.")),
        }
    }

    /// The answer to a held `report screenshot`, once the picture is attached.
    pub(crate) fn report_shot_answer(
        &self,
        request: &Request,
        count: usize,
    ) -> Option<unluminous_cli::protocol::Reply> {
        let draft = self.report.as_ref()?;
        (draft.shots > count).then(|| {
            unluminous_cli::protocol::Reply::done(
                &request.command,
                "The window was photographed and the picture attached.",
                self.report_status(),
            )
        })
    }

    /// The kind, the title, the description and the two switches a `report open` or `report set` named.
    fn set_the_report_from(&mut self, request: &Request) -> Result<(), String> {
        let Some(draft) = self.report.as_mut() else { return Ok(()) };
        if let Some(said) = request.text("kind") {
            draft.report.kind = Kind::parse(&said).ok_or_else(|| {
                format!("{said} is not a kind of report: bug, feature or question.")
            })?;
        }
        if let Some(title) = request.text("title") {
            draft.report.title = title;
        }
        if let Some(description) = request.text("description") {
            draft.report.description = description;
        }
        for (name, value) in
            [("logs", &mut draft.report.include_logs), ("system", &mut draft.report.include_system)]
        {
            if let Some(said) = request.text(name) {
                *value = match said.trim().to_ascii_lowercase().as_str() {
                    "on" | "true" | "yes" => true,
                    "off" | "false" | "no" => false,
                    _ => return Err(format!("--{name} is on or off, not {said}.")),
                };
            }
        }
        Ok(())
    }
}

/// Make a thumbnail for each picture that has none yet.
fn make_the_thumbnails(ctx: &egui::Context, draft: &mut ReportDraft) {
    let wanted: Vec<PathBuf> = draft
        .report
        .attachments
        .iter()
        .filter(|attachment| attachment.picture && !draft.thumbnails.contains_key(&attachment.path))
        .map(|attachment| attachment.path.clone())
        .collect();
    for path in wanted {
        let Ok(picture) = image::open(&path) else {
            continue;
        };
        let small = picture.thumbnail(THUMBNAIL, THUMBNAIL).to_rgba8();
        let size = [small.width() as usize, small.height() as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, small.as_raw());
        let texture = ctx.load_texture(
            format!("unluminous-report-{}", path.display()),
            image,
            egui::TextureOptions::LINEAR,
        );
        draft.thumbnails.insert(path, texture);
    }
}
