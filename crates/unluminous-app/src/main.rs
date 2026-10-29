//! Starts Unluminous.
//!
//! The window is created transparent so that the background opacity setting can let the desktop show
//! through. On macOS the window shadow is turned off as well, because the egui documentation records
//! that a translucent window with a shadow leaves ghosting artefacts behind it. Windows needs more
//! than a transparent window before anything shows through, and `services::windows_transparency` is
//! where that lives and why.
//!
//! Usage: `unluminous [path] [--opacity N] [--view raw|side|preview] [--menu-bar native|in-window] [--terminal] [--control on|off]`
//!
//! **What the command line means is `unluminous_app::arguments`, not this file.** It is read there so
//! that a test can run it, which is the same reason `resolve_target` and `starting_folder` live in the
//! library; `main` is left with what a test cannot do anyway — printing, exiting, and building a
//! window. `task-1812` is why: the loop that used to be here treated an argument it did not recognise
//! as a path, so `unluminous --version` opened a window on a folder called `--version` and created it.
//!
//! `--version` prints the version and the build date and stops. `--print-menus` prints the menus and
//! their shortcuts and stops; the macOS menu bar cannot be looked at from a test, so this is how what
//! it was built from can be read. Both go through `services::console` first, because a program in the
//! windows subsystem has no console to print to until it borrows one.
//!
//! `path` is a folder to show in the explorer, or a file to open, in which case the explorer shows the
//! folder that file is in. With no path at all it is the current directory — or, when Unluminous was started
//! from the desktop and the current directory is only wherever the shortcut points, the project that was
//! open last time. `unluminous_app::starting_folder` is that rule and says why it is drawn so narrowly.
//! When there is no such project, which is the first launch after an install, or the folder would be a root of
//! the file system, the window opens on nothing and waits for a project to be chosen in the Set Up Unluminous
//! dialog. `unluminous_app::waits_for_a_project` is that rule (`task-2305`).
//! Several Unluminous windows can run at once, each on its own project, which is what `File -> New Window`
//! and `File -> Recent Projects` start.
//!
//! `--control off` closes the command channel `unluminous-cli` drives the window down. It is open by
//! default, because a command line that needs to be switched on first is a command line an agent
//! cannot rely on being there; `services::control` records what it is and why it is safe.
//!
//! The switches are there so a starting state can be chosen without clicking, which is what makes it
//! possible to capture the window in a particular state. `--opacity` and `--view` are the same settings the
//! Settings window and the toolbar change. `--menu-bar` chooses where the menus are drawn, which on macOS is
//! the bar along the top of the screen and everywhere else is the window's own title bar; naming it is how
//! the bar inside the window can be looked at on a Mac. `--terminal` opens the terminal at startup.

// Do not open a console window alongside the application on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use unluminous_app::arguments::{self, Arguments, Start};
use unluminous_app::components::title_bar::MenuPlacement;
use unluminous_app::services::console;
use unluminous_app::UnluminousApp;

/// Read the command line, and answer it here if it was a question rather than a request for a window.
///
/// Printing and exiting is all this adds to `arguments::read`: the reading itself is a rule with
/// tests over it, and this is the part that has to happen in a real process.
fn settings_or_stop() -> Arguments {
    let line = std::env::args().skip(1);
    let control = std::env::var("UNLUMINOUS_CONTROL").ok();
    match arguments::read(line, control.as_deref()) {
        Start::Window(settings) => settings,
        Start::Answer(said) => {
            console::attach_to_the_calling_terminal();
            println!("{said}");
            std::process::exit(0);
        }
        // Two, rather than one, so that a script can tell a command line it got wrong from one that
        // worked. `unluminous-cli` uses its own exit codes for the same reason.
        Start::Refuse(why) => {
            console::attach_to_the_calling_terminal();
            eprintln!("{why}");
            std::process::exit(2);
        }
    }
}

