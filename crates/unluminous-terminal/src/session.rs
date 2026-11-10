//! One terminal: a program in a pseudoterminal, and the emulator that reads what it writes.
//!
//! This is the one file that knows Termoil exists (`task-2489`). Everything above it works on [`Screen`], so a
//! change to Termoil's interface is a change to this file, [`crate::keys`], [`crate::mouse`] and
//! [`crate::palette`], and nothing else. `tasks/task-2489-termoil-tdd.md` is the design.
//!
//! How the pieces fit, and where each one runs:
//!
//! - `termoil_pty::Pty` opens the pseudoterminal and starts the program, in a job object on Windows and in a
//!   process group of its own on Unix, so killing the session ends everything the program started.
//! - `termoil_vt::Terminal` holds the screens, the scrollback, the modes and the palette. It is a plain value
//!   with no thread of its own, and it lives behind a `Mutex` because two threads reach it.
//! - The **reader thread** reads the pseudoterminal, passes every byte through [`crate::reported::Scanner`],
//!   applies the bytes to the terminal in slices, hands the terminal's replies to the writer, queues its events
//!   and wakes the window.
//! - The **writer thread** writes what the window sends, so a program with a full input pipe never stops the
//!   window drawing.
//! - The **exit watcher** asks the pseudoterminal whether the program has ended, and records how.
//! - Anything the window has to act on, such as a title, the bell or a program writing to the clipboard, is
//!   dealt with in [`Session::pump`] on the window's thread.
//!
//! A session can also be built with no pseudoterminal at all, with [`Session::detached`], and fed bytes
//! directly with [`Session::feed`]. That is what makes the screen testable: a test writes
//! `ESC [ 31 m hello` and asserts that the first five cells are red, with no shell, no thread and no
//! waiting.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use termoil_pty::{ExitStatus, Pty, PtySize, SpawnOptions};
use termoil_vt::cell::Wide;
use termoil_vt::modes::Mode as VtMode;
use termoil_vt::page::Page;
use termoil_vt::pagelist::{PageList, ScrollbackLimit};
use termoil_vt::selection::{Selection, SelectionMode, DEFAULT_WORD_SEPARATORS};
use termoil_vt::style::{attr, Color, Rgb as VtRgb, Style};
use termoil_vt::terminal::{CursorShape as VtCursorShape, DynamicColors, Event};
use termoil_vt::{Terminal, TerminalConfig};

use crate::keys::Mode;

/// How often the operating system is asked where a shell process is.
///
/// **`task-1984` A4.** `Session::folder`'s fallback is `OpenProcess`, `NtQueryInformationProcess`,
/// two `ReadProcessMemory` calls and a `stat` on Windows, and `/proc/<pid>/cwd` on Linux -- and
/// `remember_the_project` asks it for every terminal tab on every frame. Half a second is
/// `app::HEARTBEAT`, which is how often an idle window draws at all, so nothing a person can see is
/// slower to notice and the ordinary frame asks nothing.
pub const FOLDER_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
use crate::mouse::MouseMode;
use crate::palette::{Palette, Rgb};
use crate::screen::{
    Cursor, CursorShape, ImagePixels, Screen, ScreenCell, ScreenImage, UnderlineStyle,
};

/// The serial the next session is given. See [`Session::serial`].
static NEXT_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A serial no session has had before.
fn next_serial() -> u64 {
    NEXT_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// How many lines of output are kept above the top of the screen.
pub const SCROLLBACK: usize = 10_000;

/// How many bytes the reader thread reads at once, and the most it applies to the terminal under one lock.
///
/// Termoil's own figure (`termoil-app`'s `SessionOptions::shell`): large enough that a flood of output is a few
/// locks a frame, small enough that the window never waits long for one.
const READ_CHUNK: usize = 64 << 10;

/// How often the exit watcher asks whether the program has ended.
const EXIT_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// How long after the program has ended its end is reported.
///
/// **What a program wrote just before it ended has to reach the screen first.** On Windows the console host
/// writes to the pipe on its own frame, and on a busy machine a program's only line arrived after the program had
/// gone (`task-2353`). The reader goes on reading whatever the watcher says, so a line that is later still reaches
/// the screen anyway; this is only how long the tab waits before it says the program stopped.
const EXIT_GRACE: std::time::Duration = std::time::Duration::from_millis(100);

/// How wide and tall the grid is, in cells and in points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub rows: usize,
    pub columns: usize,
    /// The width of one cell in whole points, which the shell needs so that a program drawing a picture
    /// knows how large the area is.
    pub cell_width: u16,
    pub cell_height: u16,
}

impl Size {
    pub fn new(rows: usize, columns: usize) -> Self {
        Self { rows: rows.max(1), columns: columns.max(1), cell_width: 8, cell_height: 16 }
    }

    pub fn with_cell(mut self, width: f32, height: f32) -> Self {
        self.cell_width = width.round().max(1.0) as u16;
        self.cell_height = height.round().max(1.0) as u16;
        self
    }

    /// The columns as Termoil counts them, which is a `u16`.
    fn cols(&self) -> u16 {
        self.columns.min(u16::MAX as usize) as u16
    }

    /// The rows as Termoil counts them.
    fn rows_u16(&self) -> u16 {
        self.rows.min(u16::MAX as usize) as u16
    }

    /// What the pseudoterminal is told: the cells and the area they cover in pixels.
    fn pty(&self) -> PtySize {
        PtySize {
            cols: self.cols(),
            rows: self.rows_u16(),
            pixel_width: self.cols().saturating_mul(self.cell_width),
            pixel_height: self.rows_u16().saturating_mul(self.cell_height),
        }
    }
}

/// What to run, and where.
#[derive(Clone, Default)]
pub struct SessionSettings {
    /// The program to run. `None` uses the shell the operating system says the user has.
    pub shell: Option<String>,
    pub args: Vec<String>,
    /// The folder the program starts in, which is the project the window has open.
    pub working_directory: Option<std::path::PathBuf>,
    /// Variables laid **over** the environment Unluminous itself started with, never replacing it.
    ///
    /// `task-1683` asks for it: a run configuration carries `NAME=value` pairs, and a dev server
    /// started with `PORT=3000` has to see the rest of the machine's environment as well — a child
    /// with no `PATH` is a bug nobody enjoys finding. The terminal's own shells leave it empty and
    /// change by nothing.
    pub env: Vec<(String, String)>,
    /// What to call this terminal, when the program that is started is not the program that matters.
    ///
    /// **What a restored node needs.** A node coming back showing what was on it does not start its shell: it
    /// starts `unluminous-cli --replay-screen …`, which prints the remembered screen and then becomes the
    /// shell — `unluminous_cli::restore` says why it has to be a program inside the console that prints it.
    /// The tab is still the shell's, so the name is said rather than derived, and every other caller leaves
    /// this empty and gets the name of the program it asked for.
    pub name: Option<String>,
}

/// **Written by hand so that `env` prints its names and not its values.**
///
/// What goes in `env` is sometimes a secret: a run configuration can carry a token, and Agent-Tasks puts the
/// authentication key it read out of the keychain there. A derived `Debug` would put every one of those values
/// into whatever printed it — a log line, a panic message, a `dbg!` somebody left in. Nothing prints
/// `SessionSettings` today, and this is what keeps that true whatever anybody adds later.
impl std::fmt::Debug for SessionSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionSettings")
            .field("shell", &self.shell)
            .field("args", &self.args)
            .field("working_directory", &self.working_directory)
            // The names, so a reader can tell what was set, and the count, so a reader can tell how much.
            .field("env", &self.env.iter().map(|(name, _)| name.as_str()).collect::<Vec<&str>>())
            .field("name", &self.name)
            .finish()
    }
}

/// Told when something happened that the window has to redraw for.
///
/// This is how the terminal wakes the window without knowing what the window is: the caller hands over a
/// function, and the caller's function is the one that knows about egui.
pub type Waker = Arc<dyn Fn() + Send + Sync + 'static>;

/// What the threads behind a session share with the window's thread.
struct Shared {
    /// The terminal's events, queued by the reader for [`Session::pump`].
    events: Mutex<Vec<Event>>,
    /// Set once the program has ended and its end has been recorded.
    exited: AtomicBool,
    /// How it ended, when that could be read.
    exit: Mutex<Option<ExitStatus>>,
}

/// Locks a mutex, carrying on with the data if a thread panicked while it held it.
///
/// A panic on the reader thread must not take every later frame of the window down with it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One terminal.
pub struct Session {
    term: Arc<Mutex<Terminal>>,
    shared: Arc<Shared>,
    /// The pseudoterminal and its program. Absent in a detached session, and once the program was killed.
    pty: Option<Arc<Mutex<Option<Pty>>>>,
    /// Hands bytes to the writer thread. Absent in a detached session, which has no program.
    writer: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    /// What was sent to a session with **no shell behind it**, kept so a test can read it back.
    ///
    /// A detached session is what every screenshot test of a terminal uses, and until `task-1914` a key
    /// press reaching the program was the one thing about one that could not be checked: `send` had
    /// nowhere to put the bytes and dropped them. Four of that ticket's reports were "I cannot type in
    /// X", so what the grid really encoded and really sent is exactly what a test has to be able to ask.
    ///
    /// `None` for a session with a program, where the bytes go to the program and holding a second copy
    /// of everything typed would be a transcript nobody asked for.
    sent_while_detached: Option<Mutex<Vec<u8>>>,
    /// The pseudoterminal, kept only to be asked what program is in the foreground of it.
    ///
    /// See [`crate::foreground`]: a node's recorded command is what it was *given*, and what is running in it
    /// is a different question that only the terminal can answer.
    master: crate::foreground::Master,
    size: Size,
    /// The palette this session's emulator was last given. A `Cell`, because drawing is `&self` and is
    /// where a theme change is noticed: see [`Session::follow_the_palette`].
    palette: std::cell::Cell<Palette>,
    /// The serial of the row at the top of the view while it is scrolled back, and `None` at the bottom.
    ///
    /// **A serial rather than a count of rows**, so output that arrives while somebody reads the history does
    /// not move what they are reading: the rows under the view are the same rows, and only how far they are
    /// from the bottom has changed. `termoil_vt` leaves the view to its host. See [`Session::display_offset`].
    scroll_anchor: Option<u64>,
    /// The selection, and whether it was made on the alternate screen.
    ///
    /// Its two ends are pins in one screen's page list, so it means nothing on the other screen.
    selection: Option<Picked>,
    /// The title the program set, which is what the tab is named after.
    title: String,
    /// What the program asked to be put on the clipboard, for the window to hand over.
    clipboard: Option<String>,
    /// False once the program has stopped.
    running: bool,
    /// What the program exited with, once it has. `None` while it is still running, and `None`
    /// after [`Session::kill`], because a program Unluminous killed did not choose a code.
    exit_code: Option<i32>,
    /// How many times the program rang the bell, which a test can check.
    bells: usize,
    /// The name shown on the tab before the program sets a title of its own.
    name: String,
    /// The program that was really started, which is not always the one the tab is named after.
    ///
    /// **What a restored node needs.** It starts `unluminous-cli --replay-screen …`, which prints the screen
    /// it was left showing and then becomes the shell — and a pseudoconsole sets its title to the program it
    /// was created with, so the node came back with `\\…\\unluminous-cli.exe` written across its header.
    /// That title says no more than the name of a program, which is the case `title_is_only_the_program`
    /// already exists for; it could not see it, because the name it was asked about was the shell's.
    started: String,
    /// Where the shell has said it is, when it says so at all.
    ///
    /// **The half of "where the shell is" the operating system cannot answer**, which is PowerShell: measured
    /// again on `task-1950`, `Set-Location C:\jason\dev\ai-service` leaves the process's own current
    /// directory where it was, and running a native command after it does not move it either. See
    /// [`crate::reported`] for the sequences this is filled in from; the reader thread reads them.
    reported: crate::reported::Reported,
    /// When the process was last asked where it is, and what it said.
    ///
    /// **`task-1984` A4.** See [`Session::folder`]: the fallback is kernel round trips and a `stat`,
    /// and the caller asks once a frame for every tab. A `RefCell` because `folder` takes `&self` and
    /// is called from a draw -- the value is a cache rather than state, and making the accessor
    /// mutable would push `&mut` through `project_state` and the status bar for nothing.
    process_folder_asked:
        std::cell::RefCell<Option<(std::time::Instant, Option<std::path::PathBuf>)>>,
    /// The name a person typed for this tab, which beats both of the two above.
    ///
    /// Empty until somebody renames the tab. A name that was asked for by hand is the one thing in
    /// the tab's name a program must not be able to take away again, so it is held apart from
    /// `title` rather than written into it: `claude` sets a title on every prompt, and a rename
    /// written into `title` would last until the next one.
    given: String,
    /// The pixels of every picture the program has sent, shared with the snapshots, and the generation each
    /// was given. See [`Session::images_on_screen`].
    images: std::cell::RefCell<ImageTracker>,
    /// A number no other session in this process has, which is how the window keeps what it holds about a
    /// terminal (its split panes, `task-2597`) against the terminal it belongs to.
    serial: u64,
    /// Held for as long as the session lives. [`Session::liveness`] hands out a weak copy, so whatever keeps
    /// something for this session can tell that it has gone without being told.
    alive: Arc<()>,
}

/// Remembers the pixels handed to the painter for each picture, so an unchanged picture keeps its generation and
/// the painter uploads it once.
#[derive(Default)]
struct ImageTracker {
    seen: std::collections::HashMap<u32, Arc<ImagePixels>>,
    next_generation: u64,
}

impl ImageTracker {
    /// The shared pixels for a picture in the emulator's store, reusing the last copy while they are the same.
    fn pixels_of(&mut self, image: &termoil_vt::graphics::Image) -> Arc<ImagePixels> {
        let rgba = image.current_rgba();
        if let Some(old) = self.seen.get(&image.id) {
            if Arc::ptr_eq(&old.rgba, rgba)
                && old.width == image.width
                && old.height == image.height
            {
                return Arc::clone(old);
            }
        }
        self.next_generation += 1;
        let pixels = Arc::new(ImagePixels {
            id: image.id,
            generation: self.next_generation,
            width: image.width,
            height: image.height,
            rgba: Arc::clone(rgba),
        });
        self.seen.insert(image.id, Arc::clone(&pixels));
        pixels
    }
}

/// The `OSC 8` links met while copying rows, numbered in the order they were met.
#[derive(Default)]
struct Links {
    /// The emulator's link id, and the index it was given in [`Screen::links`].
    numbered: std::collections::HashMap<u32, u32>,
    addresses: Vec<String>,
}

impl Links {
    /// The index of the emulator's link `id` in this screen's list, adding it the first time it is met.
    fn index_of(&mut self, term: &Terminal, id: u32) -> Option<u32> {
        if let Some(index) = self.numbered.get(&id) {
            return Some(*index);
        }
        let address = term.hyperlink(id)?.uri.clone();
        let index = self.addresses.len() as u32;
        self.addresses.push(address);
        self.numbered.insert(id, index);
        Some(index)
    }
}

