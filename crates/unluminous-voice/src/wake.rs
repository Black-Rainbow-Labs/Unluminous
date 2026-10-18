//! The "Sable" wake word, which is Sable's detector (`C:/jason/dev/sable/crates/sable-wake`) unchanged in
//! what it computes.
//!
//! 16 kHz audio in 80 ms hops goes through openWakeWord's mel spectrogram and speech embedding models, and
//! the trained `sable.onnx` classifier scores the newest 16 embeddings. The detector fires when two hops in
//! a row reach the threshold. A Silero gate keeps the models idle while nobody speaks: audio only goes into
//! a 1.5 s ring, which is replayed through the models when speech starts so the classifier sees the whole
//! word, and the models keep running for 1 s after speech ends because the score peaks a few hops after
//! the word. Sable measured 0.33 false accepts an hour on three hours of podcasts with these settings, and
//! 2.7% of one core while somebody talks. `docs/wake-word.md` in Sable has every number.
//!
//! The four model files are compiled into the binary, 4.5 MB, so always listen needs nothing downloaded.

use std::collections::VecDeque;

use tract_onnx::prelude::*;

use crate::vad::SileroVad;

/// openWakeWord's mel spectrogram model, v0.5.1.
pub const MEL: &[u8] = include_bytes!("../models/melspectrogram.onnx");
/// openWakeWord's speech embedding model, v0.5.1.
pub const EMBEDDING: &[u8] = include_bytes!("../models/embedding_model.onnx");
/// The "Sable" classifier Sable trained (v3).
pub const CLASSIFIER: &[u8] = include_bytes!("../models/sable.onnx");
/// Silero VAD v5 as a 16 kHz graph `tract` can load.
pub const SILERO: &[u8] = include_bytes!("../models/silero_vad_16k.onnx");

/// Samples per hop: 80 ms at 16 kHz.
pub const HOP: usize = 1280;
const MEL_CONTEXT: usize = 480;
const EMBED_WINDOW: usize = 76;
const MEL_BINS: usize = 32;
const EMBED_DIM: usize = 96;
const CLASSIFIER_FRAMES: usize = 16;
const MEL_MAX: usize = 970;
const EMBED_MAX: usize = 120;

/// Hops in a row over the threshold that fire the detector.
const CONSECUTIVE: usize = 2;
/// Hops ignored after a detection, so one word fires once (1 second).
const COOLDOWN_HOPS: usize = 12;
/// Audio kept while the gate hears nothing: 1.5 seconds.
const RING: usize = 24_000;
/// The models keep running this long after the gate stops hearing speech (1 second).
const ACTIVE_TAIL: usize = 16_000;

type Plan = TypedRunnableModel<TypedModel>;

/// The threshold for a wake sensitivity: 0.0 maps to 0.8, 0.5 to 0.6, 1.0 to 0.4, as in Sable.
/// @param sensitivity - from 0.0 to 1.0, higher wakes more easily
pub fn threshold_for_sensitivity(sensitivity: f32) -> f32 {
    0.8 - 0.4 * sensitivity.clamp(0.0, 1.0)
}

/// Loads an ONNX model from bytes with a fixed input shape.
/// @param bytes - the ONNX file
/// @param shape - the input shape
fn load_plan(bytes: &[u8], shape: &[usize]) -> TractResult<Plan> {
    tract_onnx::onnx()
        .model_for_read(&mut std::io::Cursor::new(bytes))?
        .with_input_fact(0, f32::fact(shape).into())?
        .into_optimized()?
        .into_runnable()
}

/// openWakeWord's front end, streamed the way its Python `AudioFeatures` class streams it.
struct FeatureStream {
    mel: Plan,
    embed: Plan,
    raw: Vec<f32>,
    pending: Vec<f32>,
    mel_frames: Vec<[f32; MEL_BINS]>,
    embeddings: Vec<[f32; EMBED_DIM]>,
}

impl FeatureStream {
    /// Builds the front end from the two openWakeWord models.
    fn new() -> TractResult<Self> {
        let mel = load_plan(MEL, &[1, HOP + MEL_CONTEXT])?;
        let embed = load_plan(EMBEDDING, &[1, EMBED_WINDOW, MEL_BINS, 1])?;
        let mut stream = FeatureStream {
            mel,
            embed,
            raw: vec![],
            pending: vec![],
            mel_frames: vec![],
            embeddings: vec![],
        };
        stream.reset()?;
        Ok(stream)
    }

