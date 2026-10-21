//! `Settings -> Voice`: speech to text for the Agent-Chat prompt. `task-2290`.
//!
//! Five cards in the Settings design's language, built from `components::settings_kit`: the engine, the
//! endpoint, the microphone (which device, and access), the microphone button, and always listen for "Sable". Every value is one of the `voice.*` settings, so
//! the page, `settings set` and the Chat settings' switch are three ways of writing the same line.
//!
//! What the page cannot know on its own (whether the model is on disk, how far a download has got, whether
//! the system lets Unluminous use the microphone) is handed in as [`VoiceFacts`], and what it asks for that is
//! not a setting comes back as [`VoiceAsk`].

use egui::Rect;

use crate::components::settings_dialog::{drawn, Drawn};
use crate::components::settings_kit::{self as kit, Column};
use crate::settings::{Settings, VoiceEngine};

/// What the window knows about speech to text, for the page to show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoiceFacts {
    /// One line about the local model: on disk, loaded, missing, or how far a download is.
    pub model: String,
    /// Whether a download could be started now: the model or the runtime is missing and none is running.
    pub may_download: bool,
    /// One line about the microphone: the device, allowed, or why it is refused.
    pub microphone: String,
    /// Whether the system refuses the microphone, which is when the page offers its settings page.
    pub refused: bool,
    /// The input devices on this machine, by name, which the Microphone dropdown offers after the
    /// system's default (`task-2320`).
    pub microphones: Vec<String>,
}

/// What the Microphone dropdown offers: the system's default first, then every device, then the chosen one
/// when it is not connected, so the dropdown still says what is chosen. Answers the words and which is chosen.
/// @param devices - the devices on this machine
/// @param chosen - `voice.microphone`, empty for the default
pub fn microphone_choices(devices: &[String], chosen: &str) -> (Vec<String>, usize) {
    let mut words = vec![DEFAULT_MICROPHONE.to_owned()];
    words.extend(devices.iter().cloned());
    let index = match chosen.is_empty() {
        true => 0,
        false => match devices.iter().position(|device| device == chosen) {
            Some(at) => at + 1,
            None => {
                words.push(format!("{chosen} (not connected)"));
                words.len() - 1
            }
        },
    };
    (words, index)
}

/// The first row of the Microphone dropdown.
pub const DEFAULT_MICROPHONE: &str = "System default";

/// What the page asked for that is not a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceAsk {
    /// Download what the local engine needs.
    Download,
    /// Open the system's microphone privacy page.
    MicrophoneSettings,
}

/// What the page reports: the usual [`Drawn`], and anything it asked for.
pub(crate) struct VoicePage {
    pub(crate) drawn: Drawn,
    pub ask: Option<VoiceAsk>,
}

