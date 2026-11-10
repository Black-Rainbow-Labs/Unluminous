//! The terminal behind Unluminous's bottom tile.
//!
//! This crate has no user interface dependencies, for the same reason `unluminous-core` has none: its tests run
//! with no window and no graphics card, and the parts most likely to be wrong, which are the key encoding
//! and the colour palette, are then plain functions with plain expected values.
//!
//! What is here and what is not is set out in `tasks/unluminous-terminal-tdd.md`, and what changed when Termoil
//! took over in `tasks/task-2489-termoil-tdd.md`. In short: the escape sequence emulation, the pseudoterminal and
//! the key and mouse encoding come from Termoil (`termoil-vt`, `termoil-pty`, `termoil-input`), and the palette,
//! the screen the painter reads, the replay of a screen and the tabs are written here. Termoil puts a session's
//! program in a job object on Windows and a process group on Unix, so killing a session ends everything it
//! started; that used to be this crate's `reap` module.
//!
//! - [`foreground`] answers what program is running in a terminal, as opposed to what it was started with.
//! - [`keys`] turns a key press into the bytes a terminal sends.
//! - [`mouse`] turns a click or a turn of the wheel into the bytes a program that asked for them expects.
//! - [`palette`] turns a terminal colour into red, green and blue.
//! - [`panes`] splits one terminal into several panes, the way Termoil splits a tab (`task-2597`).
//! - [`paths`] takes the verbatim prefix off a Windows path, which is the one form a shell cannot start in.
//! - [`reported`] reads the sequence a shell reports its own folder with, which is the one answer
//!   PowerShell has: `Set-Location` never moves the process's current directory.
//! - [`replay`] turns a screen into the bytes that would draw it, so a terminal can come back showing it.
//! - [`screen`] is the snapshot the painter reads: plain data, no locks held.
//! - [`session`] is one terminal: a shell in a pseudoterminal, and the emulator behind it.
//! - [`tabs`] is several sessions with one of them showing.
//! - [`themes`] is Termoil's 894 color themes, copied from Termoil (`task-2597`).

pub mod foreground;
pub mod keys;
pub mod mouse;
pub mod palette;
pub mod panes;
pub mod paths;
pub mod replay;
pub mod reported;
pub mod screen;
pub mod session;
pub mod sprite;
pub mod tabs;
pub mod themes;

pub use keys::{encode, KeyPress, Mode};
pub use mouse::MouseMode;
pub use palette::{Palette, Rgb};
pub use screen::{Cursor, CursorShape, Screen, ScreenCell};
pub use session::{Session, SessionSettings, Size, Waker, SCROLLBACK};
pub use tabs::Tabs;
