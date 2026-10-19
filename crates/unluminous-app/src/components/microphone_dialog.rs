//! The dialog the microphone button opens when dictation cannot start. `task-2290`.
//!
//! Two reasons, one dialog: the system refuses Unluminous the microphone, or the local speech model is not
//! downloaded yet. *"Make sure our install prompts the user for mic access, same if they didn't allow it
//! initially but then click the mic icon."* The installer asks once; this is the second half, and it says
//! which switch is off and opens the page that has it, because a desktop program on Windows cannot raise the
//! prompt itself.

use egui::{Pos2, Rect};

use crate::components::modal;

const WIDTH: f32 = 520.0;
const HEIGHT: f32 = 260.0;

/// Why dictation could not start.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceDialog {
    /// The microphone is refused, with the sentence saying which switch refuses it.
    Microphone(String),
    /// The local model is missing, with how far a download has got when one is running, and why the last
    /// download failed when it did.
    Model { downloading: Option<String>, failed: Option<String> },
}

/// What the dialog asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Outcome {
    #[default]
    Nothing,
    Close,
    /// Open the system's microphone privacy page.
    OpenSettings,
    /// Try dictation again, after the switch was turned on.
    TryAgain,
    /// Download the local model.
    Download,
    /// Open Settings, Voice, to choose an endpoint instead.
    VoiceSettings,
}

/// Draw the dialog and say what was pressed.
/// @param ctx - the window
/// @param dialog - why it is open
pub fn show(ctx: &egui::Context, dialog: &VoiceDialog) -> Outcome {
    let mut outcome = Outcome::Nothing;
    let (_, closed) = modal::show(ctx, "unluminous-voice", WIDTH, HEIGHT, |ui, area| {
        let (title, said, buttons): (&str, String, Vec<(&str, bool)>) = match dialog {
            VoiceDialog::Microphone(why) => (
                "Microphone access",
                format!("{why}\n\nUnluminous uses the microphone only while you dictate into the Agent-Chat prompt, or while it listens for \"Sable\". Turn access on, then press Try Again."),
                vec![("Open Microphone Settings", true), ("Try Again", true)],
            ),
            VoiceDialog::Model { downloading, failed } => (
                "Speech model",
                match (downloading, failed) {
                    (Some(progress), _) => format!("{progress}\n\nThe words will fill the prompt once it is ready."),
                    // `task-2302`: a failure is said here, because the toast that said it is drawn under the
                    // dialog and under the ticket it was opened from, so the person saw nothing happen.
                    (None, Some(why)) => format!("The download failed: {why}\n\nPress Try Again, or send speech to a transcription endpoint instead, on Settings, Voice."),
                    (None, None) => "Dictation on this machine needs the Moonshine speech model: 66 MB, downloaded once from Hugging Face and checked before it is used. Nothing you say leaves this machine.\n\nOr send speech to a transcription endpoint instead, on Settings, Voice.".to_owned(),
                },
                vec![
                    ("Voice Settings", true),
                    (if failed.is_some() { "Try Again" } else { "Download" }, downloading.is_none()),
                ],
            ),
        };
        let crossed = modal::header(ui, area, title);
        let body = modal::body(area);
        modal::note(
            ui,
            Rect::from_min_max(Pos2::new(body.left(), body.top()), body.max),
            body.top() + 4.0,
            &said,
        );
        outcome = match (dialog, modal::footer(ui, area, &buttons)) {
            (VoiceDialog::Microphone(_), Some(0)) => Outcome::OpenSettings,
            (VoiceDialog::Microphone(_), Some(_)) => Outcome::TryAgain,
            (VoiceDialog::Model { .. }, Some(0)) => Outcome::VoiceSettings,
            (VoiceDialog::Model { .. }, Some(_)) => Outcome::Download,
            (_, None) if crossed => Outcome::Close,
            _ => Outcome::Nothing,
        };
    });
    match (closed, outcome) {
        (true, Outcome::Nothing) => Outcome::Close,
        _ => outcome,
    }
}