/// Draw the page.
/// @param ui - the dialog's page area
/// @param area - where the cards go
/// @param settings - the settings, written in place
/// @param facts - what the window knows
pub(crate) fn show(
    ui: &mut egui::Ui,
    area: Rect,
    settings: &mut Settings,
    facts: &VoiceFacts,
) -> VoicePage {
    let mut changed = false;
    let mut ask = None;
    let mut column = Column::new(area.left(), area.right(), area.top());
    kit::card(ui, &mut column, "Speech to text", None, |card| {
        let names: Vec<&str> = VoiceEngine::ALL.iter().map(|engine| engine.title()).collect();
        let chosen = VoiceEngine::ALL.iter().position(|engine| *engine == settings.voice_engine);
        card.row("Engine", Some("The microphone button inside the left of the Agent-Chat prompt fills the prompt as you speak. On this machine is Moonshine, which runs locally and sends nothing anywhere; an endpoint is any OpenAI compatible transcription URL."), |row| {
            if let Some(index) = row.segmented("Speech to text engine", &names, chosen, 120.0, false) {
                settings.voice_engine = VoiceEngine::ALL[index];
                changed = true;
            }
        });
        card.row("Local model", Some("Moonshine base, 66 MB on disk and about 200 MB in memory while it is used. It is loaded when dictation starts and let go after the time below with nothing said."), |row| {
            row.status(&facts.model);
            if facts.may_download && row.button("Download") {
                ask = Some(VoiceAsk::Download);
            }
        });
        let mut idle = settings.voice_model_idle;
        card.row(
            "Keep it loaded for",
            Some(&format!("{:.0} seconds after the last words.", settings.voice_model_idle)),
            |row| {
                if row.slider("Speech model kept for", &mut idle, 10.0, 600.0) {
                    settings.voice_model_idle = idle.round();
                    changed = true;
                }
            },
        );
    });
    kit::card(ui, &mut column, "Endpoint", None, |card| {
        let before = (
            settings.voice_endpoint.clone(),
            settings.voice_endpoint_model.clone(),
            settings.voice_endpoint_key.clone(),
            settings.voice_language.clone(),
        );
        card.row("URL", Some("An OpenAI compatible /v1/audio/transcriptions. The audio goes as a WAV built in memory."), |row| {
            row.code_text("Transcription endpoint", &mut settings.voice_endpoint, "http://localhost:4141/v1/audio/transcriptions", 340.0);
        });
        card.row("Model", None, |row| {
            row.code_text(
                "Transcription model",
                &mut settings.voice_endpoint_model,
                "whisper-1",
                200.0,
            );
        });
        card.row("Key", Some("The name of an environment variable holding the key, never the key. Empty for an endpoint that needs none."), |row| {
            row.code_text("Transcription key variable", &mut settings.voice_endpoint_key, "OPENAI_API_KEY", 200.0);
        });
        card.row("Language", None, |row| {
            row.code_text("Transcription language", &mut settings.voice_language, "en", 80.0);
        });
        changed |= before
            != (
                settings.voice_endpoint.clone(),
                settings.voice_endpoint_model.clone(),
                settings.voice_endpoint_key.clone(),
                settings.voice_language.clone(),
            );
    });
    kit::card(ui, &mut column, "Microphone", None, |card| {
        let (choices, chosen) = microphone_choices(&facts.microphones, &settings.voice_microphone);
        card.row("Input", Some("The microphone dictation and \"Sable\" listen through. System default follows the system when its default device changes; a device that is unplugged falls back to the default until it is back."), |row| {
            if let Some(index) = row.select("Microphone", &choices, Some(chosen), 300.0) {
                let wanted = match index {
                    0 => String::new(),
                    at => facts.microphones.get(at - 1).cloned().unwrap_or_else(|| settings.voice_microphone.clone()),
                };
                if wanted != settings.voice_microphone {
                    settings.voice_microphone = wanted;
                    changed = true;
                }
            }
        });
        card.row("Access", Some("The microphone is open only while dictating or while always listen is on, so the system's microphone light is off otherwise."), |row| {
            row.status(&facts.microphone);
            if facts.refused && row.button("Open Microphone Settings") {
                ask = Some(VoiceAsk::MicrophoneSettings);
            }
        });
    });
    kit::card(ui, &mut column, "Microphone button", None, |card| {
        changed |= card.switch("Show the microphone in the Agent-Chat prompt", Some("Off removes the button. Dictation is still reached through \"Sable\" and `voice dictate`."), &mut settings.voice_button);
    });
    kit::card(ui, &mut column, "Listen for Sable", None, |card| {
        changed |= card.switch("Always listen for \"Sable\"", Some("Say \"Sable\" and then what you want: the Agent-Chat pane opens and the words fill its prompt. Off by default. A voice activity detector keeps the wake word model idle in a quiet room. With several windows open, the one you used last is the one that listens."), &mut settings.voice_always_listen);
        changed |= card.switch("Send when I stop talking", Some("After \"Sable\", the message is sent at the first pause once something was said. Off leaves it in the prompt."), &mut settings.voice_send_after_wake);
        // `task-2293`: how long that pause is, in seconds. It also ends a dictation whose message is not sent.
        let mut pause = settings.voice_send_pause;
        card.row(
            "Pause before sending",
            Some("How many seconds of quiet, once something was said, end what \"Sable\" heard and send it."),
            |row| {
                if row.slider("Pause before sending", &mut pause, 0.7, 10.0) {
                    settings.voice_send_pause = (pause * 10.0).round() / 10.0;
                    changed = true;
                }
                row.suffix(&format!("{:.1} s", settings.voice_send_pause));
            },
        );
        let mut sensitivity = settings.voice_wake_sensitivity;
        card.row(
            "Sensitivity",
            Some("Higher wakes more easily, and more often by mistake."),
            |row| {
                if row.slider("Wake word sensitivity", &mut sensitivity, 0.0, 1.0) {
                    settings.voice_wake_sensitivity = (sensitivity * 20.0).round() / 20.0;
                    changed = true;
                }
            },
        );
    });
    VoicePage { drawn: drawn(area.top(), &column, changed), ask }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn devices() -> Vec<String> {
        vec!["MacBook Pro Microphone".to_owned(), "Headset (USB Audio)".to_owned()]
    }

    #[test]
    fn the_system_default_is_first_and_chosen_when_nothing_is_named() {
        let (words, chosen) = microphone_choices(&devices(), "");
        assert_eq!(words, ["System default", "MacBook Pro Microphone", "Headset (USB Audio)"]);
        assert_eq!(chosen, 0);
    }

    #[test]
    fn a_named_device_is_chosen_by_its_place_after_the_default() {
        let (_, chosen) = microphone_choices(&devices(), "Headset (USB Audio)");
        assert_eq!(chosen, 2);
    }

    #[test]
    fn a_device_that_is_unplugged_stays_in_the_list_and_says_so() {
        let (words, chosen) = microphone_choices(&devices(), "Blue Yeti");
        assert_eq!(words.last().map(String::as_str), Some("Blue Yeti (not connected)"));
        assert_eq!(chosen, words.len() - 1);
    }
}
