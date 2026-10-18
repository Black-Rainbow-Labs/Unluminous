//! The voice engine: one worker thread that owns the microphone, the wake detector and dictation, and the
//! transcriber thread beside it. The window talks to it through [`Voice`] and reads [`Event`]s back.
//!
//! **Everything is decided on the worker thread**, from one channel that carries the window's commands, the
//! microphone's audio and the transcriber's answers. One channel means one order of events and no locks
//! between them; the only shared state is the [`Snapshot`] `voice status` reads.
//!
//! **What is open follows what is needed** (`tasks/task-2290-speech-to-text-tdd.md` section 6): the
//! microphone while dictation or always listen needs it, the wake models while always listen is on, the
//! voice activity detector while the microphone is open or a file is being transcribed, and the local model
//! while the transcriber has been used in the last `model_idle`. [`Worker::settle`] is the one function that
//! opens and closes them, called after every change, so no path can forget to close something.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::access::{self, Access};
use crate::capture::{self, Microphone};
use crate::phonetic;
use crate::segmenter::{Ended, Pass, Segmenter, Started, ENDPOINT_PARTIAL, LOCAL_PARTIAL};
use crate::transcriber::{self, Engine, Heard, Job, ModelState};
use crate::vad::SileroVad;
use crate::wake::{threshold_for_sensitivity, WakeDetector};

/// What the window asks of the engine. The window builds it from its settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub engine: Engine,
    pub always_listen: bool,
    pub sensitivity: f32,
    pub model_idle: Duration,
    /// How long a pause ends a dictation "Sable" started, once something was said (`task-2293`).
    pub send_pause: Duration,
    /// Whether the microphone may be opened at all. The window's released binary says yes; a window a test
    /// builds says no, so a test never listens to the room it runs in. A file can always be transcribed.
    pub microphone: bool,
}

impl Default for Config {
    /// The settings' defaults: the local engine, always listen off, and the microphone shut. The window's
    /// released binary is what opens it, so a test that builds an engine from this never hears the room.
    fn default() -> Self {
        Config {
            engine: Engine::Local { threads: 2 },
            always_listen: false,
            sensitivity: 0.5,
            model_idle: Duration::from_secs(60),
            send_pause: Duration::from_millis(1_500),
            microphone: false,
        }
    }
}

/// What the engine tells the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The wake word was heard, and dictation `tag` has started.
    Woke { tag: u64, score: f32 },
    /// Dictation `tag` has these words so far: everything final plus the newest partial.
    Words { tag: u64, text: String },
    /// Dictation `tag` is over, with these words.
    Ended { tag: u64, text: String, ended: Ended, started: Started },
    /// The microphone cannot be used, and why.
    Microphone(String),
    /// A pass failed; dictation goes on.
    Problem(String),
}

/// What `voice status` reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    /// The microphone, when it is open.
    pub microphone: Option<String>,
    /// Whether the wake detector is listening.
    pub listening: bool,
    /// The dictation in progress.
    pub dictating: Option<u64>,
    /// How dictation started.
    pub started: Option<Started>,
    /// The words of the dictation in progress, or of the last one.
    pub text: String,
    /// How loud the microphone is, 0 to 1.
    pub level: f32,
    /// Why the microphone was given up, when it was.
    pub refused: Option<String>,
    /// The last thing that failed.
    pub problem: Option<String>,
    /// The local model.
    pub model: ModelState,
    /// The last pass's time.
    pub last_pass: Option<Duration>,
    /// The highest wake score lately, for tuning.
    pub wake_score: f32,
    /// Whether the "Sable" wake word models are in memory, which is only while always listen is on.
    pub wake_loaded: bool,
    /// Whether the voice activity detector is in memory, which is only while the microphone is open or a
    /// dictation is running.
    pub vad_loaded: bool,
}