    /// Back to the start state: mel frames of ones and embeddings of silence, as the Python class starts.
    fn reset(&mut self) -> TractResult<()> {
        self.raw = vec![0.0; MEL_CONTEXT];
        self.pending.clear();
        self.mel_frames = vec![[1.0; MEL_BINS]; EMBED_WINDOW];
        self.embeddings.clear();
        let zero_frames = self.mel_of(&vec![0.0; HOP + MEL_CONTEXT])?;
        let window: Vec<[f32; MEL_BINS]> = std::iter::repeat_n(
            zero_frames.first().copied().unwrap_or([0.0; MEL_BINS]),
            EMBED_WINDOW,
        )
        .collect();
        let e = self.embed_of(&window)?;
        self.embeddings = vec![e; CLASSIFIER_FRAMES];
        Ok(())
    }

    /// Adds samples and answers how many new embeddings were made, one per full hop.
    /// @param samples - 16 kHz audio
    fn push(&mut self, samples: &[i16]) -> TractResult<usize> {
        self.pending.extend(samples.iter().map(|s| f32::from(*s)));
        let mut made = 0;
        while self.pending.len() >= HOP {
            let hop: Vec<f32> = self.pending.drain(..HOP).collect();
            self.raw.extend_from_slice(&hop);
            let start = self.raw.len() - (HOP + MEL_CONTEXT);
            let frames = self.mel_of(&self.raw[start..].to_vec())?;
            self.raw.drain(..self.raw.len() - MEL_CONTEXT);
            self.mel_frames.extend(frames);
            if self.mel_frames.len() > MEL_MAX {
                let cut = self.mel_frames.len() - MEL_MAX;
                self.mel_frames.drain(..cut);
            }
            let window = self.mel_frames[self.mel_frames.len() - EMBED_WINDOW..].to_vec();
            let e = self.embed_of(&window)?;
            self.embeddings.push(e);
            if self.embeddings.len() > EMBED_MAX {
                self.embeddings.remove(0);
            }
            made += 1;
        }
        Ok(made)
    }

    /// The newest 16 embeddings, flattened, oldest first.
    fn classifier_input(&self) -> Vec<f32> {
        self.embeddings[self.embeddings.len() - CLASSIFIER_FRAMES..]
            .iter()
            .flat_map(|e| e.iter().copied())
            .collect()
    }

    /// Runs the mel model on 1760 samples and applies openWakeWord's `x / 10 + 2`.
    /// @param samples - audio values in the 16 bit range, as floats
    fn mel_of(&self, samples: &[f32]) -> TractResult<Vec<[f32; MEL_BINS]>> {
        let input = tract_ndarray::Array2::from_shape_vec((1, samples.len()), samples.to_vec())?
            .into_tensor();
        let out = self.mel.run(tvec!(input.into()))?;
        let flat: Vec<f32> = out[0].to_array_view::<f32>()?.iter().copied().collect();
        Ok(flat
            .chunks_exact(MEL_BINS)
            .map(|c| {
                let mut frame = [0.0; MEL_BINS];
                for (i, v) in c.iter().enumerate() {
                    frame[i] = v / 10.0 + 2.0;
                }
                frame
            })
            .collect())
    }

    /// Runs the embedding model on 76 mel frames.
    /// @param window - the frames
    fn embed_of(&self, window: &[[f32; MEL_BINS]]) -> TractResult<[f32; EMBED_DIM]> {
        let flat: Vec<f32> = window.iter().flat_map(|f| f.iter().copied()).collect();
        let input = tract_ndarray::Array4::from_shape_vec((1, EMBED_WINDOW, MEL_BINS, 1), flat)?
            .into_tensor();
        let out = self.embed.run(tvec!(input.into()))?;
        let mut e = [0.0; EMBED_DIM];
        for (i, v) in out[0].to_array_view::<f32>()?.iter().take(EMBED_DIM).enumerate() {
            e[i] = *v;
        }
        Ok(e)
    }
}

/// A detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WakeEvent {
    /// The score of the hop that fired.
    pub score: f32,
    /// How many samples ago, from the end of the audio just pushed, the firing hop ended.
    pub samples_ago: usize,
}

/// The streaming wake word detector.
pub struct WakeDetector {
    features: FeatureStream,
    classifier: Plan,
    vad: SileroVad,
    threshold: f32,
    above: usize,
    cooldown: usize,
    ring: VecDeque<i16>,
    active: bool,
    quiet: usize,
    last_score: f32,
}

impl WakeDetector {
    /// Builds the detector from the models compiled into this crate.
    /// @param sensitivity - from 0 to 1, higher wakes more easily
    pub fn new(sensitivity: f32) -> Result<Self, String> {
        let err = |e: TractError| format!("the wake word model did not load: {e}");
        let features = FeatureStream::new().map_err(err)?;
        let classifier = load_plan(CLASSIFIER, &[1, CLASSIFIER_FRAMES, EMBED_DIM]).map_err(err)?;
        let vad = SileroVad::bundled().map_err(err)?;
        Ok(WakeDetector {
            features,
            classifier,
            vad,
            threshold: threshold_for_sensitivity(sensitivity),
            above: 0,
            cooldown: 0,
            ring: VecDeque::with_capacity(RING),
            active: false,
            quiet: 0,
            last_score: 0.0,
        })
    }