/// A selection, with the screen it was made on.
struct Picked {
    selection: Selection,
    /// Made on the alternate screen.
    alternate: bool,
    /// A plain selection that has only been pressed, not dragged, selects nothing: a click is not a selection.
    extended: bool,
}

impl Session {
    /// Start a program in a pseudoterminal.
    pub fn spawn(settings: &SessionSettings, size: Size, waker: Waker) -> std::io::Result<Self> {
        let shell = settings.shell.clone().unwrap_or_else(default_shell);
        // **The name is what the caller says it is, whatever is really started.** A node restoring its screen
        // starts `unluminous-cli --replay-screen …`, which prints the screen and then becomes the shell, and a
        // tab named after the program that was spawned would be a tab named `unluminous-cli`. See
        // `SessionSettings::name`.
        let started = program_name(&shell);
        let name = settings.name.clone().unwrap_or_else(|| started.clone());
        let options = SpawnOptions {
            program: shell,
            args: settings.args.clone(),
            // Through `paths::plain`, because a verbatim Windows path is a path `cmd.exe` will not
            // start in: it says so and starts in `C:\Windows` instead, which is a terminal that opens,
            // works, and is quietly in the wrong folder. That module says where such a path comes from.
            cwd: settings.working_directory.as_deref().map(crate::paths::plain),
            env: environment(&settings.env),
            size: size.pty(),
            // A login shell is what a terminal application opens by itself. Every caller here names the program
            // it wants, and a run configuration's `node server.js` is not a login.
            login: false,
        };
        let mut pty = Pty::spawn_with_config(&options, &conpty())?;
        let reader = pty.take_reader().ok_or_else(|| std::io::Error::other("no pty reader"))?;
        let output = pty.take_writer().ok_or_else(|| std::io::Error::other("no pty writer"))?;
        // **The pseudoterminal itself, so what is running in it can be asked for later.** What it is for is
        // `Session::foreground`: a node on the canvas records the *command it was given*, and a person adds a
        // plain terminal node and then types `claude` into the shell, so a canvas that knew only the command
        // came back as a shell whatever had been running in it. `task-1907`.
        let master = crate::foreground::Master::duplicating(&pty);

        let palette = Palette::current();
        let term = Arc::new(Mutex::new(new_terminal(size, &palette)));
        let shared = Arc::new(Shared {
            events: Mutex::new(Vec::new()),
            exited: AtomicBool::new(false),
            exit: Mutex::new(None),
        });
        let writer = spawn_writer(output)?;
        // **Every byte the emulator is given is read on its way there**, for the one thing the emulator does
        // not answer: which folder the shell says it is in. See [`crate::reported`]. It is always on, because a
        // shell that reports its folder is followed and one that does not is unchanged.
        let reported = crate::reported::Reported::new();
        let scanner = crate::reported::Scanner::new(reported.clone());
        let pty = Arc::new(Mutex::new(Some(pty)));
        spawn_reader(reader, scanner, term.clone(), writer.clone(), shared.clone(), waker.clone())?;
        spawn_exit_watcher(pty.clone(), shared.clone(), waker)?;

        Ok(Self {
            term,
            shared,
            pty: Some(pty),
            writer: Some(writer),
            sent_while_detached: None,
            master,
            size,
            palette: std::cell::Cell::new(palette),
            scroll_anchor: None,
            selection: None,
            title: String::new(),
            clipboard: None,
            running: true,
            exit_code: None,
            bells: 0,
            name,
            started,
            reported,
            process_folder_asked: std::cell::RefCell::new(None),
            given: String::new(),
            images: std::cell::RefCell::new(ImageTracker::default()),
            serial: next_serial(),
            alive: Arc::new(()),
        })
    }

    /// A terminal with no shell behind it, fed by [`Session::feed`].
    ///
    /// This is what the tests and the screenshot tests use. It runs the same emulator over the same bytes,
    /// so what it draws is what a real shell writing those bytes would draw, and it is the same on every
    /// run because nothing is waited for.
    pub fn detached(size: Size) -> Self {
        let palette = Palette::current();
        Self {
            term: Arc::new(Mutex::new(new_terminal(size, &palette))),
            shared: Arc::new(Shared {
                events: Mutex::new(Vec::new()),
                exited: AtomicBool::new(false),
                exit: Mutex::new(None),
            }),
            pty: None,
            writer: None,
            sent_while_detached: Some(Mutex::new(Vec::new())),
            master: crate::foreground::Master::detached(),
            size,
            palette: std::cell::Cell::new(palette),
            scroll_anchor: None,
            selection: None,
            title: String::new(),
            clipboard: None,
            running: true,
            exit_code: None,
            bells: 0,
            name: "detached".to_owned(),
            started: String::new(),
            // A detached session has no shell to report anything, and `feed` is the test's own way in.
            reported: crate::reported::Reported::new(),
            process_folder_asked: std::cell::RefCell::new(None),
            given: String::new(),
            images: std::cell::RefCell::new(ImageTracker::default()),
            serial: next_serial(),
            alive: Arc::new(()),
        }
    }

    /// Run `bytes` through the emulator as if the shell had written them. Only a detached session.
    pub fn feed(&mut self, bytes: &[u8]) {
        if self.pty.is_some() {
            return;
        }
        self.apply_here(bytes);
    }

    /// Apply bytes to the terminal on this thread, and deal with what they asked for at once.
    ///
    /// The replies a detached session makes go to [`Self::send`], which keeps them, as the replies of a session
    /// with a program go to the program.
    fn apply_here(&mut self, bytes: &[u8]) {
        let (replies, events) = {
            let mut term = lock(&self.term);
            term.advance(bytes);
            (term.take_replies(), term.take_events())
        };
        self.send(replies);
        lock(&self.shared.events).extend(events);
        self.pump();
    }

    /// The bytes that would draw what is on this terminal, for [`Self::replay`] to put back later.
    ///
    /// **Nothing while the program is drawing its own screen**, and `task-1908` §1.2 is why: the alternate
    /// buffer is cleared when it is entered and abandoned when it is left, so a full-screen program's display is
    /// not a thing that can be saved even in principle. What is written down in that case is `None` rather than
    /// the shell's buffer behind it — a screen belonging to a program that is no longer running would come back
    /// as a frozen frame of its interface, which is worse than an empty terminal because it looks alive. The
    /// node says what was running and offers to start it instead, which is `task-1907`'s own control.
    pub fn screen_to_replay(&self) -> Option<Vec<u8>> {
        if self.on_alternate_screen() {
            return None;
        }
        // **Bounded by dropping rows, never by cutting bytes.** A byte cut lands inside an escape sequence
        // sooner or later — the stream is mostly `\x1b[0;38;2;232;235;241;48;2;26;31;38m` — and half a sequence
        // is not a shorter sequence, it is text: the tail `35;241m` would be *printed*. So the bound is applied
        // to the screen before it is written, which is where a row boundary exists to respect.
        //
        // **The scrollback and the screen**, which is `task-1912`'s first fault against this: a snapshot is
        // the visible grid, so a node fourteen rows tall wrote fourteen rows down and the five commands
        // somebody wanted back were three of them short. Two bounds, whichever binds first.
        Some(crate::replay::bytes_within(
            &self.screen_and_history(crate::replay::REPLAY_ROWS),
            crate::replay::REPLAY_LIMIT,
        ))
    }

    /// Write bytes into the terminal whatever the program has been doing, which [`Self::replay`] refuses to do.
    ///
    /// **The measurement `task-1912` needed, and then the mechanism it needed.** On Windows the pseudoconsole
    /// clears the screen and repaints it the first time the program writes — measured on `pwsh`, `cmd.exe` and
    /// `powershell.exe` alike, which is what says it is the console host rather than any one shell — so a
    /// screen put back *before* the program has written is pushed into the scrollback and erased from view.
    /// Putting it back has to happen after that repaint, and after it `Self::replay`'s rule is false by
    /// construction.
    ///
    /// **Nothing in Unluminous puts a screen back this way**, and this is here so that can go on being true:
    /// `examples/replay_probe` is what calls it, to measure what the console host does to a screen that was
    /// drawn from outside. What a restored node does instead is print its screen *inside* its own console —
    /// `unluminous_cli::restore`, and §2 of `tasks/task-1912-a-session-and-a-terminal-tdd.md` for why there is
    /// no other place it survives.
    pub fn draw_over_the_terminal(&mut self, bytes: &[u8]) {
        let mut term = lock(&self.term);
        term.advance(bytes);
        // What the bytes asked the terminal to answer is not the program's business: it did not ask.
        term.take_replies();
    }

    /// The name for the tab: the title the program set, or the program's own name.
    ///
    /// A title that says no more than where the program was started from is not a name, so the program's
    /// own name is used instead. `cmd.exe` sets the console title to its own full path, which put
    /// `C:\Windows\system32\cmd.exe` on a Windows tab where the same tab on macOS read `zsh`. `zsh` sets
    /// no title at all, so the fallback below did all the work there and the fault could not be seen
    /// until Unluminous ran on Windows.
    pub fn name(&self) -> &str {
        if !self.given.is_empty() {
            return &self.given;
        }
        if self.title.is_empty()
            || title_is_only_the_program(&self.title, &self.name)
            || title_is_only_the_program(&self.title, &self.started)
        {
            &self.name
        } else {
            &self.title
        }
    }

    /// The name a person typed for this tab, or `None` while it is still named after its program.
    pub fn given_name(&self) -> Option<&str> {
        (!self.given.is_empty()).then_some(self.given.as_str())
    }

