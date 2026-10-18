//! The endpoint engine: an OpenAI compatible `/v1/audio/transcriptions`.
//!
//! The audio goes as `multipart/form-data` with `file` (a 16 kHz mono WAV built in memory, never written to
//! disk), `model`, `language` and `response_format = json`, which is the shape OpenAI, ai-proxy, Speaches
//! and whisper.cpp's server all read. The answer's `text` is the transcript. A key, when the endpoint needs
//! one, is read from the environment variable the setting names at the moment of the request and never
//! held, which is `services::ai_endpoints`' rule for every other endpoint.
//!
//! The body is written by hand because the workspace's `ureq` is built without its multipart feature, and
//! four parts of a form are a dozen lines.

use std::time::Duration;

/// How long one transcription may take.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Where the endpoint is and what to ask it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Endpoint {
    /// The full URL, `.../v1/audio/transcriptions`.
    pub url: String,
    /// The `model` field.
    pub model: String,
    /// The `language` field, empty to let the server decide.
    pub language: String,
    /// The name of the environment variable holding a key, empty for none.
    pub key_variable: String,
}

/// The boundary between the form's parts. Fixed, because a WAV of 16 bit samples and four short fields
/// cannot contain it by accident with any probability worth a random one.
const BOUNDARY: &str = "unluminous-voice-6c1b0f7e2a9d4c38";

/// Builds the `multipart/form-data` body.
/// @param wav - the audio file
/// @param endpoint - the fields
pub fn form(wav: &[u8], endpoint: &Endpoint) -> Vec<u8> {
    let mut body = Vec::with_capacity(wav.len() + 512);
    let mut field = |name: &str, value: &str| {
        body.extend_from_slice(format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    };
    field(
        "model",
        if endpoint.model.trim().is_empty() { "whisper-1" } else { endpoint.model.trim() },
    );
    field("response_format", "json");
    if !endpoint.language.trim().is_empty() {
        field("language", endpoint.language.trim());
    }
    body.extend_from_slice(format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

/// The HTTP client, with the machine's own certificates. See `install::agent`.
fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build();
    ureq::Agent::new_with_config(config)
}

/// Transcribes 16 kHz mono audio at the endpoint.
/// @param endpoint - where and what
/// @param samples - the audio
pub fn transcribe(endpoint: &Endpoint, samples: &[i16]) -> Result<String, String> {
    if endpoint.url.trim().is_empty() {
        return Err("no transcription endpoint is set. Set voice.endpoint to an OpenAI compatible /v1/audio/transcriptions URL".to_owned());
    }
    let body = form(&crate::audio::wav_encode(samples), endpoint);
    let mut request = agent()
        .post(endpoint.url.trim())
        .header("content-type", format!("multipart/form-data; boundary={BOUNDARY}"));
    if !endpoint.key_variable.trim().is_empty() {
        match std::env::var(endpoint.key_variable.trim()) {
            Ok(key) if !key.trim().is_empty() => {
                request = request.header("authorization", format!("Bearer {}", key.trim()))
            }
            _ => {
                return Err(format!(
                    "the environment variable {} that should hold the endpoint's key is not set",
                    endpoint.key_variable.trim()
                ))
            }
        }
    }
    let mut reply = request
        .send(&body[..])
        .map_err(|e| format!("{} did not answer: {e}", endpoint.url.trim()))?;
    let status = reply.status().as_u16();
    let text = reply
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("the answer could not be read: {e}"))?;
    if !(200..300).contains(&status) {
        return Err(format!(
            "{} answered HTTP {status}: {}",
            endpoint.url.trim(),
            text.chars().take(300).collect::<String>()
        ));
    }
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
        format!(
            "{} answered something that is not JSON: {}",
            endpoint.url.trim(),
            text.chars().take(200).collect::<String>()
        )
    })?;
    Ok(value["text"].as_str().unwrap_or("").trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_names_the_file_and_every_field_an_openai_endpoint_reads() {
        let endpoint = Endpoint {
            url: "http://x".into(),
            model: "".into(),
            language: "en".into(),
            key_variable: "".into(),
        };
        let body = String::from_utf8_lossy(&form(b"RIFFdata", &endpoint)).to_string();
        assert!(body.contains("name=\"model\"\r\n\r\nwhisper-1\r\n"));
        assert!(body.contains("name=\"language\"\r\n\r\nen\r\n"));
        assert!(body.contains("name=\"response_format\"\r\n\r\njson\r\n"));
        assert!(body.contains(
            "name=\"file\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFFdata\r\n"
        ));
        assert!(body.ends_with(&format!("--{BOUNDARY}--\r\n")));
    }

    #[test]
    fn a_missing_url_and_a_missing_key_are_refused_before_anything_is_sent() {
        assert!(transcribe(&Endpoint::default(), &[0; 8000])
            .unwrap_err()
            .contains("voice.endpoint"));
        let endpoint = Endpoint {
            url: "http://127.0.0.1:9".into(),
            key_variable: "UNLUMINOUS_VOICE_TEST_KEY_THAT_IS_NOT_SET".into(),
            ..Endpoint::default()
        };
        assert!(transcribe(&endpoint, &[0; 8000]).unwrap_err().contains("is not set"));
    }

    /// ai-proxy on this machine, when `UNLUMINOUS_VOICE_ENDPOINT` names it.
    #[test]
    #[ignore]
    fn a_real_endpoint_transcribes_a_clip() {
        let url = std::env::var("UNLUMINOUS_VOICE_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4141/v1/audio/transcriptions".into());
        let samples = crate::audio::wav_decode(
            &std::fs::read(format!(
                "{}/tests/audio/sable-tell-me-a-long-story.wav",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .unwrap();
        let text = transcribe(
            &Endpoint {
                url,
                model: "whisper-1".into(),
                language: "en".into(),
                key_variable: String::new(),
            },
            &samples,
        )
        .unwrap();
        assert!(text.to_lowercase().contains("lighthouse"), "{text}");
    }
}
