//! The window's half of speech to text (`task-2290`): it starts the engine in `unluminous-voice` when it is
//! needed, gives it the `voice.*` settings, and sends what it hears to the chat whose prompt is listening.
//!
//! **Which chat hears the words is decided when dictation starts and kept by its tag.** The microphone in the
//! side pane's prompt, `voice dictate` and a wake all mean the side pane; a chat node's microphone means that
//! node. A transcription asked for from the command line goes nowhere unless it was asked for `--live`.
//!
//! **The engine is only started when something needs it**: always listen is on, or somebody pressed the
//! microphone, or a command asked. A window nobody speaks to never opens the microphone, never loads a model
//! and never starts a thread for any of it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use unluminous_voice::segmenter::{Ended, Started};
use unluminous_voice::transcriber::Engine;
use unluminous_voice::{Config, Event, Voice};

use crate::app::UnluminousApp;
use crate::components::microphone_dialog::{self, VoiceDialog};
use crate::components::toast::Kind;
use crate::components::voice_page::{VoiceAsk, VoiceFacts};
use crate::services::plugin_ui::{Dictation, VoiceView};
use crate::settings::{Settings, VoiceEngine};

/// The plugin whose prompt the side pane's dictation fills.
const AGENT_CHAT: &str = "agent-chat";
/// The plugin whose open ticket's description the description's microphone fills (`task-2293`).
const AGENT_TASKS: &str = "agent-tasks";

/// Whose prompt a dictation's words go into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// The Agent-Chat side pane.
    Pane,
    /// A chat node on the canvas.
    Node(crate::services::realm::NodeId),
    /// The description of the ticket open in the Agent-Tasks modal (`task-2293`).
    Description,
    /// Nobody's: a transcription the command line asked for and reads back itself.
    Nowhere,
}

/// What a finished dictation came to, kept for `voice transcribe` to answer with.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    pub text: String,
    pub ended: &'static str,
}

/// How far a download of the local model has got.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Download {
    pub said: String,
    pub done: f32,
    /// `Some` once it has finished, with why it failed when it did.
    pub finished: Option<Result<(), String>>,
}

/// Everything the window keeps about speech to text.
#[derive(Default)]
pub struct VoiceState {
    pub engine: Option<Voice>,
    applied: Option<Config>,
    targets: HashMap<u64, Target>,
    pub results: HashMap<u64, Finished>,
    pub dictating: Option<(u64, Target)>,
    told: HashMap<Target, VoiceView>,
    pub download: Option<Arc<Mutex<Download>>>,
    pub dialog: Option<VoiceDialog>,
    pub problem: Option<String>,
    /// Whether this window may open the microphone, which only the released binary says. See
    /// [`UnluminousApp::allow_the_microphone`].
    pub microphone_allowed: bool,
    /// What the Voice page shows instead of asking this machine, for a screenshot test whose picture must
    /// not depend on whether the machine running it has the model or a microphone.
    pub facts: Option<VoiceFacts>,
    /// The dictation whose message is sent once its last words are in, because it was stopped with `--send`,
    /// which is what a second press of the microphone does.
    send_when_done: Option<u64>,
    /// Whether this window is the one on the machine that listens for "Sable", and when that was last asked.
    /// See [`UnluminousApp::follow_the_listening_claim`].
    pub listening_here: bool,
    claim_asked: Option<std::time::Instant>,
}

/// The engine's configuration for these settings.
/// @param settings - the window's settings
/// @param microphone - whether this window may open the microphone
pub fn config_for(settings: &Settings, microphone: bool) -> Config {
    let engine = match settings.voice_engine {
        VoiceEngine::Local => Engine::Local { threads: 2 },
        VoiceEngine::Endpoint => Engine::Endpoint(unluminous_voice::endpoint::Endpoint {
            url: settings.voice_endpoint.clone(),
            model: settings.voice_endpoint_model.clone(),
            language: settings.voice_language.clone(),
            key_variable: settings.voice_endpoint_key.clone(),
        }),
    };
    Config {
        engine,
        always_listen: settings.voice_always_listen,
        sensitivity: settings.voice_wake_sensitivity,
        model_idle: Duration::from_secs_f32(settings.voice_model_idle),
        send_pause: Duration::from_secs_f32(settings.voice_send_pause),
        microphone,
    }
}

