//! What every test file beside this one is built from: the shared graphics devices, the harness,
//! the fixtures, and the four ways of running a command line against a real window.
//!
//! `tests/common/mod.rs` is Rust's own shape for a module inside `tests/` that is not a test binary
//! of its own, so it is compiled into each of the twelve files beside it rather than run as a
//! thirteenth.
//!
//! **The device pool is one pool per binary, and that is still the answer `task-1654` needs.**
//! [`DEVICES`] devices are built on the first call and shared by every harness in that binary; what
//! `task-1654` measured is what happens without that, which is a device built and torn down per
//! harness across as many threads as the machine has, killing the process with an access violation
//! on about one run in nine. Splitting `screenshots.rs` into twelve files makes twelve pools rather
//! than one — and `cargo test` runs one test binary at a time, so eight devices are alive at once
//! exactly as they were, and each binary pays for its own pool once.
//!
//! Everything here is `pub` because it is a module, and the module allows dead code because each
//! binary uses a different part of it: a helper that five files want is unused in the other seven,
//! and a warning about that would be a warning about the split rather than about the code.
#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use eframe::egui_wgpu::{RenderState, Renderer, RendererOptions};
use egui::epaint::mutex::RwLock;
use egui::{vec2, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotResults};
use unluminous_app::app::actions::Action;
use unluminous_app::components::title_bar::MenuPlacement;
use unluminous_app::services::run_configurations::Configuration;
use unluminous_app::UnluminousApp;
use unluminous_core::Command;

pub const WINDOW: [f32; 2] = [1180.0, 740.0];

/// How many graphics devices the ninety one screenshots share between them.
///
/// One would do, and one is what the first fix for `task-1654` used, but a single device made the
/// run four times slower — 27 seconds against 7 — because every test's renderer has a shader to
/// compile and a pipeline to build, and on one device those queue up behind each other. A handful of
/// devices gives the tests somewhere to spread out while still being a fixed number built once, which
/// is the part that matters. Measured on this machine: ninety one devices 7.00 s, one device 26.77 s,
/// eight devices 5.97 s — so this is quicker than what it replaces as well as safer.
pub const DEVICES: usize = 8;

/// A graphics device for one harness, taken from the small set the whole test binary shares.
///
/// `egui_kittest`'s `.wgpu()` builds a **new** graphics instance, adapter and device for each
/// harness. There are ninety one tests here and the test runner gives each one a thread, so that was
/// ninety one devices built and torn down across thirty two threads inside eight seconds, with the
/// Vulkan loader, both vendors' drivers, the Direct3D runtime and the software rasteriser loading and
/// unloading underneath. `task-1654` is what that cost: the process died of an access violation on
/// about one run in nine, part way through the run — eight tests in, once — while every test that had
/// finished said `ok`.
///
/// Every test wants the same thing, a device to draw the window into and read the pixels back, so
/// [`DEVICES`] of them are built on the first call and handed out in turn from there. Nothing is ever
/// torn down until the process ends, which is what removes the fault. The adapter is still chosen by
/// `egui_kittest`'s own selector, so which card draws the screenshots has not changed and neither
/// have the accepted images.
///
/// Each harness still gets a **renderer** of its own, which is what keeps the tests independent: the
/// font atlas and the textures a test uploads belong to that test and are freed with it.
pub fn shared_render_state() -> RenderState {
    static SHARED: OnceLock<Vec<RenderState>> = OnceLock::new();
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let shared = SHARED.get_or_init(|| {
        (0..DEVICES)
            .map(|_| {
                egui_kittest::wgpu::create_render_state(
                    egui_kittest::wgpu::default_wgpu_setup(),
                    RendererOptions::PREDICTABLE,
                )
            })
            .collect()
    });
    let mut state = shared[NEXT.fetch_add(1, Ordering::Relaxed) % DEVICES].clone();
    state.renderer = Arc::new(RwLock::new(Renderer::new(
        &state.device,
        state.target_format,
        RendererOptions::PREDICTABLE,
    )));
    state
}

/// A harness builder that draws on a shared device rather than making a device of its own.
///
/// Every harness in this file is built through here rather than through `Harness::builder`, so that a
/// test added later cannot go back to a device of its own without meaning to. See
/// [`shared_render_state`].
pub fn builder<State>() -> egui_kittest::HarnessBuilder<State> {
    egui_kittest::HarnessBuilder::default()
        .renderer(WgpuTestRenderer::from_render_state(shared_render_state()))
}

