//! The Unluminous window.
//!
//! The editor itself is in `unluminous-core`, which has no user interface dependencies. This crate supplies
//! a window, input, painting, and real fonts behind the editor's measurements.
//!
//! The folders, and what belongs in each one. A later change should keep to this.
//!
//! - `app` is the window's own state and the actions the menus and the keyboard ask for.
//! - `components` draws: one file per piece of the window, each taking a rectangle and returning what
//!   the user did in it rather than changing the state itself.
//! - `services` is everything that is not drawing: the file tree, the fonts and the glyph atlas, the
//!   settings and recent projects on disk, starting a second window, and the macOS menu bar.
//! - `theme` is the palette, the measurements and the drawn icons.
//! - `build_info` is what this build is: the version and the date it was built.
//! - `arguments` is what the command line asked for, read where a test can run it.

pub mod app;
pub mod arguments;
pub mod build_info;
pub mod components;
pub mod naming;
pub mod services;
pub mod settings;
pub mod theme;

pub use app::{UnluminousApp, ViewMode};
pub use arguments::{Arguments, Start};
pub use services::file_tree::FileTree;
pub use services::text_renderer::TextRenderer;
pub use settings::Settings;

use std::path::{Path, PathBuf};

/// Work out what to show from the path given on the command line.
///
/// A file argument opens that file and shows the folder it sits in. A folder argument shows that folder. No
/// argument at all shows `fallback`, which the binary passes as the current directory.
///
/// **What comes back is absolute**, resolved against `fallback`. It used to be whatever was typed, and
/// `unluminous .` — which is what `unluminous-cli launch .` runs — left the window with a project literally called
/// `.`. The explorer still worked, because the process's own directory was right, but everything that
/// *reports* the project was then lying: the title bar, the recent projects list, the instance file that
/// `unluminous-cli instances` prints, `--instance <part of a path>`, and the rule the MCP server uses to
/// choose which window a tool call is for when several are running. A path is turned into an answer once,
/// here, rather than in each of the six places that read one.
///
/// It is done lexically rather than with `canonicalize`, which on Windows answers with a `\\?\` prefix
/// that would then be in the title bar and in every reply.
///
/// This lives here rather than in `main.rs` so that it can be tested. It looked wrong once when a
/// translucent window let the folder tree of another application show through Unluminous's explorer, and the only
/// way to settle whether the resolution was at fault was to be able to run it.
pub fn resolve_target(argument: Option<&Path>, fallback: &Path) -> (PathBuf, Option<PathBuf>) {
    let against = |path: &Path| -> PathBuf {
        tidy(if path.is_absolute() { path.to_path_buf() } else { fallback.join(path) })
    };
    match argument {
        Some(path) if path.is_file() => {
            let file = against(path);
            let folder = file
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| tidy(fallback.to_path_buf()));
            (folder, Some(file))
        }
        Some(path) => (against(path), None),
        None => (tidy(fallback.to_path_buf()), None),
    }
}

/// A path with `.` dropped and `..` walked back, without touching the disk.
///
/// `Path::components` already drops a `.` that is not at the front; what it keeps is `..`, which has to be
/// applied here or `C:\jason\dev\unluminous\..\space-invaders` would be shown to a person as exactly that.
/// A `..` with nothing to walk back into is kept, because turning `..\thing` into `thing` would be a
/// different folder rather than a tidier spelling of the same one.
fn tidy(path: PathBuf) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    let mut walked_back: Vec<Component> = Vec::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                let last_is_a_name = out
                    .components()
                    .next_back()
                    .is_some_and(|part| matches!(part, Component::Normal(_)));
                if last_is_a_name {
                    out.pop();
                } else {
                    walked_back.push(part);
                    out.push(part);
                }
            }
            other => out.push(other),
        }
    }
    if out.as_os_str().is_empty() {
        // Every component cancelled out, which is what `.` on its own does.
        return PathBuf::from(".");
    }
    out
}