    /// Call this tab something else, which is what `task-1682` asks a right click to offer.
    ///
    /// An empty name puts it back to what the program calls it, so there is one way to undo a
    /// rename rather than a second command that means "forget the name I gave".
    pub fn rename(&mut self, name: &str) {
        self.given = name.trim().to_owned();
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// What the program exited with, readable once [`Self::is_running`] is false.
    ///
    /// `None` while it is still running, and `None` for a program that was killed rather than one
    /// that chose a code — a run tab says `stopped` there rather than inventing a number.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Stop the program, hard, and keep everything it wrote.
    ///
    /// The polite stop is the interrupt byte down the pty, which is `send` and belongs to the
    /// caller — `task-1683` §5.3 gives it a grace of two seconds. This is what comes after it.
    ///
    /// `Pty::kill` ends the job object on Windows and the process group on Unix, so the program and everything
    /// it started go together, whatever the program was in the middle of: closing a pseudoconsole is only a
    /// closed pipe on Windows, which a busy program does not notice, and `task-1769` found 119 shells left
    /// behind that way. The pseudoterminal is then dropped, which ends the reader. The grid stays, because this
    /// `Session` holds its own reference to it, which is what lets a run tab go on showing the output of a
    /// program that is no longer there.
    pub fn kill(&mut self) {
        if let Some(pty) = self.pty.take() {
            let taken = lock(&pty).take();
            if let Some(mut pty) = taken {
                let _ = pty.kill();
                let_go_of(pty);
            }
        }
        self.writer = None;
        self.running = false;
        self.exit_code = None;
    }

    pub fn bells(&self) -> usize {
        self.bells
    }

    /// The program in the foreground of this terminal, when it can be told.
    ///
    /// **What is running, not what it was started with**, which are different things: a terminal node on the
    /// canvas records the command it was *given*, and a person adds a plain terminal node and then types
    /// `claude` into the shell. See [`crate::foreground`] for the mechanism on each platform.
    ///
    /// A program that has ended answers nothing, because a terminal with no foreground group is not running
    /// anything — so a node whose program stopped before the window closed is not recorded as still having it.
    pub fn foreground(&self) -> Option<String> {
        self.running.then(|| self.master.foreground()).flatten()
    }

    /// The folder the shell in this terminal is in **now**, when the platform will say.
    ///
    /// **Not [`SessionSettings::working_directory`]**, which is where it was started: a person types `cd` and
    /// nothing about that reaches the pseudoterminal. `task-1945`: *"both nodes and normal terminals should
    /// be restored to exactly where they were."* See [`crate::foreground::Master::folder`] for how each
    /// platform is asked and which of them will not say.
    /// **And the shell's own answer beats the machine's**, because it is the only one PowerShell has:
    /// `Set-Location` moves PowerShell's location and never the process's current directory, so a `pwsh` tab
    /// came back in the folder it was started in whatever had been typed into it. A shell that reports its
    /// folder — with shell integration on, or because it already did — is followed; one that reports nothing
    /// falls through to the process, which is what `cmd.exe`, `bash` and `zsh` are read by. `task-1950`.
    pub fn folder(&self) -> Option<std::path::PathBuf> {
        if !self.running {
            return None;
        }
        // **The shell's own answer costs nothing**: it was read on the reader thread when the shell
        // reported it, and checked against the disk there — once a prompt rather than once a frame.
        if let Some(reported) = self.reported.folder() {
            return Some(reported);
        }
        // **The process's answer is asked on a clock** (`task-1984` A4): it is kernel round trips and a
        // `stat`, `remember_the_project` asks this for every terminal tab on every frame, and the answer
        // changes when somebody types `cd`.
        let mut asked = self.process_folder_asked.borrow_mut();
        let fresh = asked.as_ref().is_some_and(|(when, _)| when.elapsed() < FOLDER_INTERVAL);
        if !fresh {
            *asked = Some((std::time::Instant::now(), self.master.folder()));
        }
        asked.as_ref().and_then(|(_, folder)| folder.clone())
    }

    /// The two halves of [`Self::folder`] separately, so the difference between them can be measured.
    ///
    /// Which of the two answered is exactly what `task-1950` is about, and it is invisible from outside
    /// without this: `folder` prefers one and falls back to the other, so a shell reporting the right answer
    /// and a process holding the right answer look identical. `examples/folder_probe` is what asks.
    pub fn reported_folder(&self) -> Option<std::path::PathBuf> {
        self.reported.folder()
    }

    /// Where the operating system says the shell process is, which PowerShell does not move. See above.
    pub fn process_folder(&self) -> Option<std::path::PathBuf> {
        self.running.then(|| self.master.folder()).flatten()
    }

    /// What the program asked to be put on the clipboard, taken out so it is only handed over once.
    pub fn take_clipboard(&mut self) -> Option<String> {
        self.clipboard.take()
    }

    /// Deal with everything the window has to act on: the terminal's events, and the program's end.
    ///
    /// Called once a frame, and after feeding a detached session.
    pub fn pump(&mut self) {
        let events = std::mem::take(&mut *lock(&self.shared.events));
        for event in events {
            match event {
                Event::Title(title) => self.title = title,
                Event::Bell => self.bells += 1,
                Event::ClipboardWrite { data, .. } => {
                    self.clipboard = Some(String::from_utf8_lossy(&data).into_owned());
                }
                // A program asking whether the ground is light or dark (`CSI ? 996 n`). Answered here because the
                // palette is the window thread's, which the reader thread cannot read.
                Event::ColorSchemeQuery => {
                    let replies = {
                        let mut term = lock(&self.term);
                        term.reply_color_scheme(!self.palette.get().light);
                        term.take_replies()
                    };
                    self.send(replies);
                }
                // Reading the clipboard on a program's say so is never asked for: `allow_osc52_read` is off, so a
                // program that could read a password out of the clipboard is not answered.
                _ => {}
            }
        }
        // The program's end, which the exit watcher records after the last of its output has had time to
        // arrive. A killed program has already been marked stopped by `kill`, with no code.
        if self.running && self.shared.exited.load(Ordering::Acquire) {
            self.running = false;
            self.exit_code = match *lock(&self.shared.exit) {
                Some(ExitStatus::Code(code)) => Some(code),
                // A program ended by a signal on Unix chose no code, and neither did one whose status could not
                // be read.
                Some(ExitStatus::Signal(_)) | None => None,
            };
        }
    }

    /// Write bytes to the program.
    pub fn send(&self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        if let Some(kept) = &self.sent_while_detached {
            lock(kept).extend_from_slice(&bytes);
        }
        if let Some(writer) = &self.writer {
            // A send that fails means the writer has stopped because the program has gone.
            let _ = writer.send(bytes);
        }
    }

    /// What has been sent to a detached session, as text. Empty for a session with a program.
    ///
    /// See [`Session::sent_while_detached`] for why this exists.
    pub fn sent_to_a_detached_session(&self) -> String {
        self.sent_while_detached
            .as_ref()
            .map(|kept| String::from_utf8_lossy(&lock(kept)).into_owned())
            .unwrap_or_default()
    }

    /// What the program has asked for, which decides what some keys send.
    pub fn mode(&self) -> Mode {
        let term = lock(&self.term);
        Mode {
            application_cursor: term.mode(VtMode::CursorKeys),
            bracketed_paste: term.mode(VtMode::BracketedPaste),
            application_keypad: term.mode(VtMode::KeypadApplication),
            backspace_sends_bs: term.mode(VtMode::BackarrowSendsBs),
            modify_other_keys: term.modify_other_keys(),
            kitty_flags: term.screen().kitty_keyboard.flags(),
        }
    }

    /// True when the program is drawing a screen of its own, which is what `claude` and a text editor in a
    /// terminal do. There is no scrollback on that screen.
    pub fn on_alternate_screen(&self) -> bool {
        lock(&self.term).is_alt_screen()
    }

    /// True when the program has asked to be told about clicks.
    pub fn wants_mouse(&self) -> bool {
        self.mouse_mode().reports_clicks()
    }

    /// What the program has asked to be told about the mouse, and in which encoding.
    pub fn mouse_mode(&self) -> MouseMode {
        let term = lock(&self.term);
        MouseMode {
            x10: term.mode(VtMode::MouseX10),
            report_click: term.mode(VtMode::MouseNormal),
            drag: term.mode(VtMode::MouseButtonEvent),
            motion: term.mode(VtMode::MouseAnyEvent),
            sgr: term.mode(VtMode::MouseSgr),
            sgr_pixels: term.mode(VtMode::MouseSgrPixels),
            urxvt: term.mode(VtMode::MouseUrxvt),
            utf8: term.mode(VtMode::MouseUtf8),
        }
    }

    /// True when the program is drawing its own screen and has asked for the wheel to arrive as arrow
    /// keys, which is what makes scrolling work in a program that has no scrollback of its own.
    pub fn alternate_scroll(&self) -> bool {
        let term = lock(&self.term);
        term.is_alt_screen() && term.mode(VtMode::AlternateScroll)
    }

    /// True when the program has asked to be told when the terminal gains or loses the keyboard.
    pub fn wants_focus_reports(&self) -> bool {
        lock(&self.term).mode(VtMode::FocusEvents)
    }

    /// Change the size, telling the emulator and the program on the far side together.
    ///
    /// Both have to be told, and they have to be told the same thing: the emulator so the grid is the right
    /// shape, and the program through `SIGWINCH` so it draws in the right places. Telling only one is the
    /// fault that leaves a full screen program drawing into the wrong half of the tile.
    ///
    /// **A program that has ended is not told.** `ResizePseudoConsole` makes the console host
    /// re-render its buffer, and once the child has gone that buffer is empty — so telling it
    /// wipes everything the program wrote, on the frame after it finished.
    ///
    /// Resizing one that has only just **started** loses its output too, and that half is not
    /// fixed here because it cannot be: a pseudoconsole resized while its child is writing its
    /// first line loses that line. The answer is not to resize it at all, which means opening it at the size
    /// it is going to be drawn at — see `UnluminousApp::run_grid_size`.
    ///
    /// The emulator is still resized, so the finished output reflows to the width the tile is now
    /// and the view is not stuck at the shape it happened to end at. There is simply nothing on the
    /// far side left to tell.
    pub fn resize(&mut self, size: Size) {
        if size == self.size {
            return;
        }
        self.size = size;
        {
            let mut term = lock(&self.term);
            term.set_cell_pixels(size.cell_width, size.cell_height);
            term.resize(size.cols(), size.rows_u16());
        }
        // A reflow gives the rows new serials, so a view scrolled back by serial would land anywhere.
        self.scroll_anchor = None;
        if !self.running {
            return;
        }
        if let Some(pty) = &self.pty {
            if let Some(pty) = lock(pty).as_ref() {
                let _ = pty.resize(size.pty());
            }
        }
    }

    /// Move the view through the history. A positive delta goes back towards older output.
    pub fn scroll(&mut self, lines: i32) {
        if lines == 0 {
            return;
        }
        let term = lock(&self.term);
        let offset = self.display_offset(&term) as i64;
        let pages = &term.screen().pages;
        let wanted = (offset + lines as i64).clamp(0, pages.history_rows() as i64) as usize;
        self.scroll_anchor = (wanted > 0).then(|| pages.serial_of(pages.active_y(0) - wanted));
    }

    /// Put the view back at the newest output, which typing does.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_anchor = None;
    }

    /// How many rows above the bottom the view is, worked out from where it was anchored.
    ///
    /// A row that has been pruned out of the scrollback puts the view at the oldest row there is. The alternate
    /// screen has no history, so it is always at the bottom.
    fn display_offset(&self, term: &Terminal) -> usize {
        let Some(serial) = self.scroll_anchor else { return 0 };
        if term.is_alt_screen() {
            return 0;
        }
        let pages = &term.screen().pages;
        let history = pages.history_rows();
        match pages.y_of(serial) {
            Some(y) => pages.active_y(0).saturating_sub(y).min(history),
            None => history,
        }
    }

    /// Throw away everything on the screen and in the history, and put the cursor at the top left.
    ///
    /// The run tile's third button — `task-1683` §6.1 — and the one thing about a run that is not
    /// about the program: a program that has finished cannot be asked to clear its own screen, and
    /// one that is still going would take an escape sequence as *input* rather than as a command.
    /// So the emulator is given the sequences directly, which is the same change they would have made had a
    /// program written them, rather than bytes being pushed at a shell that may not be listening.
    pub fn clear(&mut self) {
        self.clear_selection();
        {
            let mut term = lock(&self.term);
            term.advance(b"\x1b[H\x1b[2J\x1b[3J");
            term.take_replies();
        }
        self.scroll_anchor = None;
    }

    /// Send the interrupt byte, which reaches the program as Ctrl+C.
    ///
    /// The polite half of `task-1683` §5.3, named here rather than written out at each of the
    /// places that stop a run — the tile's button, the widget's, the menu and the command line —
    /// because four copies of one byte is four chances to send a different one. It is always the
    /// byte, whatever keyboard protocol the program asked for: this is a signal, not a key press.
    pub fn interrupt(&self) {
        self.send(vec![0x03]);
    }

    /// Start a selection at a cell. `row` is a row of the screen, counted from the top.
    pub fn begin_selection(&mut self, row: usize, column: usize, kind: SelectionKind) {
        self.clear_selection();
        let mode = match kind {
            SelectionKind::Simple => SelectionMode::Char,
            SelectionKind::Word => SelectionMode::Word,
            SelectionKind::Line => SelectionMode::Line,
        };
        let mut term = lock(&self.term);
        let (y, col) = self.point_at(&term, row, column);
        let alternate = term.is_alt_screen();
        let selection = Selection::new(&mut term.screen_mut().pages, mode, y, col);
        self.selection =
            Some(Picked { selection, alternate, extended: kind != SelectionKind::Simple });
    }

    /// Drag a selection out to a cell.
    pub fn extend_selection(&mut self, row: usize, column: usize) {
        let mut term = lock(&self.term);
        let (y, col) = self.point_at(&term, row, column);
        let alternate = term.is_alt_screen();
        if let Some(picked) = self.selection.as_mut() {
            if picked.alternate == alternate {
                picked.selection.update(&mut term.screen_mut().pages, y, col);
                picked.extended = true;
            }
        }
    }

    pub fn clear_selection(&mut self) {
        let Some(picked) = self.selection.take() else { return };
        let mut term = lock(&self.term);
        // The pins belong to the screen the selection was made on. If that is no longer the one showing, its
        // page list may have gone with the alternate screen, and nothing is held there to let go of.
        if picked.alternate == term.is_alt_screen() {
            picked.selection.release(&mut term.screen_mut().pages);
        }
    }

    /// What is selected, as text, ready for the clipboard.
    pub fn selected_text(&self) -> Option<String> {
        let term = lock(&self.term);
        let picked = self.live_selection(&term)?;
        let text = picked.selection.text(&term.screen().pages, DEFAULT_WORD_SEPARATORS);
        (!text.is_empty()).then_some(text)
    }

    /// The selection, when there is one on the screen that is showing.
    fn live_selection(&self, term: &Terminal) -> Option<&Picked> {
        self.selection
            .as_ref()
            .filter(|picked| picked.extended && picked.alternate == term.is_alt_screen())
    }

    /// Everything the program has written that the terminal still holds: the scrollback and then the
    /// screen, one line each, oldest first.
    ///
    /// [`Screen::text`] is what is *showing*, which is the right answer for a painter and for a test
    /// asserting on a prompt. It is the wrong answer for a caller that wants what a program wrote,
    /// because a program that printed more lines than the tile is tall has had the start of its output
    /// scrolled off the top — and `run output` promises what a run has written. A dev server whose
    /// port scrolled past could not be asked for its port at all.
    ///
    /// `most_recent` bounds the answer to that many lines from the end. `None` is everything, which
    /// [`SCROLLBACK`] already bounds.
    pub fn written_text(&self, most_recent: Option<usize>) -> String {
        let term = lock(&self.term);
        let pages = &term.screen().pages;
        let total = pages.total_rows();
        // The last line with something on it, which is where counting back from the end starts. The
        // rows under it are the empty part of the screen rather than output — after `line60\r\n` the
        // cursor is on a fresh blank row — so a bound applied before they were dropped would spend
        // one of the lines asked for on a blank.
        let Some(last) = (0..total).rev().find(|&y| !row_text(pages, y).is_empty()) else {
            return String::new();
        };
        // Bounded before the rows are read rather than after, so a `tail` of 20 over ten thousand
        // lines of history builds twenty strings instead of ten thousand. A blank line *between* two
        // lines of output is output and is kept.
        let first = match most_recent {
            Some(0) => return String::new(),
            Some(wanted) => (last + 1).saturating_sub(wanted),
            None => 0,
        };
        (first..=last).map(|y| row_text(pages, y)).collect::<Vec<String>>().join("\n")
    }

    /// The last `rows` rows of the scrollback **and** the screen, as one [`Screen`].
    ///
    /// **What "five commands and their results" needs**, and what [`Self::snapshot`] cannot give: a snapshot is
    /// the visible grid, so a node fourteen rows tall wrote fourteen rows down and the first commands were
    /// already gone. `task-1912` measured that at 457 bytes for five commands. This is the reading
    /// [`Self::written_text`] does, with the colours and the attributes kept, which is what makes the difference
    /// between a restored terminal and a transcript of one.
    ///
    /// Trailing blank rows are left in: [`crate::replay::bytes_of`] drops them, and doing it twice in two
    /// places is two rules about the same thing.
    pub fn screen_and_history(&self, rows: usize) -> Screen {
        let mut term = lock(&self.term);
        let palette = self.follow_the_palette(&mut term);
        let colours = Colours::of(&term, &palette);
        let pages = &term.screen().pages;
        let columns = pages.cols() as usize;
        let total = pages.total_rows();
        let first = total.saturating_sub(rows);
        let mut screen =
            Screen::empty(total - first, columns, colours.foreground, colours.background);
        let mut links = Links::default();
        copy_rows(&term, pages, first..total, &colours, &mut links, &mut screen);
        screen.links = links.addresses;
        screen
    }

    /// Move this session onto the thread's palette if the window's theme has changed since it was last
    /// drawn, and answer with the palette it is on.
    ///
    /// The emulator holds every colour as one a program could have set, which is how a program asking for the
    /// background colour gets Unluminous's answer, so a theme change has to give the emulator the whole table
    /// again. It is done here, under the lock the drawing already takes, rather than by the window walking every
    /// terminal it knows about: the tile, the run tile, a canvas node and a ticket's terminal each hold
    /// sessions, and a list of the places that have to be told is a list whose next entry is the one that
    /// forgets. `task-2215`.
    fn follow_the_palette(&self, term: &mut Terminal) -> Palette {
        let wanted = Palette::current();
        if self.palette.get() != wanted {
            give_the_palette(term, &wanted);
            self.palette.set(wanted);
        }
        wanted
    }

    /// A row and column in the page list from a row on the screen, taking the scrollback into account.
    fn point_at(&self, term: &Terminal, row: usize, column: usize) -> (usize, u16) {
        let pages = &term.screen().pages;
        let top = pages.active_y(0) - self.display_offset(term);
        let row = row.min(pages.rows() as usize - 1);
        let column = column.min(pages.cols() as usize - 1);
        (top + row, column as u16)
    }

    /// Everything the painter needs, copied out under the lock.
    ///
    /// The lock is held for this copy and not for the drawing, so the reader thread never waits for a frame.
    pub fn snapshot(&self) -> Screen {
        let mut term = lock(&self.term);
        let palette = self.follow_the_palette(&mut term);
        let colours = Colours::of(&term, &palette);
        let offset = self.display_offset(&term);
        let pages = &term.screen().pages;
        let rows = pages.rows() as usize;
        let columns = pages.cols() as usize;
        let top = pages.active_y(0) - offset;
        let mut screen = Screen::empty(rows, columns, colours.foreground, colours.background);
        screen.history = pages.history_rows();
        screen.scrollback = offset;
        let mut links = Links::default();
        copy_rows(&term, pages, top..top + rows, &colours, &mut links, &mut screen);
        screen.links = links.addresses;
        screen.images = self.images_on_screen(&term, top, rows);
        screen.cursor = cursor_of(&term, offset);
        screen.selection = self.live_selection(&term).and_then(|picked| {
            let range = picked.selection.range(pages, DEFAULT_WORD_SEPARATORS)?;
            visible_range(range.start, range.end, top, rows, columns)
        });
        screen.title = self.name().to_owned();
        screen
    }

