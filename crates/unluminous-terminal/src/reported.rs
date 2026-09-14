//! The folder a shell reports for itself, read out of the bytes it writes.
//!
//! [`crate::foreground::Master::folder`] asks the operating system where the shell process is, which is the
//! right answer for `cmd.exe`, `bash` and `zsh` and is no answer at all for PowerShell: `Set-Location` moves
//! PowerShell's own location and never the process's current directory, measured on this machine and again in
//! `task-1950`. So a `pwsh` tab came back in the folder it was started in whatever had been typed into it.
//!
//! The one mechanism that answers for a shell like that is the shell saying so, which is what every terminal
//! with shell integration reads:
//!
//! - `ESC ] 7 ; file://<host>/<path> ST` — xterm's, and what `vte.sh`, zsh and Starship already write.
//! - `ESC ] 9 ; 9 ; <path> ST` — ConEmu's, which Windows Terminal reads and which Microsoft's own PowerShell
//!   snippet writes, with the path in quotes.
//!
//! `alacritty_terminal` handles neither: `vte`'s `osc_dispatch` knows 0, 2, 4, 8, 10-19, 22, 52, 104-111 and
//! 112, and logs everything else as unhandled. There is no hook to add one. So [`Scanner`] reads the byte
//! stream on its way into the parser and keeps what it finds; see `session::Watching` for where it sits.
//!
//! **Reading costs nothing and is always on.** A shell that already reports its directory is followed and one
//! that does not is unchanged, which is why this half needs no setting. Making PowerShell report it is the
//! other half and is a setting, because it means adding to somebody's own prompt.
//!
//! Nothing here is executed and nothing is guessed: a prompt is prose, and the one thing read is a sequence
//! the shell wrote on purpose to be read.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// How many bytes of one OSC sequence are kept before it is abandoned.
///
/// A path is a path; an OSC that has run past this is something else — `OSC 52` carries a whole clipboard —
/// and the point of the bound is that a program writing megabytes between `ESC ]` and its terminator must not
/// be able to grow a buffer on the reader thread. Abandoning means the rest of that sequence is skipped, not
/// that the stream is misread: the terminator still ends it.
const LARGEST_SEQUENCE: usize = 4096;

/// The folder the shell last reported, shared between the reader thread and the window.
///
/// A handle rather than a value, because the thread that reads the pseudoterminal is not the thread that asks.
/// The lock is held for one clone of a `PathBuf` at each end, which is the shape `Session`'s own term lock has.
#[derive(Clone, Default)]
pub struct Reported {
    folder: Arc<Mutex<Option<PathBuf>>>,
}

impl Reported {
    pub fn new() -> Self {
        Self::default()
    }

    /// The folder the shell last said it was in, or `None` while it has said nothing.
    pub fn folder(&self) -> Option<PathBuf> {
        self.folder.lock().ok().and_then(|held| held.clone())
    }

    /// Record a folder the shell reported.
    fn record(&self, folder: PathBuf) {
        if let Ok(mut held) = self.folder.lock() {
            *held = Some(folder);
        }
    }
}

impl std::fmt::Debug for Reported {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Reported").field("folder", &self.folder()).finish()
    }
}

/// Where in an OSC sequence the reader is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Ordinary output, looking for `ESC`.
    Ground,
    /// `ESC` has been seen and what follows decides whether this is an OSC.
    Escape,
    /// Inside `ESC ] … `, collecting the body.
    Body,
    /// Inside the body and past this reader's interest, skipping to the terminator.
    Skipping,
    /// Inside the body, having seen an `ESC` that may be the `ESC \` that ends it.
    BodyEscape,
    /// Skipping, having seen an `ESC` that may be the `ESC \` that ends it.
    SkippingEscape,
}

/// Reads a terminal's output for the sequences a shell reports its directory with.
///
/// **A state machine rather than a search**, because the bytes arrive in whatever chunks the pseudoterminal
/// hands over and a sequence is split across two of them as often as not. It is fed every byte the emulator
/// is fed and keeps at most [`LARGEST_SEQUENCE`] of them.
#[derive(Debug)]
pub struct Scanner {
    state: State,
    body: Vec<u8>,
    reported: Reported,
    /// Whether a reported path is checked against the disk before it is kept.
    ///
    /// **On in the window and off in the tests.** The check is what stops a directory on another machine, or a
    /// sequence that was never a path at all, becoming the folder a tab reopens in — and it runs once a prompt
    /// on the reader thread rather than once a frame on the window's, which is where `task-1805` says a
    /// question about the disk belongs. A test asserts on what was *read*, and a fixed expected path that had
    /// to exist on the machine running the test would be a test about the machine.
    checks_the_disk: bool,
}

