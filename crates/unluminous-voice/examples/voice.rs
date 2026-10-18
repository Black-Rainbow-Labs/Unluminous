//! The voice engine with no window: install what the local engine needs, transcribe a WAV, or measure.
//!
//! ```text
//! cargo run --release -p unluminous-voice --example voice -- install
//! cargo run --release -p unluminous-voice --example voice -- transcribe <file.wav> [--endpoint <url>]
//! cargo run --release -p unluminous-voice --example voice -- bench <file.wav> [threads]
//! ```
//!
//! `bench` loads Moonshine once and times a pass over the whole file, which is the cost of one partial
//! for a segment that long.

use std::sync::Arc;
use std::time::{Duration, Instant};

use unluminous_voice::endpoint::Endpoint;
use unluminous_voice::transcriber::Engine;
use unluminous_voice::{Config, Event, Voice};

/// Runs the command the arguments name.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.first().map(String::as_str) {
        Some("install") => install(),
        Some("transcribe") => transcribe(&arguments[1..]),
        Some("bench") => bench(&arguments[1..]),
        Some("memory") => memory(&arguments[1..]),
        _ => Err("usage: voice install | transcribe <file.wav> [--endpoint <url>] | bench <file.wav> [threads]".to_owned()),
    };
    if let Err(problem) = result {
        eprintln!("{problem}");
        std::process::exit(1);
    }
}

/// Downloads whatever is missing and says where everything is.
fn install() -> Result<(), String> {
    let started = Instant::now();
    let installed = unluminous_voice::install::install(&|said, done| {
        eprint!("\r{said}: {:>3.0}%   ", done * 100.0)
    })?;
    eprintln!();
    println!(
        "model {}\nruntime {}\nin {:.1} s",
        installed.model.unwrap_or_default().display(),
        installed.runtime.unwrap_or_default().display(),
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

/// Reads a WAV file as 16 kHz samples.
/// @param path - the file
fn read(path: Option<&String>) -> Result<Vec<i16>, String> {
    let path = path.ok_or("name a WAV file")?;
    unluminous_voice::audio::wav_decode(&std::fs::read(path).map_err(|e| format!("{path}: {e}"))?)
}

/// Transcribes a file through the engine, printing each set of words as it arrives.
/// @param arguments - the file, and `--endpoint <url>` for the endpoint engine
fn transcribe(arguments: &[String]) -> Result<(), String> {
    let samples = read(arguments.first())?;
    let engine = match arguments.iter().position(|a| a == "--endpoint") {
        Some(at) => Engine::Endpoint(Endpoint {
            url: arguments.get(at + 1).cloned().unwrap_or_default(),
            model: "whisper-1".into(),
            language: "en".into(),
            key_variable: String::new(),
        }),
        None => Engine::Local { threads: 2 },
    };
    let voice = Voice::start(Arc::new(|| {}));
    voice.configure(Config { engine, ..Config::default() });
    let started = Instant::now();
    let tag = voice.transcribe(samples, true);
    loop {
        for event in voice.take_events() {
            match event {
                Event::Words { tag: t, text } if t == tag => {
                    println!("{:>6} ms  {text}", started.elapsed().as_millis())
                }
                Event::Ended { tag: t, text, ended, .. } if t == tag => {
                    println!(
                        "{:>6} ms  ended ({}): {text}",
                        started.elapsed().as_millis(),
                        ended.name()
                    );
                    return Ok(());
                }
                Event::Problem(problem) => eprintln!("problem: {problem}"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Prints a line a phase so a sampler outside the process can read its memory in each: always listen off,
/// on with a "Sable" request heard in real time, and off again. `task-2290`.
/// @param arguments - a WAV file that says "Sable" and something
fn memory(arguments: &[String]) -> Result<(), String> {
    let samples = read(arguments.first())?;
    let say = |phase: &str, voice: &Voice| {
        let s = voice.snapshot();
        println!(
            "phase {phase} pid {} wake {} vad {} model {}",
            std::process::id(),
            s.wake_loaded,
            s.vad_loaded,
            s.model.loaded
        );
    };
    let voice = Voice::start(Arc::new(|| {}));
    voice.configure(Config::default());
    std::thread::sleep(Duration::from_secs(5));
    say("off", &voice);
    voice.configure(Config { always_listen: true, ..Config::default() });
    voice.hear_live(samples);
    std::thread::sleep(Duration::from_secs(9));
    say("on", &voice);
    voice.configure(Config::default());
    std::thread::sleep(Duration::from_secs(5));
    say("off-again", &voice);
    Ok(())
}

/// Times passes of Moonshine over a whole file.
/// @param arguments - the file and how many threads
fn bench(arguments: &[String]) -> Result<(), String> {
    let samples = read(arguments.first())?;
    let threads = arguments.get(1).and_then(|t| t.parse().ok()).unwrap_or(2);
    let installed = unluminous_voice::install::status();
    let started = Instant::now();
    let mut model = unluminous_voice::moonshine::Moonshine::load(
        &installed.model.ok_or("not installed")?,
        &installed.runtime.ok_or("no runtime")?,
        threads,
    )?;
    println!("loaded in {} ms", started.elapsed().as_millis());
    for _ in 0..5 {
        let pass = Instant::now();
        let text = model.transcribe(&samples)?;
        println!(
            "{:.1} s of audio in {} ms: {text}",
            samples.len() as f32 / 16_000.0,
            pass.elapsed().as_millis()
        );
    }
    Ok(())
}