/// A folder with a nested structure, for the explorer screenshots. Written once and left in place, so
/// that the tree looks the same in every run and the images stay comparable.
///
/// Written once **per run** as well, which it was not before. Most of the tests in this file want this
/// folder, they run at the same time, and every one of them used to rewrite it — so a test could be
/// reading `readme.md` at the moment another test's `File::create` had truncated it and not yet
/// written the bytes back. That is what
/// `clicking_a_file_in_the_explorer_opens_it_in_the_editor` failing with
///
/// ```text
/// assertion `left == right` failed: clicking the file should have loaded it
///   left: ""
///  right: "# Unluminous\n"
/// ```
///
/// was: not a fault in the explorer, a fixture being written out from underneath it. The lock builds
/// the folder once and everyone else waits for it and then reads a file nobody is writing.
pub fn sample_folder() -> std::path::PathBuf {
    static FOLDER: OnceLock<std::path::PathBuf> = OnceLock::new();
    FOLDER.get_or_init(build_sample_folder).clone()
}

/// Write the sample folder out. Called once, through [`sample_folder`].
///
/// **Cleared first**, which is `task-1922`: this only ever added to the folder, so anything an older
/// version of a test had written into it stayed there for ever and appeared in the explorer of every
/// picture taken afterwards. On this machine that was a `space-round-trip` row, left by the test that
/// `a_canvas_comes_back_when_the_project_is_opened_again`'s own comment records moving out of here --
/// the code was fixed and the folder on disk was not, so the fixture said one thing and the machine
/// held another. It is safe to clear because every caller comes through `sample_folder`'s `OnceLock`,
/// so this runs before any test has the path; and it is the rule `git_folder(name)` and
/// `repository(name)` already keep about a fixture a test writes to.
///
/// **The `OnceLock` is per test binary**, so each of the twelve files beside this module builds
/// this folder once. That is still one writer, because `cargo test` runs one test binary at a
/// time; two running at once would clear each other's folder, since this begins by removing it.
///
/// **Not [`fixture`], because two of these are not text**: `picture.png` is drawn pixel by pixel
/// and `bundle.zip` is written as bytes.
pub fn build_sample_folder() -> std::path::PathBuf {
    let root = std::env::temp_dir().join("unluminous-screenshot-folder");
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(root.join("chapters/appendix")).expect("make the nested folders");
    std::fs::create_dir_all(root.join("drafts")).expect("make the drafts folder");
    std::fs::write(root.join("readme.md"), "# Unluminous\n").expect("write readme.md");
    std::fs::write(root.join("notes.txt"), "notes\n").expect("write notes.txt");
    std::fs::write(root.join("chapters/one.md"), "# One\n").expect("write chapters/one.md");
    std::fs::write(root.join("chapters/two.md"), "# Two\n").expect("write chapters/two.md");
    std::fs::write(root.join("chapters/appendix/tables.txt"), "tables\n")
        .expect("write the deep file");
    std::fs::write(root.join("drafts/idea.md"), "an idea\n").expect("write drafts/idea.md");
    // A file Unluminous has no special handling for. It opens as plain text, which is what
    // `tasks/improvements.md` asks for.
    std::fs::write(root.join("program.rs"), "fn main() {}\n").expect("write program.rs");
    // A real picture. It is not text, and since `task-1658` it opens all the same, in a tab that shows
    // it. Written rather than checked in, so the tests carry no binary fixture, and drawn as a plain
    // gradient with a band across it so that a screenshot of it is obviously the picture and obviously
    // the right way up.
    write_sample_picture(&root.join("picture.png"));
    // A file that is neither text nor a picture. It is listed, dimmed, and does not respond to a click.
    // The bytes are the start of a zip, including the zero byte that says it is not text.
    std::fs::write(root.join("bundle.zip"), [0x50, 0x4B, 0x03, 0x04, 0]).expect("write bundle.zip");
    root
}

/// Write a small PNG for the explorer's picture row and the picture tab to show.
///
/// A hundred and sixty by a hundred, which is smaller than the editing area, so the tab shows it at its
/// own size and a test that zooms has somewhere to go. A blue to green gradient with a lighter band
/// across the top third, so that a person looking at the screenshot can see at a glance that it is the
/// right picture, the right way up and the right size.
pub fn write_sample_picture(path: &std::path::Path) {
    let (width, height) = (160_u32, 100_u32);
    let mut picture = image::RgbaImage::new(width, height);
    for (x, y, pixel) in picture.enumerate_pixels_mut() {
        let across = x as f32 / width as f32;
        let down = y as f32 / height as f32;
        let band = if (0.30..0.42).contains(&down) { 70 } else { 0 };
        *pixel = image::Rgba([
            (0x28 as f32 + across * 40.0) as u8 + band,
            (0x60 as f32 + down * 90.0) as u8 + band,
            (0xF0 as f32 - across * 110.0) as u8,
            255,
        ]);
    }
    picture.save(path).expect("write picture.png");
}

