//! Cuts dictation into segments at pauses, and says when to transcribe and when dictation is over.
//!
//! Pure: it is handed samples and the voice activity detector's answer for them, and it answers what to do.
//! No model, no clock and no thread, so every rule here is a unit test.
//!
//! - While a segment has speech in it, the whole segment is transcribed again every `partial_every`
//!   samples. Each answer is the segment's **partial** text, which the prompt shows as it changes.
//! - After [`PAUSE`] of silence, or at [`LONGEST`], the segment is transcribed once more as **final**, its
//!   text is kept, and a new segment starts. Short segments are what keep every pass cheap.
//! - Before any speech, only the last [`LEAD`] is kept, so the start of the first word is not cut off and a
//!   long silence does not grow the segment.
//!
//! When dictation ends depends on how it started: see [`Segmenter::should_end`].

/// Silence that ends a segment: 700 ms, which is Sable's end of turn wait.
pub const PAUSE: usize = 11_200;
/// The longest a segment grows before it is cut: 20 s, so the most one pass costs stays under a second.
pub const LONGEST: usize = 320_000;
/// Audio kept from before the first speech of a segment: 300 ms.
pub const LEAD: usize = 4_800;
/// No partial pass until a segment holds this much audio: 750 ms. A speech model given a quarter of a second
/// writes a guess ("Cb, cb") that the next pass takes back, and the prompt would flicker with it.
pub const FIRST_PARTIAL: usize = 12_000;
/// A partial every 300 ms of audio for the local model.
pub const LOCAL_PARTIAL: usize = 4_800;
/// A partial every 1.2 s for an endpoint, where each one is a request.
pub const ENDPOINT_PARTIAL: usize = 19_200;

/// How dictation started, which decides how it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    /// The button, or `voice dictate`: it goes on until it is stopped, or until a long quiet.
    Button,
    /// The wake word: it ends at the first real pause after something was said.
    Wake,
    /// A file: it ends when the file does.
    File,
}

/// Why dictation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// Somebody stopped it: the button again, `voice dictate off`, or the message was sent.
    Stopped,
    /// Nothing was said for a long time.
    Quiet,
    /// After a wake, somebody said something and then stopped.
    Finished,
    /// After a wake, the transcript did not start with "Sable".
    FalseWake,
    /// The file ran out.
    EndOfFile,
    /// The microphone stopped or was refused.
    Lost,
}

impl Ended {
    /// The word `voice status` and the window use.
    pub fn name(self) -> &'static str {
        match self {
            Ended::Stopped => "stopped",
            Ended::Quiet => "quiet",
            Ended::Finished => "finished",
            Ended::FalseWake => "false wake",
            Ended::EndOfFile => "end of file",
            Ended::Lost => "lost",
        }
    }
}

/// One thing to transcribe.
#[derive(Debug, Clone, PartialEq)]
pub struct Pass {
    /// Which segment, counting from 0.
    pub segment: u32,
    /// The whole segment so far.
    pub audio: Vec<i16>,
    /// Whether this is the segment's last pass.
    pub last: bool,
}

/// The state.
#[derive(Debug, Clone)]
pub struct Segmenter {
    started: Started,
    partial_every: usize,
    segment: u32,
    audio: Vec<i16>,
    heard: bool,
    silence: usize,
    since_pass: usize,
    quiet: usize,
    spoke_at_all: bool,
    wake_done: usize,
}

/// After the button, dictation ends after this much with no speech: 15 s.
const BUTTON_QUIET: usize = 240_000;
/// After a wake, dictation ends after this much silence once something has been said: 1.5 s, unless the
/// window's `voice.send_pause` asks for another length through [`Segmenter::ending_a_wake_after`].
pub const WAKE_DONE: usize = 24_000;
/// After a wake, dictation ends after this much silence when nothing has been said yet: 6 s.
const WAKE_QUIET: usize = 96_000;

