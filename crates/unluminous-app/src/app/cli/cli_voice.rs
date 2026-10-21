//! `voice`: speech to text in the Agent-Chat prompt, and listening for "Sable". `task-2290`.
//!
//! The agent's half of the microphone button, the Voice settings page and the Chat settings' switch, through
//! the same functions those use: `start_dictation`, `stop_dictation` and `set_settings`. `voice transcribe`
//! and `voice hear` take a WAV file through the same engine the microphone feeds, which is how both engines
//! and the wake word are driven with no microphone.

use super::*;
use crate::app::voice::Target;

/// How long `voice transcribe` waits for its words when `--timeout` was not given: two minutes.
const TRANSCRIBE_WAIT: Duration = Duration::from_secs(120);

impl UnluminousApp {
    /// Answers a `voice` command.
    /// @param request - the command and its arguments
    /// @param verb - the verb after `voice.`
    pub(crate) fn cli_voice(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "status" => ok(request, self.voice_sentence(), self.voice_status_value()),
            "dictate" => self.cli_voice_dictate(request),
            "listen" => self.cli_voice_listen(request),
            "transcribe" => self.cli_voice_transcribe(request),
            "hear" => self.cli_voice_hear(request),
            "install" => {
                let installed = unluminous_voice::install::status();
                if installed.is_complete() {
                    return ok(
                        request,
                        match installed.packaged {
                            true => "The speech model and ONNX Runtime came with Unluminous.",
                            false => "The speech model and ONNX Runtime are already installed.",
                        },
                        self.voice_status_value(),
                    );
                }
                self.start_the_voice_download();
                ok(request, format!("Downloading what the local engine needs ({} MB). `voice status` says how far it got.", installed.download_megabytes()), self.voice_status_value())
            }
            "microphone" => self.cli_voice_microphone(request),
            _ => no(
                request,
                code::UNKNOWN_COMMAND,
                format!("There is no command called {}.", request.command),
            ),
        }
    }

    /// `voice dictate [on|off] [--from <node>]`.
    fn cli_voice_dictate(&mut self, request: &Request) -> Outcome {
        let target = match (request.whole("from"), request.text("into").as_deref()) {
            (Some(node), _) => Target::Node(node as crate::services::realm::NodeId),
            (None, Some("description")) => Target::Description,
            (None, Some(other)) => {
                return no(
                    request,
                    code::USAGE,
                    format!("`--into` takes description, the open ticket's, and not {other}."),
                )
            }
            (None, None) => Target::Pane,
        };
        let here = self.voice.dictating.is_some_and(|(_, t)| t == target);
        let on = match request.text("state").as_deref() {
            Some("on") => true,
            Some("off") => false,
            _ => !here,
        };
        if !on {
            if self.voice.dictating.is_some() {
                self.stop_dictation(request.switch("send"));
                return ok(
                    request,
                    "Dictation stopped; its last words still arrive in the prompt.",
                    self.voice_status_value(),
                );
            }
            return ok(request, "Nothing was being dictated.", self.voice_status_value());
        }
        match self.start_dictation(target) {
            Ok(tag) => ok(
                request,
                "Dictating: the words fill the prompt as they are spoken.",
                json!({ "dictation": tag, "status": self.voice_status_value() }),
            ),
            Err(problem) => no(request, code::FAILED, problem),
        }
    }

    /// `voice listen [on|off]`, which is the `voice.always_listen` setting.
    fn cli_voice_listen(&mut self, request: &Request) -> Outcome {
        let on = match request.text("state").as_deref() {
            Some("on") => true,
            Some("off") => false,
            _ => !self.settings.voice_always_listen,
        };
        let mut settings = self.settings.clone();
        settings.voice_always_listen = on;
        self.set_settings(settings);
        let said = match on {
            true => "Listening for \"Sable\": say it, then what you want, and the words fill the Agent-Chat prompt.",
            false => "No longer listening for \"Sable\". The microphone is closed unless dictation is on.",
        };
        ok(request, said, json!({ "listening": on }))
    }

    /// `voice transcribe <path> [--live]`: held until the words are back.
    fn cli_voice_transcribe(&mut self, request: &Request) -> Outcome {
        let samples = match self.read_a_recording(request) {
            Ok(samples) => samples,
            Err(problem) => return no(request, code::FAILED, problem),
        };
        if self.voice.dictating.is_some() {
            return no(
                request,
                code::FAILED,
                "dictation is in progress; `voice dictate off` first",
            );
        }
        let live = request.switch("live");
        let target = if live { Target::Pane } else { Target::Nowhere };
        let tag = self.transcribe_samples(samples, live, target);
        let wait = request
            .whole("timeout")
            .map(|ms| Duration::from_millis(ms as u64))
            .unwrap_or(TRANSCRIBE_WAIT);
        Outcome::Hold(Waiting::VoiceTranscribe { tag, until: Instant::now() + wait })
    }

    /// `voice hear <path>`.
    fn cli_voice_hear(&mut self, request: &Request) -> Outcome {
        let samples = match self.read_a_recording(request) {
            Ok(samples) => samples,
            Err(problem) => return no(request, code::FAILED, problem),
        };
        let seconds = samples.len() as f32 / 16_000.0;
        let listening = self.settings.voice_always_listen;
        match request.switch("live") {
            true => self.voice_engine().hear_live(samples),
            false => self.voice_engine().hear(samples),
        }
        let said = match (listening, self.voice.dictating.is_some()) {
            (true, _) => format!("Heard {seconds:.1} s through the wake word detector. `voice status` says whether it woke."),
            (false, true) => format!("Heard {seconds:.1} s into the dictation in progress."),
            (false, false) => format!("Heard {seconds:.1} s, but always listen is off and nothing is being dictated, so it went nowhere."),
        };
        ok(request, said, self.voice_status_value())
    }

    /// `voice microphone [ask|open-settings]`.
    fn cli_voice_microphone(&mut self, request: &Request) -> Outcome {
        match request.text("action").as_deref() {
            Some("ask") => unluminous_voice::access::request(),
            Some("open-settings") => {
                if let Err(problem) = unluminous_voice::access::open_settings() {
                    return no(request, code::FAILED, problem);
                }
            }
            _ => {}
        }
        let access = unluminous_voice::access::status();
        // Every input device, and which one `voice.microphone` chooses, so an agent can choose another with
        // `settings set voice.microphone <name>`. `task-2320`.
        let devices = unluminous_voice::capture::device_names();
        let default = unluminous_voice::capture::default_device_name();
        let chosen = self.settings.voice_microphone.clone();
        let connected = chosen.is_empty() || devices.contains(&chosen);
        let in_use = match connected && !chosen.is_empty() {
            true => Some(chosen.clone()),
            false => default.clone(),
        };
        let mut said = match &access {
            unluminous_voice::access::Access::Refused(why) => why.clone(),
            other => format!("Microphone access: {}.", other.name()),
        };
        match (&in_use, connected) {
            (Some(name), true) => said.push_str(&format!(" Listening through {name}.")),
            (Some(name), false) => {
                said.push_str(&format!(" {chosen} is not connected, so {name} is used."))
            }
            (None, _) => said.push_str(" No microphone is connected."),
        }
        let listed: Vec<Value> = devices
            .iter()
            .map(|name| {
                json!({
                    "name": name,
                    "default": default.as_deref() == Some(name.as_str()),
                    "chosen": *name == chosen,
                })
            })
            .collect();
        ok(
            request,
            said,
            json!({
                "access": access.name(),
                "device": in_use,
                "default": default,
                "chosen": (!chosen.is_empty()).then_some(chosen),
                "connected": connected,
                "devices": listed,
            }),
        )
    }

    /// The WAV file a command names, as 16 kHz samples.
    fn read_a_recording(&self, request: &Request) -> Result<Vec<i16>, String> {
        let path = self.cli_path_argument(request, "path").ok_or("name a WAV file")?;
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("{} could not be read: {e}", path.display()))?;
        unluminous_voice::audio::wav_decode(&bytes)
    }

    /// The answer to a held `voice transcribe`, once its words are back.
    pub(crate) fn voice_transcribe_answer(&mut self, request: &Request, tag: u64) -> Option<Reply> {
        let finished = self.voice.results.remove(&tag)?;
        Some(Reply::done(
            &request.command,
            finished.text.clone(),
            json!({ "text": finished.text, "ended": finished.ended, "engine": self.settings.voice_engine.name() }),
        ))
    }

    /// One sentence for `voice status`.
    fn voice_sentence(&self) -> String {
        let engine = self.settings.voice_engine.title();
        match (self.voice.dictating, self.settings.voice_always_listen) {
            (Some(_), _) => format!("Dictating, with the engine {engine}."),
            (None, true) => format!("Listening for \"Sable\", with the engine {engine}."),
            (None, false) => format!("Idle. The engine is {engine}."),
        }
    }

    /// Everything `voice status` answers.
    pub(crate) fn voice_status_value(&self) -> Value {
        let snapshot =
            self.voice.engine.as_ref().map(|engine| engine.snapshot()).unwrap_or_default();
        let installed = unluminous_voice::install::status();
        let target = self.voice.dictating.map(|(_, target)| match target {
            Target::Pane => json!("pane"),
            Target::Node(node) => json!({ "node": node }),
            Target::Description => json!("description"),
            Target::Nowhere => json!(null),
        });
        json!({
            "engine": self.settings.voice_engine.name(),
            "endpoint": self.settings.voice_endpoint,
            "listening": self.settings.voice_always_listen,
            "wakeDetectorRunning": snapshot.listening,
            "listeningInThisWindow": self.voice.listening_here,
            "wakeModelLoaded": snapshot.wake_loaded,
            "vadLoaded": snapshot.vad_loaded,
            "dictating": target,
            "words": snapshot.text,
            "microphone": snapshot.microphone,
            "microphoneAccess": unluminous_voice::access::status().name(),
            "refused": snapshot.refused,
            "level": snapshot.level,
            "model": {
                "folder": installed.model.as_ref().map(|path| path.display().to_string()),
                "runtime": installed.runtime.as_ref().map(|path| path.display().to_string()),
                "downloaded": installed.is_complete(),
                "packaged": installed.packaged,
                "loaded": snapshot.model.loaded,
                "loads": snapshot.model.loads,
                "lastLoadMs": snapshot.model.last_load.map(|took| took.as_millis() as u64),
                "lastPassMs": snapshot.last_pass.map(|took| took.as_millis() as u64),
                "download": self.download_progress(),
            },
            "problem": snapshot.problem.or_else(|| self.voice.problem.clone()),
        })
    }
}