    /// Changes the sensitivity without loading anything again.
    /// @param sensitivity - from 0 to 1
    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.threshold = threshold_for_sensitivity(sensitivity);
    }

    /// The score of the last hop the classifier ran on.
    pub fn last_score(&self) -> f32 {
        self.last_score
    }

    /// Whether the models are running, because the gate hears speech.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Forgets the audio it was given, for after a dictation the wake started.
    pub fn reset(&mut self) {
        self.vad.reset();
        self.ring.clear();
        self.active = false;
        self.quiet = 0;
        self.above = 0;
        self.cooldown = COOLDOWN_HOPS;
    }

    /// Feeds microphone audio and answers any detections.
    /// @param samples - 16 kHz mono audio, any length
    pub fn push(&mut self, samples: &[i16]) -> Vec<WakeEvent> {
        let speech = self.vad.push(samples);
        if !speech && !self.active {
            self.ring.extend(samples.iter().copied());
            while self.ring.len() > RING {
                self.ring.pop_front();
            }
            return vec![];
        }
        let mut audio: Vec<i16> = Vec::new();
        if !self.active {
            self.active = true;
            let _ = self.features.reset();
            self.above = 0;
            audio.extend(self.ring.drain(..));
        }
        audio.extend_from_slice(samples);
        let events = self.run_models(&audio);
        self.quiet = if speech { 0 } else { self.quiet + samples.len() };
        if self.quiet >= ACTIVE_TAIL {
            self.active = false;
            self.quiet = 0;
            self.ring.clear();
        }
        events
    }

    /// Runs the front end and the classifier over audio, one score per new hop.
    /// @param audio - the samples
    fn run_models(&mut self, audio: &[i16]) -> Vec<WakeEvent> {
        let mut events = Vec::new();
        let mut consumed = 0;
        for chunk in audio.chunks(HOP) {
            consumed += chunk.len();
            if self.features.push(chunk).unwrap_or(0) == 0 {
                continue;
            }
            let score = self.classify().unwrap_or(0.0);
            self.last_score = score;
            if self.cooldown > 0 {
                self.cooldown -= 1;
                continue;
            }
            self.above = if score >= self.threshold { self.above + 1 } else { 0 };
            if self.above >= CONSECUTIVE {
                self.above = 0;
                self.cooldown = COOLDOWN_HOPS;
                events.push(WakeEvent { score, samples_ago: audio.len() - consumed });
            }
        }
        events
    }

    /// Runs the classifier on the newest 16 embeddings.
    fn classify(&self) -> TractResult<f32> {
        let input = tract_ndarray::Array3::from_shape_vec(
            (1, CLASSIFIER_FRAMES, EMBED_DIM),
            self.features.classifier_input(),
        )?
        .into_tensor();
        let out = self.classifier.run(tvec!(input.into()))?;
        Ok(*out[0].to_array_view::<f32>()?.iter().next().unwrap_or(&0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitivity_maps_to_the_threshold_sable_uses() {
        assert!((threshold_for_sensitivity(0.5) - 0.6).abs() < 1e-6);
        assert!((threshold_for_sensitivity(1.0) - 0.4).abs() < 1e-6);
        assert!((threshold_for_sensitivity(0.0) - 0.8).abs() < 1e-6);
    }

    /// Every clip is fed in 20 ms frames, which is how the microphone delivers it.
    fn fires_on(clip: &str) -> bool {
        let samples = crate::audio::wav_decode(
            &std::fs::read(format!("{}/tests/audio/{clip}.wav", env!("CARGO_MANIFEST_DIR")))
                .unwrap(),
        )
        .unwrap();
        let mut detector = WakeDetector::new(0.5).unwrap();
        let mut fired = false;
        let mut padded = samples;
        padded.extend(std::iter::repeat_n(0i16, 32_000));
        for frame in padded.chunks(320) {
            fired |= !detector.push(frame).is_empty();
        }
        fired
    }

    #[test]
    fn the_detector_fires_on_sable_and_not_on_a_sentence_without_it() {
        assert!(fires_on("sable-what-time-is-it"), "\"Sable, what time is it\" wakes it");
        assert!(fires_on("sable-tell-me-a-long-story"), "\"Sable, tell me a long story\" wakes it");
        assert!(!fires_on("and-what-day-is-it"), "\"And what day is it\" does not");
    }
}