impl UnluminousApp {
    /// Let this window open the microphone. Called from `main` and by nothing else, so a window a test
    /// builds never listens to the room the test runs in; it can still transcribe a file.
    pub fn allow_the_microphone(&mut self) {
        self.voice.microphone_allowed = true;
    }

    /// Once a frame: start or configure the engine, act on what it heard, and tell every chat what it needs
    /// to draw. Cheap when nothing is happening: no engine is started unless always listen is on.
    pub(crate) fn follow_the_voice(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|input| input.viewport().focused.unwrap_or(false));
        self.follow_the_listening_claim(focused);
        if self.voice.engine.is_none() && self.voice.listening_here {
            self.voice_engine();
        }
        let mut config = config_for(&self.settings, self.voice.microphone_allowed);
        config.always_listen &= self.voice.listening_here;
        if let Some(engine) = self.voice.engine.as_ref() {
            if self.voice.applied.as_ref() != Some(&config) {
                engine.configure(config.clone());
                self.voice.applied = Some(config);
            }
        }
        let events = self.voice.engine.as_ref().map(Voice::take_events).unwrap_or_default();
        for event in events {
            self.on_a_voice_event(event);
        }
        self.follow_the_download();
        self.tell_the_chats_about_the_voice();
        if self.voice.dictating.is_some() || self.voice.download.is_some() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }

    /// **One window on the machine listens for "Sable"** (`task-2290`). Every Unluminous window is a process
    /// and each would otherwise open the microphone and answer the same "Sable", typing the request into two
    /// prompts and sending it twice. `voice/listening.pid` names the window that listens: a window takes it
    /// when nobody holds it, when its holder has closed, or when the window has the keyboard, so the window
    /// a person is using is the one that hears them. Asked at most once a second.
    /// @param focused - whether this window has the keyboard
    fn follow_the_listening_claim(&mut self, focused: bool) {
        if !self.settings.voice_always_listen || !self.voice.microphone_allowed {
            self.voice.listening_here = false;
            return;
        }
        if self.voice.claim_asked.is_some_and(|asked| asked.elapsed() < Duration::from_secs(1)) {
            return;
        }
        self.voice.claim_asked = Some(std::time::Instant::now());
        let path = unluminous_voice::install::home().join("listening.pid");
        let me = std::process::id();
        let holder =
            std::fs::read_to_string(&path).ok().and_then(|text| text.trim().parse::<u32>().ok());
        let take = match holder {
            Some(pid) if pid == me => false,
            Some(pid) => focused || !unluminous_cli::instances::is_running(pid),
            None => true,
        };
        if take {
            let _ = std::fs::create_dir_all(unluminous_voice::install::home());
            let _ = crate::services::store::write_atomically(&path, me.to_string().as_bytes());
        }
        self.voice.listening_here = take || holder == Some(me);
    }

    /// The engine, started now if it is not running.
    pub(crate) fn voice_engine(&mut self) -> &Voice {
        if self.voice.engine.is_none() {
            let engine = Voice::start(self.thread_waker());
            let config = config_for(&self.settings, self.voice.microphone_allowed);
            engine.configure(config.clone());
            self.voice.applied = Some(config);
            self.voice.engine = Some(engine);
        }
        self.voice.engine.as_ref().expect("started just above")
    }

    /// Starts dictation into `target`'s prompt, or says why it cannot, opening the dialog that helps.
    /// @param target - whose prompt
    pub(crate) fn start_dictation(&mut self, target: Target) -> Result<u64, String> {
        if let Some((tag, already)) = self.voice.dictating {
            return match already == target {
                true => Ok(tag),
                false => {
                    Err("dictation is already filling another prompt; stop it first".to_owned())
                }
            };
        }
        if let unluminous_voice::access::Access::Refused(why) = unluminous_voice::access::status() {
            self.voice.dialog = Some(VoiceDialog::Microphone(why.clone()));
            return Err(why);
        }
        match self.settings.voice_engine {
            VoiceEngine::Local if !unluminous_voice::install::status().is_complete() => {
                self.voice.dialog =
                    Some(VoiceDialog::Model { downloading: self.download_progress() });
                return Err("the speech model is not downloaded yet. `voice install` downloads it"
                    .to_owned());
            }
            VoiceEngine::Endpoint if self.settings.voice_endpoint.trim().is_empty() => {
                return Err("the endpoint engine is chosen and voice.endpoint is empty".to_owned());
            }
            _ => {}
        }
        let tag = self.voice_engine().dictate();
        self.begin_dictating(tag, target);
        Ok(tag)
    }

    /// The microphone button, as the `Dictate` menu entry and its chord press it: start dictation into the
    /// side pane's prompt, or, while one is running, stop it and send once the last words are in.
    pub(crate) fn press_the_microphone(&mut self) {
        if self.voice.dictating.is_some() {
            self.stop_dictation(true);
            return;
        }
        if let Err(problem) = self.start_dictation(Target::Pane) {
            self.message = Some(problem);
        }
    }

    /// Records whose prompt a dictation fills and tells that chat it has started.
    /// @param tag - the dictation
    /// @param target - whose prompt
    fn begin_dictating(&mut self, tag: u64, target: Target) {
        self.voice.targets.insert(tag, target);
        if target == Target::Nowhere {
            return;
        }
        self.voice.dictating = Some((tag, target));
        if target == Target::Pane {
            self.open_the_chat_pane();
        }
        self.tell_a_chat(target, &Dictation::Began);
    }

    /// Stops the dictation in progress. Its last words still arrive.
    /// With `send`, the message goes once the last words are in: *"When I click the record, then click it
    /// again after it transcribes, it should send the message."* (`task-2290`).
    /// @param send - whether the message is sent once the last words are in
    pub(crate) fn stop_dictation(&mut self, send: bool) {
        if send {
            self.voice.send_when_done = self.voice.dictating.map(|(tag, _)| tag);
        }
        if let Some(engine) = self.voice.engine.as_ref() {
            engine.stop();
        }
    }

    /// Transcribes a file, its words going to `target` as they arrive when `live`.
    /// @param samples - 16 kHz audio
    /// @param live - played at the speed it was spoken
    /// @param target - whose prompt, or nobody's
    pub(crate) fn transcribe_samples(
        &mut self,
        samples: Vec<i16>,
        live: bool,
        target: Target,
    ) -> u64 {
        let tag = self.voice_engine().transcribe(samples, live);
        self.begin_dictating(tag, target);
        tag
    }

    /// Acts on one thing the engine said.
    /// @param event - what it said
    fn on_a_voice_event(&mut self, event: Event) {
        match event {
            Event::Woke { tag, .. } => {
                self.begin_dictating(tag, Target::Pane);
                self.message = Some("Listening: \"Sable\" was heard".to_owned());
            }
            Event::Words { tag, text } => {
                if let Some(target) = self.voice.targets.get(&tag).copied() {
                    self.tell_a_chat(target, &Dictation::Words(text));
                }
            }
            Event::Ended { tag, text, ended, started } => {
                self.dictation_ended(tag, text, ended, started)
            }
            Event::Microphone(why) => {
                self.voice.dialog = Some(VoiceDialog::Microphone(why.clone()));
                self.message = Some(why);
            }
            Event::Problem(problem) => {
                self.message = Some(format!("Speech to text: {problem}"));
                self.voice.problem = Some(problem);
            }
        }
    }

    /// A dictation is over: its words go to its chat, which sends them when a wake asked for it.
    fn dictation_ended(&mut self, tag: u64, text: String, ended: Ended, started: Started) {
        let target = self.voice.targets.remove(&tag).unwrap_or(Target::Nowhere);
        if self.voice.dictating.is_some_and(|(dictating, _)| dictating == tag) {
            self.voice.dictating = None;
        }
        let asked = self.voice.send_when_done == Some(tag);
        if asked {
            self.voice.send_when_done = None;
        }
        let woke = started == Started::Wake
            && ended == Ended::Finished
            && self.settings.voice_send_after_wake;
        let send = (asked || woke) && ended != Ended::FalseWake;
        if target != Target::Nowhere {
            self.tell_a_chat(target, &Dictation::Ended { words: text.clone(), send });
        }
        if ended == Ended::FalseWake {
            self.message = Some("\"Sable\" was a false wake, so nothing was written".to_owned());
        }
        self.voice.results.insert(tag, Finished { text, ended: ended.name() });
    }

    /// Hands a note to the chat that is `target`.
    /// @param target - whose prompt
    /// @param note - the note
    fn tell_a_chat(&mut self, target: Target, note: &Dictation) {
        match target {
            Target::Pane => {
                if let Ok(provider) = self.plugin_ui.opened(AGENT_CHAT, AGENT_CHAT) {
                    provider.dictation(note);
                }
            }
            Target::Node(node) => {
                if let Some(chat) = self.realm.live.chat_mut(node) {
                    crate::services::plugin_ui::UiProvider::dictation(chat, note);
                }
            }
            Target::Description => {
                if let Some(board) = self.plugin_ui.provider(AGENT_TASKS) {
                    board.dictation(note);
                }
            }
            Target::Nowhere => {}
        }
    }

    /// Shows the Agent-Chat side pane, which is where a wake's words go.
    fn open_the_chat_pane(&mut self) {
        let key = self
            .plugin_ui
            .surfaces()
            .panes
            .iter()
            .find(|pane| pane.plugin == AGENT_CHAT)
            .map(|pane| pane.key(&pane.what.id));
        if let Some(key) = key {
            self.show_the_plugin_pane(&key, true);
        }
    }

    /// Tells the side pane's chat and every chat node what to draw: whether its microphone is on, how loud,
    /// and whether always listen is on. Only a chat whose view changed is told.
    fn tell_the_chats_about_the_voice(&mut self) {
        let snapshot = match self.voice.dictating.is_some() || self.voice.engine.is_some() {
            true => self.voice.engine.as_ref().map(Voice::snapshot).unwrap_or_default(),
            false => Default::default(),
        };
        let mut targets = vec![Target::Pane, Target::Description];
        targets.extend(self.realm.live.chat_nodes().into_iter().map(Target::Node));
        for target in targets {
            let dictating = self.voice.dictating.is_some_and(|(_, t)| t == target);
            let view = VoiceView {
                dictating,
                listening: self.settings.voice_always_listen,
                button: self.settings.voice_button,
                level: if dictating { (snapshot.level * 50.0).round() / 50.0 } else { 0.0 },
                working: dictating && snapshot.dictating.is_some(),
                refused: snapshot.refused.clone(),
            };
            if self.voice.told.get(&target) == Some(&view) {
                continue;
            }
            match target {
                Target::Pane => match self.plugin_ui.provider(AGENT_CHAT) {
                    Some(provider) => provider.voice(&view),
                    None => continue,
                },
                Target::Node(node) => match self.realm.live.chat_mut(node) {
                    Some(chat) => crate::services::plugin_ui::UiProvider::voice(chat, &view),
                    None => continue,
                },
                Target::Description => match self.plugin_ui.provider(AGENT_TASKS) {
                    Some(board) => board.voice(&view),
                    None => continue,
                },
                Target::Nowhere => continue,
            }
            self.voice.told.insert(target, view);
        }
    }

    /// Starts downloading what the local engine needs, on a thread, unless a download is already running.
    pub(crate) fn start_the_voice_download(&mut self) {
        if self
            .voice
            .download
            .as_ref()
            .is_some_and(|d| d.lock().map(|d| d.finished.is_none()).unwrap_or(false))
        {
            return;
        }
        let progress = Arc::new(Mutex::new(Download {
            said: "Starting the download".to_owned(),
            ..Download::default()
        }));
        let shared = progress.clone();
        let waker = self.thread_waker();
        let _ =
            std::thread::Builder::new().name("unluminous-voice-install".into()).spawn(move || {
                let result = unluminous_voice::install::install(&|said, done| {
                    if let Ok(mut progress) = shared.lock() {
                        progress.said = said.to_owned();
                        progress.done = done;
                    }
                });
                if let Ok(mut progress) = shared.lock() {
                    progress.finished = Some(result.map(|_| ()));
                }
                waker();
            });
        self.voice.download = Some(progress);
    }

    /// How far a running download is, in words, or `None` when none is running.
    pub(crate) fn download_progress(&self) -> Option<String> {
        let download = self.voice.download.as_ref()?.lock().ok()?;
        download
            .finished
            .is_none()
            .then(|| format!("{}: {:.0}%", download.said, download.done * 100.0))
    }

    /// Says when a download has finished, and lets the dialog go once the model is there.
    fn follow_the_download(&mut self) {
        let finished = self
            .voice
            .download
            .as_ref()
            .and_then(|d| d.lock().ok().and_then(|d| d.finished.clone()));
        let Some(finished) = finished else {
            let progress = self.download_progress();
            if let Some(VoiceDialog::Model { downloading }) = self.voice.dialog.as_mut() {
                *downloading = progress;
            }
            return;
        };
        self.voice.download = None;
        match finished {
            Ok(()) => {
                self.toasts.say("The speech model is ready. Press the microphone in the Agent-Chat prompt to dictate.", Kind::Done);
                if matches!(self.voice.dialog, Some(VoiceDialog::Model { .. })) {
                    self.voice.dialog = None;
                }
            }
            Err(problem) => {
                self.toasts.say(
                    format!("The speech model could not be downloaded: {problem}"),
                    Kind::Problem,
                );
                self.voice.problem = Some(problem);
            }
        }
    }

    /// What the Voice settings page shows.
    pub(crate) fn voice_facts(&self) -> VoiceFacts {
        if let Some(facts) = self.voice.facts.clone() {
            return facts;
        }
        let installed = unluminous_voice::install::status();
        let loaded = self
            .voice
            .engine
            .as_ref()
            .map(|engine| engine.snapshot().model.loaded)
            .unwrap_or(false);
        let model = match (self.download_progress(), installed.is_complete(), loaded) {
            (Some(progress), _, _) => progress,
            (None, true, true) => "Downloaded and loaded".to_owned(),
            (None, true, false) => "Downloaded; it loads when dictation starts".to_owned(),
            (None, false, _) => {
                format!("Not downloaded ({} MB)", installed.download_megabytes().max(1))
            }
        };
        let access = unluminous_voice::access::status();
        let device = unluminous_voice::capture::default_device_name();
        let microphone = match (&access, device) {
            (unluminous_voice::access::Access::Refused(why), _) => why.clone(),
            (_, None) => "No microphone is connected".to_owned(),
            (unluminous_voice::access::Access::NotAsked, Some(name)) => {
                format!("{name}: Unluminous will ask the first time it listens")
            }
            (_, Some(name)) => format!("{name}: allowed"),
        };
        VoiceFacts {
            model,
            may_download: !installed.is_complete() && self.download_progress().is_none(),
            microphone,
            refused: !access.may_open(),
        }
    }

    /// Acts on what the Voice settings page asked for.
    /// @param ask - the request
    pub(crate) fn voice_page_asked(&mut self, ask: VoiceAsk) {
        match ask {
            VoiceAsk::Download => self.start_the_voice_download(),
            VoiceAsk::MicrophoneSettings => {
                if let Err(problem) = unluminous_voice::access::open_settings() {
                    self.message = Some(problem);
                }
            }
        }
    }

    /// Draws the microphone or model dialog when one is open, and acts on its buttons.
    pub(crate) fn show_the_voice_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.voice.dialog.clone() else { return };
        match microphone_dialog::show(ctx, &dialog) {
            microphone_dialog::Outcome::Nothing => {}
            microphone_dialog::Outcome::Close => self.voice.dialog = None,
            microphone_dialog::Outcome::OpenSettings => {
                if let Err(problem) = unluminous_voice::access::open_settings() {
                    self.message = Some(problem);
                }
            }
            microphone_dialog::Outcome::TryAgain => {
                self.voice.dialog = None;
                if let Err(problem) = self.start_dictation(Target::Pane) {
                    self.message = Some(problem);
                }
            }
            microphone_dialog::Outcome::Download => self.start_the_voice_download(),
            microphone_dialog::Outcome::VoiceSettings => {
                self.voice.dialog = None;
                self.settings_window.page = crate::settings::Page::Voice;
                self.settings_window.open();
            }
        }
    }
}