/// Build the application with `text` already in the document.
pub fn harness(text: &str) -> Harness<'static, UnluminousApp> {
    let folder = sample_folder();
    let text = text.to_owned();
    let mut harness = builder().with_size(vec2(WINDOW[0], WINDOW[1])).build_eframe(move |cc| {
        let mut app = UnluminousApp::with_text(folder, &text);
        // The same setup the released binary does, and for the same reason: the fonts have to be
        // installed before the first frame.
        app.prepare(&cc.egui_ctx);
        // A plugin's decoration is rasterised on the processor, and `vello_cpu` picks the widest SIMD it
        // has. Pinned here so an accepted image is a property of the code rather than of the machine that
        // took it — the same reason the terminal's screenshots feed fixed bytes to a session with no shell.
        app.draw_deterministically();
        app
    });
    harness.run();
    harness
}

/// Build the application on a folder of its own, for a test that needs a second window.
pub fn harness_in(folder: &std::path::Path) -> Harness<'static, UnluminousApp> {
    let folder = folder.to_path_buf();
    let mut harness = builder().with_size(vec2(WINDOW[0], WINDOW[1])).build_eframe(move |cc| {
        let mut app = UnluminousApp::new(folder);
        app.prepare(&cc.egui_ctx);
        // A plugin's decoration is rasterised on the processor, and `vello_cpu` picks the widest SIMD it
        // has. Pinned here so an accepted image is a property of the code rather than of the machine that
        // took it — the same reason the terminal's screenshots feed fixed bytes to a session with no shell.
        app.draw_deterministically();
        app
    });
    harness.run();
    harness
}

/// Select `range` and run `command`, then let the application settle.
pub fn select_and(
    harness: &mut Harness<'static, UnluminousApp>,
    range: std::ops::Range<usize>,
    commands: &[Command],
) {
    harness.state_mut().command(Command::PlaceCaret { offset: range.start, extend: false });
    harness.state_mut().command(Command::PlaceCaret { offset: range.end, extend: true });
    for command in commands {
        harness.state_mut().command(command.clone());
    }
    harness.run();
}

/// Select the first occurrence of `phrase` and run `commands` on it.
///
/// The offsets are found in the text rather than written down, because a hand counted offset drifts as
/// soon as the text is edited. An earlier version of these tests counted wrongly and left the first
/// letter of a line out of the selection, which the screenshot showed as one small letter in front of a
/// large word.
pub fn select_phrase(
    harness: &mut Harness<'static, UnluminousApp>,
    phrase: &str,
    commands: &[Command],
) {
    let text = harness.state().document().text().to_string();
    let start = text
        .find(phrase)
        .unwrap_or_else(|| panic!("{phrase:?} is not in the document, which holds {text:?}"));
    select_and(harness, start..start + phrase.len(), commands);
}

/// Open the panel behind the toolbar's `F` button, which is where the formatting controls live.
///
/// `task-1657` moved them there: bold, the colours, the alignments and the line spacings are all one
/// click further away than they were, and a test that wants to press one presses this first. The
/// names did not change, so what a test asks for afterwards is what it always asked for.
pub fn open_text_options(harness: &mut Harness<'static, UnluminousApp>) {
    harness.get_by_label("Text options").click();
    harness.run();
}

/// Put the caret at the start with nothing selected, so that a screenshot shows the formatting rather
/// than a selection highlight sitting on top of it.
pub fn collapse(harness: &mut Harness<'static, UnluminousApp>) {
    harness.state_mut().command(Command::MoveDocumentStart { extend: false });
    harness.run();
}

/// Where the accepted image for `name` lives on the platform the test is running on.
///
/// The window is deliberately not the same on both platforms. macOS puts the menus in the bar along the
/// top of the screen and the window buttons at the left; Windows draws the menus in Unluminous's own title bar
/// and the buttons at the right. The text is not the same either, because Helvetica is not installed on
/// Windows and the family falls through to Arial. So one set of images cannot be the baseline for both:
/// run against the macOS set, 32 of the 72 differed on Windows for reasons that are the program working
/// exactly as it is meant to.
///
/// Each platform therefore has its own accepted set, and a difference in one really is a change to what
/// Unluminous draws there. macOS keeps the folder it already had, because those images were looked at and
/// accepted by a person and moving them would have said they were new.
pub fn shot(name: &str) -> String {
    if cfg!(target_os = "macos") {
        name.to_owned()
    } else if cfg!(target_os = "windows") {
        format!("windows/{name}")
    } else {
        format!("linux/{name}")
    }
}

/// Whether the contributed pane called `key` is showing.
///
/// **By name rather than by slot**, which is the rule `PluginUi` itself keeps: which slot a pane is in
/// comes from the manifests and moves when a plugin is switched on or off.
pub fn showing(harness: &Harness<'static, UnluminousApp>, key: &str) -> bool {
    harness
        .state()
        .plugin_ui
        .slot_of(key)
        .is_some_and(|slot| harness.state().plugin_ui.is_visible(slot))
}

