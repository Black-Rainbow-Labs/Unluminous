//! The thread that turns audio into text, and the one place the local model is loaded and let go.
//!
//! **Only the newest partial is worked on.** Passes arrive every 300 ms and a pass can take longer than
//! that on a slow machine, so before each pass the queue is drained: every last pass is kept, in order, and
//! of the partials only the newest one for a segment that has no last pass waiting. A slow pass therefore
//! never builds a queue, and the prompt shows the newest words it can.
//!
//! **The local model is in memory only while it is being used.** It is loaded when a job (or a warm up,
//! which dictation sends as it starts) arrives, and dropped after `model_idle` with no job and no dictation
//! holding it. That is about 200 MB that is not there between dictations, which is the "load and unload" half
//! of `tasks/task-2290-speech-to-text-tdd.md` section 6. Audio that arrives while it loads waits in the
//! queue, so nothing said in the first 300 ms is lost.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::endpoint::Endpoint;
use crate::moonshine::Moonshine;

/// Which engine transcribes.
#[derive(Debug, Clone, PartialEq)]
pub enum Engine {
    /// Moonshine on this machine, with the ONNX Runtime threads it may use.
    Local { threads: usize },
    /// An OpenAI compatible endpoint.
    Endpoint(Endpoint),
}

/// What the thread is asked.
#[derive(Debug)]
pub enum Job {
    /// Transcribe a pass of dictation `dictation`.
    Pass { dictation: u64, pass: crate::segmenter::Pass },
    /// Load the local model now, because dictation has started.
    Warm,
    /// Use this engine from now on, and keep a loaded model this long after its last use.
    Configure { engine: Engine, model_idle: Duration },
    /// Let go of the model now.
    Release,
}

/// What the thread answers.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub dictation: u64,
    pub segment: u32,
    pub last: bool,
    /// The text, or why there is none.
    pub text: Result<String, String>,
    /// How long the pass took.
    pub took: Duration,
}

/// What the thread tells the rest of the engine about the model, for `voice status`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelState {
    pub loaded: bool,
    pub loads: u32,
    pub last_load: Option<Duration>,
    pub last_error: Option<String>,
}

/// The thread's state.
struct Worker {
    engine: Engine,
    model_idle: Duration,
    model: Option<Moonshine>,
    last_used: Instant,
    held: Arc<AtomicBool>,
    state: Arc<std::sync::Mutex<ModelState>>,
}

/// Starts the thread.
/// @param jobs - what it is asked
/// @param heard - where it answers
/// @param held - true while dictation is on, which keeps the model loaded however long it has been idle
/// @param state - what it says about the model
pub fn spawn(
    jobs: Receiver<Job>,
    heard: Sender<Heard>,
    held: Arc<AtomicBool>,
    state: Arc<std::sync::Mutex<ModelState>>,
) {
    let worker = Worker {
        engine: Engine::Local { threads: 2 },
        model_idle: Duration::from_secs(60),
        model: None,
        last_used: Instant::now(),
        held,
        state,
    };
    let _ = std::thread::Builder::new()
        .name("unluminous-voice-transcriber".into())
        .spawn(move || worker.run(jobs, heard));
}