/// The folder to show when nothing was named on the command line.
///
/// `current_directory` is the honest answer when a person typed `unluminous` in a terminal: they are standing
/// in the folder they mean. It is not an answer at all when Unluminous was started from the desktop, the Start
/// menu or a file association, because then the current directory is whatever the shortcut points at —
/// which for Unluminous's own installer is the folder `unluminous.exe` sits in. `task-1670` is what that looked
/// like: quit Unluminous with a project open, start it again from the desktop, and the explorer and the
/// terminal both came up in `AppData\Local\Programs\Unluminous`.
///
/// So: when the current directory is the folder the program itself lives in, nobody chose it, and the
/// project that was open last time is what was meant. Otherwise the current directory stands. That is a
/// narrower rule than "always reopen the last project", and deliberately — `unluminous` typed in a folder has
/// to open *that* folder, or the command line would be lying about what it does.
///
/// `most_recent` is the head of the recent projects list, and `program` is `std::env::current_exe`. Both
/// are passed in rather than read here, so this is a rule that can be tested rather than a rule that
/// depends on where the test binary happens to live.
/// True when Unluminous was started from the desktop rather than from a terminal.
///
/// The same question [`starting_folder`] asks, given a name of its own because `task-1693` asks it
/// too: the windows that were open last time are brought back **only** on this launch, because
/// `unluminous .` typed in a folder has to open that folder and nothing else.
///
/// **Two launches look different and both are the desktop**, which is what `task-1908` found.
///
/// On Windows an installed `unluminous.exe` really does start in the folder it lives in, so the test is that
/// the current directory is the program's own folder — which is what a shortcut leaves it as, and what
/// `task-1670` wrote this for.
///
/// On macOS an application is a **bundle**, and a bundle opened by the Dock, by Finder or by Spotlight starts
/// with its working directory at the root of the file system. Measured with a real `.app`:
///
/// ```text
/// cwd    = /
/// exe    = /private/tmp/Probe.app/Contents/MacOS/Probe
/// parent = /private/tmp/Probe.app/Contents/MacOS
/// ```
///
/// So the Windows test answered `false` for every launch a person makes on this platform, and everything gated
/// on it never happened: the windows that were open last time were read out of `session.txt` and thrown away.
/// The report is *"if I quit Unluminous and open back up, it's supposed to open the windows I had open, but
/// it's never doing that."*
///
/// **What identifies it is the bundle plus a working directory nobody chose.** A binary inside
/// `…app/Contents/MacOS` is a bundle's binary, and `/` is not a folder anybody opens an editor in — a person
/// standing in a project has that project as their working directory, which is the narrow half of the rule and
/// is what keeps `unluminous .` opening the folder it was typed in. Both halves have a test.
pub fn started_from_the_desktop(current_directory: &Path, program: Option<&Path>) -> bool {
    let beside_the_program = program
        .and_then(|program| program.parent())
        .is_some_and(|folder| same_folder(folder, current_directory));
    beside_the_program || opened_as_a_bundle(current_directory, program)
}

/// Whether this is a macOS application bundle started with a working directory nobody chose.
///
/// Split out so the two halves of [`started_from_the_desktop`] can be read apart, and because this one is a
/// statement about a platform's own launch mechanism rather than about a path.
fn opened_as_a_bundle(current_directory: &Path, program: Option<&Path>) -> bool {
    // The root of the file system, which is where `launchd` leaves a bundle it opened and nowhere a person
    // opens an editor from. Compared before the bundle test because it is a cheap comparison and it is the half
    // that says the working directory was not chosen.
    if current_directory != Path::new("/") {
        return false;
    }
    // `…/Something.app/Contents/MacOS/binary`, which is the only layout a bundle's executable has.
    let Some(folder) = program.and_then(|program| program.parent()) else {
        return false;
    };
    let inside_macos = folder.file_name().is_some_and(|name| name == "MacOS");
    let inside_contents =
        folder.parent().and_then(Path::file_name).is_some_and(|name| name == "Contents");
    let inside_a_bundle = folder
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .is_some_and(|name| name.to_string_lossy().ends_with(".app"));
    inside_macos && inside_contents && inside_a_bundle
}

pub fn starting_folder(
    current_directory: &Path,
    program: Option<&Path>,
    most_recent: Option<&Path>,
) -> PathBuf {
    let installed_here = started_from_the_desktop(current_directory, program);
    match most_recent {
        Some(project) if installed_here && project.is_dir() => project.to_path_buf(),
        _ => current_directory.to_path_buf(),
    }
}