impl Segmenter {
    /// A new dictation.
    /// @param started - how it started
    /// @param partial_every - samples between partial passes
    pub fn new(started: Started, partial_every: usize) -> Self {
        Segmenter {
            started,
            partial_every: partial_every.max(1_600),
            segment: 0,
            audio: Vec::new(),
            heard: false,
            silence: 0,
            since_pass: 0,
            quiet: 0,
            spoke_at_all: false,
            wake_done: WAKE_DONE,
        }
    }

    /// How long a pause ends a dictation the wake word started once something has been said, in samples.
    /// `task-2293`: *"The "Send when I stop talking" control should have a seconds configuration for how long
    /// the pause is before send is sent."* Never shorter than [`PAUSE`], so the segment holding the last words
    /// is always transcribed before the dictation ends.
    /// @param samples - the pause, in 16 kHz samples
    pub fn ending_a_wake_after(mut self, samples: usize) -> Self {
        self.wake_done = samples.max(PAUSE);
        self
    }

    /// How it started.
    pub fn started(&self) -> Started {
        self.started
    }

    /// The segment now open.
    pub fn segment(&self) -> u32 {
        self.segment
    }

    /// Whether any speech has been heard since dictation began.
    pub fn spoke_at_all(&self) -> bool {
        self.spoke_at_all
    }

    /// Feeds audio and answers the passes it calls for.
    /// @param samples - 16 kHz audio
    /// @param speech - whether the voice activity detector hears speech after these samples
    pub fn push(&mut self, samples: &[i16], speech: bool) -> Vec<Pass> {
        self.audio.extend_from_slice(samples);
        if speech {
            self.heard = true;
            self.spoke_at_all = true;
            self.silence = 0;
            self.quiet = 0;
        } else {
            self.silence += samples.len();
            self.quiet += samples.len();
        }
        if !self.heard {
            let keep = self.audio.len().saturating_sub(LEAD);
            self.audio.drain(..keep);
            return Vec::new();
        }
        self.since_pass += samples.len();
        if self.silence >= PAUSE || self.audio.len() >= LONGEST {
            return vec![self.cut()];
        }
        if self.since_pass >= self.partial_every && speech && self.audio.len() >= FIRST_PARTIAL {
            self.since_pass = 0;
            return vec![Pass { segment: self.segment, audio: self.audio.clone(), last: false }];
        }
        Vec::new()
    }

    /// The open segment's last pass, when it has speech in it. Called when dictation is stopped.
    pub fn finish(&mut self) -> Option<Pass> {
        self.heard.then(|| self.cut())
    }

    /// Whether dictation is over on its own, given whether its text so far has any words.
    /// @param has_words - whether the committed and partial text has a word in it
    pub fn should_end(&self, has_words: bool) -> Option<Ended> {
        match self.started {
            Started::Button => (self.quiet >= BUTTON_QUIET).then_some(Ended::Quiet),
            Started::Wake if has_words && self.spoke_at_all => {
                (self.quiet >= self.wake_done).then_some(Ended::Finished)
            }
            Started::Wake => (self.quiet >= WAKE_QUIET).then_some(Ended::Quiet),
            Started::File => None,
        }
    }

