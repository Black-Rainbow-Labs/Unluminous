//! The microphone: the input device `voice.microphone` names, or cpal's default when it names none, converted
//! to 16 kHz mono on the way in.
//!
//! A cpal stream is made and dropped on a thread of its own, because on macOS it may not move between
//! threads, and the thread holds it until the `Microphone` is dropped. That drop is what turns the system's
//! microphone indicator off, so the stream exists only while dictation or always listen needs it.
//!
//! Sable's capture (`app/src/audio/capture.rs`), with every sample format a desktop device reports rather
//! than float alone.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::{to_i16, to_mono, Resampler, RATE};

/// An open microphone. Dropping it closes the stream.
pub struct Microphone {
    stop: Arc<AtomicBool>,
    /// The device and its format, for `voice status`.
    pub description: String,
}

impl Drop for Microphone {
    /// Closes the stream.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Converts whatever the device delivers to 16 kHz mono and hands it on.
struct Converter<F: FnMut(Vec<i16>)> {
    resampler: Resampler,
    channels: usize,
    deliver: F,
}

impl<F: FnMut(Vec<i16>)> Converter<F> {
    /// Converts one block.
    /// @param data - interleaved float samples
    fn take(&mut self, data: &[f32]) {
        let mono = to_mono(data, self.channels);
        let mut out = Vec::with_capacity(mono.len() / 2 + 1);
        self.resampler.process(&mono, &mut out);
        (self.deliver)(out.into_iter().map(to_i16).collect());
    }
}

/// The default input device's name, when there is one.
pub fn default_device_name() -> Option<String> {
    let device = cpal::default_host().default_input_device()?;
    #[allow(deprecated)]
    device.name().ok()
}

/// The names of every input device on this machine, in the order the system lists them. A name is what
/// `voice.microphone` holds (`task-2320`).
pub fn device_names() -> Vec<String> {
    let Ok(devices) = cpal::default_host().input_devices() else {
        return Vec::new();
    };
    #[allow(deprecated)]
    let mut names: Vec<String> = devices.filter_map(|device| device.name().ok()).collect();
    names.dedup();
    names
}

/// Opens the microphone named `device`, or the default one when `device` is empty. A named device that is
/// not connected falls back to the default, and the description says so, because dictation that stops
/// working when a headset is unplugged is worse than dictation from the laptop's own microphone.
/// `deliver` is called on cpal's own thread with every block, already 16 kHz mono, so it must be quick:
/// sending on a channel is what it is for.
/// @param device - the device's name, or empty for the default
/// @param deliver - receives the audio
pub fn open(
    device: &str,
    deliver: impl FnMut(Vec<i16>) + Send + 'static,
) -> Result<Microphone, String> {
    let device = device.to_owned();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("unluminous-voice-microphone".into())
        .spawn(move || match build(&device, deliver) {
            Ok((stream, description)) => {
                let _ = ready_tx.send(Ok(description));
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                drop(stream);
            }
            Err(error) => {
                let _ = ready_tx.send(Err(error));
            }
        })
        .map_err(|e| e.to_string())?;
    let description = ready_rx
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| "the microphone did not open within ten seconds".to_owned())??;
    Ok(Microphone { stop, description })
}

/// The device named `wanted`, or the default one, and a note when the named one was not there.
/// @param wanted - the device's name, or empty for the default
fn choose(wanted: &str) -> Result<(cpal::Device, Option<String>), String> {
    let host = cpal::default_host();
    if !wanted.is_empty() {
        #[allow(deprecated)]
        let named = host.input_devices().ok().and_then(|mut devices| {
            devices.find(|device| device.name().ok().as_deref() == Some(wanted))
        });
        if let Some(device) = named {
            return Ok((device, None));
        }
    }
    let device = host.default_input_device().ok_or("this machine has no microphone")?;
    let note =
        (!wanted.is_empty()).then(|| format!("{wanted} is not connected, so the default is used"));
    Ok((device, note))
}

/// Builds and starts the stream, in whichever sample format the device prefers.
/// @param wanted - the device's name, or empty for the default
/// @param deliver - receives the audio
fn build(
    wanted: &str,
    deliver: impl FnMut(Vec<i16>) + Send + 'static,
) -> Result<(cpal::Stream, String), String> {
    let (device, note) = choose(wanted)?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("the microphone could not be read: {e}"))?;
    let format = supported.sample_format();
    let config = supported.config();
    #[allow(deprecated)]
    let name = device.name().unwrap_or_else(|_| "the default microphone".into());
    let mut description =
        format!("{name}, {} Hz, {} channels", config.sample_rate, config.channels);
    if let Some(note) = note {
        description = format!("{description} ({note})");
    }
    let mut converter = Converter {
        resampler: Resampler::new(config.sample_rate, RATE),
        channels: usize::from(config.channels),
        deliver,
    };
    let error = |e: cpal::StreamError| eprintln!("unluminous-voice: microphone: {e}");
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| converter.take(data),
            error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                converter.take(&data.iter().map(|s| crate::audio::to_f32(*s)).collect::<Vec<f32>>())
            },
            error,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream(
            &config,
            move |data: &[i32], _: &cpal::InputCallbackInfo| {
                converter
                    .take(&data.iter().map(|s| *s as f32 / 2_147_483_648.0).collect::<Vec<f32>>())
            },
            error,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                converter.take(
                    &data.iter().map(|s| (f32::from(*s) - 32768.0) / 32768.0).collect::<Vec<f32>>(),
                )
            },
            error,
            None,
        ),
        other => {
            return Err(format!(
                "the microphone delivers {other} samples, which Unluminous cannot read"
            ))
        }
    }
    .map_err(|e| format!("the microphone could not be opened: {e}"))?;
    stream.play().map_err(|e| format!("the microphone could not be started: {e}"))?;
    Ok((stream, description))
}
