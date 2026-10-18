//! The engines against real audio: Sable's recordings, through the same worker the microphone feeds.
//!
//! The local model tests need the model and ONNX Runtime installed (`unluminous-cli voice install`, or
//! `unluminous --setup-voice`). Without them they print that they did nothing and pass, unless
//! `UNLUMINOUS_REQUIRE_VOICE=1`, which makes a missing model a failure; the release sets it. The endpoint test
//! runs when `UNLUMINOUS_VOICE_ENDPOINT` names an OpenAI compatible transcription URL, such as ai-proxy's
//! `http://localhost:4141/v1/audio/transcriptions`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use unluminous_voice::endpoint::Endpoint;
use unluminous_voice::segmenter::{Ended, Started};
use unluminous_voice::transcriber::Engine;
use unluminous_voice::{Config, Event, Voice};

/// One of the recordings in `tests/audio`, as 16 kHz samples.
fn clip(name: &str) -> Vec<i16> {
    let bytes =
        std::fs::read(format!("{}/tests/audio/{name}.wav", env!("CARGO_MANIFEST_DIR"))).unwrap();
    unluminous_voice::audio::wav_decode(&bytes).unwrap()
}

/// Whether the local engine can run here, failing when the release asked for it to be.
fn local_engine_is_installed() -> bool {
    let installed = unluminous_voice::install::status().is_complete();
    if !installed {
        assert!(
            std::env::var("UNLUMINOUS_REQUIRE_VOICE").is_err(),
            "UNLUMINOUS_REQUIRE_VOICE is set and the speech model is not installed"
        );
        eprintln!("the speech model is not installed, so this test did nothing");
    }
    installed
}

/// Waits for the dictation with `tag` to end, collecting every set of words on the way.
fn until_ended(voice: &Voice, tag: u64) -> (Vec<String>, String, Ended, Started) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut words = Vec::new();
    while Instant::now() < deadline {
        for event in voice.take_events() {
            match event {
                Event::Words { tag: t, text } if t == tag => words.push(text),
                Event::Ended { tag: t, text, ended, started } if t == tag => {
                    return (words, text, ended, started)
                }
                Event::Problem(problem) => panic!("{problem}"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("dictation {tag} did not end within a minute");
}

/// The tag of the dictation a wake started, when one did.
fn woke(voice: &Voice) -> Option<u64> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        for event in voice.take_events() {
            if let Event::Woke { tag, .. } = event {
                return Some(tag);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

#[test]
fn the_local_model_writes_a_recording_out_as_words() {
    if !local_engine_is_installed() {
        return;
    }
    let voice = Voice::start(Arc::new(|| {}));
    voice.configure(Config::default());
    let tag = voice.transcribe(clip("sable-tell-me-a-long-story"), false);
    let (_, text, ended, _) = until_ended(&voice, tag);
    assert_eq!(ended, Ended::EndOfFile);
    assert!(text.to_lowercase().contains("lighthouse keeper"), "{text}");
    assert!(voice.snapshot().model.loaded, "the model is still loaded straight after a pass");
}

#[test]
fn spoken_live_the_words_arrive_while_the_recording_plays() {
    if !local_engine_is_installed() {
        return;
    }
    let voice = Voice::start(Arc::new(|| {}));
    voice.configure(Config::default());
    let tag = voice.transcribe(clip("sable-tell-me-a-long-story"), true);
    let (words, text, _, _) = until_ended(&voice, tag);
    assert!(words.len() >= 3, "partials arrive as it plays: {words:?}");
    assert!(
        words.first().unwrap().len() < text.len(),
        "the first words are a start of the last ones: {words:?}"
    );
}

#[test]
fn always_listening_a_wake_becomes_dictation_without_the_word_sable() {
    if !local_engine_is_installed() {
        return;
    }
    let voice = Voice::start(Arc::new(|| {}));
    voice.configure(Config { always_listen: true, ..Config::default() });
    voice.hear(clip("sable-what-time-is-it"));
    let tag = woke(&voice).expect("\"Sable, what time is it\" wakes it");
    let (_, text, ended, started) = until_ended(&voice, tag);
    assert_eq!(started, Started::Wake);
    assert_eq!(ended, Ended::Finished);
    let lower = text.to_lowercase();
    assert!(lower.contains("what time"), "{text}");
    assert!(!lower.contains("sable"), "the wake word is taken off: {text}");
}

#[test]
fn always_listening_a_sentence_without_sable_starts_nothing() {
    let voice = Voice::start(Arc::new(|| {}));
    voice.configure(Config { always_listen: true, ..Config::default() });
    voice.hear(clip("and-what-day-is-it"));
    assert_eq!(woke(&voice), None);
}

#[test]
fn an_endpoint_writes_a_recording_out_as_words() {
    let Ok(url) = std::env::var("UNLUMINOUS_VOICE_ENDPOINT") else {
        eprintln!("UNLUMINOUS_VOICE_ENDPOINT is not set, so this test did nothing");
        return;
    };
    let voice = Voice::start(Arc::new(|| {}));
    let endpoint = Endpoint {
        url,
        model: "whisper-1".into(),
        language: "en".into(),
        key_variable: String::new(),
    };
    voice.configure(Config { engine: Engine::Endpoint(endpoint), ..Config::default() });
    let tag = voice.transcribe(clip("sable-tell-me-a-long-story"), false);
    let (_, text, _, _) = until_ended(&voice, tag);
    assert!(text.to_lowercase().contains("lighthouse"), "{text}");
}