/// The handle the window holds.
pub struct Voice {
    inputs: Sender<Input>,
    events: Mutex<Receiver<Event>>,
    shared: Arc<Mutex<Snapshot>>,
    tags: Arc<AtomicU64>,
    /// What the transcriber says about the model, read when asked: the transcriber changes it on its own
    /// thread, after the worker's last message, so the copy in [`Snapshot`] can be a step behind.
    model_state: Arc<Mutex<ModelState>>,
}

/// What the worker's one channel carries.
enum Input {
    Configure(Config),
    Dictate(u64),
    Stop,
    File { tag: u64, samples: Vec<i16>, live: bool },
    FileChunk { tag: u64, samples: Vec<i16>, last: bool },
    Hear(Vec<i16>),
    HearLive(Vec<i16>),
    HearChunk { samples: Vec<i16>, last: bool },
    Audio { generation: u64, samples: Vec<i16> },
    Heard(Heard),
}

impl Voice {
    /// Starts the engine. Nothing is opened or loaded until a [`Config`] or a command asks for it.
    /// @param waker - called whenever there are events to read, so the window draws a frame
    pub fn start(waker: Arc<dyn Fn() + Send + Sync>) -> Voice {
        let (inputs, receiver) = channel::<Input>();
        let (events_tx, events) = channel::<Event>();
        let shared = Arc::new(Mutex::new(Snapshot::default()));
        let tags = Arc::new(AtomicU64::new(1));
        let (jobs_tx, jobs_rx) = channel::<Job>();
        let (heard_tx, heard_rx) = channel::<Heard>();
        let held = Arc::new(AtomicBool::new(false));
        let model_state = Arc::new(Mutex::new(ModelState::default()));
        transcriber::spawn(jobs_rx, heard_tx, held.clone(), model_state.clone());
        let model_kept = model_state.clone();
        let forward = inputs.clone();
        let _ =
            std::thread::Builder::new().name("unluminous-voice-heard".into()).spawn(move || {
                for heard in heard_rx {
                    if forward.send(Input::Heard(heard)).is_err() {
                        return;
                    }
                }
            });
        let worker = Worker::new(
            inputs.clone(),
            events_tx,
            waker,
            shared.clone(),
            tags.clone(),
            jobs_tx,
            held,
            model_state,
        );
        let _ = std::thread::Builder::new()
            .name("unluminous-voice".into())
            .spawn(move || worker.run(receiver));
        Voice { inputs, events: Mutex::new(events), shared, tags, model_state: model_kept }
    }

    /// Changes what the engine does.
    /// @param config - the new configuration
    pub fn configure(&self, config: Config) {
        let _ = self.inputs.send(Input::Configure(config));
    }

    /// Starts dictation from the microphone and answers its tag.
    pub fn dictate(&self) -> u64 {
        let tag = self.tags.fetch_add(1, Ordering::Relaxed);
        let _ = self.inputs.send(Input::Dictate(tag));
        tag
    }

    /// Stops the dictation in progress. Its last words still arrive, in an [`Event::Ended`].
    pub fn stop(&self) {
        let _ = self.inputs.send(Input::Stop);
    }

    /// Transcribes 16 kHz audio through the same segmenter the microphone feeds, and answers its tag.
    /// @param samples - the audio
    /// @param live - fed at the speed it was spoken, so its words arrive as they would from the microphone
    pub fn transcribe(&self, samples: Vec<i16>, live: bool) -> u64 {
        let tag = self.tags.fetch_add(1, Ordering::Relaxed);
        let _ = self.inputs.send(Input::File { tag, samples, live });
        tag
    }

    /// Hears a recording as if it came from the microphone: through the wake detector when always listen is
    /// on, and into the dictation in progress otherwise. This is how the whole always listen path is tested,
    /// by a test and by an agent, with no microphone.
    /// @param samples - 16 kHz audio
    pub fn hear(&self, samples: Vec<i16>) {
        let _ = self.inputs.send(Input::Hear(samples));
    }

