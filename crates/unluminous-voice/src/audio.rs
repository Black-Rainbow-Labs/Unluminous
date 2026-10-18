//! Audio in memory: the resampler that turns a microphone's rate into 16 kHz, and WAV encode and decode.
//!
//! Every model here reads 16 kHz mono, so everything that comes in is converted to that once, at the edge.
//! The resampler is Sable's (`app/src/audio/resample.rs`): a moving average before downsampling and linear
//! interpolation, carried across calls so the output does not depend on how the input was chunked. Speech
//! for a speech model does not need a studio converter; what it needs is no aliasing on the way down, and a
//! moving average as wide as the ratio removes most of what would fold back into the voice band.
//!
//! Nothing here writes a file. A WAV sent to an endpoint is built in memory.

use std::collections::VecDeque;

/// The rate every model here reads.
pub const RATE: u32 = 16_000;

/// Converts a stream from one rate to another.
pub struct Resampler {
    step: f64,
    pos: f64,
    prev: f32,
    window: VecDeque<f32>,
    width: usize,
    sum: f32,
}

impl Resampler {
    /// A converter from `from` Hz to `to` Hz.
    /// @param from - the input rate
    /// @param to - the output rate
    pub fn new(from: u32, to: u32) -> Self {
        let step = f64::from(from.max(1)) / f64::from(to.max(1));
        let width = if step > 1.5 { step.round() as usize } else { 1 };
        Resampler {
            step,
            pos: 0.0,
            prev: 0.0,
            window: VecDeque::with_capacity(width),
            width,
            sum: 0.0,
        }
    }

    /// Converts a chunk, appending to `out`.
    /// @param input - samples at the input rate
    /// @param out - where samples at the output rate go
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if (self.step - 1.0).abs() < f64::EPSILON {
            out.extend_from_slice(input);
            return;
        }
        for &raw in input {
            let x = self.smooth(raw);
            while self.pos < 1.0 {
                out.push(self.prev + (x - self.prev) * self.pos as f32);
                self.pos += self.step;
            }
            self.pos -= 1.0;
            self.prev = x;
        }
    }

    /// The moving average over the last `width` input samples.
    /// @param x - the next input sample
    fn smooth(&mut self, x: f32) -> f32 {
        if self.width == 1 {
            return x;
        }
        self.window.push_back(x);
        self.sum += x;
        if self.window.len() > self.width {
            self.sum -= self.window.pop_front().unwrap_or(0.0);
        }
        self.sum / self.window.len() as f32
    }
}

/// Mixes interleaved frames down to mono.
/// @param data - interleaved samples
/// @param channels - samples per frame
pub fn to_mono(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect()
}

/// A float sample as signed 16 bit.
/// @param x - a sample from -1 to 1
pub fn to_i16(x: f32) -> i16 {
    (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16
}

/// A signed 16 bit sample as a float from -1 to 1.
/// @param x - the sample
pub fn to_f32(x: i16) -> f32 {
    f32::from(x) / 32768.0
}

/// How loud a block is, 0 to 1: the root mean square.
/// @param samples - 16 bit samples
pub fn level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|s| f64::from(to_f32(*s)).powi(2)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

/// Wraps 16 kHz mono 16 bit samples in a WAV header, in memory.
/// @param samples - the samples
pub fn wav_encode(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Reads a WAV file held in memory and answers 16 kHz mono 16 bit samples.
///
/// PCM 16 bit or 32 bit float, any channel count and any rate, which covers what a recorder and every TTS
/// engine on this machine writes. A data chunk whose size is not believable runs to the end of the buffer,
/// because a streaming writer leaves it at 0 or `0xFFFFFFFF`.
/// @param bytes - the whole file
pub fn wav_decode(bytes: &[u8]) -> Result<Vec<i16>, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("that is not a WAV file".to_owned());
    }
    let (mut format, mut channels, mut rate, mut bits) = (0u16, 0u16, 0u32, 0u16);
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let size =
            u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]])
                as usize;
        let body = pos + 8;
        match &bytes[pos..pos + 4] {
            b"fmt " if body + 16 <= bytes.len() => {
                format = u16::from_le_bytes([bytes[body], bytes[body + 1]]);
                channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
                rate = u32::from_le_bytes([
                    bytes[body + 4],
                    bytes[body + 5],
                    bytes[body + 6],
                    bytes[body + 7],
                ]);
                bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
                if format == 0xFFFE && body + 26 <= bytes.len() {
                    format = u16::from_le_bytes([bytes[body + 24], bytes[body + 25]]);
                }
            }
            b"data" => {
                let end =
                    if size == 0 || body + size > bytes.len() { bytes.len() } else { body + size };
                let floats = samples_of(&bytes[body..end], format, bits)?;
                let mono = to_mono(&floats, usize::from(channels.max(1)));
                let mut out = Vec::with_capacity(mono.len());
                Resampler::new(rate, RATE).process(&mono, &mut out);
                return Ok(out.into_iter().map(to_i16).collect());
            }
            _ => {}
        }
        pos = body + size + (size & 1);
    }
    Err("that WAV file has no audio in it".to_owned())
}

/// The samples of a data chunk as floats.
/// @param data - the chunk's bytes
/// @param format - 1 for PCM, 3 for float
/// @param bits - bits per sample
fn samples_of(data: &[u8], format: u16, bits: u16) -> Result<Vec<f32>, String> {
    match (format, bits) {
        (1, 16) => Ok(data.chunks_exact(2).map(|b| to_f32(i16::from_le_bytes([b[0], b[1]]))).collect()),
        (3, 32) => Ok(data.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()),
        (1, 32) => Ok(data.chunks_exact(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0).collect()),
        _ => Err(format!("a WAV file of format {format} with {bits} bit samples cannot be read; 16 bit PCM and 32 bit float can")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forty_eight_to_sixteen_keeps_the_length_ratio() {
        let mut r = Resampler::new(48_000, 16_000);
        let mut out = Vec::new();
        for _ in 0..10 {
            r.process(&[0.5; 480], &mut out);
        }
        assert_eq!(out.len(), 1600);
        assert!((out[1000] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn chunking_does_not_change_the_output() {
        let input: Vec<f32> = (0..999).map(|i| (i as f32 * 0.01).sin()).collect();
        let (mut a, mut b) = (Resampler::new(44_100, 16_000), Resampler::new(44_100, 16_000));
        let (mut whole, mut parts) = (Vec::new(), Vec::new());
        a.process(&input, &mut whole);
        for chunk in input.chunks(37) {
            b.process(chunk, &mut parts);
        }
        assert_eq!(whole, parts);
    }

    #[test]
    fn a_wav_written_here_reads_back_the_same() {
        let samples: Vec<i16> = (0..1600).map(|i| ((i % 100) * 300 - 15_000) as i16).collect();
        let bytes = wav_encode(&samples);
        assert_eq!(bytes.len(), 44 + 3200);
        assert_eq!(wav_decode(&bytes).unwrap(), samples);
    }

    #[test]
    fn a_file_that_is_not_a_wav_is_refused_with_a_sentence() {
        assert!(wav_decode(b"ID3 not a wav file at all").unwrap_err().contains("not a WAV"));
    }
}