/// Report the snapshots taken by a test that builds more than one window.
///
/// The harness requires every snapshot result in one test to be collected together, so that a run with
/// `UPDATE_SNAPSHOTS=1` updates all of them instead of stopping at the first difference. Taking the
/// errors out of the collection is also what marks it as handled, which `unwrap` does not do.
#[track_caller]
pub fn report(results: SnapshotResults) {
    let errors = results.into_inner();
    assert!(errors.is_empty(), "snapshot differences: {errors:#?}");
}

/// Copy a folder to a place that is not inside any git repository.
///
/// A window looks for a repository the moment it opens, and what it finds goes in the status bar and
/// tints the explorer. A test folder that lives inside Unluminous's own repository therefore draws
/// something different depending on what is uncommitted at the time, which is not a difference in
/// Unluminous.
pub fn copy_out_of_the_repository(source: &std::path::Path, name: &str) -> std::path::PathBuf {
    let target = std::env::temp_dir().join(name);
    std::fs::remove_dir_all(&target).ok();
    fn walk(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).expect("make the folder");
        for entry in std::fs::read_dir(from).expect("read the folder").flatten() {
            let path = entry.path();
            let into = to.join(entry.file_name());
            if path.is_dir() {
                walk(&path, &into);
            } else {
                std::fs::copy(&path, &into).expect("copy the file");
            }
        }
    }
    walk(source, &target);
    target
}

/// A folder that is a real git repository, for the tests about git.
///
/// Built with its identity and its settings named on the command line, so a test does not depend on
/// the `.gitconfig` of whoever is running it, and with two commits on separated dates so blame has a
/// spread of ages to colour. Rebuilt each time, so the pictures are the same on every run.
///
/// **Not [`fixture`], because the files are only half of it**: this runs the machine's real `git`
/// to make the commits, the second author and the two dates blame colours by.
pub fn git_folder(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join("unluminous-screenshot-repository").join(name);
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("make the folder");
    let git = |arguments: &[&str]| {
        let outcome = unluminous_git::command::run(&root, arguments);
        assert!(outcome.ok, "git {arguments:?}: {}", outcome.message());
    };
    git(&["init", "--initial-branch=main"]);
    for (name, value) in [
        ("user.name", "Unluminous Test"),
        ("user.email", "test@unluminous.invalid"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(&["config", name, value]);
    }
    std::fs::write(root.join("readme.md"), "# a repository\n").expect("write readme.md");
    std::fs::write(
        root.join("sqlClient.ts"),
        "import { createScopedSql } from '../db/sqlClient';\n\n/** Lists messages in a chat. */\nexport class MessageRepository {\n  private sql = createScopedSql();\n}\n",
    )
    .expect("write sqlClient.ts");
    git(&["add", "-A"]);
    git(&["commit", "--date", "2026-01-14T09:00:00+00:00", "-m", "the first commit"]);
    std::fs::write(root.join("version.ts"), "export const version = '0.1.0';\n")
        .expect("write version.ts");
    // The second commit also touches the annotated file, so blame has two authors and two dates in
    // it and the column really shows its gradient rather than one flat colour.
    std::fs::write(
        root.join("sqlClient.ts"),
        "import { createScopedSql } from '../db/sqlClient';\n\n/** Lists messages in a chat. */\nexport class MessageRepository {\n  private sql = createScopedSql();\n\n  /** Deletes every message in a chat. */\n  async deleteByChat(chatId: number) {}\n}\n",
    )
    .expect("change sqlClient.ts");
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.name=Sam Okafor",
        "-c",
        "user.email=sam@example.com",
        "commit",
        "--date",
        "2026-07-21T16:00:00+00:00",
        "-m",
        "add a version",
    ]);
    // A change that has not been committed, and a file git has never seen, so the commit panel and
    // the gutter's change bars both have something to show.
    std::fs::write(root.join("version.ts"), "export const version = '0.2.0';\nconst extra = 1;\n")
        .expect("change version.ts");
    std::fs::write(root.join("notes.txt"), "scratch\n").expect("write notes.txt");
    root
}

/// Draw the window while a loop above waits for something a thread is still working on.
///
/// A polling loop cannot use `Harness::run`. That gives the window four steps to go quiet and panics
/// if it has not — the right budget for a settled window, and the wrong one here, because while git
/// is still running or a picture is still being decoded the window is *meant* to keep asking to be
/// drawn, and on a loaded machine it can ask for longer than four steps. Under a debugger, which
/// slows the run by about two and a half times, that is exactly how
/// `every_git_operation_can_be_driven_from_the_window` failed:
///
/// ```text
/// Harness::run exceeded max_steps (4). Repaint causes: []
/// ```
///
/// The waiting is what the loop is for, so running out of steps inside one attempt is not a failure.
/// Running out of *attempts* is, and the caller says so.
pub fn pump(harness: &mut Harness<'static, UnluminousApp>) {
    let _ = harness.try_run();
}

