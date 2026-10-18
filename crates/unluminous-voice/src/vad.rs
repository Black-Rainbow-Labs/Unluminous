//! Voice activity detection: Silero VAD v5 under `tract`, which is Sable's choice and its code
//! (`crates/sable-wake/src/vad.rs`).
//!
//! Sable measured both detectors it had on one hour of podcasts and on labelled clips: Silero caught all
//! 3,996 speech clips and passed 103 of 327 noise clips, and earshot (the WebRTC port) passed all 327 noise
//! clips. As a gate in front of the wake models, a detector that passes noise keeps the models running all
//! the time in a noisy room, so Silero's 25 s of processor time per audio hour is the cheaper answer. Here
//! it does a second job: it is what cuts dictation into segments at pauses.
//!
//! Silero v5 as published does not load under `tract` (its `If` nodes fail type analysis), so the copy in
//! `models/` is Sable's 16 kHz graph with each `If` replaced by the branch that runs.

use tract_onnx::prelude::*;

/// Speech is held on for this many samples after the last voiced chunk (300 ms).
const HANGOVER: usize = 4_800;

/// Samples per Silero chunk at 16 kHz.
const CHUNK: usize = 512;

/// Context samples Silero v5 reads before each chunk.
const CONTEXT: usize = 64;

/// The speech probability a chunk must reach.
const THRESHOLD: f32 = 0.5;

/// The detector.
pub struct SileroVad {
    plan: TypedRunnableModel<TypedModel>,
    state: Tensor,
    context: Vec<f32>,
    pending: Vec<f32>,
    since_voice: usize,
}

impl SileroVad {
    /// Loads the 16 kHz copy of Silero v5 compiled into this crate.
    pub fn bundled() -> TractResult<Self> {
        Self::new(crate::wake::SILERO)
    }

    /// Loads Silero from its ONNX bytes.
    /// @param onnx - `silero_vad_16k.onnx`
    pub fn new(onnx: &[u8]) -> TractResult<Self> {
        let mut model = tract_onnx::onnx().model_for_read(&mut std::io::Cursor::new(onnx))?;
        model = model
            .with_input_fact(0, f32::fact([1, CHUNK + CONTEXT]).into())?
            .with_input_fact(1, f32::fact([2, 1, 128]).into())?;
        if model.input_outlets()?.len() > 2 {
            model = model.with_input_fact(2, InferenceFact::from(tensor0(16_000i64)))?;
        }
        let plan = model.into_optimized()?.into_runnable()?;
        Ok(SileroVad {
            plan,
            state: Tensor::zero::<f32>(&[2, 1, 128])?,
            context: vec![0.0; CONTEXT],
            pending: vec![],
            since_voice: usize::MAX / 2,
        })
    }

    /// Feeds 16 kHz samples and answers whether speech is going on after them.
    /// @param samples - 16 kHz mono audio, any length
    pub fn push(&mut self, samples: &[i16]) -> bool {
        self.pending.extend(samples.iter().map(|s| crate::audio::to_f32(*s)));
        while self.pending.len() >= CHUNK {
            let chunk: Vec<f32> = self.pending.drain(..CHUNK).collect();
            let p = self.probability(&chunk).unwrap_or(0.0);
            self.since_voice =
                if p >= THRESHOLD { 0 } else { self.since_voice.saturating_add(CHUNK) };
        }
        self.since_voice < HANGOVER
    }

    /// Forgets everything, so the next audio is judged on its own.
    pub fn reset(&mut self) {
        if let Ok(zero) = Tensor::zero::<f32>(&[2, 1, 128]) {
            self.state = zero;
        }
        self.context = vec![0.0; CONTEXT];
        self.pending.clear();
        self.since_voice = usize::MAX / 2;
    }

    /// The speech probability of one chunk.
    /// @param chunk - 512 samples from -1 to 1
    fn probability(&mut self, chunk: &[f32]) -> TractResult<f32> {
        let mut input = self.context.clone();
        input.extend_from_slice(chunk);
        let x = tract_ndarray::Array2::from_shape_vec((1, input.len()), input)?.into_tensor();
        let out = self.plan.run(tvec!(x.into(), self.state.clone().into()))?;
        self.state = out[1].clone().into_tensor();
        self.context = chunk[chunk.len() - CONTEXT..].to_vec();
        Ok(*out[0].to_array_view::<f32>()?.iter().next().unwrap_or(&0.0))
    }
}