    /// The pictures that touch the rows `top..top + rows` of the page list, measured in cells from the top of
    /// the view, lowest `z` first.
    ///
    /// The kitty, sixel and iTerm2 protocols all end up in Termoil's one graphics store, which keeps a picture by
    /// the serial of the row it was placed on. The pixels are shared with the store rather than copied, and an
    /// unchanged picture keeps its generation, so the painter uploads each picture once.
    fn images_on_screen(&self, term: &Terminal, top: usize, rows: usize) -> Vec<ScreenImage> {
        let store = term.graphics();
        let mut tracker = self.images.borrow_mut();
        let live: std::collections::HashSet<u32> = store.images().map(|image| image.id).collect();
        tracker.seen.retain(|id, _| live.contains(id));
        if live.is_empty() || rows == 0 {
            return Vec::new();
        }
        let pages = &term.screen().pages;
        let first = pages.serial_of(top);
        let last = first + rows as u64 - 1;
        let cell = term.cell_pixels();
        let mut shown: Vec<ScreenImage> = store
            .placements_in_rows(first, last)
            .into_iter()
            .filter_map(|placement| {
                let image = store.image(placement.image_id)?;
                Some(ScreenImage {
                    pixels: tracker.pixels_of(image),
                    row: placement.serial as i64 - first as i64,
                    column: placement.col as i64,
                    offset: (placement.x_offset, placement.y_offset),
                    size: drawn_size(placement, cell),
                    cell,
                    source: placement.src,
                    z: placement.z,
                })
            })
            .collect();
        shown.sort_by_key(|image| (image.z, image.pixels.id));
        shown
    }

    /// Scroll so the previous or the next prompt the shell marked with `OSC 133` is at the top of the view.
    ///
    /// Answers whether there was one to go to. Going forward past the last prompt goes back to the newest output,
    /// which is where the prompt being typed at is. A shell that does not mark its prompts has none, and nothing
    /// moves. The alternate screen has no history and no prompts.
    pub fn jump_to_prompt(&mut self, forward: bool) -> bool {
        let term = lock(&self.term);
        if term.is_alt_screen() {
            return false;
        }
        let pages = &term.screen().pages;
        let mut prompts: Vec<usize> = term
            .marks()
            .iter()
            .filter(|mark| mark.kind == termoil_vt::terminal::MarkKind::PromptStart)
            .filter_map(|mark| pages.pin_get(mark.pin))
            .map(|(y, _)| y)
            .collect();
        prompts.dedup();
        let bottom = pages.active_y(0);
        let top = bottom - self.display_offset(&term);
        let target = match forward {
            // A prompt below the top of the newest screen cannot be scrolled to the top; past it is the bottom.
            true => prompts.iter().copied().find(|&y| y > top && y <= bottom),
            false => prompts.iter().rev().copied().find(|&y| y < top),
        };
        let wanted = match target {
            Some(y) => bottom.saturating_sub(y).min(pages.history_rows()),
            None if forward && top < bottom => 0,
            None => return false,
        };
        self.scroll_anchor = (wanted > 0).then(|| pages.serial_of(bottom - wanted));
        true
    }

    /// How many prompts the shell has marked with `OSC 133`, which says whether prompt jumping can do anything.
    pub fn prompt_count(&self) -> usize {
        let term = lock(&self.term);
        term.marks()
            .iter()
            .filter(|mark| mark.kind == termoil_vt::terminal::MarkKind::PromptStart)
            .count()
    }

    /// The number this session was given when it was made, which no other session in the process has.
    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// A weak handle that stops upgrading once this session is dropped (`task-2597`).
    ///
    /// The split panes beside a terminal belong to whatever holds that terminal: a tab, a node on the canvas,
    /// a ticket. Those hold the session and nothing else, so the window keeps the panes against this handle and
    /// lets go of them on the frame the handle stops answering, which is what closing the tab, the node or the
    /// ticket does.
    pub fn liveness(&self) -> std::sync::Weak<()> {
        Arc::downgrade(&self.alive)
    }

    /// Put the emulator back the way it started, keeping the program running: Termoil's `reset-terminal`.
    ///
    /// `RIS` given to the emulator directly, for the reason [`Session::clear`] gives its sequences directly.
    /// Termoil puts the host's palette back on `RIS` (`task-2489`), so a program that changed the colours
    /// leaves them changed no longer.
    pub fn reset(&mut self) {
        self.clear_selection();
        {
            let mut term = lock(&self.term);
            term.advance(b"\x1bc");
            term.take_replies();
            give_the_palette(&mut term, &self.palette.get());
        }
        self.scroll_anchor = None;
    }

    /// Throw away the history above the screen and keep the screen: Termoil's `clear-scrollback`.
    pub fn clear_history(&mut self) {
        self.clear_selection();
        {
            let mut term = lock(&self.term);
            term.advance(b"\x1b[3J");
            term.take_replies();
        }
        self.scroll_anchor = None;
    }

    /// Select everything the terminal holds, the history and the screen: Termoil's `select-all`.
    pub fn select_all(&mut self) {
        self.clear_selection();
        let mut term = lock(&self.term);
        let alternate = term.is_alt_screen();
        let pages = &term.screen().pages;
        let bottom = pages.active_y(0);
        let first = bottom - pages.history_rows();
        let last = bottom + pages.rows() as usize - 1;
        let last_column = pages.cols().saturating_sub(1);
        let pages = &mut term.screen_mut().pages;
        let selection = Selection::new(pages, SelectionMode::Char, first, 0);
        selection.update(pages, last, last_column);
        self.selection = Some(Picked { selection, alternate, extended: true });
    }

    /// Scroll the view to the oldest line the history holds: Termoil's `scroll-to-top`.
    pub fn scroll_to_top(&mut self) {
        let history = lock(&self.term).screen().pages.history_rows();
        self.scroll(history.min(i32::MAX as usize) as i32);
    }

    /// True for a session with no program behind it, which the tests use. A pane split from one is detached
    /// too, so a test of splitting starts no shell.
    pub fn is_detached(&self) -> bool {
        self.sent_while_detached.is_some()
    }

    /// How many rows the screen has, which is how far a page scroll moves.
    pub fn rows(&self) -> usize {
        self.size.rows
    }
}

/// The size in pixels a picture is drawn at for a placement, in the pixels of the cell size `cell`.
///
/// Termoil's own rule (`termoil-render`'s `drawn_size`): a picture whose cell counts are what its own size needs is
/// drawn at its own size, otherwise it is scaled to fit the cells it was given with its proportions kept, or
/// stretched to fill them when the placement says so.
fn drawn_size(placement: &termoil_vt::graphics::Placement, cell: (u16, u16)) -> (u32, u32) {
    let ceil_div = |a: i64, b: i64| (a + b - 1) / b.max(1);
    let cell = (cell.0.max(1) as i64, cell.1.max(1) as i64);
    let (width, height) = (placement.src.2.max(1) as i64, placement.src.3.max(1) as i64);
    let (x_offset, y_offset) = (placement.x_offset as i64, placement.y_offset as i64);
    let natural = (ceil_div(width + x_offset, cell.0), ceil_div(height + y_offset, cell.1));
    if (placement.cols as i64, placement.rows as i64) == natural && !placement.stretch {
        return (width as u32, height as u32);
    }
    let box_width = (placement.cols as i64 * cell.0 - x_offset).max(1);
    let box_height = (placement.rows as i64 * cell.1 - y_offset).max(1);
    if placement.stretch {
        return (box_width as u32, box_height as u32);
    }
    let scale = (box_width as f64 / width as f64).min(box_height as f64 / height as f64);
    (
        ((width as f64 * scale).round() as u32).max(1),
        ((height as f64 * scale).round() as u32).max(1),
    )
}

impl Drop for Session {
    /// End the program and let go of the pseudoterminal, so nothing it started is left behind.
    fn drop(&mut self) {
        self.writer = None;
        if let Some(pty) = self.pty.take() {
            if let Some(pty) = lock(&pty).take() {
                let_go_of(pty);
            }
        }
    }
}

/// What a drag over the grid selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    /// The cells the pointer went over.
    Simple,
    /// The whole word under the pointer, which a double click selects.
    Word,
    /// The whole line, which a triple click selects.
    Line,
}

/// A terminal of `size`, holding `palette`.
fn new_terminal(size: Size, palette: &Palette) -> Terminal {
    let config = TerminalConfig {
        scrollback: ScrollbackLimit { lines: Some(SCROLLBACK), ..ScrollbackLimit::default() },
        // A program reading the clipboard could read a password out of it, and nothing Unluminous runs needs to.
        allow_osc52_read: false,
        version_string: format!("Unluminous({})", env!("CARGO_PKG_VERSION")),
        ..TerminalConfig::default()
    };
    let mut term = Terminal::new(size.cols(), size.rows_u16(), config);
    term.set_cell_pixels(size.cell_width, size.cell_height);
    give_the_palette(&mut term, palette);
    // **The wheel scrolls a full screen program that has no mouse reporting** (mode 1007, alternate scroll).
    // xterm starts with it off and so does Termoil; alacritty started with it on, which is what made the wheel
    // move through `less`, `git log` and `man` in Unluminous. A program that does not want it can still turn it
    // off.
    term.advance(b"\x1b[?1007h");
    term
}

/// Give the terminal Unluminous's colours, so that a program asking what colour something is gets an answer and
/// so that resetting a colour puts Unluminous's own back rather than xterm's.
fn give_the_palette(term: &mut Terminal, palette: &Palette) {
    let mut table = [VtRgb::new(0, 0, 0); 256];
    for (index, entry) in table.iter_mut().enumerate() {
        *entry = vt_rgb(palette.indexed(index as u8));
    }
    term.set_palette(table);
    term.set_default_colors(DynamicColors {
        foreground: Some(vt_rgb(palette.foreground)),
        background: Some(vt_rgb(palette.background)),
        cursor: Some(vt_rgb(palette.cursor)),
        ..DynamicColors::default()
    });
}

/// Unluminous's colour as Termoil's.
fn vt_rgb(colour: Rgb) -> VtRgb {
    VtRgb::new(colour.r, colour.g, colour.b)
}

/// Termoil's colour as Unluminous's.
fn rgb(colour: VtRgb) -> Rgb {
    Rgb::new(colour.r, colour.g, colour.b)
}

/// The variables every program in an Unluminous terminal is given, under the caller's own.
///
/// `TERM` and `COLORTERM` tell the program what it is talking to. Without them a program either draws nothing
/// clever or draws it wrongly. `xterm-256color` rather than Termoil's own name, because a machine Unluminous is
/// installed on has the xterm description and not Termoil's. They are set on the child rather than in
/// Unluminous's own environment, which alacritty did through `setup_env`: rewriting the process's environment
/// while another thread starts a program is not safe, and per child there is nothing to race.
///
/// A name the caller gives replaces the one here, compared without regard to case on Windows, where
/// environment names are.
fn environment(caller: &[(String, String)]) -> Vec<(String, String)> {
    let same = |left: &str, right: &str| {
        if cfg!(windows) {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    };
    let ours = [
        ("TERM", "xterm-256color".to_owned()),
        ("COLORTERM", "truecolor".to_owned()),
        ("TERM_PROGRAM", "Unluminous".to_owned()),
        ("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION").to_owned()),
    ];
    let mut env: Vec<(String, String)> = ours
        .into_iter()
        .filter(|(name, _)| !caller.iter().any(|(given, _)| same(given, name)))
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    env.extend(caller.iter().cloned());
    env
}

/// The Windows pseudoconsole to use: Microsoft's own `conpty.dll` when it is beside the executable, and the one built
/// into Windows otherwise.
///
/// **`task-2514`.** The pseudoconsole built into Windows removes the kitty graphics (APC) and sixel (DCS) sequences
/// before the terminal sees them, so a picture a program sent never arrived, however well the painter could draw
/// it. The newer one Microsoft publishes passes them through byte for byte, and also leaves an `OSC 8` link as it
/// was written rather than adding an id to it. The Windows installer puts `conpty.dll` and `OpenConsole.exe` beside
/// `unluminous.exe` (`tools/fetch-conpty.ps1`), which is where `ConptyChoice::Auto` looks, as Termoil's own window
/// does. A build run from `target`, which every test is, has no `conpty.dll` beside it and keeps the built in one.
///
/// `unluminous_cli::restore` prints a remembered screen from inside the console, so it works the same way through
/// either host. Ignored on Unix.
fn conpty() -> termoil_pty::ConptyConfig {
    termoil_pty::ConptyConfig { choice: termoil_pty::ConptyChoice::Auto, ..Default::default() }
}

/// Drop a pseudoterminal on a thread of its own.
///
/// Closing a pseudoconsole waits for the console host to finish, which can take tens of milliseconds, and the
/// window's thread is where a session is dropped. Dropping `Pty` ends the job object, so the program goes with
/// it whichever thread does it.
fn let_go_of(pty: Pty) {
    let spawned =
        std::thread::Builder::new().name("unluminous pty close".into()).spawn(move || drop(pty));
    // A thread that could not be started is no reason to keep the program: drop it here.
    if let Err(error) = spawned {
        drop(error);
    }
}

/// Start the thread that writes what the window sends to the program.
fn spawn_writer(
    mut output: Box<dyn Write + Send>,
) -> std::io::Result<std::sync::mpsc::Sender<Vec<u8>>> {
    let (sender, receiver) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::Builder::new().name("unluminous pty writer".into()).spawn(move || {
        for bytes in receiver {
            if output.write_all(&bytes).and_then(|()| output.flush()).is_err() {
                break;
            }
        }
    })?;
    Ok(sender)
}

/// Start the thread that reads the program's output into the terminal.
///
/// It runs until the pseudoterminal closes, which is when the session is dropped or the program killed. On
/// Windows the pipe stays open after the program ends, so output the console host writes late still reaches the
/// screen (`task-2353`).
fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    mut scanner: crate::reported::Scanner,
    term: Arc<Mutex<Terminal>>,
    writer: std::sync::mpsc::Sender<Vec<u8>>,
    shared: Arc<Shared>,
    waker: Waker,
) -> std::io::Result<()> {
    std::thread::Builder::new().name("unluminous pty reader".into()).spawn(move || {
        let mut buffer = vec![0_u8; READ_CHUNK];
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            let bytes = &buffer[..read];
            scanner.read(bytes);
            let (replies, events) = {
                let mut term = lock(&term);
                term.advance(bytes);
                (term.take_replies(), term.take_events())
            };
            if !replies.is_empty() {
                let _ = writer.send(replies);
            }
            if !events.is_empty() {
                lock(&shared.events).extend(events);
            }
            waker();
        }
    })?;
    Ok(())
}

/// Start the thread that notices the program ending.
///
/// The reader alone cannot tell on Windows, where the output pipe stays open until the pseudoconsole closes. So
/// the program is asked, and its end is recorded [`EXIT_GRACE`] later, once its last output has had time to land.
fn spawn_exit_watcher(
    pty: Arc<Mutex<Option<Pty>>>,
    shared: Arc<Shared>,
    waker: Waker,
) -> std::io::Result<()> {
    std::thread::Builder::new().name("unluminous pty exit".into()).spawn(move || loop {
        let status = match lock(&pty).as_mut() {
            Some(pty) => pty.try_wait(),
            // The session let go of the program: it was killed or dropped, and that is not an exit to report.
            None => return,
        };
        match status {
            Ok(None) => std::thread::sleep(EXIT_POLL),
            // An error asking is read as an end with no code: the program cannot be asked about any more.
            ended => {
                std::thread::sleep(EXIT_GRACE);
                *lock(&shared.exit) = ended.ok().flatten();
                shared.exited.store(true, Ordering::Release);
                waker();
                return;
            }
        }
    })?;
    Ok(())
}