impl Worker {
    /// The loop: take the jobs that matter, do them, and let the model go when it has been idle.
    fn run(mut self, jobs: Receiver<Job>, heard: Sender<Heard>) {
        loop {
            let first = match jobs.recv_timeout(Duration::from_secs(1)) {
                Ok(job) => job,
                Err(RecvTimeoutError::Timeout) => {
                    self.release_if_idle();
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            };
            let mut waiting = vec![first];
            waiting.extend(jobs.try_iter());
            for job in newest_only(waiting) {
                if let Some(answer) = self.handle(job) {
                    if heard.send(answer).is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// Does one job.
    /// @param job - the job
    fn handle(&mut self, job: Job) -> Option<Heard> {
        match job {
            Job::Configure { engine, model_idle } => {
                if engine != self.engine {
                    self.drop_model();
                }
                self.engine = engine;
                self.model_idle = model_idle;
                None
            }
            Job::Release => {
                self.drop_model();
                None
            }
            Job::Warm => {
                if matches!(self.engine, Engine::Local { .. }) {
                    let _ = self.local();
                }
                None
            }
            Job::Pass { dictation, pass } => {
                let started = Instant::now();
                let text = self.transcribe(&pass.audio);
                self.last_used = Instant::now();
                Some(Heard {
                    dictation,
                    segment: pass.segment,
                    last: pass.last,
                    text,
                    took: started.elapsed(),
                })
            }
        }
    }

    /// Transcribes with whichever engine is chosen.
    /// @param audio - 16 kHz samples
    fn transcribe(&mut self, audio: &[i16]) -> Result<String, String> {
        match self.engine.clone() {
            Engine::Endpoint(endpoint) => crate::endpoint::transcribe(&endpoint, audio),
            Engine::Local { .. } => self.local()?.transcribe(audio),
        }
    }

    /// The local model, loaded now if it is not.
    fn local(&mut self) -> Result<&mut Moonshine, String> {
        if self.model.is_none() {
            let threads = match self.engine {
                Engine::Local { threads } => threads,
                Engine::Endpoint(_) => 2,
            };
            let started = Instant::now();
            let loaded = load(threads);
            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match loaded {
                Ok(model) => {
                    state.loaded = true;
                    state.loads += 1;
                    state.last_load = Some(started.elapsed());
                    state.last_error = None;
                    self.model = Some(model);
                }
                Err(error) => {
                    state.last_error = Some(error.clone());
                    return Err(error);
                }
            }
        }
        self.last_used = Instant::now();
        self.model.as_mut().ok_or_else(|| "the speech model is not loaded".to_owned())
    }

    /// Lets the model go when nothing has used it for `model_idle` and dictation is not holding it.
    fn release_if_idle(&mut self) {
        if self.model.is_some()
            && !self.held.load(Ordering::Relaxed)
            && self.last_used.elapsed() >= self.model_idle
        {
            self.drop_model();
        }
    }

    /// Drops the model and says so.
    fn drop_model(&mut self) {
        if self.model.take().is_some() {
            self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).loaded = false;
        }
    }
}

/// Loads Moonshine from the installed files.
/// @param threads - ONNX Runtime threads
fn load(threads: usize) -> Result<Moonshine, String> {
    let installed = crate::install::status();
    let model: PathBuf = installed.model.ok_or("the speech model is not downloaded yet. Run `unluminous-cli voice install`, or press Download on Settings, Voice")?;
    let runtime = installed.runtime.ok_or("ONNX Runtime is not installed yet. Run `unluminous-cli voice install`, or press Download on Settings, Voice")?;
    Moonshine::load(&model, &runtime, threads)
}

/// The jobs worth doing out of a queue: every one that is not a pass, every last pass, and of the partial
/// passes only the newest for a segment with no last pass waiting. Order is kept.
/// @param waiting - the queue, oldest first
pub fn newest_only(waiting: Vec<Job>) -> Vec<Job> {
    let finished: Vec<(u64, u32)> = waiting
        .iter()
        .filter_map(|job| match job {
            Job::Pass { dictation, pass } if pass.last => Some((*dictation, pass.segment)),
            _ => None,
        })
        .collect();
    let newest_partial = |at: usize, dictation: u64, segment: u32| {
        !waiting[at + 1..].iter().any(|later| matches!(later, Job::Pass { dictation: d, pass } if *d == dictation && pass.segment == segment && !pass.last))
    };
    let keep: Vec<bool> = waiting
        .iter()
        .enumerate()
        .map(|(at, job)| match job {
            Job::Pass { dictation, pass } if !pass.last => {
                !finished.contains(&(*dictation, pass.segment))
                    && newest_partial(at, *dictation, pass.segment)
            }
            _ => true,
        })
        .collect();
    waiting.into_iter().zip(keep).filter_map(|(job, keep)| keep.then_some(job)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmenter::Pass;

    fn pass(dictation: u64, segment: u32, length: usize, last: bool) -> Job {
        Job::Pass { dictation, pass: Pass { segment, audio: vec![0; length], last } }
    }

    fn lengths(jobs: &[Job]) -> Vec<(u32, usize, bool)> {
        jobs.iter()
            .filter_map(|job| match job {
                Job::Pass { pass, .. } => Some((pass.segment, pass.audio.len(), pass.last)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_queue_of_partials_is_cut_to_the_newest_and_a_last_pass_is_never_dropped() {
        let queue = vec![pass(1, 0, 10, false), pass(1, 0, 20, false), pass(1, 0, 30, false)];
        assert_eq!(lengths(&newest_only(queue)), vec![(0, 30, false)]);
        let queue = vec![
            pass(1, 0, 10, false),
            pass(1, 0, 40, true),
            pass(1, 1, 5, false),
            pass(1, 1, 9, false),
        ];
        assert_eq!(lengths(&newest_only(queue)), vec![(0, 40, true), (1, 9, false)]);
    }

    #[test]
    fn two_dictations_do_not_drop_each_others_passes() {
        let queue = vec![pass(1, 0, 10, false), pass(2, 0, 20, false)];
        assert_eq!(newest_only(queue).len(), 2);
    }
}