    /// The same, at the speed it was spoken, in 20 ms frames, which is how the microphone delivers it. The
    /// words come back while it plays and a dictation ends at its own pause, so this is what shows a problem
    /// that only happens in real time.
    /// @param samples - 16 kHz audio
    pub fn hear_live(&self, samples: Vec<i16>) {
        let _ = self.inputs.send(Input::HearLive(samples));
    }

    /// The events that have arrived since the last call.
    pub fn take_events(&self) -> Vec<Event> {
        self.events.lock().map(|events| events.try_iter().collect()).unwrap_or_default()
    }

    /// What `voice status` reads.
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.shared.lock().map(|snapshot| snapshot.clone()).unwrap_or_default();
        if let Ok(model) = self.model_state.lock() {
            snapshot.model = model.clone();
        }
        snapshot
    }
}

/// One dictation.
struct Dictation {
    tag: u64,
    segmenter: Segmenter,
    finals: BTreeMap<u32, String>,
    partial: Option<(u32, String)>,
    outstanding: usize,
    ending: Option<Ended>,
    wake_score: f32,
    confirmed: bool,
    said: String,
    microphone: bool,
}

impl Dictation {
    /// A dictation that has just started.
    /// @param tag - its tag
    /// @param started - how
    /// @param engine - which engine, for how often to ask for a partial
    fn new(tag: u64, started: Started, engine: &Engine, send_pause: Duration) -> Self {
        let every = match engine {
            Engine::Local { .. } => LOCAL_PARTIAL,
            Engine::Endpoint(_) => ENDPOINT_PARTIAL,
        };
        Dictation {
            tag,
            segmenter: Segmenter::new(started, every)
                .ending_a_wake_after((send_pause.as_secs_f32() * 16_000.0) as usize),
            finals: BTreeMap::new(),
            partial: None,
            outstanding: 0,
            ending: None,
            wake_score: 0.0,
            confirmed: started != Started::Wake,
            said: String::new(),
            microphone: false,
        }
    }

    /// Every final segment's words, then the newest partial, as one line.
    fn text(&self) -> String {
        let mut words: Vec<&str> = self.finals.values().map(String::as_str).collect();
        if let Some((segment, partial)) = &self.partial {
            if !self.finals.contains_key(segment) {
                words.push(partial);
            }
        }
        words.into_iter().map(str::trim).filter(|w| !w.is_empty()).collect::<Vec<&str>>().join(" ")
    }
}

/// Audio kept for the pre roll of a wake: 3 s.
const RECENT: usize = 48_000;
/// How far before the hop that fired the pre roll starts: 1.8 s, which holds "hey Sable" said slowly.
const WORD_BEFORE_THE_HOP: usize = 28_800;
/// A newly opened microphone that delivers only zeros for this long is taken as refused: 1.5 s.
const SILENT: usize = 24_000;
/// Silence added after a file, so its last segment ends at a pause: 1 s.
const FILE_TAIL: usize = 16_000;

/// The worker thread's state.
struct Worker {
    inputs: Sender<Input>,
    events: Sender<Event>,
    waker: Arc<dyn Fn() + Send + Sync>,
    shared: Arc<Mutex<Snapshot>>,
    tags: Arc<AtomicU64>,
    jobs: Sender<Job>,
    held: Arc<AtomicBool>,
    model_state: Arc<Mutex<ModelState>>,
    config: Config,
    microphone: Option<Microphone>,
    generation: u64,
    zeros: Option<usize>,
    refused: Option<String>,
    wake: Option<WakeDetector>,
    vad: Option<SileroVad>,
    recent: VecDeque<i16>,
    dictation: Option<Dictation>,
    hearing: bool,
    level: f32,
}