/// A window on a real repository, with the repository already read.
///
/// The window looks for a repository on its first frame, and reading it happens on a thread, so the
/// harness is run several times to let the answer arrive before the picture is taken. Each run is a
/// frame; nothing here waits on a clock, so the test is the same on every machine.
pub fn git_harness(name: &str) -> Harness<'static, UnluminousApp> {
    let folder = git_folder(name);
    let mut harness = harness_in(&folder);
    for _ in 0..600 {
        pump(&mut harness);
        if harness.state().git.as_ref().is_some_and(|git| !git.snapshot.status.entries.is_empty()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    harness.run();
    harness
}

// The three view modes, and the Markdown preview behind them.

/// A document with one of everything the parser handles, used by the preview screenshots.
pub const MARKDOWN: &str = "\
# Unluminous preview

A paragraph with **bold**, *italic*, ~~struck~~ and `inline code` in it, wrapped
over two lines of source so that it comes out as one paragraph.

## A smaller heading

- a bullet
- another bullet
  - one nested under it

### A list of things to do

- [x] a box that is ticked
- [ ] one that is not

1. first
2. second

> a quoted line
> > and one quoted inside it

| Crate | Lines | Tests |
| ----- | ----: | :---: |
| core | 9132 | 412 |
| terminal | 3004 | 88 |

```rust
fn main() {
    println!(\"code keeps its spacing\");
}
```

See [the design](https://example.com/design) for more.

---

The last paragraph.";

// The Settings window, which is where the font and the background moved to.

/// Drag from one point to another, which is how a divider between two panes is moved.
///
/// egui has a threshold a pointer has to pass before a press becomes a drag, so this presses, moves and
/// releases over several frames rather than in one.
pub fn drag(harness: &mut Harness<'static, UnluminousApp>, from: egui::Pos2, to: egui::Pos2) {
    let modifiers = Modifiers::default();
    harness.input_mut().events.push(egui::Event::PointerMoved(from));
    harness.run();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    harness.run();
    harness.input_mut().events.push(egui::Event::PointerMoved(to));
    harness.run();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.run();
}

/// Open the About box the way a person does: `Unluminous` in the bar, then `About Unluminous`.
pub fn open_about(harness: &mut Harness<'static, UnluminousApp>) {
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.run();
    harness.get_by_label("Unluminous").click();
    harness.run();
    harness.get_by_label("About Unluminous").click();
    harness.run();
}

/// Open the Settings window the way a person does on Windows: `Edit` in the bar, then `Settings`.
pub fn open_settings(harness: &mut Harness<'static, UnluminousApp>) {
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.run();
    harness.get_by_label("Edit").click();
    harness.run();
    harness.get_by_label("Settings").click();
    harness.run();
}

/// Press twice at `at`, which is what a double click is from inside the window.
///
/// The one beside it presses a control found by its name; this one presses a **point**, which is what a
/// header with no control on it needs.
pub fn double_click_at(harness: &mut Harness<'static, UnluminousApp>, at: egui::Pos2) {
    let modifiers = Modifiers::default();
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            });
        }
    }
    // **`pump`, not `Harness::run`**, which is the third of this file's three rules and the one this
    // helper was breaking. `run` gives the window four steps to go quiet and panics otherwise, and
    // this window never goes quiet on demand: `UnluminousApp::update` asks for a frame every frame
    // through `request_repaint_after(HEARTBEAT)`, and a terminal's waker fires from another thread
    // whenever a shell says anything. Measured on `task-1804`: with a terminal open,
    // `two_presses_on_a_panels_header_fill_the_window_with_it_and_two_more_put_it_back` failed on
    // **two runs in three** with "Harness::run exceeded max_steps (4)", naming exactly those two as
    // the repaint causes.
    //
    // Running out of steps inside one attempt is not a failure; running out of attempts is, and a
    // fixed number of frames is what a press needs — the press is delivered, the frames are drawn,
    // and what the window did is asserted afterwards.
    for _ in 0..8 {
        pump(harness);
    }
}

// The terminal.

/// A window with a terminal open that has no shell behind it, so that what it draws is the same on every
/// run. The bytes a test feeds it go through the same emulator a real shell's output does.
pub fn with_terminal(text: &str, rows: usize, columns: usize) -> Harness<'static, UnluminousApp> {
    let mut harness = harness(text);
    harness.state_mut().new_detached_terminal_tab(rows, columns);
    harness.run();
    harness
}

pub fn feed(harness: &mut Harness<'static, UnluminousApp>, bytes: &[u8]) {
    harness.state_mut().terminal.tabs.active_mut().expect("a terminal tab").feed(bytes);
    harness.run();
}

