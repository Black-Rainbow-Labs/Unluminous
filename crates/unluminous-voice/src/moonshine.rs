//! The local speech model: Moonshine base, quantized, through ONNX Runtime.
//!
//! Moonshine takes raw 16 kHz samples, so there is no spectrogram to compute: the encoder's one input is
//! `input_values [1, samples]` and it answers `last_hidden_state [1, frames, 416]`. The merged decoder takes
//! the last token, the encoder's states, `use_cache_branch`, and eight layers of `past_key_values`, each
//! `[1, 8, n, 52]` for the decoder's own attention and the encoder's. It answers `logits` and the matching
//! `present.*`. Decoding is greedy from token 1 until token 2, at most 6.5 tokens a second of audio, which
//! is the model card's own limit against a loop.
//!
//! Measured with the same files through Python's onnxruntime on this machine, two threads: 4.3 s of speech
//! in 0.18 s. That is what makes transcribing a whole segment again every 300 ms affordable, which is how
//! the prompt fills while somebody speaks. `tasks/task-2290-speech-to-text-tdd.md` section 3.

use std::collections::HashMap;
use std::path::Path;

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

/// Decoder layers, heads and the size of one head, from the model's `config.json`.
const LAYERS: usize = 8;
const HEADS: usize = 8;
const HEAD_DIM: usize = 52;
/// The start and end tokens.
const START: i64 = 1;
const END: i64 = 2;
/// Tokens a second of audio the decoder may write, the model card's limit.
const TOKENS_PER_SECOND: f32 = 6.5;
/// The shortest audio worth transcribing: a quarter of a second.
const SHORTEST: usize = 4_000;

/// The loaded model.
pub struct Moonshine {
    encoder: Session,
    decoder: Session,
    vocabulary: Vec<String>,
}

/// Turns an `ort` error into a sentence.
/// @param error - the error
fn said(error: impl std::fmt::Display) -> String {
    format!("the speech model failed: {error}")
}

impl Moonshine {
    /// Loads the model from its folder, after loading ONNX Runtime from `runtime` if no library is loaded
    /// yet in this process.
    /// @param dir - the folder with `encoder_model.onnx`, `decoder_model_merged.onnx` and `tokenizer.json`
    /// @param runtime - the ONNX Runtime library
    /// @param threads - how many threads each session may use
    pub fn load(dir: &Path, runtime: &Path, threads: usize) -> Result<Moonshine, String> {
        let _ = ort::init_from(runtime)
            .map_err(|e| {
                format!("ONNX Runtime could not be loaded from {}: {e}", runtime.display())
            })?
            .with_name("unluminous-voice")
            .commit();
        let session = |file: &str| -> Result<Session, String> {
            Session::builder()
                .map_err(said)?
                .with_optimization_level(GraphOptimizationLevel::All)
                .map_err(said)?
                .with_intra_threads(threads.max(1))
                .map_err(said)?
                .commit_from_file(dir.join(file))
                .map_err(said)
        };
        let encoder = session("encoder_model.onnx")?;
        let decoder = session("decoder_model_merged.onnx")?;
        let tokenizer = std::fs::read_to_string(dir.join("tokenizer.json"))
            .map_err(|e| format!("tokenizer.json could not be read: {e}"))?;
        Ok(Moonshine { encoder, decoder, vocabulary: vocabulary(&tokenizer)? })
    }

    /// Transcribes 16 kHz mono audio.
    /// @param samples - the audio
    pub fn transcribe(&mut self, samples: &[i16]) -> Result<String, String> {
        if samples.len() < SHORTEST {
            return Ok(String::new());
        }
        let samples = crate::audio::trim_silence(samples);
        if samples.len() < SHORTEST {
            return Ok(String::new());
        }
        let input: Vec<f32> = samples.iter().map(|s| crate::audio::to_f32(*s)).collect();
        let (hidden_shape, hidden) = {
            let encoded = self.encoder.run(ort::inputs!["input_values" => Tensor::from_array(([1usize, input.len()], input)).map_err(said)?]).map_err(said)?;
            let (shape, states) =
                encoded["last_hidden_state"].try_extract_tensor::<f32>().map_err(said)?;
            (shape.iter().map(|d| *d as usize).collect::<Vec<usize>>(), states.to_vec())
        };
        let limit = ((samples.len() as f32 / 16_000.0) * TOKENS_PER_SECOND).ceil() as usize + 4;
        let tokens = self.decode(&hidden_shape, &hidden, limit)?;
        Ok(detokenise(&self.vocabulary, &tokens))
    }