/// Print the menus, the way both menu bars are built from them.
///
/// The bar along the top of the screen on macOS cannot be read by a test, so this is how what went into it
/// can be checked without looking at the screen.
fn print_menus() {
    use unluminous_app::app::actions::{self, Entry, MenuState};
    let state = MenuState {
        can_undo: true,
        can_redo: true,
        has_selection: true,
        recent: vec![PathBuf::from("/a/recent/project")],
        terminal_tabs: 1,
        ..MenuState::default()
    };
    fn rows(entries: &[Entry], indent: usize) {
        for entry in entries {
            match entry {
                Entry::Separator => println!("{:indent$}  ---", "", indent = indent),
                Entry::Item { name, shortcut, enabled, checked, .. } => {
                    let keys = shortcut.map(|s| s.label()).unwrap_or_default();
                    let marks = match (enabled, checked) {
                        (false, _) => " (dimmed)",
                        (_, true) => " (on)",
                        _ => "",
                    };
                    println!("{:indent$}  {name:<24}{keys}{marks}", "", indent = indent);
                }
                Entry::Submenu { name, entries } => {
                    println!("{:indent$}  {name} >", "", indent = indent);
                    rows(entries, indent + 4);
                }
            }
        }
    }
    for menu in actions::menus(&state) {
        println!("{}", menu.name);
        rows(&menu.entries, 2);
    }
}

