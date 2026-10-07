//! What plays a sound node, behind one trait so a test can use a player that makes no sound.
//!
//! `task-2202`, §5.4 of `tasks/task-2199-realm-tdd.md`. **The real player is `rodio`**, which is pure Rust:
//! `cpal` for the output device and `symphonia` for the decoders, so it builds on Windows and macOS with no
//! SDK and fetches nothing when it runs. One output device a window, opened the first time something
//! plays, and one [`RodioPlayer`] a sound node.
//!
//! **A test uses [`SilentPlayer`]**, which opens no device and moves its position along with the clock. A
//! window only plays sound when the released binary asked it to, which is the same switch that lets it read
//! a person's settings (`UnluminousApp::load_settings`); a window a test builds never makes a sound, and
//! the machine running the suite needs no sound card.
//!
//! A video is played by the window's native web view rather than through this trait, because decoding a
//! video in process is a native dependency on two platforms. The trait is what a decoder would implement
//! the day there is one; the realm file's keys would not change.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Something that plays one file.
pub trait Player: Send {
    /// Open `path`, paused at the start, and answer how long it is.
    fn load(&mut self, path: &Path) -> Result<Duration, String>;
    fn play(&mut self);
    fn pause(&mut self);
    fn playing(&self) -> bool;
    fn seek(&mut self, to: Duration);
    fn position(&self) -> Duration;
    fn duration(&self) -> Duration;
    /// From 0 to 1.
    fn set_volume(&mut self, volume: f32);
    fn set_loop(&mut self, looping: bool);
    /// Called once a frame while it is playing: start again at the end when it loops, or stop there. Answers
    /// whether it is still playing.
    fn catch_up(&mut self) -> bool;
}

/// A player that makes no sound: its position follows the clock while it plays. What a test uses.
#[derive(Debug, Default)]
pub struct SilentPlayer {
    duration: Duration,
    /// Where it was when it last started or stopped.
    at: Duration,
    /// When it started playing, while it is playing.
    since: Option<Instant>,
    looping: bool,
    volume: f32,
}

impl SilentPlayer {
    pub fn new() -> SilentPlayer {
        SilentPlayer { volume: 1.0, ..SilentPlayer::default() }
    }
}

impl Player for SilentPlayer {
    fn load(&mut self, path: &Path) -> Result<Duration, String> {
        let bytes = std::fs::read(path)
            .map_err(|problem| format!("{} could not be read: {problem}", path.display()))?;
        // A WAV file says how long it is in its header, which is what a test writes; anything else is taken to
        // be a minute long, which is long enough to drag a seek bar along.
        self.duration = wav_duration(&bytes).unwrap_or(Duration::from_secs(60));
        self.at = Duration::ZERO;
        self.since = None;
        Ok(self.duration)
    }

    fn play(&mut self) {
        if self.since.is_none() {
            if self.at >= self.duration {
                self.at = Duration::ZERO;
            }
            self.since = Some(Instant::now());
        }
    }

    fn pause(&mut self) {
        self.at = self.position();
        self.since = None;
    }

    fn playing(&self) -> bool {
        self.since.is_some()
    }

    fn seek(&mut self, to: Duration) {
        self.at = to.min(self.duration);
        if self.since.is_some() {
            self.since = Some(Instant::now());
        }
    }

    fn position(&self) -> Duration {
        let moved = self.since.map(|since| since.elapsed()).unwrap_or_default();
        let at = self.at + moved;
        match (self.looping, self.duration.is_zero()) {
            (true, false) => {
                Duration::from_secs_f64(at.as_secs_f64() % self.duration.as_secs_f64())
            }
            _ => at.min(self.duration),
        }
    }

    fn duration(&self) -> Duration {
        self.duration
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    fn set_loop(&mut self, looping: bool) {
        self.looping = looping;
    }

    fn catch_up(&mut self) -> bool {
        if self.since.is_some() && !self.looping && self.position() >= self.duration {
            self.at = self.duration;
            self.since = None;
        }
        self.playing()
    }
}

/// How long a WAV file plays, read from its header: the data chunk's size over the bytes a second.
pub fn wav_duration(bytes: &[u8]) -> Option<Duration> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12;
    let mut bytes_a_second = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?) as usize;
        let body = at + 8;
        if id == b"fmt " && body + 12 <= bytes.len() {
            bytes_a_second = Some(u32::from_le_bytes(bytes[body + 8..body + 12].try_into().ok()?));
        }
        if id == b"data" {
            let rate = bytes_a_second.filter(|rate| *rate > 0)?;
            return Some(Duration::from_secs_f64(size as f64 / f64::from(rate)));
        }
        at = body + size + (size % 2);
    }
    None
}

/// A WAV file of silence `seconds` long, at 8 kHz, one channel, 16 bits. What a test plays.
pub fn a_silent_wav(seconds: f32) -> Vec<u8> {
    let rate: u32 = 8_000;
    let samples = (rate as f32 * seconds) as u32;
    let data = samples * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    out.resize(44 + data as usize, 0);
    out
}

/// The window's one audio output, opened the first time a sound plays.
///
/// **Not `Send`**, because a `cpal` stream is not on every platform, so it lives on the window's own thread in
/// `Live` and only the [`RodioPlayer`]s, which are, are handed out.
pub struct Output {
    sink: rodio::MixerDeviceSink,
}

