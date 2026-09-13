//! The plugins: what one is, where they come from, and which one claims a file.
//!
//! ## A plugin is data, not code
//!
//! `task-1649` asks for plugins that give a file type an icon, identify its keywords, function
//! names, imports and comments, and supply a theme. That is a **description of a language**, not a
//! program. So a plugin is a folder holding a manifest, an icon and the words that make up a
//! language, and loading one is reading a file. Nothing is executed.
//!
//! The two alternatives were considered and both are the right answer to a question this is not
//! asking yet.
//!
//! A **dynamic library** would let a plugin run arbitrary Rust. It also means an unstable interface
//! across a `dlopen` boundary — a Rust structure passed over one is undefined behaviour unless both
//! sides were built by the same compiler with the same flags — so every plugin would have to be
//! rebuilt for every release of Unluminous, and a plugin that crashes takes the editor with it. For
//! "colour these keywords" that is a great deal of risk bought for nothing.
//!
//! **WebAssembly** answers both of those and costs a runtime, plus a host interface that has to be
//! designed, versioned and documented before the first plugin can be written. It is the right answer
//! the day a plugin wants to *do* something: run a formatter, talk to a language server, add a tool
//! window.
//!
//! So the seam is named now and left empty. `plugin.kind` is read and checked, and a manifest saying
//! anything but `language` is refused with a message rather than half-loaded. That is the line a
//! later version widens.
//!
//! ## A language that has a picture
//!
//! `task-1660` asks for a Mermaid plugin, and Mermaid **is** a language: it has keywords, comments,
//! strings and a file extension, and colouring `.mmd` source is worth having on its own. So it is an
//! ordinary `language` plugin, and the seam above stays exactly where it was.
//!
//! It carries one new key, `language.renders`, which names a renderer that is **built into Unluminous**.
//! Nothing is loaded from the plugin and nothing is executed: the manifest is data saying "files of
//! this language have a picture, and this is which picture", and the code that draws it shipped with
//! the binary. The value is checked against [`RENDERERS`], and a manifest naming one this version
//! does not have is refused with a message — the same rule `plugin.kind` already keeps.
//!
//! What it buys is that switching the plugin off actually withdraws the feature: the window asks
//! [`Plugins::renders`] before it draws a diagram anywhere, so `.mmd` files stop being drawn and
//! mermaid blocks in Markdown go back to being code, in the same frame.
//!
//! ## The manifest
//!
//! `plugin.conf`, in the same `name = value` format the settings file already uses, read by the same
//! [`crate::services::store::Values`]. No new dependency, and a plugin can be read and corrected in
//! a text editor, which is fitting in a text editor.
//!
//! ## The folder
//!
//! `task-1922` split this file into one folder: `types.rs` is the data model, `registries.rs` is
//! the lists of names built into Unluminous that a manifest may point at, `manifest.rs` reads a
//! `plugin.conf` into a `Plugin` and refuses one that asks for something this version cannot do,
//! `theme.rs` reads a `plugin.kind = theme` manifest's own groups, `grammar.rs` is which grammar
//! reads which file, `store.rs` is [`Plugins`] itself — loading, installing, switching on and off —
//! and `bundled.rs` is the plugins that ship inside the binary. Every path that was public before
//! the split is re-exported here under the same name, so nothing outside this folder changed.

mod grammar;
mod manifest;
mod registries;
mod store;
mod theme;
mod types;

pub mod bundled;

pub use grammar::{scheme_of, Grammars};
pub use manifest::{colour, parse};
pub use registries::{
    CHROME, DEBUGGERS, ICON_SETS, PANE_CONDITIONS, PANE_ICONS, PROJECT_RUNNERS, RENDERERS,
    UI_PROVIDERS,
};
pub use store::{Plugins, FOLDER};
pub use types::{
    Contributions, Kind, MenuContribution, MenuItem, PageContribution, PaneContribution, Plugin,
    RailGroup, Surface, Surfaces, SyntaxTheme, TabContribution,
};
