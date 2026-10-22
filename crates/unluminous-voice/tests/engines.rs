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

/// Jason, on `task-2290`: *"When I use sable, the first time it works, but then there are times when the
/// transcribed message doesn't get sent."* Two requests, each "Sable" and a question, with the pauses a
/// person leaves, heard at the speed they were spoken. Each must wake, and each must end at its pause as
/// `Finished`, which is the ending the window sends on.
#[test]
fn saying_sable_twice_in_real_time_ends_both_dictations_ready_to_send() {
    if !local_engine_is_installed() {
        return;
    }
    let voice = Voice::start(Arc::new(|| {}));
    voice.configure(Config { always_listen: true, ..Config::default() });
    let silence = |seconds: f32| vec![0i16; (seconds * 16_000.0) as usize];
    let mut recording = silence(1.0);
    recording.extend(clip("sable-what-time-is-it"));
    recording.extend(silence(3.0));
    recording.extend(clip("sable-tell-me-a-long-story"));
    voice.hear_live(recording);
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut ended = Vec::new();
    let mut woke = 0;
    while Instant::now() < deadline && ended.len() < 2 {
        for event in voice.take_events() {
            eprintln!("{:?} {event:?}", Instant::now());
            match event {
                Event::Woke { .. } => woke += 1,
                Event::Ended { text, ended: why, started, .. } => ended.push((text, why, started)),
                Event::Problem(problem) => panic!("{problem}"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!("woke {woke} times; ended {ended:?}");
    assert_eq!(woke, 2, "both requests woke it");
    assert_eq!(ended.len(), 2, "both dictations ended: {ended:?}");
    for (text, why, started) in &ended {
        assert_eq!(*started, Started::Wake);
        assert_eq!(*why, Ended::Finished, "each ends at its own pause, ready to send: {text:?}");
        assert!(!text.is_empty() && !text.to_lowercase().starts_with("sable"), "{text:?}");
    }
}

/// Waits up to ten seconds for the snapshot to say `wanted`.
///
/// Two seconds was not always long enough to load the wake models while the release ran every other crate's
/// tests on the same processors, and the release stopped on it twice in `task-2323`. A snapshot that arrives
/// sooner still returns at once.
fn until(voice: &Voice, wanted: impl Fn(&unluminous_voice::Snapshot) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if wanted(&voice.snapshot()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Jason, on `task-2290`: *"Verify that any model used for "sable" detection is not loaded when that toggle is
/// disabled, and unloads immediately when toggled off."* Nothing is in memory with the toggle off; the wake
/// models and the detector load while listening; and switching it off drops them, and the speech model, at once.
#[test]
fn nothing_for_sable_is_in_memory_with_the_toggle_off_and_it_all_goes_when_switched_off() {
    if !local_engine_is_installed() {
        return;
    }
    let voice = Voice::start(Arc::new(|| {}));
    voice.configure(Config::default());
    std::thread::sleep(Duration::from_millis(200));
    let idle = voice.snapshot();
    assert!(
        !idle.wake_loaded && !idle.vad_loaded && !idle.model.loaded,
        "nothing loaded with the toggle off: {idle:?}"
    );

    // Listening: the wake models and the detector are loaded while audio is being heard.
    voice.configure(Config { always_listen: true, ..Config::default() });
    voice.hear_live(clip("sable-what-time-is-it"));
    assert!(
        until(&voice, |s| s.wake_loaded && s.vad_loaded),
        "the wake models load while listening"
    );
    assert!(
        until(&voice, |s| s.model.loaded),
        "the wake started a dictation, which loads the speech model"
    );
    // Let the dictation end, so nothing is holding the speech model.
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline
        && !voice.take_events().iter().any(|e| matches!(e, Event::Ended { .. }))
    {
        std::thread::sleep(Duration::from_millis(20));
    }

    // Switched off: everything goes at once, not after the speech model's idle time.
    let switched = Instant::now();
    voice.configure(Config::default());
    assert!(
        until(&voice, |s| !s.wake_loaded && !s.vad_loaded && !s.model.loaded),
        "all of it is released: {:?}",
        voice.snapshot()
    );
    eprintln!("released {} ms after the switch", switched.elapsed().as_millis());
    assert!(switched.elapsed() < Duration::from_secs(2));
}