    /// Ends the open segment and starts the next.
    fn cut(&mut self) -> Pass {
        let pass =
            Pass { segment: self.segment, audio: std::mem::take(&mut self.audio), last: true };
        self.segment += 1;
        self.heard = false;
        self.silence = 0;
        self.since_pass = 0;
        pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `seconds` of audio in 20 ms frames with the detector saying `speech`, and collects the passes.
    fn feed(s: &mut Segmenter, seconds: f32, speech: bool) -> Vec<Pass> {
        let frames = (seconds * 50.0) as usize;
        (0..frames).flat_map(|_| s.push(&[100; 320], speech)).collect()
    }

    #[test]
    fn silence_before_speech_keeps_only_the_lead_and_asks_for_nothing() {
        let mut s = Segmenter::new(Started::Button, LOCAL_PARTIAL);
        assert!(feed(&mut s, 3.0, false).is_empty());
        assert!(feed(&mut s, 0.4, true).is_empty(), "not enough audio for a partial yet");
        let passes = feed(&mut s, 0.1, true);
        assert_eq!(passes.len(), 1, "the first partial once the segment holds 750 ms");
        assert!(!passes[0].last);
        assert!(passes[0].audio.len() <= LEAD + 8_000, "the three silent seconds were not kept");
    }

    #[test]
    fn speech_asks_for_partials_and_a_pause_asks_for_the_final_then_a_new_segment() {
        let mut s = Segmenter::new(Started::Button, LOCAL_PARTIAL);
        let partials = feed(&mut s, 2.0, true);
        assert_eq!(partials.len(), 5, "from 750 ms, every 300 ms");
        assert!(partials.iter().all(|p| !p.last && p.segment == 0));
        assert!(
            partials.windows(2).all(|w| w[1].audio.len() > w[0].audio.len()),
            "each pass is the whole segment so far"
        );
        let at_pause = feed(&mut s, 0.8, false);
        assert_eq!(at_pause.len(), 1);
        assert!(at_pause[0].last && at_pause[0].segment == 0);
        assert_eq!(s.segment(), 1);
        assert!(feed(&mut s, 0.5, true).iter().all(|p| p.segment == 1));
    }

    #[test]
    fn an_endpoint_is_asked_less_often() {
        let mut s = Segmenter::new(Started::Button, ENDPOINT_PARTIAL);
        assert_eq!(feed(&mut s, 2.4, true).len(), 2);
    }

    #[test]
    fn a_wake_ends_after_the_pause_it_was_given() {
        let mut wake = Segmenter::new(Started::Wake, LOCAL_PARTIAL).ending_a_wake_after(48_000);
        feed(&mut wake, 1.0, true);
        feed(&mut wake, 2.5, false);
        assert_eq!(wake.should_end(true), None, "two and a half seconds is shorter than three");
        feed(&mut wake, 0.6, false);
        assert_eq!(wake.should_end(true), Some(Ended::Finished));
        let short = Segmenter::new(Started::Wake, LOCAL_PARTIAL).ending_a_wake_after(10);
        assert_eq!(short.wake_done, PAUSE, "never shorter than the pause that ends a segment");
    }

    #[test]
    fn a_long_segment_is_cut_at_twenty_seconds_without_a_pause() {
        let mut s = Segmenter::new(Started::Button, LOCAL_PARTIAL);
        let passes = feed(&mut s, 21.0, true);
        let finals: Vec<&Pass> = passes.iter().filter(|p| p.last).collect();
        assert_eq!(finals.len(), 1);
        assert_eq!(finals[0].audio.len(), LONGEST);
    }

    #[test]
    fn stopping_finishes_the_open_segment_and_an_empty_one_needs_no_pass() {
        let mut s = Segmenter::new(Started::Button, LOCAL_PARTIAL);
        assert_eq!(s.finish(), None);
        feed(&mut s, 0.5, true);
        let last = s.finish().unwrap();
        assert!(last.last);
    }

    #[test]
    fn a_wake_ends_at_the_first_pause_after_words_and_the_button_waits() {
        let mut wake = Segmenter::new(Started::Wake, LOCAL_PARTIAL);
        feed(&mut wake, 1.0, true);
        feed(&mut wake, 1.0, false);
        assert_eq!(wake.should_end(false), None, "only \"Sable\" so far, so it keeps listening");
        assert_eq!(wake.should_end(true), None, "one second is a pause, not the end");
        feed(&mut wake, 0.6, false);
        assert_eq!(wake.should_end(true), Some(Ended::Finished));
        let mut button = Segmenter::new(Started::Button, LOCAL_PARTIAL);
        feed(&mut button, 1.0, true);
        feed(&mut button, 10.0, false);
        assert_eq!(button.should_end(true), None);
        feed(&mut button, 6.0, false);
        assert_eq!(button.should_end(true), Some(Ended::Quiet));
    }
}