/// Run the window until `ready` is true, or give up.
///
/// Git runs on a thread, so an answer arrives some frames after it was asked for. Each turn is a
/// frame and a short wait; nothing here depends on how fast the machine is, because it stops as soon
/// as the thing it is waiting for has happened.
///
/// Patient on purpose. The tests run at the same time, each starting real git processes, so a step
/// that takes 40 milliseconds on its own can take several seconds when seven of them are running
/// together — which is what made this fail about one run in five while passing every time on its
/// own. Waiting longer costs nothing when nothing is slow.
#[track_caller]
/// A few frames, without insisting that the window goes quiet.
///
/// `Harness::run` gives the window four steps to settle and panics otherwise, which is right for a
/// settled window and wrong while git is still working: the worker thread asks for a repaint whenever a
/// command finishes, so a `run` that happens to land in the middle of one fails for a reason that is not
/// a fault in Unluminous. This is what a step between git operations uses instead.
pub fn nudge(harness: &mut Harness<'static, UnluminousApp>) {
    for _ in 0..4 {
        pump(harness);
    }
}

pub fn settle(
    harness: &mut Harness<'static, UnluminousApp>,
    what: &str,
    ready: impl Fn(&UnluminousApp) -> bool,
) {
    for _ in 0..600 {
        pump(harness);
        if ready(harness.state()) {
            pump(harness);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("gave up waiting for {what}");
}

/// What git says about a repository, asked directly rather than through Unluminous.
pub fn ask_git(root: &std::path::Path, arguments: &[&str]) -> String {
    let outcome = unluminous_git::command::run(root, arguments);
    assert!(outcome.ok, "git {arguments:?}: {}", outcome.message());
    outcome.stdout.trim().to_owned()
}

/// Double click a control found by name, which egui has no helper for.
///
/// Two presses and releases in one frame: egui reads a second click as a double click when it comes
/// within its own double click time of the first, and both of these carry the same frame's time.
pub fn double_click(harness: &mut Harness<'static, UnluminousApp>, label: &str) {
    let at = harness.get_by_label(label).rect().center();
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::default(),
            });
        }
    }
    harness.run();
}

/// A configuration, spelled out.
pub fn configuration(name: &str, command: &str) -> Configuration {
    Configuration::new(name, command)
}

// ============================================================================================
// The command line, driven through a real window.
//
// `task-1661`. These go through the whole of the command line path apart from the socket: the
// words are parsed against `unluminous_cli::catalogue`, dispatched by `UnluminousApp::run_cli`, and what is
// checked afterwards is the window's own state — not the reply's opinion of itself. A command that
// says it opened a file and a window with that file open are two different claims, and only the
// second one is worth testing.
//
// No pictures. What a command does to the rendering is already covered by the screenshot tests
// above, which set the same states up by hand; what is unproven, and what these prove, is that the
// command line reaches those states at all.

/// Run a command line against the window and take the reply, insisting it was answered.
pub fn run(
    harness: &mut Harness<'static, UnluminousApp>,
    line: &str,
) -> unluminous_cli::protocol::Reply {
    let ctx = harness.ctx.clone();
    let reply = harness
        .state_mut()
        .run_command_line(line, &ctx)
        .unwrap_or_else(|| panic!("`{line}` was not answered on the frame it was asked"));
    harness.run();
    reply
}

/// The same, insisting it worked.
pub fn did(harness: &mut Harness<'static, UnluminousApp>, line: &str) -> serde_json::Value {
    let reply = run(harness, line);
    assert!(reply.ok, "`{line}` was refused: {}", reply.message);
    reply.result
}

/// The same, for a command that leaves the window asking to be drawn again later.
///
/// A polite stop asks for one frame two seconds hence, so the window has not gone quiet when the
/// reply lands. `Harness::run` gives it four steps to settle and panics otherwise, which is right
/// for a settled window and wrong here — the rule `task-1654` already wrote down about waiting
/// loops, wearing a different hat.
pub fn did_while_waiting(
    harness: &mut Harness<'static, UnluminousApp>,
    line: &str,
) -> serde_json::Value {
    let ctx = harness.ctx.clone();
    let reply = harness
        .state_mut()
        .run_command_line(line, &ctx)
        .unwrap_or_else(|| panic!("`{line}` was not answered on the frame it was asked"));
    harness.step();
    assert!(reply.ok, "`{line}` was refused: {}", reply.message);
    reply.result
}

/// The same, insisting it was refused, and returning the code it was refused with.
pub fn refused(harness: &mut Harness<'static, UnluminousApp>, line: &str) -> String {
    let reply = run(harness, line);
    assert!(!reply.ok, "`{line}` should have been refused, and was not");
    reply.error.expect("a refusal carries an error").code
}