impl Worker {
    /// The worker with nothing open.
    #[allow(clippy::too_many_arguments)]
    fn new(
        inputs: Sender<Input>,
        events: Sender<Event>,
        waker: Arc<dyn Fn() + Send + Sync>,
        shared: Arc<Mutex<Snapshot>>,
        tags: Arc<AtomicU64>,
        jobs: Sender<Job>,
        held: Arc<AtomicBool>,
        model_state: Arc<Mutex<ModelState>>,
    ) -> Self {
        Worker {
            inputs,
            events,
            waker,
            shared,
            tags,
            jobs,
            held,
            model_state,
            config: Config::default(),
            microphone: None,
            generation: 0,
            zeros: None,
            refused: None,
            wake: None,
            vad: None,
            recent: VecDeque::with_capacity(RECENT),
            dictation: None,
            hearing: false,
            level: 0.0,
        }
    }

    /// The loop.
    /// @param receiver - the one channel
    fn run(mut self, receiver: Receiver<Input>) {
        for input in receiver {
            match input {
                Input::Configure(config) => self.configure(config),
                Input::Dictate(tag) => self.begin(tag, Started::Button, Vec::new(), true),
                Input::Hear(samples) => self.hear(&samples),
                Input::HearLive(samples) => self.hear_live(samples),
                Input::HearChunk { samples, last } => self.hear_chunk(&samples, last),
                Input::Stop => self.end(Ended::Stopped),
                Input::File { tag, samples, live } => self.file(tag, samples, live),
                Input::FileChunk { tag, samples, last } => self.file_chunk(tag, &samples, last),
                Input::Audio { generation, samples } if generation == self.generation => {
                    self.audio(&samples, true)
                }
                Input::Audio { .. } => {}
                Input::Heard(heard) => self.heard(heard),
            }
            self.publish();
        }
    }

    /// Takes a new configuration, telling the transcriber what concerns it.
    /// @param config - the configuration
    fn configure(&mut self, config: Config) {
        if config.engine != self.config.engine || config.model_idle != self.config.model_idle {
            let _ = self.jobs.send(Job::Configure {
                engine: config.engine.clone(),
                model_idle: config.model_idle,
            });
        }
        if config.always_listen && !self.config.always_listen {
            self.refused = None;
        }
        // **Always listen switched off lets the speech model go at once** rather than after `model_idle`,
        // unless a dictation is still using it: everything "Sable" loaded is released with the switch.
        if !config.always_listen && self.config.always_listen && self.dictation.is_none() {
            let _ = self.jobs.send(Job::Release);
        }
        if let Some(wake) = self.wake.as_mut() {
            wake.set_sensitivity(config.sensitivity);
        }
        self.config = config;
        self.settle();
    }

    /// Opens what is needed and closes what is not. See the module comment.
    fn settle(&mut self) {
        let dictating_from_the_microphone = self.dictation.as_ref().is_some_and(|d| d.microphone);
        let wanted = (self.config.always_listen || dictating_from_the_microphone)
            && self.refused.is_none()
            && self.config.microphone;
        if wanted && self.microphone.is_none() {
            self.open_microphone();
        }
        if !wanted && self.microphone.is_some() {
            self.microphone = None;
            self.generation += 1;
            self.level = 0.0;
        }
        let listening = self.config.always_listen && (self.microphone.is_some() || self.hearing);
        if listening && self.wake.is_none() {
            match WakeDetector::new(self.config.sensitivity) {
                Ok(detector) => self.wake = Some(detector),
                Err(problem) => self.say(Event::Problem(problem)),
            }
        }
        if !listening {
            self.wake = None;
        }
        if self.microphone.is_none() && self.dictation.is_none() {
            self.vad = None;
        }
        self.held.store(self.dictation.is_some(), Ordering::Relaxed);
    }

    /// Opens the microphone, after asking the system whether it may.
    fn open_microphone(&mut self) {
        match access::status() {
            Access::Refused(why) => return self.give_up_the_microphone(why),
            Access::NotAsked => access::request(),
            Access::Allowed | Access::Unknown => {}
        }
        self.generation += 1;
        let generation = self.generation;
        let inputs = self.inputs.clone();
        match capture::open(move |samples| {
            let _ = inputs.send(Input::Audio { generation, samples });
        }) {
            Ok(microphone) => {
                self.microphone = Some(microphone);
                self.zeros = Some(0);
            }
            Err(why) => self.give_up_the_microphone(why),
        }
    }