impl Scanner {
    /// A scanner that keeps only a folder that really is one on this machine.
    pub fn new(reported: Reported) -> Self {
        Self { state: State::Ground, body: Vec::new(), reported, checks_the_disk: true }
    }

    /// A scanner that keeps whatever was reported, for a test that asserts on the reading.
    pub fn reading_only(reported: Reported) -> Self {
        Self { checks_the_disk: false, ..Self::new(reported) }
    }

    /// Read `bytes` on their way to the emulator, keeping any folder they report.
    pub fn read(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.byte(byte);
        }
    }

    fn byte(&mut self, byte: u8) {
        const ESC: u8 = 0x1b;
        const BEL: u8 = 0x07;
        const CAN: u8 = 0x18;
        const SUB: u8 = 0x1a;
        match self.state {
            State::Ground => {
                if byte == ESC {
                    self.state = State::Escape;
                }
            }
            State::Escape => {
                self.body.clear();
                self.state = match byte {
                    b']' => State::Body,
                    ESC => State::Escape,
                    _ => State::Ground,
                };
            }
            State::Body => match byte {
                BEL => self.finish(),
                ESC => self.state = State::BodyEscape,
                // `CAN` and `SUB` abandon a control sequence wherever it has got to, which is what the
                // emulator behind this does with them as well.
                CAN | SUB => self.state = State::Ground,
                _ => {
                    self.body.push(byte);
                    // Four bytes is enough to tell `9;9;` from `9;12` and `7;` from `71;`, and after them
                    // there is nothing to be gained by keeping a clipboard in memory.
                    if self.body.len() >= 4 && !could_be_a_folder(&self.body) {
                        self.body.clear();
                        self.state = State::Skipping;
                    } else if self.body.len() > LARGEST_SEQUENCE {
                        self.body.clear();
                        self.state = State::Skipping;
                    }
                }
            },
            State::Skipping => match byte {
                BEL => self.state = State::Ground,
                ESC => self.state = State::SkippingEscape,
                CAN | SUB => self.state = State::Ground,
                _ => {}
            },
            State::BodyEscape => match byte {
                b'\\' => self.finish(),
                // `ESC ]` inside a body is a sequence nobody terminated followed by another one.
                b']' => {
                    self.body.clear();
                    self.state = State::Body;
                }
                _ => self.state = State::Ground,
            },
            State::SkippingEscape => match byte {
                b']' => {
                    self.body.clear();
                    self.state = State::Body;
                }
                _ => self.state = State::Ground,
            },
        }
    }

    fn finish(&mut self) {
        let body = std::mem::take(&mut self.body);
        self.state = State::Ground;
        let Some(folder) = folder_in(&body) else {
            return;
        };
        if self.checks_the_disk && !folder.is_dir() {
            return;
        }
        self.reported.record(folder);
    }
}

/// Whether an OSC body this far in could still be one of the two this reads.
fn could_be_a_folder(body: &[u8]) -> bool {
    body.starts_with(b"7;") || body.starts_with(b"9;9;")
}

/// The folder an OSC body names, or `None` when it names none.
///
/// Public so a test can hold a sequence and its answer side by side, which is the whole of what this module
/// decides.
pub fn folder_in(body: &[u8]) -> Option<PathBuf> {
    if let Some(rest) = body.strip_prefix(b"7;") {
        return from_a_file_url(std::str::from_utf8(rest).ok()?);
    }
    if let Some(rest) = body.strip_prefix(b"9;9;") {
        return from_a_plain_path(std::str::from_utf8(rest).ok()?);
    }
    None
}