    /// Greedy decoding with the cache.
    /// @param hidden_shape - the encoder's output shape
    /// @param hidden - the encoder's output
    /// @param limit - the most tokens to write
    fn decode(
        &mut self,
        hidden_shape: &[usize],
        hidden: &[f32],
        limit: usize,
    ) -> Result<Vec<i64>, String> {
        let mut past: HashMap<String, (Vec<usize>, Vec<f32>)> = HashMap::new();
        for layer in 0..LAYERS {
            for part in ["decoder", "encoder"] {
                for kind in ["key", "value"] {
                    past.insert(
                        format!("past_key_values.{layer}.{part}.{kind}"),
                        (vec![1, HEADS, 0, HEAD_DIM], Vec::new()),
                    );
                }
            }
        }
        let mut tokens = vec![START];
        for step in 0..limit {
            let mut inputs: Vec<(String, ort::value::DynValue)> =
                Vec::with_capacity(4 + past.len());
            inputs.push((
                "input_ids".into(),
                Tensor::from_array(([1usize, 1], vec![*tokens.last().unwrap_or(&START)]))
                    .map_err(said)?
                    .into_dyn(),
            ));
            inputs.push((
                "encoder_hidden_states".into(),
                Tensor::from_array((hidden_shape.to_vec(), hidden.to_vec()))
                    .map_err(said)?
                    .into_dyn(),
            ));
            inputs.push((
                "use_cache_branch".into(),
                Tensor::from_array(([1usize], vec![step > 0])).map_err(said)?.into_dyn(),
            ));
            for (name, (shape, data)) in &past {
                inputs.push((
                    name.clone(),
                    Tensor::from_array((shape.clone(), data.clone())).map_err(said)?.into_dyn(),
                ));
            }
            let outputs = self.decoder.run(inputs).map_err(said)?;
            let (shape, logits) = outputs["logits"].try_extract_tensor::<f32>().map_err(said)?;
            let vocab = *shape.last().unwrap_or(&1) as usize;
            let last = &logits[logits.len() - vocab..];
            let next = last
                .iter()
                .enumerate()
                .fold((0usize, f32::MIN), |best, (i, v)| if *v > best.1 { (i, *v) } else { best })
                .0 as i64;
            for (name, value) in outputs.iter() {
                let Some(rest) = name.strip_prefix("present.") else { continue };
                // The encoder's half is worked out on the first step and the same thereafter.
                if step > 0 && rest.contains(".encoder.") {
                    continue;
                }
                let (shape, data) = value.try_extract_tensor::<f32>().map_err(said)?;
                past.insert(
                    format!("past_key_values.{rest}"),
                    (shape.iter().map(|d| *d as usize).collect(), data.to_vec()),
                );
            }
            if next == END {
                break;
            }
            tokens.push(next);
            if let Some(cut) = looping(&tokens) {
                tokens.truncate(cut);
                break;
            }
        }
        Ok(tokens)
    }
}

/// Where to cut a decode that has started repeating itself, when it has: the same run of one to twelve
/// tokens three times in a row at the end ("about Along Story about Along Story about"). A partial pass over
/// a word that is half spoken does this, and the next pass, with more audio, does not.
/// @param tokens - the tokens so far
pub fn looping(tokens: &[i64]) -> Option<usize> {
    (1..=12).find_map(|run| {
        let n = tokens.len();
        if n < run * 3 + 1 {
            return None;
        }
        let last = &tokens[n - run..];
        (tokens[n - 2 * run..n - run] == *last && tokens[n - 3 * run..n - 2 * run] == *last)
            .then_some(n - 2 * run)
    })
}

/// The vocabulary of a `tokenizer.json`, as a list indexed by token id.
/// @param json - the file's text
pub fn vocabulary(json: &str) -> Result<Vec<String>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("tokenizer.json is not JSON: {e}"))?;
    let map = value["model"]["vocab"].as_object().ok_or("tokenizer.json has no vocabulary")?;
    let mut list = vec![String::new(); map.len()];
    for (piece, id) in map {
        if let Some(slot) = id.as_u64().and_then(|id| list.get_mut(id as usize)) {
            *slot = piece.clone();
        }
    }
    Ok(list)
}

/// Turns token ids back into text: `▁` is a space, `<0xNN>` is a byte, and a special token such as
/// `<s>` is dropped.
/// @param vocabulary - the list from [`vocabulary`]
/// @param tokens - the ids, the start token included
pub fn detokenise(vocabulary: &[String], tokens: &[i64]) -> String {
    let mut bytes: Vec<u8> = Vec::new();
    for id in tokens.iter().filter(|id| **id > END) {
        let Some(piece) = vocabulary.get(*id as usize) else { continue };
        if let Some(hex) = piece.strip_prefix("<0x").and_then(|rest| rest.strip_suffix('>')) {
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                bytes.push(byte);
            }
            continue;
        }
        if piece.starts_with('<') && piece.ends_with('>') {
            continue;
        }
        bytes.extend_from_slice(piece.replace('\u{2581}', " ").as_bytes());
    }
    String::from_utf8_lossy(&bytes).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pieces_are_joined_into_words_with_bytes_read_back() {
        let vocabulary: Vec<String> = [
            "<unk>",
            "<s>",
            "</s>",
            "\u{2581}Fix",
            "\u{2581}the",
            "\u{2581}t",
            "est",
            ".",
            "<0xE2>",
            "<0x80>",
            "<0x99>",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(detokenise(&vocabulary, &[1, 3, 4, 5, 6, 7]), "Fix the test.");
        assert_eq!(detokenise(&vocabulary, &[1, 8, 9, 10]), "\u{2019}");
    }

    #[test]
    fn a_decode_that_repeats_itself_is_cut_after_the_first_time() {
        assert_eq!(looping(&[1, 5, 6, 7, 8, 7, 8, 7, 8]), Some(5));
        assert_eq!(looping(&[1, 9, 9, 9]), Some(2));
        assert_eq!(looping(&[1, 5, 6, 7, 8]), None);
    }

    #[test]
    fn a_vocabulary_is_read_out_of_a_tokenizer_file() {
        let json = r#"{"model":{"type":"BPE","vocab":{"<unk>":0,"<s>":1,"</s>":2,"▁hi":3}}}"#;
        assert_eq!(vocabulary(json).unwrap()[3], "\u{2581}hi");
    }
}