fn main() -> eframe::Result {
    let arguments = settings_or_stop();
    unluminous_app::services::frame_trace::mark("arguments");
    // The heap by call site, in a diagnostic build run with `UNLUMINOUS_HEAP_PROFILE` (`task-2279`).
    #[cfg(feature = "diagnostic-allocations")]
    unluminous_app::services::allocation_profile::start();

    if arguments.print_menus {
        console::attach_to_the_calling_terminal();
        print_menus();
        std::process::exit(0);
    }
    // What speech to text needs, for the installer and `unluminous-cli setup`. `task-2290`.
    if arguments.setup_voice {
        console::attach_to_the_calling_terminal();
        std::process::exit(unluminous_app::app::voice::set_up_from_the_installer());
    }

    // Before anything else, so that a panic while the window is being built is written down too. A
    // graphical application on macOS has no standard error to print one to, and a panic that unwinds
    // out of the event loop exits rather than aborting, so the operating system files no crash report
    // either: without this a crash leaves nothing at all behind. `services::crash_log` says more.
    //
    // The backtrace is asked for rather than left to `RUST_BACKTRACE`, because the person whose Unluminous
    // just disappeared did not set an environment variable before it happened.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        // SAFETY: nothing else is running yet; this is the first statement of the program.
        unsafe { std::env::set_var("RUST_BACKTRACE", "1") };
    }
    unluminous_app::services::crash_log::install(
        unluminous_app::services::store::folder_for_this_person(),
    );
    unluminous_app::services::frame_trace::mark("crash-log");

    // Started here, and by nothing else, because it runs the person's shell profile: an Unluminous started
    // from the Finder or the Dock has `PATH=/usr/bin:/bin:/usr/sbin:/sbin` and cannot find a program
    // installed under the home folder, which is where `claude` and `codex` install themselves. It runs
    // on a thread and takes about a second and a half, so it is asked for as early as there is
    // anything to ask, long before a person can press a button that starts one.
    // `services::login_shell` says what is read and why the profile rather than a list of folders.
    unluminous_app::services::login_shell::start_reading();
    unluminous_app::services::frame_trace::mark("login-shell");

    // Proving the crash log works has to be possible on the machine it matters on, from the installed
    // application, where there is no terminal and no test harness. `UNLUMINOUS_PANIC_TEST=1 unluminous` panics
    // here on purpose, and what it writes to crash.log is what a real crash writes.
    if std::env::var_os("UNLUMINOUS_PANIC_TEST").is_some() {
        panic!("UNLUMINOUS_PANIC_TEST was set, so this is a panic on purpose");
    }

    // A file argument opens that file and shows the folder it sits in. A folder argument just shows the
    // folder. With no argument at all, the explorer shows the current directory — except when Unluminous was
    // started from the desktop rather than from a terminal, where the current directory is the folder
    // holding `unluminous.exe` and the project that was open last time is what was meant. Both rules live in
    // the library so that they are tested.
    //
    // The recent projects are read here rather than waiting for `load_settings`, because which folder to
    // show has to be decided before the window is built. It is still the released binary reading the
    // person's own files, which is the rule `Store` keeps.
    let current_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let program = std::env::current_exe().ok();
    let store = unluminous_app::services::store::Store::open();
    let recent = store.recent_projects();
    unluminous_app::services::frame_trace::mark("recent-projects");

    // The windows that were open last time, which `task-1693` asks to have back. Only on a launch
    // from the desktop: `unluminous .` typed in a folder has to open that folder and nothing else, which
    // is the same rule `starting_folder` keeps about reopening the last project.
    //
    // The list is oldest first, so the **last** entry is the project this process opens itself and
    // every other entry gets a process of its own — an Unluminous window is a process, which
    // `services::launcher` records as a deliberate decision.
    let restoring = arguments.path.is_none()
        && unluminous_app::started_from_the_desktop(&current_directory, program.as_deref());
    // A window of the last session is its project tabs in order and the one it was showing (`task-2322`).
    let mut session: Vec<(Vec<PathBuf>, usize)> = match restoring {
        true => store.open_window_groups(),
        false => Vec::new(),
    };
    // A project that already has a window is left alone. Two Unluminous windows on one folder would be two
    // processes writing one `.unluminous` folder, and whichever wrote last would win — which is the same
    // reason `OpenFiles::open` shows a file that is already open rather than opening it twice. The
    // instance files are how a running window says which project it is on; `unluminous-cli instances`
    // reads the same list.
    // `running` rather than `listed`, because a window that was killed rather than closed leaves its
    // instance file behind — and a project skipped on the strength of a dead window is a project
    // that never comes back.
    //
    // Asked **only when there is a session to filter**, which is the desktop launch. Every other
    // start — `unluminous .` in a terminal, `unluminous-cli launch`, `File -> New Window`, a file
    // opened from the shell — names its folder, so `session` is empty and there is nothing for this
    // answer to say anything about. It used to be asked on every start regardless, and `task-1805`
    // measured that at **414 ms of a 1234 ms startup**: a third of the time to a usable window spent
    // working out something that was then thrown away.
    if !session.is_empty() {
        let already_open: Vec<PathBuf> =
            unluminous_cli::client::running().into_iter().map(|instance| instance.folder).collect();
        for (folders, showing) in session.iter_mut() {
            let shown = folders.get(*showing).cloned();
            folders.retain(|folder| !already_open.iter().any(|open| open == folder));
            *showing = shown
                .and_then(|shown| folders.iter().position(|folder| *folder == shown))
                .unwrap_or(0);
        }
        session.retain(|(folders, _)| !folders.is_empty());
    }
    unluminous_app::services::frame_trace::mark("running-instances");
    let mine = session.pop();

    let most_recent = mine
        .as_ref()
        .and_then(|(folders, _)| folders.first())
        .map(PathBuf::as_path)
        .or_else(|| recent.first().map(PathBuf::as_path));
    let fallback =
        unluminous_app::starting_folder(&current_directory, program.as_deref(), most_recent);
    let (folder, file) = unluminous_app::resolve_target(arguments.path.as_deref(), &fallback);

    // **No project to open is not a reason to open the folder this process is standing in** (`task-2305`). The
    // first launch after an install opened the root of the file system on macOS and the installation folder on
    // Windows, walked and indexed it, and started a terminal in it: permission errors and a great deal of
    // memory, for a folder nobody chose. Such a window opens on an empty folder of Unluminous's own instead, and
    // the Set Up Unluminous dialog asks for the project.
    let waiting = unluminous_app::waits_for_a_project(
        arguments.path.as_deref(),
        &current_directory,
        program.as_deref(),
        most_recent,
    );
    let (folder, file) = match waiting {
        true => {
            let placeholder = store.folder().join("No Folder Open");
            let _ = std::fs::create_dir_all(&placeholder);
            (placeholder, None)
        }
        false => (folder, file),
    };

    // The other windows, each with its project named on the command line so that none of them tries
    // to restore the session in its turn. Each reads its own project's geometry, so they come back
    // where they were left.
    //
    // **And the session is written down as exactly what was restored**, with the process id of the window
    // that holds each project. `task-1912`: `write_session` has said since it was written that this is what
    // restoring does, and nothing outside the tests called it — so the list only ever grew. Writing it here,
    // where every one of those process ids is known, also means no restored window has to append a row of its
    // own, which is the read-modify-write three windows starting at once would have raced over.
    //
    // **A window of several project tabs comes back as one window**, started with its first project and
    // `--tab` for each of the others (`task-2322`).
    // The project tabs this window opens after its first: the ones the window restored here held, then the ones
    // named with `--tab`, and which of them to show.
    let (mut other_tabs, mut show_tab) = match (&mine, waiting) {
        (Some((folders, showing)), false) if folders.first() == Some(&folder) => {
            (folders[1..].to_vec(), *showing)
        }
        _ => (Vec::new(), 0),
    };
    if !waiting {
        other_tabs.extend(arguments.tabs.iter().cloned());
        if let Some(wanted) = arguments.show_tab {
            show_tab = wanted;
        }
    }
    // A window started with `--tab` by the one restoring a session was written down by that one already.
    let restored_tabs_here = mine.as_ref().is_some_and(|(folders, _)| folders.len() > 1);
    if !session.is_empty() || restored_tabs_here {
        let mut restored: Vec<(u32, PathBuf, bool)> = Vec::new();
        for (folders, showing) in &session {
            if folders.first() == Some(&folder) {
                continue;
            }
            let started = unluminous_app::services::launcher::open_window_with_tabs(
                &folders[0],
                &folders[1..],
                *showing,
            );
            if let Some(pid) = started {
                for (index, tab) in folders.iter().enumerate() {
                    restored.push((pid, tab.clone(), index == *showing));
                }
            }
        }
        // This window's own rows last, because the list is oldest first and this is the one that is opening
        // now. `session` has had this window's projects taken off it by `pop` above, so they are added here
        // rather than looked for — and it is what lets every window started here find itself in the file,
        // which is what `remember_open_window` needs to know the session is not over.
        restored.push((std::process::id(), folder.clone(), show_tab == 0));
        for (index, tab) in other_tabs.iter().enumerate() {
            restored.push((std::process::id(), tab.clone(), show_tab == index + 1));
        }
        store.write_session_tabs(&restored);
    }

    // Where this project's window was left, which `task-1693` asks for and which has to be known
    // before the window is built. It is read from the project's own `.unluminous` folder rather than from
    // the person's settings, because Unluminous's windows are one per project: a geometry kept per person
    // would open the second window exactly on top of the first.
    let place = unluminous_app::services::project_state::load(&folder).window;
    unluminous_app::services::frame_trace::mark("project-state");

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Unluminous")
        .with_inner_size([1100.0, 720.0])
        // One number, shared with `unluminous-cli window size` and with the resize grips, so that a
        // size the window refuses to open at cannot be asked for from anywhere else.
        .with_min_inner_size(unluminous_app::app::SMALLEST_WINDOW)
        // An empty icon, which eframe reads as "set none" (`task-2279`). Without one it decodes egui's own
        // logo, keeps the 1 MB of pixels for the life of the window, and on Windows sets that logo over the
        // icon `build.rs` compiles into `unluminous.exe`. The operating system takes the program's own icon
        // from the executable on Windows and from the bundle on macOS.
        .with_icon(std::sync::Arc::new(egui::IconData::default()));
    if let Some(place) = place.filter(|place| place.is_sensible()) {
        viewport = viewport
            .with_position([place.x, place.y])
            .with_inner_size([place.width, place.height])
            .with_maximized(place.maximised);
    }

    // **`--background` opens the window without taking the keyboard.** `winit` turns this into
    // `SW_SHOWNOACTIVATE` on Windows, so the window appears where it was started and the foreground window
    // does not change - which also means the virtual desktop does not switch, and that is `task-1914`'s
    // report. It is `open -g`'s answer for the other platform, and `tools/drive-a-window.ps1` is what
    // asks for it. See `Arguments::background`.
    if arguments.background {
        viewport = viewport.with_active(false);
    }
    // And whether this window is part of the person's session, which a window something is driving is not.
    // See `store::joins_the_session`.
    unluminous_app::services::store::set_joins_the_session(
        unluminous_app::services::store::joins_the_session(
            arguments.background,
            std::env::var("UNLUMINOUS_SESSION").ok().as_deref(),
        ),
    );

    let options = eframe::NativeOptions {
        viewport: viewport
            .with_transparent(true)
            // Unluminous draws its own title bar, because rounded corners and a translucent background need the
            // operating system's own window frame turned off.
            .with_decorations(false)
            .with_has_shadow(false),
        // Run the event loop the way winit says to run it. eframe's default is `true`, which drives the
        // loop through winit's `run_app_on_demand` so that `run_native` can return to its caller;
        // winit's own documentation for it says "You are strongly encouraged to use
        // `EventLoop::run_app()` for portability, unless you specifically need the ability to re-run a
        // single event loop more than once", and eframe's own comment on the setting says the `false`
        // option "is only there so we can revert if we find any bugs". Unluminous runs the loop once and
        // then the process ends, so it needs none of what the default buys, and a window was found
        // asleep on macOS inside `run_app_on_demand` with wakes no longer reaching it. This takes that
        // difference out of the picture. `app::HEARTBEAT` is what makes the window recover by itself if
        // a wake is ever lost again.
        run_and_return: false,
        ..Default::default()
    };

    // The desktop shows through the window on macOS with nothing more than `with_transparent`. Windows
    // also needs a swapchain that can carry alpha, which is a wgpu setting rather than a window one.
    #[cfg(windows)]
    let options = unluminous_app::services::windows_transparency::with_direct_composition(options);
    // A graphics device that allocates in blocks the size of what is drawn - `task-2218`.
    let options = unluminous_app::services::graphics_memory::frugal(options);
    // Every frame drawn into a texture a paint callback can read, so a modal's backdrop is the previous frame
    // blurred on the GPU rather than a screenshot read back and blurred on this thread - `task-2288`.
    let mut options = options;
    options.wgpu_options.scene_texture = true;
    // How frames are handed to the display, which `UNLUMINOUS_PRESENT` and `UNLUMINOUS_FRAME_LATENCY` can change
    // for a measurement; see `services::graphics_memory::presentation`.
    options.wgpu_options.surface = unluminous_app::services::graphics_memory::presentation(
        options.wgpu_options.surface,
        std::env::var("UNLUMINOUS_PRESENT").ok().as_deref(),
        std::env::var("UNLUMINOUS_FRAME_LATENCY").ok().as_deref(),
    );

    // The window's thread is run first when the machine is busy - `task-2288`. This is the thread
    // `run_native` runs the window on, so it is asked here, before the window exists.
    unluminous_app::services::scheduling::favour_the_window_thread();
    // rux draws its depth at this processor's widest SIMD level, which no test does - `task-2288`.
    unluminous_app::theme::use_the_fastest_raster();
    // egui's own marks in the frame trace, in a build made for it - `task-2288`.
    #[cfg(feature = "diagnostic-spans")]
    unluminous_app::services::span_trace::start();
    // The files Finder hands over, which reach no window unless something answers macOS's open documents event.
    // Before the event loop, so the file a launch was asked to open is not missed - `task-2270`.
    unluminous_app::services::opened_by_the_system::listen();

    eframe::run_native(
        "Unluminous",
        options,
        Box::new(move |cc| {
            unluminous_app::services::frame_trace::mark("eframe-window");
            if let Some(render_state) = cc.wgpu_render_state.as_ref() {
                unluminous_app::services::scene_blur::install(render_state, &cc.egui_ctx);
            }
            let mut app = UnluminousApp::new(folder);
            if waiting {
                app.wait_for_a_project();
            }
            unluminous_app::services::frame_trace::mark("app-new");
            app.prepare(&cc.egui_ctx);
            unluminous_app::services::opened_by_the_system::wake_with(&cc.egui_ctx);
            unluminous_app::services::frame_trace::mark("prepare");
            // The settings, the pane sizes and the recent projects come from disk here rather than in
            // `UnluminousApp::new`, so that a test never reads or writes the settings of the person running it.
            app.load_settings();
            // The operating system's clipboard is the person's, so the binary switches it on and a
            // test, which does not, never overwrites what they copied.
            app.use_the_system_clipboard();
            // Usage statistics, while `telemetry.enabled` is on. A test never calls this, so a window a
            // test builds sends nothing; a `--background` window sends nothing unless the environment
            // asks. See `app::telemetry`.
            app.allow_telemetry(arguments.background, arguments.path.is_none());
            // The code index's file is in the person's own cache, so the binary lets the window be the
            // project's index host and a test, which does not, holds its index in memory.
            app.index_on_disk();
            // And it may start rust-analyzer and tsserver, which a test's window does not.
            app.allow_language_servers();
            // And open the microphone for dictation and for "Sable", which a test's window never does.
            app.allow_the_microphone();
            // And hand a Word document's PDF to the system's print dialog, which a test's window never does.
            unluminous_app::services::word::print::allow_printing();
            // And start a new Word document from Word's own Normal.dotm and the locale's page size.
            unluminous_app::services::word::files::allow_the_office_template();
            // And read the ACP registry when the AI Configuration page is shown, which a test's window does not.
            app.endpoints.acp.read_when_shown();
            // And read unluminous.com's skills catalogue when the Skills Manager is shown (`task-2396`).
            unluminous_app::services::skills::allow_fetching();
            // The Background grid's thumbnails for every picture already there, on a thread, so the grid opens
            // at once (`task-2293`). A test's window makes them only when its grid asks.
            app.make_the_missing_background_thumbnails(&cc.egui_ctx);
            // The Set Up Unluminous dialog looks at this machine, and opens by itself while setup is not
            // finished: the command line tool, the coding agents, the speech model and the microphone. A
            // test never calls this, so a test's window never reads or changes any of them. `task-2301`.
            // A window started with `--background` is one something is driving, so it never opens it by itself.
            app.allow_the_setup(!arguments.background);
            unluminous_app::services::frame_trace::mark("settings");
            // What was left open in this project last time. After the settings, because it opens files
            // and they have to be set in the font the settings name; before the file argument, so a file
            // named on the command line is the tab that ends up showing.
            // A window waiting for a project is on a placeholder with nothing in it to restore.
            if !waiting {
                app.restore_project();
            }
            unluminous_app::services::frame_trace::mark("restore-project");
            if let Some(file) = file {
                // A file named on the command line that will not open leaves its reason in the
                // status bar of the window that has just come up, which is where a person is
                // looking. Unluminous still starts.
                let _ = app.open_path(&file);
            }
            if let Some(opacity) = arguments.opacity {
                app.settings.opacity = opacity.clamp(0.05, 1.0);
                // Asking for an opacity is asking for the background (`task-2293`).
                app.settings.background_enabled = true;
            }
            if let Some(view) = arguments.view {
                app.set_view_mode(view);
            }
            app.menu_placement =
                arguments.menu_bar.unwrap_or_else(MenuPlacement::for_this_platform);
            if app.menu_placement == MenuPlacement::Native {
                // The bar along the top of the screen. Built here rather than in `prepare`, because it needs
                // a real application to attach itself to and the screenshot tests have none.
                app.install_native_menu();
            }
            if arguments.terminal {
                // Through the one function that shows a tile, as everything else does. Nothing is
                // running yet at this point, so it changes nothing here — but a path that set the
                // flag by hand would be the next one to forget the other tile.
                app.show_the_terminal_tile(true);
            }
            // Last, so that a command arriving at once finds the window as the switches left it.
            if arguments.control {
                app.open_control_channel(&cc.egui_ctx);
            }
            // **A window of project tabs** (`task-2322`): this project is its first tab, and the others it was
            // restored with or named with `--tab` open after it, each set up the way this one has just been.
            let mut window = unluminous_app::app::project_tabs::ProjectWindow::new(app);
            window.open_the_other_tabs(&other_tabs, show_tab, &cc.egui_ctx);
            unluminous_app::services::frame_trace::mark("ready");
            Ok(Box::new(window))
        }),
    )
}