/// Press the right mouse button at a point in the window.
///
/// The one interaction in Unluminous that has to be sent as raw events: `kittest` can click a control it
/// can find by name, and the editing area is not a named control — it is the whole surface, and
/// which *point* was pressed is the whole question.
pub fn right_click_at(harness: &mut Harness<'static, UnluminousApp>, at: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(at));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::default(),
        });
    }
    harness.run();
}

// -------------------------------------------------------------------------------------- task-1664
//
// The explorer following the tab, and the editing area split into panes.

/// Run an action the way a menu row does.
pub fn choose(harness: &mut Harness<'static, UnluminousApp>, action: Action) {
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(action, &ctx);
    harness.run();
}

/// The names on offer, in the order the popup is showing them.
pub fn completions(harness: &Harness<'static, UnluminousApp>) -> Vec<String> {
    harness
        .state()
        .completion()
        .map(|state| state.rows.iter().map(|row| row.name.clone()).collect())
        .unwrap_or_default()
}

/// A folder of its own for a test that changes what is in it.
///
/// `draw` is made afterwards because it holds no file, and [`fixture`] is a list of files: an empty
/// folder is a row in the explorer that nothing would write.
pub fn scratch_folder(name: &str) -> std::path::PathBuf {
    let root = fixture(
        &format!("unluminous-1681-{name}"),
        &[
            ("readme.md", "# Notes\n"),
            ("app/main.ts", "import { draw } from './layout';\n"),
            ("app/other.ts", "import { draw } from './layout';\n"),
            ("app/layout.ts", "export function draw() {}\n"),
        ],
    );
    std::fs::create_dir_all(root.join("draw")).expect("make the draw folder");
    root
}

/// A folder holding one real Rust file with blocks worth folding in it.
///
/// A folder of its own rather than an addition to `sample_folder`: that fixture's file count is in
/// the status bar of a dozen accepted screenshots, and a tenth file would change every one of them.
pub fn folding_folder(name: &str) -> std::path::PathBuf {
    fixture(
        &format!("unluminous-screenshot-folding/{name}"),
        &[(
            "source.rs",
            "/// Adds two numbers together.\n\
         /// The second line of the comment.\n\
         fn add(left: usize, right: usize) -> usize {\n\
        \x20   let total = left + right;\n\
        \x20   if total > 100 {\n\
        \x20       return 100;\n\
        \x20   }\n\
        \x20   total\n\
         }\n\
         \n\
         fn subtract(left: usize, right: usize) -> usize {\n\
        \x20   left - right\n\
         }\n\
         \n\
         fn main() {\n\
        \x20   println!(\"{}\", add(1, 2));\n\
        \x20   println!(\"{}\", subtract(4, 3));\n\
         }\n",
        )],
    )
}

/// Open `source.rs` in a window on a folder of its own.
pub fn folding_harness(name: &str) -> Harness<'static, UnluminousApp> {
    let folder = folding_folder(name);
    let mut harness = harness_in(&folder);
    harness.get_by_label_contains("source.rs").click();
    harness.run();
    harness.run();
    harness
}

/// A small JavaScript project, with names a stem of `get` matches so that the list really opens.
pub fn javascript_folder() -> std::path::PathBuf {
    fixture(
        "unluminous-screenshot-javascript",
        &[
            ("person.js", ""),
            (
                "people.js",
                "export function getName(person) { return person.name; }\n\
                 export function getAge(person) { return person.age; }\n\
                 export const getters = { getName, getAge };\n\
                 export class Employee {\n  get title() { return this.role; }\n  \
                 getSalary() { return 0; }\n}\n",
            ),
        ],
    )
}