impl Output {
    /// The default output device, or why there is none. A machine with no sound card, or one whose device is
    /// held by something else, answers here and the node says so rather than the window falling over.
    pub fn open() -> Result<Output, String> {
        let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|problem| format!("No audio output: {problem}"))?;
        // rodio prints a line when the device is closed, which is every time a window closes.
        sink.log_on_drop(false);
        Ok(Output { sink })
    }

    /// A player on this output.
    pub fn player(&self) -> RodioPlayer {
        RodioPlayer {
            player: rodio::Player::connect_new(self.sink.mixer()),
            path: PathBuf::new(),
            duration: Duration::ZERO,
            looping: false,
        }
    }
}

/// A sound node's player: a `rodio::Player` with the file it was given.
pub struct RodioPlayer {
    player: rodio::Player,
    path: PathBuf,
    duration: Duration,
    looping: bool,
}

impl RodioPlayer {
    /// Put the file on the player again, paused at the start, which is how a sound that ended starts over.
    fn queue(&mut self) -> Result<(), String> {
        use rodio::Source;
        let decoder = decoder_for(&self.path)?;
        if let Some(length) = decoder.total_duration() {
            self.duration = length;
        }
        self.player.clear();
        self.player.append(decoder);
        self.player.pause();
        Ok(())
    }
}

/// What a sound file is decoded by.
pub type FileDecoder = rodio::Decoder<std::io::BufReader<std::fs::File>>;

/// A decoder over a sound file, which is the half of playing one that needs no audio device.
pub fn decoder_for(path: &Path) -> Result<FileDecoder, String> {
    let file = std::fs::File::open(path)
        .map_err(|problem| format!("{} could not be read: {problem}", path.display()))?;
    let length = file.metadata().map(|about| about.len()).unwrap_or(0);
    let mut builder = rodio::Decoder::builder()
        .with_data(std::io::BufReader::new(file))
        .with_byte_len(length)
        .with_seekable(true);
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        builder = builder.with_hint(extension);
    }
    builder.build().map_err(|problem| format!("{} could not be played: {problem}", path.display()))
}

impl Player for RodioPlayer {
    fn load(&mut self, path: &Path) -> Result<Duration, String> {
        self.path = path.to_path_buf();
        self.queue()?;
        Ok(self.duration)
    }

    fn play(&mut self) {
        if self.player.empty() {
            let _ = self.queue();
        }
        self.player.play();
    }

    fn pause(&mut self) {
        self.player.pause();
    }

    fn playing(&self) -> bool {
        !self.player.is_paused() && !self.player.empty()
    }

    fn seek(&mut self, to: Duration) {
        if self.player.empty() {
            let _ = self.queue();
        }
        let _ = self.player.try_seek(to.min(self.duration));
    }

    fn position(&self) -> Duration {
        match (self.player.empty(), self.duration.is_zero()) {
            (true, _) => self.duration,
            // A stream that never said how long it is cannot be clamped to it.
            (false, true) => self.player.get_pos(),
            (false, false) => self.player.get_pos().min(self.duration),
        }
    }

    fn duration(&self) -> Duration {
        self.duration
    }

    fn set_volume(&mut self, volume: f32) {
        self.player.set_volume(volume.clamp(0.0, 1.0));
    }

    fn set_loop(&mut self, looping: bool) {
        self.looping = looping;
    }

    fn catch_up(&mut self) -> bool {
        if self.player.empty() && !self.player.is_paused() {
            match self.looping {
                true => {
                    if self.queue().is_ok() {
                        self.player.play();
                    }
                }
                false => {
                    let _ = self.queue();
                    let _ = self.player.try_seek(self.duration);
                }
            }
        }
        self.playing()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2207`: an `.m4a` is a container, and the two things found in one are AAC and Apple Lossless.
    /// The second was refused with "The format of the data has not been recognized" until the decoder for it
    /// was turned on, because rodio's `mp4` feature brings the AAC decoder only.
    #[test]
    fn an_m4a_holding_aac_or_apple_lossless_can_be_decoded() {
        use rodio::Source;
        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("sounds");
        for name in ["aac.m4a", "apple-lossless.m4a"] {
            let decoder = decoder_for(&fixtures.join(name))
                .unwrap_or_else(|problem| panic!("{name} should decode: {problem}"));
            let samples = decoder.count();
            assert!(samples > 1_000, "{name} decoded to {samples} samples");
        }
    }

    #[test]
    fn a_wav_says_how_long_it_is() {
        let wav = a_silent_wav(1.0);
        assert_eq!(wav_duration(&wav), Some(Duration::from_secs(1)));
        assert_eq!(wav_duration(b"not a wav at all"), None);
    }

    #[test]
    fn a_silent_player_moves_with_the_clock_and_stops_where_it_was_paused() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-silent-player-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("make the folder");
        let file = folder.join("one-second.wav");
        std::fs::write(&file, a_silent_wav(1.0)).expect("write the file");
        let mut player = SilentPlayer::new();
        assert_eq!(player.load(&file), Ok(Duration::from_secs(1)));
        assert!(!player.playing(), "it opens paused");
        player.play();
        std::thread::sleep(Duration::from_millis(60));
        assert!(player.position() > Duration::ZERO, "it moves while it plays");
        player.pause();
        let paused = player.position();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(player.position(), paused, "and stays where it was paused");
        player.seek(Duration::from_millis(900));
        player.play();
        std::thread::sleep(Duration::from_millis(200));
        assert!(!player.catch_up(), "a sound that is not looping stops at its end");
        assert_eq!(player.position(), Duration::from_secs(1));
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&folder);
    }
}