    /// Stops trying the microphone until something asks again, and says why.
    /// @param why - the sentence
    fn give_up_the_microphone(&mut self, why: String) {
        self.microphone = None;
        self.generation += 1;
        self.refused = Some(why.clone());
        self.say(Event::Microphone(why));
        if self.dictation.as_ref().is_some_and(|d| d.microphone) {
            self.end(Ended::Lost);
        }
    }

    /// Starts dictation.
    /// @param tag - its tag
    /// @param started - how it started
    /// @param pre_roll - audio from before it started, which is the wake word itself after a wake
    /// @param microphone - whether its audio comes from the microphone, which then has to be open
    fn begin(&mut self, tag: u64, started: Started, pre_roll: Vec<i16>, microphone: bool) {
        if self.dictation.is_some() {
            return;
        }
        if microphone && !self.config.microphone {
            self.say(Event::Microphone("This window may not use the microphone.".to_owned()));
            self.say(Event::Ended { tag, text: String::new(), ended: Ended::Lost, started });
            return;
        }
        if started == Started::Button {
            self.refused = None;
        }
        let mut dictation =
            Dictation::new(tag, started, &self.config.engine, self.config.send_pause);
        dictation.microphone = microphone;
        if started == Started::Wake {
            dictation.wake_score = self.wake.as_ref().map(WakeDetector::last_score).unwrap_or(0.0);
        }
        self.dictation = Some(dictation);
        let _ = self.jobs.send(Job::Warm);
        if self.vad.is_none() {
            match SileroVad::bundled() {
                Ok(vad) => self.vad = Some(vad),
                Err(e) => self
                    .say(Event::Problem(format!("the voice activity detector did not load: {e}"))),
            }
        }
        if let Some(vad) = self.vad.as_mut() {
            vad.reset();
        }
        self.settle();
        if !pre_roll.is_empty() {
            self.dictate_audio(&pre_roll, Some(true));
        }
    }

    /// Ends dictation: its open segment gets its last pass, and it is over once every last pass is back.
    /// @param why - why it ended
    fn end(&mut self, why: Ended) {
        let Some(dictation) = self.dictation.as_mut() else { return };
        if dictation.ending.is_some() {
            return;
        }
        dictation.ending = Some(why);
        if let Some(pass) = dictation.segmenter.finish() {
            dictation.outstanding += 1;
            let _ = self.jobs.send(Job::Pass { dictation: dictation.tag, pass });
        }
        self.finish_if_done();
    }

    /// Says the dictation is over once nothing is outstanding.
    fn finish_if_done(&mut self) {
        let done =
            self.dictation.as_ref().is_some_and(|d| d.ending.is_some() && d.outstanding == 0);
        if !done {
            return;
        }
        let Some(dictation) = self.dictation.take() else { return };
        let ended = dictation.ending.unwrap_or(Ended::Stopped);
        let text = match ended {
            Ended::FalseWake => String::new(),
            _ => dictation.text(),
        };
        if let Ok(mut shared) = self.shared.lock() {
            shared.text = text.clone();
        }
        self.say(Event::Ended {
            tag: dictation.tag,
            text,
            ended,
            started: dictation.segmenter.started(),
        });
        if let Some(wake) = self.wake.as_mut() {
            wake.reset();
        }
        self.settle();
    }