/// Whether two paths name the same folder, as far as this can be told without asking the disk twice.
///
/// Compared through `canonicalize` when both resolve, because one of them has come from the operating
/// system and the other from the person, and `C:\Unluminous` and `C:\unluminous\` are the same folder.
fn same_folder(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PathBuf {
        let root = std::env::temp_dir().join("unluminous-resolve-target");
        std::fs::create_dir_all(root.join("inner")).expect("make the folder");
        std::fs::write(root.join("inner/note.md"), "# note\n").expect("write the file");
        root
    }

    #[test]
    fn what_comes_back_is_absolute_however_it_was_typed() {
        // `unluminous .` is what `unluminous-cli launch .` runs, and it used to leave the window with a project
        // called `.` — which the title bar, the recent list, the instance file and the MCP server's
        // choice of window all then repeated.
        let root = sample();
        let (folder, opened) = resolve_target(Some(Path::new(".")), &root);
        assert_eq!(folder, root, "a dot means the folder we are standing in");
        assert_eq!(opened, None);

        let (folder, _) = resolve_target(Some(Path::new("inner")), &root);
        assert_eq!(folder, root.join("inner"));

        let (folder, opened) =
            resolve_target(Some(&root.join("inner/note.md")), Path::new("/nowhere"));
        assert_eq!(folder, root.join("inner"));
        assert_eq!(opened, Some(root.join("inner/note.md")));

        // An absolute argument is left where it is, and nothing at all is the folder we are in.
        let (folder, _) = resolve_target(Some(&root), Path::new("/nowhere"));
        assert_eq!(folder, root);
        let (folder, _) = resolve_target(None, &root);
        assert_eq!(folder, root);
    }

    #[test]
    fn a_step_back_is_walked_rather_than_shown_to_a_person() {
        let root = sample();
        let (folder, _) = resolve_target(Some(Path::new("inner/..")), &root);
        assert_eq!(folder, root);
        // With nothing to walk back into it is kept, because dropping it would name a different
        // folder rather than spell the same one more tidily.
        assert_eq!(tidy(PathBuf::from("../elsewhere")), PathBuf::from("../elsewhere"));
        assert_eq!(tidy(PathBuf::from(".")), PathBuf::from("."));
    }

    #[test]
    fn a_file_shows_the_folder_it_sits_in_and_opens_the_file() {
        let root = sample();
        let file = root.join("inner/note.md");
        let (folder, opened) = resolve_target(Some(&file), Path::new("/nowhere"));
        assert_eq!(folder, root.join("inner"), "the explorer shows the folder holding the file");
        assert_eq!(opened, Some(file));
    }

    #[test]
    fn a_folder_is_shown_with_nothing_opened() {
        let root = sample();
        let (folder, opened) = resolve_target(Some(&root), Path::new("/nowhere"));
        assert_eq!(folder, root);
        assert_eq!(opened, None);
    }

    #[test]
    fn no_argument_falls_back_to_the_current_directory() {
        let (folder, opened) = resolve_target(None, Path::new("/some/where"));
        assert_eq!(folder, Path::new("/some/where"));
        assert_eq!(opened, None);
    }

    #[test]
    fn a_file_with_no_folder_in_front_of_it_resolves_to_the_current_directory() {
        // `unluminous note.md` run inside the folder holding it. `parent` is empty rather than absent, and an
        // empty path is not a folder anything can be read from — so it used to come back as `.`, which
        // was true of the process and useless to anything that reports the project. It is now the folder
        // itself, and the file is named in full.
        let root = sample();
        let inner = root.join("inner");
        let previous = std::env::current_dir().expect("read the current directory");
        std::env::set_current_dir(&inner).expect("move into the folder");
        // The fallback is the current directory, which is what `main.rs` passes.
        let here = std::env::current_dir().expect("read it back");
        let (folder, opened) = resolve_target(Some(Path::new("note.md")), &here);
        std::env::set_current_dir(previous).expect("move back");
        assert_eq!(folder, here, "the folder is the one the file is in, named in full");
        assert_eq!(opened, Some(here.join("note.md")));
    }

    #[test]
    fn started_from_the_desktop_shows_the_project_that_was_open_last_time() {
        // `task-1670`: the current directory is then the folder holding `unluminous.exe`, which nobody chose.
        let root = sample();
        let program = root.join("Programs/Unluminous/unluminous.exe");
        std::fs::create_dir_all(program.parent().expect("a folder")).expect("make the folder");
        let installed = program.parent().expect("a folder").to_path_buf();
        let project = root.join("inner");

        assert_eq!(
            starting_folder(&installed, Some(&program), Some(&project)),
            project,
            "the last project is what was meant"
        );
        assert_eq!(
            starting_folder(&installed, Some(&program), None),
            installed,
            "with nothing opened before, there is nothing better than where it started"
        );
    }

    /// A macOS bundle launched from the Dock is a desktop launch, and it does not look like one.
    ///
    /// **Measured with a real `.app`** rather than reasoned about: a bundle opened through `open` — which is what
    /// the Dock, Finder and Spotlight all do — starts with
    ///
    /// ```text
    /// cwd    = /
    /// exe    = /private/tmp/Probe.app/Contents/MacOS/Probe
    /// parent = /private/tmp/Probe.app/Contents/MacOS
    /// ```
    ///
    /// so the working directory is **not** the binary's folder and `started_from_the_desktop` answered `false`.
    /// `task-1670` wrote that test against the Windows layout, where an installed `unluminous.exe` really does
    /// start in the folder it lives in, and macOS was never checked. The cost was everything gated on it:
    /// `Store::open_windows` was never read, so *"if I quit Unluminous and open back up, it's supposed to open
    /// the windows I had open, but it's never doing that"* — `task-1908`.
    #[test]
    fn a_macos_bundle_opened_from_the_dock_is_a_desktop_launch() {
        let root = sample();
        let program = root.join("Unluminous.app/Contents/MacOS/unluminous");
        std::fs::create_dir_all(program.parent().expect("a folder")).expect("make the folder");
        let project = root.join("inner");

        // What `open` really gives: the root of the file system, and the binary inside the bundle.
        assert!(
            started_from_the_desktop(Path::new("/"), Some(&program)),
            "a bundle opened from the Dock is a desktop launch"
        );
        assert_eq!(
            starting_folder(Path::new("/"), Some(&program), Some(&project)),
            project,
            "so the last project is what was meant"
        );
    }

    /// And a bundle's binary run **from a terminal** still opens the folder the person is standing in.
    ///
    /// The narrow half of the rule above, and the one that keeps `unluminous .` honest: the bundle is what says
    /// this might be a desktop launch, and the working directory is what says it is not. Somebody standing in a
    /// project who runs the binary inside the bundle by its full path means that project.
    #[test]
    fn a_bundles_binary_run_from_a_project_opens_that_project() {
        let root = sample();
        let program = root.join("Unluminous.app/Contents/MacOS/unluminous");
        std::fs::create_dir_all(program.parent().expect("a folder")).expect("make the folder");
        let standing_in = root.join("inner");
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("make the folder");

        assert!(
            !started_from_the_desktop(&standing_in, Some(&program)),
            "a working directory somebody chose is not a desktop launch"
        );
        assert_eq!(
            starting_folder(&standing_in, Some(&program), Some(&elsewhere)),
            standing_in,
            "so the folder they are standing in wins over the last project"
        );
    }

    #[test]
    fn started_from_a_terminal_shows_the_folder_the_person_is_standing_in() {
        // The rule has to be this narrow, or `unluminous` typed in a folder would open a different one.
        let root = sample();
        let program = root.join("Programs/Unluminous/unluminous.exe");
        std::fs::create_dir_all(program.parent().expect("a folder")).expect("make the folder");
        let here = root.join("inner");
        let project = root.to_path_buf();
        assert_eq!(starting_folder(&here, Some(&program), Some(&project)), here);
    }

    #[test]
    fn a_last_project_that_has_gone_leaves_the_current_directory() {
        let root = sample();
        let program = root.join("Programs/Unluminous/unluminous.exe");
        std::fs::create_dir_all(program.parent().expect("a folder")).expect("make the folder");
        let installed = program.parent().expect("a folder").to_path_buf();
        let missing = root.join("a-project-that-was-deleted");
        std::fs::remove_dir_all(&missing).ok();
        assert_eq!(starting_folder(&installed, Some(&program), Some(&missing)), installed);
    }

    #[test]
    fn a_path_that_does_not_exist_is_treated_as_a_folder_to_show() {
        // The explorer then reports that it cannot be read, rather than Unluminous refusing to start.
        let missing = std::env::temp_dir().join("unluminous-resolve-target-missing");
        std::fs::remove_dir_all(&missing).ok();
        let (folder, opened) = resolve_target(Some(&missing), Path::new("/nowhere"));
        assert_eq!(folder, missing);
        assert_eq!(opened, None);
    }
}
