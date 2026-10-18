//! Speech to text for the Agent-Chat prompt, with no user interface in it.
//!
//! `tasks/task-2290-speech-to-text-tdd.md` is the design. In short: the microphone ([`capture`]) feeds a
//! worker thread ([`voice`]) that listens for "Sable" when always listen is on ([`wake`], Sable's detector)
//! and, while dictation is on, cuts speech into segments at pauses ([`segmenter`], [`vad`]) and hands each
//! one to the transcriber thread ([`transcriber`]), which runs Moonshine on this machine ([`moonshine`]) or
//! asks an OpenAI compatible endpoint ([`endpoint`]). What it hears comes back as [`voice::Event`]s, which
//! the window turns into words in the prompt. [`install`] downloads what the local engine needs and
//! [`access`] asks about the microphone.

pub mod access;
pub mod audio;
pub mod capture;
pub mod endpoint;
pub mod install;
pub mod moonshine;
pub mod phonetic;
pub mod segmenter;
pub mod transcriber;
pub mod vad;
pub mod voice;
pub mod wake;

pub use voice::{Config, Event, Snapshot, Voice};