/// A window on that project with `person.js` open and the symbol index built, because the completion
/// list is built from the index and means nothing until it has arrived.
pub fn javascript_harness() -> Harness<'static, UnluminousApp> {
    let folder = javascript_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("person.js")).expect("the file opens");
    for _ in 0..600 {
        pump(&mut harness);
        if harness.state().symbols_indexer().is_some_and(|indexer| !indexer.is_building()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    harness.run();
    harness
}

/// Everything the session has asked for since this was last called, and forget it.
pub fn asked(harness: &mut Harness<'static, UnluminousApp>) -> Vec<serde_json::Value> {
    harness.state_mut().debug.as_mut().expect("a session").requested()
}

/// Type `text` a character at a time, painting every frame, and walking the completion list whenever
/// it opens.
///
/// Painting is the point. `Harness::run` builds a frame's shapes; `render` is what turns them into a
/// picture, and the completion list's own drawing — a row's matched letters picked out one at a time —
/// only happens there.
pub fn type_and_paint(harness: &mut Harness<'static, UnluminousApp>, text: &str) {
    for letter in text.chars() {
        if letter == '\n' {
            harness.key_press(egui::Key::Enter);
        } else {
            harness.input_mut().events.push(egui::Event::Text(letter.to_string()));
        }
        harness.run();
        harness.render().expect("paint the frame");
        if harness.state().completion().is_some() {
            harness.key_press(egui::Key::ArrowDown);
            harness.run();
            harness.render().expect("paint the frame with a row chosen");
        }
    }
}

pub fn side_of(
    harness: &Harness<'static, UnluminousApp>,
    panel: unluminous_app::app::dock::Panel,
) -> unluminous_app::app::dock::Side {
    harness.state().panes.dock.side_of(panel)
}

// ---------------------------------------------------------------------------- the Agent-Chat plugin
//
// `task-1767`: a pane docked to the right, opened from a button in the rail, that streams an answer
// from a model. `tasks/task-1767-agent-chat-tdd.md` is the design and
// `_agent_output/task-1767-agent-chat/reference-chat.png` is the picture it is measured against.
//
// **No test here makes a network request or starts an agent.** A conversation is built out of the
// same `Reply` values the transport would have produced, which is the terminal's own rule — its
// screenshot tests feed fixed bytes to a session with no shell behind it, because when a real one
// answers is not something a test can know. `unluminous-chat`'s own tests drive the whole client against a
// scripted server on loopback, and its `agent` tests assert on the command line that *would* be run.
//
// The two tests that really call `send` therefore point the chosen row at [`NO_SUCH_AGENT`] first.
// That matters more than it used to: the rows that ship run `claude` and `codex`, and **both are
// installed on the machine this is developed on**, so a test that sent would spawn a real agent, bill
// a real account and never settle. Both of those tests failed exactly that way when the transport
// changed, which is how the rule came to be written down here.

/// A program nothing on any machine answers to, so a send is refused before anything is spawned.
pub const NO_SUCH_AGENT: &str = "unluminous-no-such-agent-anywhere";

/// Ask for input the way `unluminous-cli input` does, and feed the frames it queued.
///
/// **The command holds until its frames have been drawn**, and a harness has no control channel to be
/// answered on — so what this does is what the window does: run the command, then hand each step to
/// `RawInput` before a pass, which is `Harness::input_mut`. See `UnluminousApp::take_the_next_input_frame`
/// for why the harness cannot use `raw_input_hook` itself.
pub fn drove(harness: &mut Harness<'static, UnluminousApp>, line: &str) {
    let ctx = harness.ctx.clone();
    let answered = harness.state_mut().run_command_line(line, &ctx);
    assert!(answered.is_none(), "`{line}` should hold until its frames are drawn");
    // Bounded, because a gesture that never drained would otherwise hang the test rather than fail it.
    for _ in 0..600 {
        let Some(events) = harness.state_mut().take_the_next_input_frame() else { break };
        harness.input_mut().events.extend(events);
        harness.step();
    }
    // One more, so what the input did has been drawn — the same settling `Waiting::Input` does.
    harness.step();
}

/// A folder under the temporary directory holding `files`, written once per test binary.
///
/// This is the eleven near-identical `<feature>_folder` functions `task-1922` found, which each
/// made a folder, wrote a handful of files into it and handed the path back. What is left of each of
/// them is the list of files, which is the only part that was ever different. The seven it could
/// not replace say so in a line of their own.
///
/// **The name is a parameter because it is on the screen.** A window's title bar names the project
/// and the explorer's heading repeats it, so the folder's own name is in every accepted picture
/// taken of that fixture; a generated name would change 588 baselines for no reason. It may hold a
/// `/`, for a fixture that wants a folder per test under one parent, and a file's own path may hold
/// one too — the folders above each file are made for it.
///
/// **Cleared first, and written once**, which are the two halves of [`build_sample_folder`]'s own
/// comment. Clearing is what stops a file an older version of a test wrote staying in the folder for
/// ever and appearing in the explorer of every picture taken afterwards; writing once is what stops
/// one test reading a file another test is part way through writing. Every caller comes through the
/// registry, so the clearing runs before any test has the path.
///
/// The registry is per **binary**, so each of the twelve test files builds the fixtures it uses.
/// Two binaries running at once would clear each other's folders — `cargo test` runs one test binary
/// at a time, which is what makes that safe, and is the same thing [`sample_folder`] relies on.
pub fn fixture(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    static BUILT: OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    let built = BUILT.get_or_init(Default::default);
    let root = std::env::temp_dir().join(name);
    let mut names = built.lock().expect("the fixture registry");
    if names.insert(name.to_owned()) {
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).expect("make the fixture folder");
        for (file, text) in files {
            let at = root.join(file);
            if let Some(parent) = at.parent() {
                std::fs::create_dir_all(parent).expect("make the folders above the file");
            }
            std::fs::write(&at, text).unwrap_or_else(|_| panic!("write {file}"));
        }
    }
    root
}