/// The path in `OSC 9;9`, which is written plainly and, in Microsoft's own snippet, in quotes.
fn from_a_plain_path(text: &str) -> Option<PathBuf> {
    let text = text.trim().trim_matches('"');
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// The path in `OSC 7`, which is a `file://` URL.
///
/// **A URL rather than a path**, so the folder is percent encoded and carries the machine it is on. A folder on
/// another machine is not a folder this window can open, so a host that is not this one is refused rather than
/// being read as a local path that happens to exist — `file://build-box/c:/jason` would otherwise reopen a tab
/// in somebody else's `C:\jason`.
///
/// A body with no scheme in it is read as a plain path. That is tolerant rather than correct: the sequence is
/// specified as a URL, and a shell that writes a bare path meant the path.
fn from_a_file_url(text: &str) -> Option<PathBuf> {
    let text = text.trim();
    let Some(rest) = text.strip_prefix("file://") else {
        return match text.contains("://") {
            true => None,
            false => from_a_plain_path(text),
        };
    };
    let (host, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, ""),
    };
    if !host_is(host, machine_name().as_deref()) {
        return None;
    }
    let path = percent_decoded(path);
    let path = strip_a_windows_drive_slash(&path);
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// `/C:/jason` is how a Windows path is spelt in a `file:` URL, and `C:\jason` is how it is opened.
///
/// Left alone everywhere else, where a leading slash is the root and means what it says.
fn strip_a_windows_drive_slash(path: &str) -> &str {
    let bytes = path.as_bytes();
    let looks_like_a_drive = bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && (bytes[2] == b':' || bytes[2] == b'|');
    match looks_like_a_drive {
        true => &path[1..],
        false => path,
    }
}

/// Turn `%20` back into a space, and every other escape back into its byte.
///
/// A sequence that is not valid UTF-8 once it is decoded is left exactly as it was written, because a path
/// that cannot be read is worse than one that was never decoded.
fn percent_decoded(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }
    let source = text.as_bytes();
    let mut out = Vec::with_capacity(source.len());
    let mut at = 0;
    while at < source.len() {
        if source[at] == b'%' && at + 2 < source.len() {
            let high = (source[at + 1] as char).to_digit(16);
            let low = (source[at + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                at += 3;
                continue;
            }
        }
        out.push(source[at]);
        at += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// Whether a `file:` URL's host names the machine this window is running on.
///
/// Empty and `localhost` are what a shell writes when it has nothing to say about the host, and the machine's
/// own name is what `vte.sh` and Starship write — so both have to be accepted and a third machine's name has
/// not. The first label only, because a shell writes `$HOSTNAME` and that is sometimes the whole domain name.
///
/// **A machine that will not say its own name takes what it is told**, and what stops a wrong folder there is
/// the check against the disk in [`Scanner::checks_the_disk`]. Refusing instead would silently stop following
/// a shell that is reporting correctly, which is the worse of the two ways to be wrong.
fn host_is(host: &str, machine: Option<&str>) -> bool {
    if host.is_empty() || host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
        return true;
    }
    let Some(machine) = machine else {
        return true;
    };
    first_label(host).eq_ignore_ascii_case(first_label(machine))
}

/// The part of a name in front of the first dot, which is what two names have in common when one of them is
/// the whole domain name and the other is not.
fn first_label(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

/// What this machine calls itself, when it will say.
///
/// **Asked of the operating system rather than of the environment**, because the environment does not
/// reliably answer: `COMPUTERNAME` is set for a Windows process started from the desktop and is *empty* under
/// Git Bash, and `HOSTNAME` is a shell variable that `bash` does not export — so a reader that trusted either
/// would say this machine has no name on a machine that has one, and then follow a folder on another.
fn machine_name() -> Option<String> {
    if let Some(name) = from_the_operating_system() {
        return Some(name);
    }
    for variable in ["COMPUTERNAME", "HOSTNAME"] {
        if let Ok(name) = std::env::var(variable) {
            if !name.trim().is_empty() {
                return Some(name.trim().to_owned());
            }
        }
    }
    for file in ["/etc/hostname", "/proc/sys/kernel/hostname"] {
        if let Ok(text) = std::fs::read_to_string(Path::new(file)) {
            let name = text.lines().next().unwrap_or_default().trim().to_owned();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

#[cfg(windows)]
fn from_the_operating_system() -> Option<String> {
    use windows_sys::Win32::System::SystemInformation::{
        ComputerNameDnsHostname, GetComputerNameExW,
    };
    // Asked twice: once for the length and once for the name. The first call is expected to fail, which is
    // how this function is documented to report the size it wants.
    let mut length: u32 = 0;
    unsafe { GetComputerNameExW(ComputerNameDnsHostname, std::ptr::null_mut(), &mut length) };
    if length == 0 {
        return None;
    }
    let mut buffer = vec![0u16; length as usize];
    let wrote =
        unsafe { GetComputerNameExW(ComputerNameDnsHostname, buffer.as_mut_ptr(), &mut length) };
    if wrote == 0 {
        return None;
    }
    let name = String::from_utf16_lossy(&buffer[..length as usize]);
    (!name.trim().is_empty()).then(|| name.trim().to_owned())
}

#[cfg(unix)]
fn from_the_operating_system() -> Option<String> {
    // `HOST_NAME_MAX` is 64 on Linux and 255 on macOS; this is larger than either, and the answer ends at the
    // first zero rather than at the end of the buffer.
    let mut buffer = vec![0 as libc::c_char; 512];
    if unsafe { libc::gethostname(buffer.as_mut_ptr(), buffer.len() - 1) } != 0 {
        return None;
    }
    let bytes: Vec<u8> =
        buffer.iter().take_while(|byte| **byte != 0).map(|byte| *byte as u8).collect();
    let name = String::from_utf8(bytes).ok()?;
    (!name.trim().is_empty()).then(|| name.trim().to_owned())
}

#[cfg(not(any(windows, unix)))]
fn from_the_operating_system() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read `bytes` and answer with whatever folder they reported, without asking the disk about it.
    fn read(bytes: &[u8]) -> Option<PathBuf> {
        let reported = Reported::new();
        let mut scanner = Scanner::reading_only(reported.clone());
        scanner.read(bytes);
        reported.folder()
    }

    #[test]
    fn osc_nine_nine_reports_a_plain_path() {
        assert_eq!(
            read(b"\x1b]9;9;C:\\jason\\dev\\unluminous\x1b\\"),
            Some(PathBuf::from("C:\\jason\\dev\\unluminous"))
        );
    }

    #[test]
    fn osc_nine_nine_reports_a_path_in_quotes_which_is_how_microsoft_writes_it() {
        // The snippet Microsoft publishes for Windows Terminal writes the path in quotes, and those are
        // punctuation round the value rather than part of it.
        assert_eq!(
            read(b"\x1b]9;9;\"C:\\jason\\dev\"\x1b\\"),
            Some(PathBuf::from("C:\\jason\\dev"))
        );
    }

    #[test]
    fn a_bell_ends_a_sequence_as_well_as_a_string_terminator() {
        assert_eq!(read(b"\x1b]9;9;/home/jason\x07"), Some(PathBuf::from("/home/jason")));
    }

    #[test]
    fn osc_seven_reports_a_file_url() {
        assert_eq!(read(b"\x1b]7;file://localhost/home/jason\x1b\\"), Some(PathBuf::from("/home/jason")));
    }

    #[test]
    fn a_file_url_with_no_host_at_all_is_this_machine() {
        assert_eq!(read(b"\x1b]7;file:///home/jason\x1b\\"), Some(PathBuf::from("/home/jason")));
    }

    #[test]
    fn a_windows_drive_loses_the_slash_the_url_put_in_front_of_it() {
        assert_eq!(
            read(b"\x1b]7;file:///C:/jason/dev\x1b\\"),
            Some(PathBuf::from("C:/jason/dev"))
        );
    }

    #[test]
    fn a_percent_escape_comes_back_as_the_character_it_stands_for() {
        assert_eq!(
            read(b"\x1b]7;file:///home/jason/two%20words\x1b\\"),
            Some(PathBuf::from("/home/jason/two words"))
        );
    }

    #[test]
    fn a_folder_on_another_machine_is_not_a_folder_this_window_can_open() {
        // The path very often exists on this machine too, which is exactly why the host has to be read:
        // `file://build-box/C:/jason` would otherwise reopen the tab in this machine's own `C:\jason`.
        assert!(!host_is("some-other-box", Some("this-box")));
        assert!(host_is("this-box", Some("this-box")));
        // `$HOSTNAME` is sometimes the whole domain name and sometimes not, and on either side of this.
        assert!(host_is("this-box.example.com", Some("this-box")));
        assert!(host_is("this-box", Some("this-box.example.com")));
        // What a shell writes when it has nothing to say about the host.
        assert!(host_is("", Some("this-box")));
        assert!(host_is("localhost", Some("this-box")));
        assert!(host_is("127.0.0.1", Some("this-box")));
        // And a machine that will not say its own name takes what it is told, because the folder still has to
        // be a folder on this machine before it is kept.
        assert!(host_is("some-other-box", None));
    }

    #[test]
    fn this_machine_says_what_it_is_called() {
        // The whole of the rule above rests on it: a reader that could not name this machine would follow a
        // folder on any machine at all. Measured rather than assumed, because the two environment variables
        // that look like the answer are empty and unexported under Git Bash, which is what a `cargo test` here
        // runs in.
        let name = machine_name().expect("this machine has a name");
        assert!(!name.trim().is_empty());
        let reported = read(format!("\x1b]7;file://{name}/tmp/x\x1b\\").as_bytes());
        assert_eq!(reported, Some(PathBuf::from("/tmp/x")), "its own name was not recognised");
        assert_eq!(read(b"\x1b]7;file://some-other-box/tmp/x\x1b\\"), None);
    }

    #[test]
    fn a_sequence_split_across_two_reads_is_still_one_sequence() {
        // Which is the ordinary case rather than a corner: a pseudoterminal hands over whatever it has.
        let reported = Reported::new();
        let mut scanner = Scanner::reading_only(reported.clone());
        scanner.read(b"\x1b]9;9;C:\\jas");
        assert_eq!(reported.folder(), None, "nothing is reported until it is terminated");
        scanner.read(b"on\x1b\\");
        assert_eq!(reported.folder(), Some(PathBuf::from("C:\\jason")));
    }

    #[test]
    fn every_split_of_one_sequence_reads_the_same() {
        // `unluminous_chat::sse` is fed the same stream split at every byte boundary for this reason, and a
        // reader that keeps state across chunks earns the same test.
        let stream = b"ordinary output\r\n\x1b]0;a title\x07\x1b]9;9;C:\\jason\\dev\x1b\\PS> ";
        for at in 0..=stream.len() {
            let reported = Reported::new();
            let mut scanner = Scanner::reading_only(reported.clone());
            scanner.read(&stream[..at]);
            scanner.read(&stream[at..]);
            assert_eq!(
                reported.folder(),
                Some(PathBuf::from("C:\\jason\\dev")),
                "split at {at} read something else"
            );
        }
    }

    #[test]
    fn the_title_and_the_clipboard_are_not_folders() {
        assert_eq!(read(b"\x1b]0;claude\x07"), None);
        assert_eq!(read(b"\x1b]2;a window title\x07"), None);
        assert_eq!(read(b"\x1b]52;c;aGVsbG8=\x07"), None);
        // `OSC 9;12` is the same family and says something else entirely.
        assert_eq!(read(b"\x1b]9;12\x07"), None);
        // And a number that merely starts with a seven is not `OSC 7`.
        assert_eq!(read(b"\x1b]71;file:///home/jason\x07"), None);
    }

    #[test]
    fn a_sequence_nobody_terminated_does_not_swallow_the_one_after_it() {
        assert_eq!(
            read(b"\x1b]52;c;never-ended\x1b]9;9;/home/jason\x07"),
            Some(PathBuf::from("/home/jason"))
        );
    }

    #[test]
    fn a_clipboard_larger_than_the_bound_costs_the_bound_and_not_the_clipboard() {
        let mut stream = b"\x1b]52;c;".to_vec();
        stream.resize(stream.len() + LARGEST_SEQUENCE * 4, b'A');
        stream.extend(b"\x07\x1b]9;9;/home/jason\x07");
        let reported = Reported::new();
        let mut scanner = Scanner::reading_only(reported.clone());
        scanner.read(&stream);
        assert!(scanner.body.capacity() <= LARGEST_SEQUENCE + 8, "the body grew with the clipboard");
        assert_eq!(reported.folder(), Some(PathBuf::from("/home/jason")));
    }

    #[test]
    fn a_path_that_is_not_a_folder_on_this_machine_is_not_kept() {
        let reported = Reported::new();
        let mut scanner = Scanner::new(reported.clone());
        scanner.read(b"\x1b]9;9;C:\\this\\does\\not\\exist\\anywhere\x1b\\");
        assert_eq!(reported.folder(), None);
        let here = std::env::temp_dir();
        scanner.read(format!("\x1b]9;9;{}\x1b\\", here.display()).as_bytes());
        assert_eq!(reported.folder(), Some(here));
    }

    #[test]
    fn the_newest_report_is_the_one_that_is_kept() {
        let reported = Reported::new();
        let mut scanner = Scanner::reading_only(reported.clone());
        scanner.read(b"\x1b]9;9;/one\x07\x1b]9;9;/two\x07");
        assert_eq!(reported.folder(), Some(PathBuf::from("/two")));
    }

    #[test]
    fn a_stream_with_nothing_in_it_reports_nothing() {
        assert_eq!(read(b"PS C:\\jason\\dev\\unluminous> git status\r\n"), None);
    }
}