    /// Audio from the microphone, or a recording being heard as if it were.
    /// @param samples - 16 kHz audio
    /// @param from_microphone - whether it came from the microphone
    fn audio(&mut self, samples: &[i16], from_microphone: bool) {
        if from_microphone && !self.still_hearing(samples) {
            return;
        }
        self.level = self.level * 0.6 + crate::audio::level(samples) * 0.4;
        self.recent.extend(samples.iter().copied());
        while self.recent.len() > RECENT {
            self.recent.pop_front();
        }
        if let Some(microphone) = self.dictation.as_ref().map(|d| d.microphone) {
            if microphone == from_microphone {
                self.dictate_audio(samples, None);
            }
            return;
        }
        let Some(wake) = self.wake.as_mut() else { return };
        let fired = wake.push(samples);
        let score = wake.last_score();
        if let Ok(mut shared) = self.shared.lock() {
            shared.wake_score = shared.wake_score.max(score) * 0.98;
        }
        if let Some(event) = fired.first() {
            let tag = self.tags.fetch_add(1, Ordering::Relaxed);
            self.say(Event::Woke { tag, score: event.score });
            // **From where the word began**, which is a little before the hop that fired: the classifier scores
            // highest a few hops after "Sable" ends, so a fixed last two seconds could start partway through
            // the word and the model then wrote "Able" (`task-2290`). Everything after the firing hop is kept.
            let start = self.recent.len().saturating_sub(event.samples_ago + WORD_BEFORE_THE_HOP);
            let pre_roll: Vec<i16> = self.recent.iter().skip(start).copied().collect();
            self.begin(tag, Started::Wake, pre_roll, from_microphone);
            if let Some(dictation) = self.dictation.as_mut() {
                dictation.wake_score = event.score;
            }
        }
    }

    /// Whether a newly opened stream is delivering anything but zeros, which is how Windows refuses a
    /// desktop program the microphone. Gives the microphone up after [`SILENT`] of exact zeros.
    /// @param samples - the audio just delivered
    fn still_hearing(&mut self, samples: &[i16]) -> bool {
        let Some(seen) = self.zeros else { return true };
        if samples.iter().any(|s| *s != 0) {
            self.zeros = None;
            return true;
        }
        self.zeros = Some(seen + samples.len());
        if seen + samples.len() >= SILENT {
            self.zeros = None;
            self.give_up_the_microphone(access::SILENT_STREAM.to_owned());
            self.settle();
            return false;
        }
        true
    }

    /// Feeds dictation audio through the detector and the segmenter, and asks for the passes it calls for.
    /// @param samples - 16 kHz audio
    /// @param speech - the detector's answer to use instead of asking it, for the pre roll
    fn dictate_audio(&mut self, samples: &[i16], speech: Option<bool>) {
        let heard = match (speech, self.vad.as_mut()) {
            (Some(said), vad) => {
                if let Some(vad) = vad {
                    vad.push(samples);
                }
                said
            }
            (None, Some(vad)) => vad.push(samples),
            (None, None) => crate::audio::level(samples) > 0.02,
        };
        let Some(dictation) = self.dictation.as_mut() else { return };
        if dictation.ending.is_some() {
            return;
        }
        let passes: Vec<Pass> = dictation.segmenter.push(samples, heard);
        for pass in passes {
            if pass.last {
                dictation.outstanding += 1;
            }
            let _ = self.jobs.send(Job::Pass { dictation: dictation.tag, pass });
        }
        let has_words = !dictation.text().is_empty();
        if let Some(why) = dictation.segmenter.should_end(has_words) {
            self.end(why);
        }
    }