/// The colours a cell is resolved against: the terminal's palette, which a program may have changed, and the
/// defaults, which a program may have changed with `OSC 10` and `OSC 11`.
struct Colours {
    table: [VtRgb; 256],
    foreground: Rgb,
    background: Rgb,
    /// What bold text in the default colour is drawn in.
    bright_foreground: Rgb,
    palette: Palette,
    /// The program asked for reverse video (`DECSCNM`).
    reverse: bool,
}

impl Colours {
    /// The colours in effect in `term`, on `palette`.
    fn of(term: &Terminal, palette: &Palette) -> Self {
        let dynamic = term.dynamic_colors();
        let foreground = dynamic.foreground.map(rgb).unwrap_or(palette.foreground);
        Colours {
            table: *term.palette(),
            foreground,
            background: dynamic.background.map(rgb).unwrap_or(palette.background),
            bright_foreground: if dynamic.foreground.is_some() {
                foreground
            } else {
                palette.bright_foreground
            },
            palette: *palette,
            reverse: term.mode(VtMode::ReverseVideo),
        }
    }

    /// The foreground and background a cell with `style` is drawn in.
    ///
    /// Bold text in a named colour is drawn in the bright variant, which is what every terminal does; faint text
    /// is drawn in the darker one; and inverse video swaps the two, which is how a program marks a line without
    /// changing its colours. All three are resolved here, so the painter has one rule — draw the background,
    /// draw the letter — and so that a cell written down for a restore is the cell that was on the screen.
    fn resolve(&self, style: &Style) -> (Rgb, Rgb) {
        let bold = style.has(attr::BOLD);
        let faint = style.has(attr::FAINT);
        let brighten = bold && !faint;
        let mut foreground = match style.fg {
            Color::Default if brighten => self.bright_foreground,
            Color::Default => self.foreground,
            Color::Palette(index) if brighten && index < 8 => rgb(self.table[index as usize + 8]),
            Color::Palette(index) => rgb(self.table[index as usize]),
            Color::Rgb(r, g, b) => Rgb::new(r, g, b),
        };
        if faint {
            foreground = self.palette.dimmed(foreground);
        }
        let mut background = match style.bg {
            Color::Default => self.background,
            Color::Palette(index) => rgb(self.table[index as usize]),
            Color::Rgb(r, g, b) => Rgb::new(r, g, b),
        };
        if style.has(attr::INVERSE) != self.reverse {
            std::mem::swap(&mut foreground, &mut background);
        }
        (foreground, background)
    }

    /// The color the program gave a cell's underline with `SGR 58`, or `None` when it is drawn in the foreground.
    fn underline(&self, style: &Style) -> Option<Rgb> {
        match style.underline_color {
            Color::Default => None,
            Color::Palette(index) => Some(rgb(self.table[index as usize])),
            Color::Rgb(r, g, b) => Some(Rgb::new(r, g, b)),
        }
    }
}

/// Copy the rows in `range` of `pages` into `screen`, from its first row down, numbering the links met in `links`.
fn copy_rows(
    term: &Terminal,
    pages: &PageList,
    range: std::ops::Range<usize>,
    colours: &Colours,
    links: &mut Links,
    screen: &mut Screen,
) {
    let first = range.start;
    let columns = screen.columns;
    pages.for_rows(range, |y, page, row| {
        let into = y - first;
        for (column, cell) in page.cells(row).iter().enumerate().take(columns) {
            let mut copied = cell_of(page, row, column, *cell, colours);
            if cell.has_hyperlink() {
                copied.link = page
                    .hyperlink(page.cell_index(row, column))
                    .and_then(|id| links.index_of(term, id));
            }
            screen.cells[into * columns + column] = copied;
        }
    });
}

/// The underline style a Termoil style asks for.
fn underline_of(style: &Style) -> UnderlineStyle {
    use termoil_vt::style::Underline;
    match style.underline {
        Underline::None => UnderlineStyle::None,
        Underline::Single => UnderlineStyle::Single,
        Underline::Double => UnderlineStyle::Double,
        Underline::Curly => UnderlineStyle::Curly,
        Underline::Dotted => UnderlineStyle::Dotted,
        Underline::Dashed => UnderlineStyle::Dashed,
    }
}

/// One cell of a page, with its colours resolved.
///
/// Shared by [`Session::snapshot`] and [`Session::screen_and_history`], because a cell that came out looking one
/// way on the screen and another way in the file would be a restored terminal that is visibly not the one
/// somebody left.
fn cell_of(
    page: &Page,
    row: usize,
    column: usize,
    cell: termoil_vt::cell::Cell,
    colours: &Colours,
) -> ScreenCell {
    let style = page.style(cell.style_id());
    let (foreground, background) = colours.resolve(&style);
    let marks = if cell.is_cluster() {
        page.grapheme(page.cell_index(row, column)).to_vec()
    } else {
        Vec::new()
    };
    let wide = cell.wide();
    ScreenCell {
        // **A tab is a space on the screen** (`task-2325`): the painter has no glyph for one and drew a box.
        character: match cell.char() {
            None | Some('\t') => ' ',
            Some(other) => other,
        },
        marks,
        foreground,
        background,
        bold: style.has(attr::BOLD),
        italic: style.has(attr::ITALIC),
        underline: underline_of(&style),
        underline_color: colours.underline(&style),
        overline: style.has(attr::OVERLINE),
        strikethrough: style.has(attr::STRIKE),
        link: None,
        wide: wide == Wide::Wide,
        spacer: matches!(wide, Wide::SpacerTail | Wide::SpacerHead),
        hidden: style.has(attr::INVISIBLE),
    }
}

/// The text of row `y`, with wide character spacers skipped and the blanks at its end taken off.
fn row_text(pages: &PageList, y: usize) -> String {
    pages.with_row(y, |page, row| {
        let mut text = String::new();
        for (column, cell) in page.cells(row).iter().enumerate() {
            if matches!(cell.wide(), Wide::SpacerTail | Wide::SpacerHead) {
                continue;
            }
            match cell.char() {
                None | Some('\t') => text.push(' '),
                Some(character) => text.push(character),
            }
            if cell.is_cluster() {
                text.extend(page.grapheme(page.cell_index(row, column)));
            }
        }
        text.trim_end().to_owned()
    })
}

/// The cursor, where the program put it, unless it is hidden or the view is scrolled away from it.
fn cursor_of(term: &Terminal, offset: usize) -> Option<Cursor> {
    if offset > 0 || !term.mode(VtMode::CursorVisible) {
        return None;
    }
    let screen = term.screen();
    let columns = screen.pages.cols() as usize;
    Some(Cursor {
        row: screen.cursor.row as usize,
        column: (screen.cursor.col as usize).min(columns.saturating_sub(1)),
        shape: match term.cursor_shape() {
            VtCursorShape::Bar { .. } => CursorShape::Beam,
            VtCursorShape::Underline { .. } => CursorShape::Underline,
            VtCursorShape::Block { .. } | VtCursorShape::Default => CursorShape::Block,
        },
    })
}

/// A selected range in page list rows, as the run of screen cells the painter shades, or nothing when none of it
/// is on the screen.
fn visible_range(
    start: (usize, u16),
    end: (usize, u16),
    top: usize,
    rows: usize,
    columns: usize,
) -> Option<std::ops::Range<usize>> {
    let bottom = top + rows;
    if end.0 < top || start.0 >= bottom || columns == 0 {
        return None;
    }
    let from = if start.0 < top {
        0
    } else {
        (start.0 - top) * columns + (start.1 as usize).min(columns - 1)
    };
    let to = if end.0 >= bottom {
        rows * columns
    } else {
        (end.0 - top) * columns + (end.1 as usize).min(columns - 1) + 1
    };
    (to > from).then_some(from..to)
}

/// The shell to run when none was chosen.
///
/// Everywhere but Windows this is `SHELL`, which is the shell the person has actually chosen.
///
/// Windows has no `SHELL`, and `COMSPEC` — which is what this used to read — is not the answer to the
/// same question. It names the interpreter that runs a batch file, and on every Windows there has ever
/// been it says `cmd.exe`. So Unluminous opened a `cmd.exe` while the machine's own commands live in a
/// PowerShell profile, and `task-1670` reported the visible half of that: a function defined in
/// `Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1` cannot exist in `cmd`, so it is
/// `'claude-skip' is not recognized`, in a terminal that looks like every other terminal on the machine.
///
/// PowerShell it is, then, and the newer one when it is installed: `pwsh.exe` and `powershell.exe` read
/// **different** profiles — `Documents\PowerShell` and `Documents\WindowsPowerShell` — so this is not a
/// preference between two spellings of the same shell, it is which set of a person's own commands the
/// terminal comes up holding. Choosing the newest installed is what Windows Terminal does.
///
/// `COMSPEC` is still the last resort, for a Windows with no PowerShell on the path at all.
/// `terminal.shell` in the settings file beats all of it, which is how a person who wants `cmd` back
/// asks for it.
pub fn default_shell() -> String {
    if cfg!(target_os = "windows") {
        for shell in ["pwsh.exe", "powershell.exe"] {
            if on_the_path(shell) {
                return shell.to_owned();
            }
        }
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned())
    }
}

/// Whether `program` is a file in one of the folders on `PATH`.
///
/// Named rather than run: asking a program whether it exists by starting it would put a window on the
/// screen on the one system where that matters, and this is asked once for each terminal opened.
fn on_the_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|folder| folder.join(program).is_file())
}

/// Whether a title says no more than the name of the program that is running.
///
/// The last part of the title is compared, so a title that is the program's whole path counts, and so
/// does one that has something in front of it, which is what a console started with more rights has.
/// The comparison ignores case because Windows paths do.
fn title_is_only_the_program(title: &str, program: &str) -> bool {
    let last = title.rsplit(['\\', '/']).next().unwrap_or(title);
    last.eq_ignore_ascii_case(program)
}