/// `unluminous --setup-voice`: download the speech model and ONNX Runtime when they are missing, and ask about
/// the microphone, which is the install's half of `task-2290`'s *"make sure our install prompts the user for
/// mic access"*. Answers the process's exit status, which is always 0: a step that did not work is said on
/// standard output and in `voice/setup.log`, and never fails an install.
///
/// **On Windows** a desktop program cannot raise the system's prompt, so when a switch refuses the microphone
/// a message box says which one and offers to open its page. **On macOS** `access::request` shows the
/// system's own prompt the first time.
pub fn set_up_from_the_installer() -> i32 {
    let mut said = Vec::new();
    match unluminous_voice::install::install(&|step, done| {
        if done >= 1.0 {
            println!("{step}: done");
        }
    }) {
        Ok(installed) => said.push(format!(
            "speech model: {}",
            installed.model.map(|m| m.display().to_string()).unwrap_or_default()
        )),
        Err(problem) => said.push(format!("speech model: not downloaded: {problem}")),
    }
    match unluminous_voice::access::status() {
        unluminous_voice::access::Access::Refused(why) => {
            said.push(format!("microphone: refused: {why}"));
            let open = rfd::MessageDialog::new()
                .set_title("Unluminous: microphone")
                .set_description(format!("Unluminous can turn what you say into text in the Agent-Chat prompt, and listen for \"Sable\" if you turn that on. {why}

Open the microphone settings now?"))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if open == rfd::MessageDialogResult::Yes {
                let _ = unluminous_voice::access::open_settings();
            }
        }
        // On macOS this program is only ever started by `unluminous-cli setup`, very often from a terminal, and
        // macOS records a microphone request against the terminal that started it. It also exits before the
        // answer comes back. So the window's Set Up Unluminous dialog asks, in Unluminous's own process.
        // `task-2301`.
        unluminous_voice::access::Access::NotAsked if cfg!(target_os = "macos") => {
            said.push(
                "microphone: not asked yet; Set Up Unluminous asks the next time Unluminous opens"
                    .to_owned(),
            );
        }
        unluminous_voice::access::Access::NotAsked => {
            unluminous_voice::access::request();
            said.push("microphone: asked".to_owned());
        }
        other => said.push(format!("microphone: {}", other.name())),
    }
    for line in &said {
        println!("{line}");
    }
    let log = unluminous_voice::install::home().join("setup.log");
    let _ = std::fs::create_dir_all(unluminous_voice::install::home());
    let _ = crate::services::store::write_atomically(
        &log,
        (said.join(
            "
",
        ) + "
")
        .as_bytes(),
    );
    0
}