    /// A transcriber's answer.
    /// @param heard - the answer
    fn heard(&mut self, heard: Heard) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.last_pass = Some(heard.took);
        }
        let threshold = threshold_for_sensitivity(self.config.sensitivity);
        let Some(dictation) = self.dictation.as_mut().filter(|d| d.tag == heard.dictation) else {
            return;
        };
        if heard.last {
            dictation.outstanding = dictation.outstanding.saturating_sub(1);
        }
        let text = match heard.text {
            Ok(text) => text,
            Err(problem) => {
                self.say(Event::Problem(problem));
                self.finish_if_done();
                return;
            }
        };
        let Some(dictation) = self.dictation.as_mut() else { return };
        if std::env::var_os("UNLUMINOUS_VOICE_TRACE").is_some() {
            eprintln!(
                "unluminous-voice: segment {} last {} took {:?}: {text:?}",
                heard.segment, heard.last, heard.took
            );
        }
        // **An empty pass never takes away words already heard** (`task-2290`). Moonshine sometimes answers a
        // segment with nothing once it ends in a long silence, and that wiped the words out of the prompt.
        let had = match heard.last {
            true => dictation
                .partial
                .as_ref()
                .filter(|(segment, _)| *segment == heard.segment)
                .map(|(_, words)| words.clone()),
            false => None,
        };
        let text = match (text.trim().is_empty(), heard.last, had) {
            (true, false, _) => return,
            (true, true, Some(had)) if !had.is_empty() => {
                dictation.confirmed = true;
                dictation.finals.insert(heard.segment, had);
                self.say_the_words();
                self.finish_if_done();
                return;
            }
            _ => text,
        };
        let mut false_wake = false;
        let text = match (dictation.confirmed, heard.last, heard.segment) {
            (false, true, 0) => match phonetic::confirm(&text, dictation.wake_score, threshold) {
                Some(stripped) => {
                    dictation.confirmed = true;
                    stripped
                }
                None => {
                    false_wake = true;
                    String::new()
                }
            },
            // **A partial is judged the way the final will be** (`task-2290`). It used to be kept only when it
            // started with "Sable", so a pass that heard the clipped word as "Able" blanked the words already in
            // the prompt, and with no words the dictation waited for a long quiet instead of ending at the first
            // pause, swallowing the next "Sable" too. A partial that cannot be read yet keeps the last one.
            (false, false, 0) => match phonetic::confirm(&text, dictation.wake_score, threshold) {
                Some(stripped) => stripped,
                None => return,
            },
            _ => text,
        };
        if false_wake {
            dictation.ending = Some(Ended::FalseWake);
            dictation.outstanding = 0;
            self.finish_if_done();
            return;
        }
        match heard.last {
            true => {
                dictation.finals.insert(heard.segment, text);
            }
            false => dictation.partial = Some((heard.segment, text)),
        }
        self.say_the_words();
        self.finish_if_done();
    }

    /// Tells the window the dictation's words, when they changed.
    fn say_the_words(&mut self) {
        let Some(dictation) = self.dictation.as_mut() else { return };
        let words = dictation.text();
        if words != dictation.said {
            dictation.said = words.clone();
            let tag = dictation.tag;
            if let Ok(mut shared) = self.shared.lock() {
                shared.text = words.clone();
            }
            self.say(Event::Words { tag, text: words });
        }
    }

    /// Starts dictation from a file.
    /// @param tag - its tag
    /// @param samples - the audio
    /// @param live - fed at the speed it was spoken
    fn file(&mut self, tag: u64, samples: Vec<i16>, live: bool) {
        if self.dictation.is_some() {
            self.say(Event::Ended {
                tag,
                text: String::new(),
                ended: Ended::Lost,
                started: Started::File,
            });
            self.say(Event::Problem(
                "dictation is already in progress; stop it before transcribing a file".to_owned(),
            ));
            return;
        }
        self.begin(tag, Started::File, Vec::new(), false);
        let mut audio = samples;
        audio.extend(std::iter::repeat_n(0i16, FILE_TAIL));
        if !live {
            let chunks: Vec<Vec<i16>> = audio.chunks(320).map(<[i16]>::to_vec).collect();
            let count = chunks.len();
            for (at, chunk) in chunks.into_iter().enumerate() {
                self.file_chunk(tag, &chunk, at + 1 == count);
            }
            return;
        }
        let inputs = self.inputs.clone();
        let _ = std::thread::Builder::new().name("unluminous-voice-file".into()).spawn(move || {
            let chunks: Vec<&[i16]> = audio.chunks(320).collect();
            for (at, chunk) in chunks.iter().enumerate() {
                if inputs
                    .send(Input::FileChunk {
                        tag,
                        samples: chunk.to_vec(),
                        last: at + 1 == chunks.len(),
                    })
                    .is_err()
                {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
    }

    /// A recording heard as if from the microphone, at once, followed by two seconds of silence so a wake
    /// started dictation reaches its end.
    /// @param samples - 16 kHz audio
    fn hear(&mut self, samples: &[i16]) {
        self.hearing = true;
        self.settle();
        let mut audio = samples.to_vec();
        audio.extend(std::iter::repeat_n(0i16, 2 * FILE_TAIL));
        for chunk in audio.chunks(320) {
            self.audio(chunk, false);
        }
        self.hearing = false;
        // The words come back from the transcriber later, so a wake heard this fast cannot yet know it has
        // words to end on. The recording is over, so its dictation is too.
        if self.dictation.as_ref().is_some_and(|d| !d.microphone) {
            self.end(Ended::Finished);
        }
        if self.dictation.is_none() {
            self.settle();
        }
    }

    /// Starts hearing a recording in real time: the frames are sent back to this thread by a pacing thread,
    /// followed by four seconds of silence so a dictation reaches its own end.
    /// @param samples - 16 kHz audio
    fn hear_live(&mut self, samples: Vec<i16>) {
        self.hearing = true;
        self.settle();
        let mut audio = samples;
        audio.extend(std::iter::repeat_n(0i16, 4 * FILE_TAIL));
        let inputs = self.inputs.clone();
        let _ = std::thread::Builder::new().name("unluminous-voice-hear".into()).spawn(move || {
            let chunks: Vec<&[i16]> = audio.chunks(320).collect();
            for (at, chunk) in chunks.iter().enumerate() {
                if inputs
                    .send(Input::HearChunk {
                        samples: chunk.to_vec(),
                        last: at + 1 == chunks.len(),
                    })
                    .is_err()
                {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
    }

    /// One frame of a recording heard in real time.
    /// @param samples - the frame
    /// @param last - whether it is the recording's last
    fn hear_chunk(&mut self, samples: &[i16], last: bool) {
        self.audio(samples, false);
        if last {
            self.hearing = false;
            if self.dictation.is_none() {
                self.settle();
            }
        }
    }

    /// One chunk of a file.
    /// @param tag - the file's dictation
    /// @param samples - the chunk
    /// @param last - whether it is the file's last
    fn file_chunk(&mut self, tag: u64, samples: &[i16], last: bool) {
        if self.dictation.as_ref().is_none_or(|d| d.tag != tag) {
            return;
        }
        self.dictate_audio(samples, None);
        if last {
            self.end(Ended::EndOfFile);
        }
    }

    /// Sends an event and wakes the window.
    /// @param event - the event
    fn say(&mut self, event: Event) {
        if let Event::Problem(problem) = &event {
            if let Ok(mut shared) = self.shared.lock() {
                shared.problem = Some(problem.clone());
            }
        }
        let _ = self.events.send(event);
        (self.waker)();
    }

    /// Writes the snapshot `voice status` reads.
    fn publish(&mut self) {
        let model = self.model_state.lock().map(|m| m.clone()).unwrap_or_default();
        if let Ok(mut shared) = self.shared.lock() {
            shared.microphone = self.microphone.as_ref().map(|m| m.description.clone());
            shared.listening = self.wake.is_some() && self.microphone.is_some();
            shared.dictating = self.dictation.as_ref().map(|d| d.tag);
            shared.started = self.dictation.as_ref().map(|d| d.segmenter.started());
            shared.level = self.level;
            shared.refused = self.refused.clone();
            shared.wake_loaded = self.wake.is_some();
            shared.vad_loaded = self.vad.is_some();
            shared.model = model;
        }
    }
}