/// The last part of a program's path, which is what a tab is named until the program sets a title.
fn program_name(shell: &str) -> String {
    std::path::Path::new(shell)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| shell.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sixteen rows of forty columns, which is enough for anything these tests draw.
    fn detached() -> Session {
        Session::detached(Size::new(16, 40))
    }

    /// macOS `ls` separates its columns with tabs. A tab moves to the next stop and leaves nothing to draw, so
    /// the screen shows spaces there rather than a tab character the font has no glyph for (`task-2325`).
    #[test]
    fn a_tab_moves_to_the_next_stop_and_draws_nothing() {
        let mut session = detached();
        session.feed(b"README.md\tsrc\t\ttasks");
        let screen = session.snapshot();
        assert_eq!(screen.row_text(0), "README.md       src             tasks");
        assert!(
            (0..screen.columns)
                .all(|column| screen.cell(0, column).is_some_and(|cell| cell.character != '\t')),
            "no cell is drawn with a tab character in it"
        );
    }

    /// Termoil's `clear-scrollback`, `select-all`, `reset-terminal` and `scroll-to-top` (`task-2597`).
    #[test]
    fn the_termoil_commands_clear_select_reset_and_scroll() {
        let mut session = detached();
        assert!(session.serial() != detached().serial(), "every session has its own serial");
        let alive = session.liveness();
        for line in 0..40 {
            session.feed(
                format!(
                    "line {line}
"
                )
                .as_bytes(),
            );
        }
        session.scroll_to_top();
        assert!(session.snapshot().contains("line 0"), "the oldest line is at the top");
        session.scroll_to_bottom();
        session.select_all();
        let selected = session.selected_text().expect("everything is selected");
        assert!(selected.contains("line 0") && selected.contains("line 39"));
        session.clear_history();
        session.scroll_to_top();
        assert!(!session.snapshot().contains("line 0"), "the history is gone");
        assert!(session.snapshot().contains("line 39"), "the screen is kept");
        session.feed(b"[31mred");
        session.reset();
        assert!(!session.snapshot().contains("line 39"), "a reset clears the screen");
        assert!(session.is_detached());
        drop(session);
        assert!(alive.upgrade().is_none(), "the handle stops answering when the session goes");
    }

    #[test]
    fn text_written_to_the_terminal_appears_on_the_screen() {
        let mut session = detached();
        session.feed(b"hello, terminal");
        let screen = session.snapshot();
        assert_eq!(screen.row_text(0), "hello, terminal");
        assert_eq!(screen.rows, 16);
        assert_eq!(screen.columns, 40);
    }

    /// A program that writes more lines than the screen is tall has scrolled the first of them off
    /// the top. They are still what it wrote, and this is what reads them.
    ///
    /// The fault it closes: `run output` read the screen, so a `tail` of 70 over sixty lines of
    /// output answered with the sixteen that happened to be showing and called it a success.
    #[test]
    fn what_scrolled_off_the_top_is_still_what_the_program_wrote() {
        let mut session = detached();
        for line in 1..=60 {
            session.feed(format!("line{line}\r\n").as_bytes());
        }

        let showing = session.snapshot().text();
        assert!(!showing.contains("line1\n"), "line1 has scrolled off the screen: {showing:?}");

        let written = session.written_text(None);
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(lines.first(), Some(&"line1"), "the oldest line is still readable");
        assert_eq!(lines.last(), Some(&"line60"), "and so is the newest");
        assert_eq!(lines.len(), 60, "every line once: {}", lines.len());
    }

    /// The bound is over everything written rather than over what is showing, which is the whole
    /// point of reading the scrollback.
    #[test]
    fn the_last_so_many_lines_are_counted_from_the_end_of_everything_written() {
        let mut session = detached();
        for line in 1..=60 {
            session.feed(format!("line{line}\r\n").as_bytes());
        }
        assert_eq!(
            session.written_text(Some(3)).lines().collect::<Vec<_>>(),
            ["line58", "line59", "line60"]
        );
        // More than there is is everything there is, not an error and not a padded answer.
        assert_eq!(session.written_text(Some(500)).lines().count(), 60);
        assert_eq!(session.written_text(Some(0)), "");
    }

    /// A blank line between two lines of output is output. The run of them under the last thing
    /// written is the empty part of the screen, and is not.
    #[test]
    fn a_blank_line_in_the_middle_is_kept_and_the_empty_screen_below_is_not() {
        let mut session = detached();
        session.feed(b"first\r\n\r\nthird\r\n");
        assert_eq!(session.written_text(None), "first\n\nthird");
    }

    #[test]
    fn a_line_feed_and_a_carriage_return_move_to_the_next_line() {
        let mut session = detached();
        session.feed(b"first\r\nsecond");
        let screen = session.snapshot();
        assert_eq!(screen.row_text(0), "first");
        assert_eq!(screen.row_text(1), "second");
    }

    #[test]
    fn a_colour_sequence_colours_the_text_it_comes_before() {
        let mut session = detached();
        session.feed(b"\x1b[31mred\x1b[0m plain");
        let screen = session.snapshot();
        let palette = Palette::new();
        assert_eq!(screen.cell(0, 0).expect("a cell").foreground, palette.indexed(1), "red");
        assert_eq!(
            screen.cell(0, 4).expect("a cell").foreground,
            palette.foreground,
            "after the reset the text is the ordinary colour again"
        );
    }

    #[test]
    fn twenty_four_bit_colour_is_used_exactly_as_it_was_given() {
        let mut session = detached();
        session.feed(b"\x1b[38;2;10;20;30mX");
        let cell = session.snapshot().cell(0, 0).cloned().expect("a cell");
        assert_eq!(cell.foreground, crate::palette::Rgb::new(10, 20, 30));
    }

    #[test]
    fn a_background_colour_reaches_the_cell_behind_the_letter() {
        let mut session = detached();
        session.feed(b"\x1b[44mX");
        let cell = session.snapshot().cell(0, 0).cloned().expect("a cell");
        assert_eq!(cell.background, Palette::new().indexed(4), "blue behind it");
    }

    #[test]
    fn inverse_video_swaps_the_two_colours() {
        let mut session = detached();
        session.feed(b"\x1b[7mX");
        let cell = session.snapshot().cell(0, 0).cloned().expect("a cell");
        let palette = Palette::new();
        assert_eq!(cell.foreground, palette.background);
        assert_eq!(cell.background, palette.foreground);
    }

    #[test]
    fn bold_italic_underline_and_strikethrough_all_arrive() {
        let mut session = detached();
        session.feed(b"\x1b[1mB\x1b[0m\x1b[3mI\x1b[0m\x1b[4mU\x1b[0m\x1b[9mS");
        let screen = session.snapshot();
        assert!(screen.cell(0, 0).expect("B").bold);
        assert!(screen.cell(0, 1).expect("I").italic);
        assert!(screen.cell(0, 2).expect("U").underline.is_on());
        assert!(screen.cell(0, 3).expect("S").strikethrough);
    }

    /// `task-2514`: the five underline styles, an underline color and an overline reach the screen as themselves,
    /// where before every underline came out as one plain line and an overline as nothing.
    #[test]
    fn the_five_underline_styles_their_color_and_an_overline_all_arrive() {
        let mut session = detached();
        session.feed(b"\x1b[4:1mA\x1b[4:2mB\x1b[4:3mC\x1b[4:4mD\x1b[4:5mE\x1b[0m");
        session.feed(b"\x1b[4;58;2;255;0;0mF\x1b[0m\x1b[53mG\x1b[0mH");
        let screen = session.snapshot();
        let styles: Vec<UnderlineStyle> =
            (0..5).map(|column| screen.cell(0, column).expect("a cell").underline).collect();
        assert_eq!(
            styles,
            vec![
                UnderlineStyle::Single,
                UnderlineStyle::Double,
                UnderlineStyle::Curly,
                UnderlineStyle::Dotted,
                UnderlineStyle::Dashed
            ]
        );
        assert_eq!(screen.cell(0, 5).expect("F").underline_color, Some(Rgb::new(255, 0, 0)));
        assert_eq!(
            screen.cell(0, 0).expect("A").underline_color,
            None,
            "no SGR 58, no color of its own"
        );
        assert!(screen.cell(0, 6).expect("G").overline);
        assert!(!screen.cell(0, 7).expect("H").overline);
    }

    /// `task-2514`: an `OSC 8` link reaches the cells it covers with its address, and the cells around it have none.
    #[test]
    fn a_link_reaches_the_cells_it_covers_with_its_address() {
        let mut session = detached();
        session.feed(b"see \x1b]8;;https://example.com/a\x1b\\here\x1b]8;;\x1b\\ now");
        let screen = session.snapshot();
        assert_eq!(screen.links, vec!["https://example.com/a".to_owned()]);
        assert_eq!(screen.cell(0, 3).expect("space").link, None);
        for column in 4..8 {
            assert_eq!(screen.cell(0, column).expect("here").link, Some(0), "column {column}");
        }
        assert_eq!(screen.cell(0, 8).expect("space").link, None);
    }

    /// `task-2514`: a picture sent with the kitty protocol is on the screen where it was placed, with its pixels.
    #[test]
    fn a_kitty_picture_is_on_the_screen_where_it_was_placed() {
        let mut session = Session::detached(Size::new(10, 40).with_cell(8.0, 16.0));
        // Two by two red pixels, placed at row 2 column 3.
        session.feed(b"\x1b[3;4H");
        session.feed(b"\x1b_Ga=T,f=32,s=2,v=2,q=2;/wAA//8AAP//AAD//wAA/w==\x1b\\");
        let screen = session.snapshot();
        assert_eq!(screen.images.len(), 1, "one picture: {:?}", screen.images);
        let image = &screen.images[0];
        assert_eq!((image.row, image.column), (2, 3));
        assert_eq!((image.pixels.width, image.pixels.height), (2, 2));
        assert_eq!(&image.pixels.rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(image.size, (2, 2), "a picture that fits its cells is drawn at its own size");
        assert_eq!(image.cell, (8, 16));
        let again = session.snapshot();
        assert_eq!(
            again.images[0].pixels.generation, image.pixels.generation,
            "unchanged pixels keep their generation"
        );
    }

    /// `task-2514`: the prompts a shell marks with `OSC 133` can be jumped between, and past the last one is the
    /// newest output.
    #[test]
    fn the_view_jumps_between_the_prompts_the_shell_marked() {
        let mut session = Session::detached(Size::new(4, 20));
        for command in ["one", "two", "three"] {
            session.feed(format!("\x1b]133;A\x07$ {command}\r\n").as_bytes());
            for line in 0..6 {
                session.feed(format!("{command} {line}\r\n").as_bytes());
            }
        }
        session.feed(b"\x1b]133;A\x07$ ");
        assert_eq!(session.prompt_count(), 4);
        assert!(session.jump_to_prompt(false));
        assert_eq!(session.snapshot().row_text(0), "$ three");
        assert!(session.jump_to_prompt(false));
        assert_eq!(session.snapshot().row_text(0), "$ two");
        assert!(session.jump_to_prompt(true));
        assert_eq!(session.snapshot().row_text(0), "$ three");
        assert!(session.jump_to_prompt(true), "the prompt being typed at is on the newest screen");
        assert_eq!(session.snapshot().scrollback, 0, "past the last prompt is the newest output");
        assert!(!session.jump_to_prompt(true), "and there is nowhere further to go");
    }

    #[test]
    fn a_colour_a_program_has_changed_wins_over_the_default() {
        let mut session = detached();
        session.feed(b"\x1b]4;1;rgb:09/09/09\x07\x1b[31mX");
        let cell = session.snapshot().cell(0, 0).cloned().expect("a cell");
        assert_eq!(
            cell.foreground,
            crate::palette::Rgb::new(9, 9, 9),
            "the program asked for a different red"
        );
    }

    #[test]
    fn faint_text_is_drawn_darker() {
        let mut session = detached();
        session.feed(b"\x1b[2mX");
        let cell = session.snapshot().cell(0, 0).cloned().expect("a cell");
        let palette = Palette::new();
        assert_eq!(cell.foreground, palette.dimmed(palette.foreground));
    }

    #[test]
    fn bold_text_in_a_named_colour_comes_out_bright() {
        let mut session = detached();
        session.feed(b"\x1b[1;31mbold red");
        let cell = session.snapshot().cell(0, 0).cloned().expect("a cell");
        assert_eq!(cell.foreground, Palette::new().indexed(9));
    }

    #[test]
    fn a_wide_character_takes_two_columns_and_the_second_draws_nothing() {
        let mut session = detached();
        // A Japanese character, which every terminal draws two columns wide.
        session.feed("\u{3042}x".as_bytes());
        let screen = session.snapshot();
        assert_eq!(screen.cell(0, 0).expect("the character").character, '\u{3042}');
        assert!(screen.cell(0, 0).expect("the character").wide);
        assert!(screen.cell(0, 1).expect("the spacer").spacer, "the second column is a spacer");
        assert_eq!(screen.cell(0, 2).expect("what follows").character, 'x');
        assert_eq!(screen.row_text(0), "\u{3042}x", "and reading the row back skips the spacer");
    }

    #[test]
    fn a_combining_accent_stays_with_the_letter_it_is_over() {
        let mut session = detached();
        session.feed("e\u{0301}".as_bytes());
        let cell = session.snapshot().cell(0, 0).cloned().expect("a cell");
        assert_eq!(cell.character, 'e');
        assert_eq!(
            cell.marks,
            vec!['\u{0301}'],
            "the accent is drawn over the letter, not after it"
        );
    }

    #[test]
    fn moving_the_cursor_puts_the_next_text_where_it_was_sent() {
        let mut session = detached();
        // Row 5, column 10, counted from one in the sequence and from zero in the grid.
        session.feed(b"\x1b[5;10Hhere");
        let screen = session.snapshot();
        assert_eq!(screen.row_text(4).trim_start(), "here");
        assert_eq!(screen.cell(4, 9).expect("a cell").character, 'h');
        let cursor = screen.cursor.expect("the cursor is showing");
        assert_eq!((cursor.row, cursor.column), (4, 13), "the cursor is after what was written");
    }

    #[test]
    fn clearing_the_screen_leaves_it_empty() {
        let mut session = detached();
        session.feed(b"something to clear\x1b[2J");
        assert_eq!(session.snapshot().text(), "");
    }

    #[test]
    fn clearing_by_hand_empties_the_screen_and_the_history_behind_it() {
        // The run tile's `clear` button. It tells the emulator rather than writing bytes at the
        // program, because a program that has finished cannot clear its own screen and one that is
        // running would read the sequence as input.
        let mut session = Session::detached(Size::new(4, 20));
        for line in 0..20 {
            session.feed(format!("line {line}\r\n").as_bytes());
        }
        assert!(session.snapshot().history > 0, "there is scrollback to clear");
        session.clear();
        let screen = session.snapshot();
        assert_eq!(screen.text(), "", "nothing on the screen");
        assert_eq!(screen.history, 0, "and nothing above it either");
        assert_eq!(
            screen.cursor.map(|cursor| (cursor.row, cursor.column)),
            Some((0, 0)),
            "with the cursor back at the top left, ready for the next run"
        );
    }

    #[test]
    fn hiding_the_cursor_takes_it_off_the_screen_and_showing_it_brings_it_back() {
        let mut session = detached();
        session.feed(b"\x1b[?25l");
        assert!(session.snapshot().cursor.is_none(), "the program hid the cursor");
        session.feed(b"\x1b[?25h");
        assert!(session.snapshot().cursor.is_some());
    }

    #[test]
    fn a_cursor_shape_the_program_asked_for_is_reported() {
        let mut session = detached();
        session.feed(b"\x1b[6 q");
        assert_eq!(session.snapshot().cursor.expect("a cursor").shape, CursorShape::Beam);
        session.feed(b"\x1b[4 q");
        assert_eq!(session.snapshot().cursor.expect("a cursor").shape, CursorShape::Underline);
        session.feed(b"\x1b[2 q");
        assert_eq!(session.snapshot().cursor.expect("a cursor").shape, CursorShape::Block);
    }

    #[test]
    fn the_alternate_screen_leaves_the_ordinary_one_as_it_was() {
        let mut session = detached();
        session.feed(b"ordinary screen");
        assert!(!session.on_alternate_screen());
        // Switching screens saves the cursor where it was rather than moving it, so a program that wants to
        // draw from the top left says so, which is what the `ESC [ H` is here.
        session.feed(b"\x1b[?1049h\x1b[H");
        assert!(session.on_alternate_screen(), "the program is drawing its own screen now");
        session.feed(b"a screen of its own");
        assert_eq!(session.snapshot().row_text(0), "a screen of its own");
        session.feed(b"\x1b[?1049l");
        assert!(!session.on_alternate_screen());
        assert_eq!(
            session.snapshot().row_text(0),
            "ordinary screen",
            "what was there before the program started is back"
        );
    }

    #[test]
    fn application_cursor_keys_and_bracketed_paste_are_reported_when_the_program_asks() {
        let mut session = detached();
        assert_eq!(session.mode(), Mode::default());
        session.feed(b"\x1b[?1h\x1b[?2004h");
        let mode = session.mode();
        assert!(mode.application_cursor && mode.bracketed_paste, "{mode:?}");
    }

    #[test]
    fn the_wheel_moves_a_full_screen_program_unless_it_says_otherwise() {
        let mut session = detached();
        assert!(!session.alternate_scroll(), "on the ordinary screen the wheel moves the history");
        session.feed(b"\x1b[?1049h");
        assert!(
            session.alternate_scroll(),
            "a pager on its own screen is scrolled with arrow keys"
        );
        session.feed(b"\x1b[?1007l");
        assert!(!session.alternate_scroll(), "unless the program turned that off");
    }

    #[test]
    fn a_program_asking_about_the_mouse_is_reported() {
        let mut session = detached();
        assert!(!session.wants_mouse());
        session.feed(b"\x1b[?1000h");
        assert!(session.wants_mouse(), "the program asked to be told about clicks");
    }

    #[test]
    fn a_title_the_program_sets_becomes_the_name_of_the_tab() {
        let mut session = detached();
        assert_eq!(session.name(), "detached");
        session.feed(b"\x1b]0;claude\x07");
        assert_eq!(session.name(), "claude");
        session.feed(b"\x1b]2;a different title\x07");
        assert_eq!(session.name(), "a different title");
    }

    #[test]
    fn output_that_runs_off_the_top_can_be_scrolled_back_to() {
        let mut session = Session::detached(Size::new(4, 20));
        for line in 0..20 {
            session.feed(format!("line {line}\r\n").as_bytes());
        }
        let screen = session.snapshot();
        assert!(
            screen.contains("line 19"),
            "the newest output is showing, got {:?}",
            screen.text()
        );
        assert!(screen.history > 0, "there should be history to scroll back through");

        session.scroll(10);
        let scrolled = session.snapshot();
        assert!(
            scrolled.contains("line 9"),
            "scrolling back should show older output, got {:?}",
            scrolled.text()
        );
        assert!(scrolled.cursor.is_none(), "the cursor is not on the part being looked at");

        session.scroll_to_bottom();
        assert!(session.snapshot().contains("line 19"));
    }

    #[test]
    fn resizing_changes_the_grid_and_keeps_the_text() {
        let mut session = Session::detached(Size::new(10, 30));
        session.feed(b"a line of text");
        session.resize(Size::new(6, 20));
        let screen = session.snapshot();
        assert_eq!(screen.rows, 6);
        assert_eq!(screen.columns, 20);
        assert_eq!(
            screen.row_text(0),
            "a line of text",
            "the text is still there after the resize"
        );
    }

    #[test]
    fn resizing_narrower_breaks_a_long_line_up_rather_than_losing_it() {
        let mut session = Session::detached(Size::new(6, 30));
        session.feed(b"0123456789012345678901234");
        assert_eq!(session.snapshot().history, 0);
        session.resize(Size::new(6, 10));

        // Twenty five characters at ten columns is three lines, and six rows hold all three, so the whole line
        // stays on the screen and nothing is pushed into the history. alacritty kept the cursor on the row it
        // was on and pushed the first two rows into the history; Termoil wraps the line where it is (`task-2489`).
        let screen = session.snapshot();
        assert_eq!(screen.row_text(0), "0123456789", "the line starts where it started");
        assert_eq!(screen.row_text(1), "0123456789");
        assert_eq!(screen.row_text(2), "01234", "and its end is on the third row");
        assert_eq!(screen.history, 0, "nothing had to go into the history");
        let cursor = screen.cursor.expect("the cursor is showing");
        assert_eq!((cursor.row, cursor.column), (2, 5), "the cursor is after the end of the line");

        // Narrower than the screen is tall, and the start does go into the history, where it can be scrolled
        // back to.
        session.resize(Size::new(2, 10));
        assert_eq!(session.snapshot().row_text(1), "01234");
        assert_eq!(session.snapshot().history, 1);
        session.scroll(1);
        assert_eq!(session.snapshot().row_text(0), "0123456789", "which can be scrolled back to");
    }

    #[test]
    fn a_selection_can_be_read_back_as_text() {
        let mut session = detached();
        session.feed(b"select these words");
        session.begin_selection(0, 0, SelectionKind::Simple);
        session.extend_selection(0, 5);
        let selected = session.selected_text().expect("something is selected");
        assert_eq!(selected, "select", "the cells dragged over come back as text");
        assert!(session.snapshot().selection.is_some(), "and the painter is told where they are");
        session.clear_selection();
        assert!(session.selected_text().is_none());
    }

    #[test]
    fn a_double_click_selects_a_word() {
        let mut session = detached();
        session.feed(b"one two three");
        session.begin_selection(0, 5, SelectionKind::Word);
        assert_eq!(session.selected_text().as_deref(), Some("two"));
    }

    #[test]
    fn the_bell_is_counted_rather_than_making_a_noise() {
        let mut session = detached();
        session.feed(b"\x07\x07");
        assert_eq!(session.bells(), 2);
    }

    #[test]
    fn a_program_asking_to_copy_hands_the_text_to_the_window() {
        let mut session = detached();
        // OSC 52, which is how a program running over a connection copies to the clipboard: base64 for
        // "hello".
        session.feed(b"\x1b]52;c;aGVsbG8=\x07");
        assert_eq!(session.take_clipboard().as_deref(), Some("hello"));
        assert_eq!(session.take_clipboard(), None, "it is only handed over once");
    }

    /// A shell this machine certainly has, that answers `echo` and leaves on `exit`.
    ///
    /// `/bin/sh` is named rather than whatever `SHELL` says, so the test does not depend on the shell the
    /// person running it happens to use. It is a Unix path though, and there is nothing at it on Windows,
    /// where the same job is done by the program `COMSPEC` names — so on Windows the test asks for that
    /// instead. Both understand `echo` and `exit`, which is all these tests send, and neither reads a
    /// profile, so what they are waiting for is the pseudoterminal rather than somebody's own startup
    /// file. Naming it here rather than calling `default_shell` is deliberate for the same reason: these
    /// tests are about the plumbing, and what the default *is* has a test of its own.
    fn test_shell() -> String {
        if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned())
        } else {
            "/bin/sh".to_owned()
        }
    }

    /// The pids of every `pwsh.exe` started by **this test binary** whose command line carries
    /// `nonce`.
    ///
    /// By parent and by nonce, never by image alone: this box runs several agents at once, and a
    /// check that treats any new `pwsh.exe` as its own both reports and reaps somebody else's work.
    /// `task-1769` did exactly that before this was written.
    #[cfg(windows)]
    fn own_shells_named(nonce: &str) -> Vec<u32> {
        let me = std::process::id();
        let query = format!(
            "Get-CimInstance Win32_Process -Filter \"Name='pwsh.exe' AND ParentProcessId={me}\" |              Where-Object {{ $_.CommandLine -like '*{nonce}*' }} | ForEach-Object {{ $_.ProcessId }}"
        );
        let out = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &query])
            .output()
            .expect("powershell answers");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect()
    }

    /// Whether `pid` is still a process at all.
    #[cfg(windows)]
    fn still_running(pid: u32) -> bool {
        let out = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("if (Get-CimInstance Win32_Process -Filter 'ProcessId={pid}') {{ 'yes' }} else {{ 'no' }}"),
            ])
            .output()
            .expect("powershell answers");
        String::from_utf8_lossy(&out.stdout).contains("yes")
    }

    #[cfg(windows)]
    #[test]
    fn a_session_takes_its_program_with_it_even_when_the_program_is_busy() {
        // The failure this is here for: closing a pseudoconsole is a hangup on Unix and is **not** a
        // kill on Windows. A program blocked on its console notices; a program in the middle of
        // something does not, and it then waits for ever on a console host nobody will ever type
        // into. `task-1769` found 119 shells left behind that way, holding 4 GB of commit charge,
        // accruing at about sixty a day — on a box where free commit is what decides whether a
        // render runs at full speed.
        //
        // So the shell here is deliberately **busy**: it sleeps for fifteen minutes and reads
        // nothing. If dropping the session is the only thing stopping it, it outlives this test.
        let nonce = format!(
            "unluminous-reap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let settings = SessionSettings {
            shell: Some("pwsh.exe".to_owned()),
            args: vec![
                "-NoProfile".to_owned(),
                "-Command".to_owned(),
                format!(
                    "# {nonce}
Start-Sleep -Seconds 900"
                ),
            ],
            working_directory: Some(std::env::temp_dir()),
            ..SessionSettings::default()
        };
        let session =
            Session::spawn(&settings, Size::new(14, 160), Arc::new(|| {})).expect("start a shell");

        // Wait for the shell to actually be there before asking anything about it being gone.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut shells = Vec::new();
        while std::time::Instant::now() < deadline {
            shells = own_shells_named(&nonce);
            if !shells.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        assert_eq!(
            shells.len(),
            1,
            "exactly one shell, started by this test and named by its nonce"
        );
        let shell = shells[0];

        drop(session);

        // Ending a process is not instantaneous, so this waits for it rather than asserting on the
        // next line and being flaky about it.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline && still_running(shell) {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        let survived = still_running(shell);
        if survived {
            // Never leave the very thing this test is about behind, whether it passes or fails.
            let _ = std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &format!("Stop-Process -Id {shell} -Force -Confirm:$false"),
                ])
                .output();
        }
        assert!(!survived, "the shell outlived the session that started it (pid {shell})");
    }

    #[cfg(windows)]
    #[test]
    fn killing_a_busy_program_ends_it_at_once() {
        // The test above states the contract on drop. This one states it on `kill`, which a run tab's second
        // press of Stop uses: Termoil's job object ends the program whatever it is doing, so a program that
        // reads nothing and sleeps is gone within a few seconds rather than whenever it next looks at its
        // console. `task-1769` found 119 shells left behind before there was a job.
        let nonce = format!("unluminous-kill-{}", std::process::id());
        let settings = SessionSettings {
            shell: Some("pwsh.exe".to_owned()),
            args: vec![
                "-NoProfile".to_owned(),
                "-Command".to_owned(),
                format!("# {nonce}\nStart-Sleep -Seconds 300"),
            ],
            working_directory: Some(std::env::temp_dir()),
            ..SessionSettings::default()
        };
        let mut session =
            Session::spawn(&settings, Size::new(14, 160), Arc::new(|| {})).expect("start a shell");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut shells = Vec::new();
        while std::time::Instant::now() < deadline && shells.is_empty() {
            shells = own_shells_named(&nonce);
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        assert_eq!(shells.len(), 1, "exactly one shell, started by this test");
        session.kill();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline && still_running(shells[0]) {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        assert!(!still_running(shells[0]), "the shell outlived kill (pid {})", shells[0]);
    }

    #[test]
    fn a_detached_session_holds_no_program() {
        // The other half of the contract: a session with no shell behind it must not be holding a
        // pseudoterminal, or dropping one would end a process it never started.
        let session = Session::detached(Size::new(10, 40));
        assert!(session.pty.is_none());
        assert!(session.foreground().is_none());
    }

    #[test]
    fn a_shell_runs_a_command_and_its_output_appears() {
        // The one test here that starts a real shell in a real pseudoterminal. It is what proves the
        // pseudoterminal, the reader thread, the writing and the waking work together, which a detached
        // session cannot show.
        let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = woken.clone();
        let waker: Waker = Arc::new(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
        let settings = SessionSettings {
            shell: Some(test_shell()),
            working_directory: Some(std::env::temp_dir()),
            ..SessionSettings::default()
        };
        let mut session =
            Session::spawn(&settings, Size::new(12, 60), waker).expect("start a shell");
        // A carriage return, which is what the Enter key sends and what `keys.rs` puts on the wire. A line
        // feed is enough for a Unix line discipline, but a ConPTY does not take one as the line being
        // finished, so on Windows the command was typed and never run.
        session.send(b"echo unluminous-terminal-works\r".to_vec());

        // A shell answers when it answers, so this waits for the output rather than assuming it is there.
        // A generous wait, because this runs on whatever machine the tests are run on and a shell answering
        // is not something a test can hurry.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            session.pump();
            if session.snapshot().contains("unluminous-terminal-works") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the shell did not answer in thirty seconds, the screen holds {:?}",
                session.snapshot().text()
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(
            woken.load(std::sync::atomic::Ordering::Relaxed) > 0,
            "the window should have been woken to draw the output"
        );
        assert!(session.is_running());
    }

    #[test]
    fn a_shell_that_is_told_to_leave_stops_running() {
        let waker: Waker = Arc::new(|| {});
        let settings = SessionSettings { shell: Some(test_shell()), ..SessionSettings::default() };
        let mut session =
            Session::spawn(&settings, Size::new(8, 40), waker).expect("start a shell");
        session.send(b"exit\r".to_vec());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while session.is_running() {
            session.pump();
            assert!(
                std::time::Instant::now() < deadline,
                "the shell did not stop in thirty seconds"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }

    /// Run one short-lived program to the end, and say what it exited with.
    ///
    /// A real program in a real pseudoterminal, because what is being tested is that
    /// `Event::ChildExit`'s code reaches [`Session::exit_code`] — which a detached session, having
    /// no child, can say nothing about. It waits with a deadline and pumps, as the two tests above
    /// it do, rather than assuming when a process ends.
    fn run_to_the_end(settings: SessionSettings) -> Session {
        let waker: Waker = Arc::new(|| {});
        let mut session =
            Session::spawn(&settings, Size::new(8, 60), waker).expect("start the program");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while session.is_running() {
            session.pump();
            assert!(
                std::time::Instant::now() < deadline,
                "the program did not finish in thirty seconds, the screen holds {:?}",
                session.snapshot().text()
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        // One more, because the exit arrives as an event and the loop above stops on the frame it
        // was read: a code written by the same `pump` is already here, and this costs nothing.
        session.pump();
        session
    }

    /// A program that exits with a chosen code and nothing else, spelled for this platform.
    fn exiting_with(code: i32) -> SessionSettings {
        if cfg!(target_os = "windows") {
            SessionSettings {
                shell: Some(test_shell()),
                args: vec!["/c".to_owned(), format!("exit {code}")],
                ..SessionSettings::default()
            }
        } else {
            SessionSettings {
                shell: Some(test_shell()),
                args: vec!["-c".to_owned(), format!("exit {code}")],
                ..SessionSettings::default()
            }
        }
    }

    #[test]
    fn a_program_that_ends_leaves_the_code_it_chose_behind_it() {
        // `task-1683` §11.1. The code has always arrived on `Event::ChildExit` and has always been
        // thrown away; a run tab is the first thing that has a use for it.
        let session = run_to_the_end(exiting_with(3));
        assert!(!session.is_running());
        assert_eq!(session.exit_code(), Some(3), "the code the program chose");
    }

    #[test]
    fn a_program_that_ends_cleanly_says_zero_rather_than_nothing() {
        let session = run_to_the_end(exiting_with(0));
        assert_eq!(session.exit_code(), Some(0));
    }

    #[test]
    fn what_a_program_wrote_just_before_it_ended_is_still_on_the_screen() {
        // `drain_on_exit`. Without it the reader loop leaves the pseudoterminal alone the moment
        // the child exits, and a program that prints one line and stops has all of its output
        // still sitting in the pipe — so the screen is empty. `task-1683`'s run tile is where that
        // became visible, because the whole point of it is that the evidence outlives the process.
        let message = "unluminous-drain-on-exit-works";
        let settings = if cfg!(target_os = "windows") {
            SessionSettings {
                shell: Some(test_shell()),
                args: vec!["/c".to_owned(), format!("echo {message}")],
                ..SessionSettings::default()
            }
        } else {
            SessionSettings {
                shell: Some(test_shell()),
                args: vec!["-c".to_owned(), format!("echo {message}")],
                ..SessionSettings::default()
            }
        };
        let session = run_to_the_end(settings);
        assert!(!session.is_running());
        assert!(
            session.snapshot().contains(message),
            "the program's last words should still be there, and the screen holds {:?}",
            session.snapshot().text()
        );
    }

    #[test]
    fn a_program_that_ends_at_once_is_still_seen_to_end_with_its_code() {
        // `task-2353`. With alacritty a program that ended before the reader loop had registered with its
        // poller was never seen to end, and a run tab said `running` for ever. Termoil's exit watcher asks the
        // program itself, so the order of the threads does not matter; this is the shortest program there is.
        let session = run_to_the_end(exiting_with(7));
        assert!(!session.is_running());
        assert_eq!(session.exit_code(), Some(7));
    }

    #[test]
    fn many_short_programs_each_leave_their_output_and_their_code() {
        // `task-2353` was found on a busy machine, where a program's only line arrived after it had ended.
        // Several at once is the cheapest way to make the machine busy enough for the order to vary.
        let runs: Vec<std::thread::JoinHandle<(bool, Option<i32>)>> = (0..6)
            .map(|index| {
                std::thread::spawn(move || {
                    let marker = format!("unluminous-many-{index}");
                    let (flag, command) = if cfg!(target_os = "windows") {
                        ("/c", format!("echo {marker}&& exit 4"))
                    } else {
                        ("-c", format!("echo {marker}; exit 4"))
                    };
                    let settings = SessionSettings {
                        shell: Some(test_shell()),
                        args: vec![flag.to_owned(), command],
                        ..SessionSettings::default()
                    };
                    let mut session = run_to_the_end(settings);
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    while !session.snapshot().contains(&marker)
                        && std::time::Instant::now() < deadline
                    {
                        session.pump();
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    (session.snapshot().contains(&marker), session.exit_code())
                })
            })
            .collect();
        for run in runs {
            let (printed, code) = run.join().expect("the run finished");
            assert!(printed, "every program's line reached its screen");
            assert_eq!(code, Some(4), "and every program's code arrived");
        }
    }

    #[test]
    fn scrolling_back_holds_its_place_while_output_arrives() {
        // alacritty kept the view on the rows being read while new output pushed the screen up, and a person
        // reading the history of a dev server expects the lines in front of them to stay put.
        let mut session = Session::detached(Size::new(4, 20));
        for line in 0..30 {
            session.feed(format!("line {line}\r\n").as_bytes());
        }
        session.scroll(10);
        let before = session.snapshot().row_text(0);
        for line in 30..40 {
            session.feed(format!("line {line}\r\n").as_bytes());
        }
        let after = session.snapshot();
        assert_eq!(after.row_text(0), before, "the row at the top of the view is the same row");
        assert_eq!(after.scrollback, 20, "and it is further from the bottom by what arrived");
    }

    #[test]
    fn resetting_a_colour_puts_unluminous_back_not_xterm() {
        // `task-2489`. Termoil put xterm's palette back on `OSC 104` and `RIS`, so `tput reset` in Unluminous
        // turned red into xterm's red. It keeps the palette it was given now.
        let mut session = detached();
        session.feed(b"\x1b]4;1;rgb:00/00/00\x07\x1b]104\x07\x1b[31mX");
        let cell = session.snapshot().cell(0, 0).cloned().expect("a cell");
        assert_eq!(cell.foreground, Palette::new().indexed(1));
        session.feed(b"\x1bc\x1b[31mY");
        let cell = session.snapshot().cell(0, 0).cloned().expect("a cell");
        assert_eq!(cell.character, 'Y');
        assert_eq!(cell.foreground, Palette::new().indexed(1));
    }

    #[test]
    fn a_program_asking_for_a_colour_is_told_unluminous_colours() {
        // `OSC 11 ; ?` is how `claude` and `vim` decide whether the ground is light or dark.
        let mut session = detached();
        session.feed(b"\x1b]11;?\x07");
        let background = Palette::new().background;
        let wanted = format!(
            "rgb:{0:02x}{0:02x}/{1:02x}{1:02x}/{2:02x}{2:02x}",
            background.r, background.g, background.b
        );
        let sent = session.sent_to_a_detached_session();
        assert!(sent.contains(&wanted), "the answer names the editor's ground: {sent:?}");
    }

    #[test]
    fn a_click_without_a_drag_selects_nothing() {
        // A click begins a selection and must not select the cell it landed on, or `Ctrl+C` after a click
        // would copy one letter instead of interrupting the program.
        let mut session = detached();
        session.feed(b"some words");
        session.begin_selection(0, 2, SelectionKind::Simple);
        assert!(session.selected_text().is_none());
        assert!(session.snapshot().selection.is_none());
    }

    #[test]
    fn the_variables_a_program_needs_to_know_its_terminal_are_set_and_the_caller_wins() {
        let env = environment(&[("TERM".to_owned(), "dumb".to_owned())]);
        let value =
            |name: &str| env.iter().rev().find(|(given, _)| given == name).map(|(_, v)| v.clone());
        assert_eq!(value("TERM").as_deref(), Some("dumb"), "the caller's value is used");
        assert_eq!(env.iter().filter(|(name, _)| name == "TERM").count(), 1, "and only once");
        assert_eq!(value("COLORTERM").as_deref(), Some("truecolor"));
        assert_eq!(value("TERM_PROGRAM").as_deref(), Some("Unluminous"));
    }

    #[test]
    fn a_variable_in_the_settings_reaches_the_program() {
        // The whole of what `SessionSettings::env` is for: a run configuration's `PORT=3000` has to
        // be in the child's environment. Asked of the program itself rather than of Unluminous, by
        // having it print the variable back.
        let (shell_flag, command) = if cfg!(target_os = "windows") {
            ("/c", "echo [%UNLUMINOUS_RUN_TEST%]".to_owned())
        } else {
            ("-c", "echo \"[$UNLUMINOUS_RUN_TEST]\"".to_owned())
        };
        let settings = SessionSettings {
            shell: Some(test_shell()),
            args: vec![shell_flag.to_owned(), command],
            working_directory: None,
            env: vec![("UNLUMINOUS_RUN_TEST".to_owned(), "it-arrived".to_owned())],
            name: None,
        };
        let waker: Waker = Arc::new(|| {});
        let mut session =
            Session::spawn(&settings, Size::new(8, 60), waker).expect("start the program");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            session.pump();
            if session.snapshot().contains("[it-arrived]") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the variable did not reach the program, the screen holds {:?}",
                session.snapshot().text()
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }

    #[test]
    fn what_is_in_the_environment_is_named_but_never_printed() {
        // A run configuration can carry a token and Agent-Tasks puts an authentication key here, so a `{:?}` of
        // these settings must not be a way to read one.
        let settings = SessionSettings {
            shell: Some("/bin/sh".to_owned()),
            env: vec![
                ("ANTHROPIC_API_KEY".to_owned(), "sk-do-not-print-me".to_owned()),
                ("PORT".to_owned(), "3000".to_owned()),
            ],
            ..SessionSettings::default()
        };
        let printed = format!("{settings:?}");
        assert!(printed.contains("ANTHROPIC_API_KEY"), "the name is there to read: {printed}");
        assert!(printed.contains("PORT"), "every name is there: {printed}");
        assert!(!printed.contains("sk-do-not-print-me"), "the value is not: {printed}");
        assert!(!printed.contains("3000"), "no value is, secret or not: {printed}");
    }

    #[test]
    fn the_environment_unluminous_started_with_is_still_there_underneath() {
        // **The child writes a file, and the test reads the file.** `task-1922`: this used to read
        // the marker off the pseudoconsole's screen, and it failed three releases in a row on a
        // loaded machine with the screen holding `""` and the program *ended with 0* -- the shell
        // ran, and what it wrote never arrived. That is the shape `run_configurations` already
        // measured about `cmd /c echo something`: a program that writes and exits inside a
        // millisecond can lose its output to the console host, which is why
        // `a_program_that_prints_and_stops_leaves_what_it_printed_in_its_tab` exists at all.
        //
        // Nothing about a pseudoconsole is what this test is about. What it is about is that
        // `Session::spawn` lays `settings.env` **over** the inherited environment rather than
        // replacing it -- a child with no `PATH` is a bug nobody enjoys finding, and it is the one
        // thing this could get wrong silently, because the program would still start: Unluminous
        // names it in full. A file says what the child really saw, with no timing in the answer.
        // **The variable is one this process really has, and that is checked before the child is
        // started.** `task-1922`: this named `windir`, and it stopped five releases in a row -- not
        // because anything was wrong with the inheritance, but because the PowerShell that
        // `tools/release.ps1` runs in here has a 75 entry environment built by npm with no `windir`
        // in it at all, while the Git Bash beside it has 80 and does have one. So the test passed
        // from one shell and failed from the other, and what it was reporting as a fault in the
        // code was a fact about the machine. `SystemRoot` is the one Windows always carries.
        let name = if cfg!(target_os = "windows") { "SystemRoot" } else { "HOME" };
        let here = std::env::var(name).unwrap_or_else(|_| {
            panic!("this process has no {name} of its own, so there is nothing to inherit")
        });
        let folder = std::env::temp_dir().join("unluminous-terminal-tests").join("inherited");
        std::fs::create_dir_all(&folder).expect("make the folder");
        // Named after this process, because two runs of this binary at once would otherwise read
        // and delete one another's answer -- which is the fault `sample_folder`'s `OnceLock` exists
        // for in the screenshot tests, in a smaller shape.
        let wrote = folder.join(format!("seen-{}.txt", std::process::id()));
        std::fs::remove_file(&wrote).ok();
        // Written whatever the variable holds, and in brackets, so an empty answer and a missing
        // one are told apart: `cmd` echoes `%windir%` back literally when it is not defined.
        let (shell_flag, command) = if cfg!(target_os = "windows") {
            // **No quotes round the path**, and the folder is one with no space in it. `cmd /c`
            // strips the outer quotes of the whole command only while there are no others inside
            // it; with a quoted path in the middle it reads the lot as a program name and exits 1,
            // which is exactly what it did.
            ("/c", format!("echo [%{name}%]>{}", wrote.display()))
        } else {
            ("-c", format!("echo \"[${name}]\" > '{}'", wrote.display()))
        };
        let settings = SessionSettings {
            shell: Some(test_shell()),
            args: vec![shell_flag.to_owned(), command],
            working_directory: None,
            env: vec![("UNLUMINOUS_RUN_TEST".to_owned(), "one".to_owned())],
            name: None,
        };
        let waker: Waker = Arc::new(|| {});
        let mut session =
            Session::spawn(&settings, Size::new(8, 60), waker).expect("start the program");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let said = loop {
            session.pump();
            if let Ok(text) = std::fs::read_to_string(&wrote) {
                let text = text.trim().to_owned();
                if !text.is_empty() {
                    break text;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the program never wrote the file, and it has {}",
                match session.exit_code() {
                    Some(code) => format!("ended with {code}"),
                    None => "not ended".to_owned(),
                }
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        };
        std::fs::remove_file(&wrote).ok();
        assert_eq!(
            said,
            format!("[{here}]"),
            "the child should have inherited this process's own {name}"
        );
    }

    #[test]
    fn resizing_after_a_program_has_ended_keeps_what_it_wrote() {
        // The fault `task-1683` found, and the one that made a run tab come up empty: the tile
        // resizes its session on the first frame it draws, and a session is spawned at a guessed
        // size, so a program that writes and exits inside a millisecond was **always** resized
        // after it had gone — and `ResizePseudoConsole` re-renders the console host's buffer, which
        // by then is empty.
        let message = "unluminous-resize-after-exit";
        let (flag, command) = if cfg!(target_os = "windows") {
            ("/c", format!("echo {message}"))
        } else {
            ("-c", format!("echo {message}"))
        };
        let settings = SessionSettings {
            shell: Some(test_shell()),
            args: vec![flag.to_owned(), command],
            ..SessionSettings::default()
        };
        let mut session = run_to_the_end(settings);
        assert!(session.snapshot().contains(message), "it is there to begin with");
        // Resizing it to the size it already has is not a resize, and must not be one here either.
        session.resize(session.size());
        // The tile measuring itself, one frame later.
        session.resize(Size::new(24, 120));
        assert!(
            session.snapshot().contains(message),
            "and still there after the resize, which is the whole point of a tab that outlives its \
             program; the screen holds {:?}",
            session.snapshot().text()
        );
        assert_eq!(session.size().columns, 120, "and the emulator did take the new shape");
    }

    #[test]
    fn a_program_that_is_killed_stops_and_keeps_what_it_wrote() {
        // The hard half of `task-1683` §5.3. What is being checked is that the grid survives: a run
        // tab goes on showing the output of a program that is no longer there, which is what makes
        // the evidence outlive the process.
        let settings = SessionSettings { shell: Some(test_shell()), ..SessionSettings::default() };
        let waker: Waker = Arc::new(|| {});
        let mut session =
            Session::spawn(&settings, Size::new(8, 60), waker).expect("start a shell");
        session.send(b"echo unluminous-run-kill-test\r".to_vec());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            session.pump();
            if session.snapshot().contains("unluminous-run-kill-test") {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "the shell did not answer");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        session.kill();
        assert!(!session.is_running(), "killing it stops it at once rather than on a later pump");
        assert_eq!(session.exit_code(), None, "a program Unluminous killed chose no code");
        assert!(
            session.snapshot().contains("unluminous-run-kill-test"),
            "the output is still there after the program has gone"
        );
        // And killing twice is not a second shutdown down a channel that has gone.
        session.kill();
        assert!(!session.is_running());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn the_default_shell_is_powershell_rather_than_the_batch_interpreter() {
        // `COMSPEC` says `cmd.exe` on every Windows there is, so reading it meant Unluminous's terminal never
        // held the commands in a person's PowerShell profile — `task-1670`. Either PowerShell counts:
        // which one depends on whether the newer one is installed on the machine running the test.
        let shell = default_shell().to_lowercase();
        assert!(
            shell.ends_with("powershell.exe") || shell.ends_with("pwsh.exe"),
            "the default shell on Windows should be PowerShell, and is {shell:?}"
        );
    }

    /// The prefix Windows puts on a canonical path, which `cmd.exe` reads as a network share.
    #[cfg(target_os = "windows")]
    const VERBATIM: &str = r"\\?\";

    #[test]
    #[cfg(target_os = "windows")]
    fn a_shell_starts_in_the_folder_it_was_given_even_when_the_path_is_verbatim() {
        // `task-1670`: the folder reached the terminal as `\\?\C:\jason\dev\unluminous`, because that is what
        // `canonicalize` gives back on Windows and it had been written into the recent projects file.
        // `cmd.exe` reads the two leading backslashes as a network share, says so, and starts in
        // `C:\Windows` instead. So this asks for the folder in exactly that form and checks the shell
        // came up in it.
        let folder = std::env::temp_dir().join("unluminous-verbatim-working-directory");
        std::fs::create_dir_all(&folder).expect("make the folder");
        let verbatim = std::fs::canonicalize(&folder).expect("resolve the folder");
        assert!(
            verbatim.to_string_lossy().starts_with(r"\\?\"),
            "this test is only worth anything while `canonicalize` gives back a verbatim path, and it gave {verbatim:?}"
        );

        let settings = SessionSettings {
            shell: Some(test_shell()),
            working_directory: Some(verbatim.clone()),
            ..SessionSettings::default()
        };
        let waker: Waker = Arc::new(|| {});
        let mut session =
            Session::spawn(&settings, Size::new(12, 100), waker).expect("start a shell");
        // The shell's own answer to where it is standing, rather than Unluminous's. In brackets, because
        // when `cmd` refuses the folder it prints the path it was given as part of the complaint — so
        // the path appearing somewhere on the screen proves nothing, and only the path in brackets is
        // the shell saying it started there.
        session.send(b"echo [%CD%]\r".to_vec());

        // **The canonical form, not the one `temp_dir` gave.** `task-1922`: on a GitHub Windows
        // runner `TEMP` is the 8.3 short name ending `RUNNER~1`, and the shell prints the long
        // one -- so the two spellings of one folder never matched, and the test waited out its
        // thirty seconds against a screen that was saying exactly the right thing. `canonicalize`
        // resolves a short name to the long one, which is what `%CD%` prints.
        let plain = verbatim.to_string_lossy().trim_start_matches(VERBATIM).to_owned();
        let wanted = format!("[{plain}]").to_lowercase();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            session.pump();
            let screen = session.snapshot().text().to_lowercase();
            assert!(
                !screen.contains("unc paths are not supported"),
                "the shell refused the folder it was given: {screen}"
            );
            // **Read with the line breaks the screen added taken out (`task-2364`).** A path longer than
            // the grid is wide wraps onto the next row, and `tools/test.mjs` gives every run a temporary
            // folder of its own, a few dozen characters deeper than `%TEMP%`, so the answer the shell
            // gave was on the screen split across two rows and the test waited out its thirty seconds.
            if screen.contains(&wanted) || screen.replace('\n', "").contains(&wanted) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the shell did not say where it was in thirty seconds, the screen holds {screen:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}
