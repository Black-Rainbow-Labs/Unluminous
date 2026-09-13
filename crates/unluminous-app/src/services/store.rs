//! What Unluminous remembers between runs: the settings, and the projects that have been open.
//!
//! Two files in one folder, both plain text, both written by hand rather than through a format library.
//! `settings.conf` holds one `name = value` a line and `recent.txt` holds one folder a line, newest
//! first. Neither needs a parser worth a dependency, and both can be read and corrected in a text
//! editor, which is fitting for a text editor's own settings.
//!
//! A file that cannot be read is treated as a file that is not there. Unluminous starting with its defaults is
//! better than Unluminous refusing to start because a settings file has a stray line in it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Write `bytes` to `path` through a temporary and a rename, so a crash cannot truncate the file.
///
/// `task-1922` B7. Every file Unluminous remembers anything in was one `std::fs::write`:
/// `settings.conf`, `recent.txt`, `session.txt`, a project's `workspace.conf` and its two lists,
/// `space.conf`, `highlights.txt`, `breakpoints.conf` and the instance file. A `write` truncates the
/// file and then fills it, so a crash, a power cut or a full disk between those two leaves the file
/// empty or half written -- and every one of these is read at startup, so what a person loses is the
/// state of the thing they were in the middle of.
///
/// The temporary is in the **same folder** as the target, because a rename is only atomic within one
/// file system and a temporary directory is very often on another one. Its name carries the process
/// id, so two Unluminous windows writing the same settings file cannot take each other's temporary.
/// `sync_all` before the rename is what makes the bytes really be on the disk rather than in the
/// operating system's cache when the rename makes them visible.
///
/// On Windows `std::fs::rename` is `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`, so replacing an
/// existing file needs nothing extra. When the rename is refused the temporary is taken away again,
/// so a folder does not fill with them, and the old file is left exactly as it was.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let folder = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned());
    let name = name.unwrap_or_else(|| "unluminous".to_owned());
    let temporary = folder.join(format!("{name}.tmp-{}", std::process::id()));

    let written = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(problem) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(problem);
    }
    if let Err(problem) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(problem);
    }
    Ok(())
}

/// How many projects the recent list holds. Fifteen fills a menu without needing to scroll.
pub const RECENT_LIMIT: usize = 15;

const SETTINGS_FILE: &str = "settings.conf";
const RECENT_FILE: &str = "recent.txt";
const SESSION_FILE: &str = "session.txt";

/// How many windows the session list holds.
///
/// Eight is more windows than anybody has open at once and few enough that a folder opened once by
/// hand falls off the list after a week of ordinary work. See [`Store::open_windows`].
pub const SESSION_LIMIT: usize = 8;

/// Named values read from or written to the settings file.
///
/// The store knows nothing about what the names mean; `crate::settings` owns that. Keeping the two apart
/// means the settings can grow a value without the file handling changing at all.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Values(BTreeMap<String, String>);

impl Values {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        self.0.insert(name.to_owned(), value.into());
    }

    /// Take a name out, so the file no longer holds it.
    ///
    /// **What a setting that has gone back to its default needs**, and it is not the same as setting it
    /// to an empty string: several settings here mean "whatever this Unluminous's own default is" by having
    /// no line at all — `terminal.shell`, `appearance.theme`, `appearance.icons` — and an empty line
    /// would read as a shell called nothing. Saving merges over the file that is already there
    /// (`settings::save_with`), so without this a value that was cleared would stay in the file and come
    /// back at the next start. See [`Values::set_or_clear`].
    pub fn remove(&mut self, name: &str) {
        self.0.remove(name);
    }

    /// Write a value, or take the name out when it is empty.
    ///
    /// One function rather than an `if` at each of the seven places that mean "empty is the default", so
    /// a later one cannot forget the second half and leave a setting that cannot be un-chosen.
    pub fn set_or_clear(&mut self, name: &str, value: &str) {
        match value.is_empty() {
            true => self.remove(name),
            false => self.set(name, value.to_owned()),
        }
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// Every name that begins with `prefix`, with the prefix removed, in name order.
    ///
    /// What reads a family of keys whose names are not known in advance, which is what a plugin's
    /// submenus are: `menu.submenu.new` and `menu.submenu.new.entries` are two members of one family
    /// and nothing in Unluminous knows the word `new` until the manifest is read. The order is the map's
    /// order, so a family read twice is read the same way both times and a menu built from one is the
    /// same shape every time.
    pub fn starting_with(&self, prefix: &str) -> Vec<(String, String)> {
        self.0
            .iter()
            .filter_map(|(name, value)| {
                name.strip_prefix(prefix).map(|rest| (rest.to_owned(), value.clone()))
            })
            .collect()
    }

    pub fn number(&self, name: &str) -> Option<f32> {
        self.text(name).and_then(|value| value.trim().parse().ok())
    }

    pub fn flag(&self, name: &str) -> Option<bool> {
        match self.text(name)?.trim() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        }
    }

    /// Read `name = value` lines. A line without an `=` is ignored rather than making the whole file
    /// unreadable.
    ///
    /// A `#` starts a comment **when it is followed by a space or ends the line**. That rule is a
    /// little more particular than "everything after a hash", and it is that way because of colours:
    /// a plugin's colour scheme is written `theme.keyword = #FF79C6`, and the plain rule ate the
    /// value and left the plugin with no colours at all. Writing the hash is what anybody would do,
    /// so the format accommodates it rather than making it a trap. `size = 20  # after the value`
    /// still reads as a comment, because that hash is followed by a space.
    pub fn parse(text: &str) -> Self {
        let mut values = Self::new();
        for line in text.lines() {
            let line = match Self::comment_at(line) {
                Some(at) => &line[..at],
                None => line,
            };
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            values.set(name, value.trim().to_owned());
        }
        values
    }

    /// Where the comment starts on this line, if it has one.
    ///
    /// A `#` opens a comment when what follows it is whitespace **and there is something after that
    /// whitespace**. A `#` that is the last thing on the line is part of the value.
    ///
    /// **That second half is a fix rather than a nicety.** `task-1922`: without it
    /// `language.line_comment = #` parses to the *empty string*, and an empty line comment is worse
    /// than none at all, because `rest.starts_with("")` is true at every byte — every file of that
    /// language would be drawn as one comment from its first character. It is not hypothetical for
    /// a value either: `plugins/rust/plugin.conf` and `plugins/css/plugin.conf` have both ended
    /// `language.operators` with `#` since they were written, and both have been silently losing it,
    /// so Rust's attribute character and CSS's hash have never been coloured as operators.
    ///
    /// An inline comment still works, because a comment somebody wrote has words in it. A line
    /// ending `value #` with nothing after the hash now keeps the hash, which is the one thing this
    /// gives up and is not something anybody writes on purpose.
    fn comment_at(line: &str) -> Option<usize> {
        line.char_indices()
            .find(|(at, character)| {
                *character == '#'
                    && line[at + 1..].chars().next().map(char::is_whitespace).unwrap_or(true)
                    && !line[at + 1..].trim().is_empty()
            })
            .map(|(at, _)| at)
    }

    pub fn to_text(&self) -> String {
        self.to_text_headed(
            "# Unluminous settings. Written by Unluminous, and safe to edit by hand.",
        )
    }

    /// The same, under a heading of the caller's own. The project state is written in this format too
    /// and is not the settings, so it says so at the top of its own file.
    pub fn to_text_headed(&self, heading: &str) -> String {
        let mut out = format!("{heading}\n");
        for (name, value) in &self.0 {
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(value);
            out.push('\n');
        }
        out
    }
}

/// The folder Unluminous keeps its settings in, and the two files inside it.
#[derive(Debug, Clone)]
pub struct Store {
    folder: PathBuf,
}

impl Store {
    /// The store in the place the operating system keeps an application's settings.
    pub fn open() -> Self {
        Self::at(settings_folder())
    }

    /// A store in a named folder, which is how the tests use one without touching the real settings.
    pub fn at(folder: impl Into<PathBuf>) -> Self {
        Self { folder: folder.into() }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub fn settings_path(&self) -> PathBuf {
        self.folder.join(SETTINGS_FILE)
    }

    pub fn session_path(&self) -> PathBuf {
        self.folder.join(SESSION_FILE)
    }

    /// The projects Unluminous had a window open on **during the last session**, oldest first.
    ///
    /// `task-1693` asks that quitting and starting again bring back "the windows/projects I had
    /// open". An Unluminous window is a **process** — `services::launcher` records why — so the only place
    /// both of them can see is a file here, beside `recent.txt`.
    ///
    /// **A session has a beginning, and [`Self::remember_open_window`] is where it is found.** Without one
    /// this list is a history rather than a session: nothing takes a line out, so `task-1912` reported a
    /// launch from the desktop opening eight projects, six of them agent scratch folders from weeks before.
    /// What other editors restore is the *last session* — VS Code's `window.restoreWindows` defaults to
    /// `all`, which its own documentation defines as *"all windows you worked on during your previous
    /// session"* — and VS Code can say where one ends because its windows are one process. Here they are one
    /// process each, so the beginning of a session is derived: it is a window opening while no other one is
    /// running.
    ///
    /// **A line is still kept when a window closes**, and that is the trade-off, stated rather than hidden: a
    /// project closed in the middle of a session comes back at the next start, because by the time the last
    /// window closes the ones that closed before it are gone from any live registry and it cannot tell which
    /// of them were deliberate. It is bounded to one session now rather than to the life of the settings
    /// folder.
    ///
    /// A folder that is no longer there is left out, for the reason [`Self::recent_projects`] leaves
    /// one out of the menu.
    pub fn open_windows(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for (_, path) in self.session_rows() {
            if out.contains(&path) || !path.is_dir() {
                continue;
            }
            out.push(path);
        }
        out.truncate(SESSION_LIMIT);
        out
    }

    /// The session file as it is written: which window wrote each line, and which project it was on.
    ///
    /// **A line with no process id in front of it is a path with a dead window**, which is what a file written
    /// by an older Unluminous looks like. It reads, and the first launch after this version arrives finds
    /// nothing alive in it and begins a session — so an accumulated file corrects itself rather than needing
    /// anybody to delete one.
    fn session_rows(&self) -> Vec<(u32, PathBuf)> {
        let Ok(text) = std::fs::read_to_string(self.session_path()) else {
            return Vec::new();
        };
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| match line.split_once(' ') {
                Some((first, rest)) => match first.parse::<u32>() {
                    Ok(pid) => (pid, PathBuf::from(rest.trim())),
                    // A path with a space in it and no id, which is most of the paths on this machine.
                    Err(_) => (0, PathBuf::from(line)),
                },
                None => (0, PathBuf::from(line)),
            })
            .map(|(pid, path)| (pid, unluminous_terminal::paths::plain(&path)))
            .collect()
    }

    /// Record that this window, running as `pid`, has `folder` open.
    ///
    /// **One rule, and it is where a session begins.** A window that opens while no listed window is still
    /// running is the first window of a new session, and the file becomes that one row. A window that opens
    /// while another is running joins the session and is appended. That is the whole of `task-1912`'s first
    /// report: without it nothing ever takes a row out and the list is every project there has ever been.
    ///
    /// `alive` answers whether a process id belongs to an Unluminous that is still running. It is passed in
    /// rather than asked here so that this rule can be run by a test with no processes in it — and because
    /// the honest answer is *a listed instance with that id*, which is knowledge `unluminous-cli` owns. A bare
    /// process id would be fooled by one the operating system has handed to something else, and a fooled
    /// answer keeps the file from ever resetting, which is the reported fault returning by a side door.
    ///
    /// Newest **last**, which is the other way round from `recent.txt`: the list is restored in
    /// order and the last entry is the one the restoring process opens itself, so oldest-first is
    /// also what makes truncating the front of the list drop the oldest.
    pub fn remember_open_window(&self, folder: &Path, pid: u32, alive: &dyn Fn(u32) -> bool) {
        let folder = plain_absolute(folder);
        let mut rows = self.session_rows();
        // **A session nobody is in is over.** Whatever the file holds was written by windows that have all
        // gone, so this window is the first of the next one.
        //
        // **A row that is this window counts as alive without being asked about**, which is what makes
        // restoring a session safe however the starts interleave: `main` writes the whole restored session
        // down with the process id of each window it started, so every one of them finds itself in the file
        // and none of them can decide the session is over and throw the others away. Asking the instance list
        // instead would depend on when each window got round to writing its instance file.
        if !rows.iter().any(|(id, _)| *id != 0 && (*id == pid || alive(*id))) {
            self.write_session(&[(pid, folder)]);
            return;
        }
        // A project that is already listed writes nothing at all, which is what keeps three windows starting
        // at once from fighting over the file: restoring a session opens two or three processes within a few
        // hundred milliseconds and every one of them reads this list and writes it back.
        if rows.iter().any(|(_, path)| *path == folder) {
            return;
        }
        rows.push((pid, folder));
        while rows.len() > SESSION_LIMIT {
            rows.remove(0);
        }
        self.write_session(&rows);
    }

    /// Write the session list out as exactly `windows`, each with the window that has it open.
    ///
    /// What restoring does once it has started them all, so the list is what was really restored rather than
    /// growing for ever — and it is called now, by `main`, which is half of `task-1912`'s first report: this
    /// function's own comment said that was its purpose while nothing outside the tests called it.
    pub fn write_session(&self, windows: &[(u32, PathBuf)]) {
        let text: String =
            windows.iter().map(|(pid, path)| format!("{pid} {}\n", path.display())).collect();
        if let Err(problem) = self.write(&self.session_path(), &text) {
            eprintln!("Unluminous could not write its open windows: {problem}");
        }
    }

    pub fn recent_path(&self) -> PathBuf {
        self.folder.join(RECENT_FILE)
    }

    pub fn read_values(&self) -> Values {
        match std::fs::read_to_string(self.settings_path()) {
            Ok(text) => Values::parse(&text),
            Err(_) => Values::new(),
        }
    }

    /// Write the settings. A failure is reported on the error output and otherwise ignored: Unluminous going
    /// on working with the settings it has in memory is better than stopping because a disk is full.
    pub fn write_values(&self, values: &Values) {
        if let Err(problem) = self.write(&self.settings_path(), &values.to_text()) {
            eprintln!("Unluminous could not write its settings: {problem}");
        }
    }

    /// The projects that have been open, newest first.
    ///
    /// A folder that has since been removed is left out, because an entry in the menu that cannot be
    /// opened is worse than a shorter menu.
    pub fn recent_projects(&self) -> Vec<PathBuf> {
        let Ok(text) = std::fs::read_to_string(self.recent_path()) else {
            return Vec::new();
        };
        let mut out: Vec<PathBuf> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Through `plain`, so that a list written by an earlier Unluminous — every line of which held a
            // verbatim path — is read as the folders it names rather than as nine folders nobody can
            // open a terminal in. The next `remember_project` writes the repaired list back.
            let path = unluminous_terminal::paths::plain(Path::new(line));
            if out.contains(&path) || !path.is_dir() {
                continue;
            }
            out.push(path);
        }
        out.truncate(RECENT_LIMIT);
        out
    }

    /// Put `folder` at the top of the recent list, removing an older entry for the same folder.
    ///
    /// The path is made absolute first, so that opening `.` and opening the folder it stands for are one
    /// entry rather than two.
    ///
    /// And then plain, because on Windows `canonicalize` gives back a **verbatim** path —
    /// `\\?\C:\jason\dev\unluminous` — and this list is not only read back by Unluminous. It is the explorer's
    /// root, the folder the `.unluminous` state is written beside, and the directory a shell is started in,
    /// and `cmd.exe` will not start in a verbatim path at all. `task-1670` is what that looked like from
    /// the outside: a terminal that opened in `C:\Windows` and said why in a message about network
    /// shares. `unluminous_terminal::paths` says what the prefix is and why the terminal strips it again.
    pub fn remember_project(&self, folder: &Path) {
        let folder = plain_absolute(folder);
        let mut projects = self.recent_projects();
        projects.retain(|existing| existing != &folder);
        projects.insert(0, folder);
        projects.truncate(RECENT_LIMIT);
        let text: String = projects.iter().map(|path| format!("{}\n", path.display())).collect();
        if let Err(problem) = self.write(&self.recent_path(), &text) {
            eprintln!("Unluminous could not write its recent projects: {problem}");
        }
    }

    fn write(&self, path: &Path, text: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.folder)?;
        write_atomically(path, text.as_bytes())
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::open()
    }
}

/// A folder as it should be written down: absolute, and plain rather than verbatim.
///
/// Both lists Unluminous keeps of folders go through this. Absolute, so that opening `.` and opening the
/// folder it stands for are one entry rather than two; and plain because on Windows `canonicalize`
/// gives back a **verbatim** path — `\\?\C:\jason\dev\unluminous` — which `cmd.exe` will not start in.
/// `task-1670` is what that looked like from the outside.
fn plain_absolute(folder: &Path) -> PathBuf {
    let folder = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    unluminous_terminal::paths::plain(&folder)
}

/// Where the operating system expects an application to keep its settings.
///
/// macOS puts them in `Library/Application Support`, Windows in the roaming application data folder, and
/// everywhere else follows the directory specification, which is `XDG_CONFIG_HOME` when it is set and
/// `~/.config` when it is not. With no home directory at all the current folder is used, so that Unluminous
/// still runs.
/// Where Unluminous keeps its things for this person, without opening a store first.
///
/// `main` needs it before anything else exists, to point the crash log at it.
pub fn folder_for_this_person() -> PathBuf {
    settings_folder()
}

fn settings_folder() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        if let Some(home) = home {
            return home.join("Library/Application Support/Unluminous");
        }
    }
    if cfg!(target_os = "windows") {
        if let Some(data) = std::env::var_os("APPDATA") {
            return PathBuf::from(data).join("Unluminous");
        }
    }
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config).join("unluminous");
    }
    match home {
        Some(home) => home.join(".config/unluminous"),
        None => PathBuf::from(".unluminous"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(name);
        std::fs::remove_dir_all(&folder).ok();
        folder
    }

    /// **A write that fails leaves the file that was there exactly as it was.** `task-1922` B7.
    ///
    /// Every file Unluminous remembers anything in used to be one `std::fs::write`, which truncates and
    /// then fills -- so a crash, a power cut or a full disk between those two leaves the file empty
    /// or half written, and every one of them is read at startup.
    ///
    /// The rename is made to fail by pointing it at a folder, which neither platform will replace
    /// with a file. What the test then asks is the whole of the promise: the thing that was there is
    /// untouched, and no temporary is left behind.
    #[test]
    fn a_write_that_fails_leaves_the_old_file_alone_and_no_temporary_behind() {
        let folder = temporary("unluminous-atomic-write");
        std::fs::create_dir_all(&folder).expect("make the folder");

        // The ordinary case first: it writes, and it replaces.
        let file = folder.join("settings.conf");
        write_atomically(&file, b"appearance.font.size = 16\n").expect("the first write");
        write_atomically(&file, b"appearance.font.size = 20\n").expect("the second write");
        assert_eq!(
            std::fs::read_to_string(&file).expect("read it back"),
            "appearance.font.size = 20\n"
        );

        // A rename onto a folder is refused on both platforms, which stands in for a disk that is
        // full at exactly the wrong moment.
        let in_the_way = folder.join("occupied");
        std::fs::create_dir_all(&in_the_way).expect("make the folder");
        std::fs::write(in_the_way.join("inside.txt"), "still here\n").expect("write inside it");
        let refused = write_atomically(&in_the_way, b"this cannot land\n");
        assert!(refused.is_err(), "renaming a file over a folder is refused");
        assert_eq!(
            std::fs::read_to_string(in_the_way.join("inside.txt")).expect("read it back"),
            "still here\n",
            "what was there is untouched"
        );

        let leftovers: Vec<_> = std::fs::read_dir(&folder)
            .expect("list the folder")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "no temporary is left behind: {leftovers:?}");
    }

    /// Nothing is running, so nothing answers. What every test here starts from.
    fn nothing_is_alive(_: u32) -> bool {
        false
    }

    /// `task-1693`: the windows Unluminous had open, so that starting it again brings them all back.
    ///
    /// The three windows of one session are three windows that opened while each other were running, which is
    /// what `alive` says here and what makes this a session rather than a history.
    #[test]
    fn every_window_that_was_open_is_remembered_and_a_line_is_kept_when_one_closes() {
        let folder = temporary("unluminous-store-session");
        let store = Store::at(&folder);
        let first = folder.join("first");
        let second = folder.join("second");
        std::fs::create_dir_all(&first).expect("make the first project");
        std::fs::create_dir_all(&second).expect("make the second project");

        store.remember_open_window(&first, 101, &nothing_is_alive);
        store.remember_open_window(&second, 102, &|pid| pid == 101);
        let windows = store.open_windows();
        assert_eq!(windows.len(), 2, "both windows are in the list");
        assert!(windows.last().is_some_and(|last| last.ends_with("second")), "newest last");

        // Opening the first again writes nothing, which is what keeps three windows starting at
        // once from losing each other's lines.
        store.remember_open_window(&first, 101, &|pid| pid == 102);
        let windows = store.open_windows();
        assert_eq!(windows.len(), 2, "it is already there, so nothing is written");
        assert!(windows.last().is_some_and(|last| last.ends_with("second")));
    }

    /// `task-1912`: *"all projects I've ever opened are reopened, rather than just the windows I had open."*
    ///
    /// The reported file, made: three projects whose windows have all gone. A window opening into that is the
    /// first window of a new session, and what it leaves behind is itself. **Fails on the code as it was**,
    /// where every one of the three stayed for ever.
    #[test]
    fn a_new_session_replaces_the_windows_of_the_last_one() {
        let folder = temporary("unluminous-store-session-new");
        let store = Store::at(&folder);
        let mut projects = Vec::new();
        for name in ["one", "two", "three"] {
            let project = folder.join(name);
            std::fs::create_dir_all(&project).expect("make a project");
            projects.push((0, project));
        }
        let fresh = folder.join("fresh");
        std::fs::create_dir_all(&fresh).expect("make the fresh project");
        store.write_session(&[
            (301, projects[0].1.clone()),
            (302, projects[1].1.clone()),
            (303, projects[2].1.clone()),
        ]);

        store.remember_open_window(&fresh, 400, &nothing_is_alive);
        let windows = store.open_windows();
        assert_eq!(windows.len(), 1, "a session nobody is in is over, and the list is {windows:?}");
        assert!(windows[0].ends_with("fresh"));
    }

    /// The other half of the same rule: a second window while the first is running joins its session.
    #[test]
    fn a_window_that_opens_beside_a_live_one_joins_its_session() {
        let folder = temporary("unluminous-store-session-join");
        let store = Store::at(&folder);
        let first = folder.join("first");
        let second = folder.join("second");
        std::fs::create_dir_all(&first).expect("make the first project");
        std::fs::create_dir_all(&second).expect("make the second project");
        store.write_session(&[(501, first.clone())]);

        store.remember_open_window(&second, 502, &|pid| pid == 501);
        assert_eq!(
            store.open_windows().len(),
            2,
            "the live window's session is joined, not replaced"
        );
    }

    /// The reported case in the other direction, which must keep working: quitting three windows and starting
    /// again brings back three. Closing takes no row out, so the file still holds all three and they are all
    /// dead by the time anything reads it.
    #[test]
    fn the_windows_of_one_session_all_come_back() {
        let folder = temporary("unluminous-store-session-all-back");
        let store = Store::at(&folder);
        let mut rows = Vec::new();
        for (pid, name) in [(601, "one"), (602, "two"), (603, "three")] {
            let project = folder.join(name);
            std::fs::create_dir_all(&project).expect("make a project");
            rows.push((pid, project));
        }
        store.write_session(&rows);
        assert_eq!(store.open_windows().len(), 3, "every window of the last session");
    }

    /// A file written by 0.42.0 has bare paths in it and no process ids. It reads, and the first launch after
    /// this version arrives finds nothing alive in it and begins a session — so an accumulated file corrects
    /// itself rather than needing anybody to delete one.
    #[test]
    fn a_session_file_from_an_older_version_is_read_and_replaced() {
        let folder = temporary("unluminous-store-session-older");
        let store = Store::at(&folder);
        let old = folder.join("old project");
        let fresh = folder.join("fresh");
        std::fs::create_dir_all(&old).expect("make the old project");
        std::fs::create_dir_all(&fresh).expect("make the fresh project");
        // Written the way the older version wrote it: the path alone, and with a space in it, which is what
        // makes reading the process id off the front a question rather than a split.
        store.write(&store.session_path(), &format!("{}\n", old.display())).expect("write it");
        assert_eq!(store.open_windows(), vec![old], "an older file still says what was open");

        store.remember_open_window(&fresh, 700, &nothing_is_alive);
        let windows = store.open_windows();
        assert_eq!(windows.len(), 1, "and the next session replaces it: {windows:?}");
        assert!(windows[0].ends_with("fresh"));
    }

    /// The cap is what bounds the cost of keeping a line behind when a window closes.
    #[test]
    fn the_session_list_stops_at_its_limit_and_drops_the_oldest() {
        let folder = temporary("unluminous-store-session-limit");
        let store = Store::at(&folder);
        let mut made = Vec::new();
        for index in 0..SESSION_LIMIT + 3 {
            let project = folder.join(format!("project-{index}"));
            std::fs::create_dir_all(&project).expect("make a project");
            // Every window of one session, so the list grows to its limit rather than resetting.
            store.remember_open_window(&project, 800 + index as u32, &|pid| pid >= 800);
            made.push(project);
        }
        let windows = store.open_windows();
        assert_eq!(windows.len(), SESSION_LIMIT);
        assert!(
            windows.first().is_some_and(|first| first.ends_with("project-3")),
            "the three oldest fell off the front, and the list is {windows:?}"
        );
    }

    /// A project that is no longer on disk is left out, for the reason one is left out of the recent
    /// menu: a window that opens on nothing is worse than one window fewer.
    #[test]
    fn a_project_that_has_gone_is_not_a_window_to_open() {
        let folder = temporary("unluminous-store-session-gone");
        let store = Store::at(&folder);
        let here = folder.join("here");
        std::fs::create_dir_all(&here).expect("make the project");
        store.write_session(&[(901, here.clone()), (902, folder.join("never-existed"))]);
        assert_eq!(store.open_windows(), vec![here]);
    }

    #[test]
    fn values_survive_being_written_and_read_back() {
        let store = Store::at(temporary("unluminous-store-round-trip"));
        let mut values = Values::new();
        values.set("appearance.font.family", "Helvetica");
        values.set("appearance.font.size", "18");
        values.set("appearance.background.opacity", "0.62");
        values.set("explorer.width", "310");
        store.write_values(&values);

        let read = store.read_values();
        assert_eq!(read.text("appearance.font.family"), Some("Helvetica"));
        assert_eq!(read.number("appearance.font.size"), Some(18.0));
        assert_eq!(read.number("appearance.background.opacity"), Some(0.62));
        assert_eq!(read.number("explorer.width"), Some(310.0));
        assert_eq!(read, values, "what was written is what comes back");
        std::fs::remove_dir_all(store.folder()).ok();
    }

    #[test]
    fn a_missing_settings_file_reads_as_no_values_rather_than_failing() {
        let store = Store::at(temporary("unluminous-store-missing"));
        assert_eq!(store.read_values(), Values::new());
        assert!(store.recent_projects().is_empty());
    }

    #[test]
    fn comments_blank_lines_and_nonsense_are_skipped() {
        let values = Values::parse(
            "# a comment\n\nappearance.font.size = 20  # after the value\nno equals sign here\n = 5\n",
        );
        assert_eq!(values.number("appearance.font.size"), Some(20.0));
        assert_eq!(values.text("no equals sign here"), None);
    }

    #[test]
    fn a_hash_that_is_part_of_a_value_is_not_a_comment() {
        // A colour is written the way anybody would write one, and the value is not eaten.
        let values = Values::parse(
            "theme.keyword = #FF79C6  # pink
theme.comment = #6272A4
",
        );
        assert_eq!(values.text("theme.keyword"), Some("#FF79C6"));
        assert_eq!(values.text("theme.comment"), Some("#6272A4"));
    }

    #[test]
    fn a_flag_reads_the_words_and_the_numbers() {
        let values = Values::parse("a = true\nb = no\nc = 1\nd = maybe\n");
        assert_eq!(values.flag("a"), Some(true));
        assert_eq!(values.flag("b"), Some(false));
        assert_eq!(values.flag("c"), Some(true));
        assert_eq!(values.flag("d"), None, "a value that is not a flag is not guessed at");
    }

    #[test]
    fn the_newest_project_is_first_and_a_repeat_moves_up_rather_than_appearing_twice() {
        let folder = temporary("unluminous-store-recent");
        let store = Store::at(&folder);
        let first = folder.join("one");
        let second = folder.join("two");
        std::fs::create_dir_all(&first).expect("make the first project");
        std::fs::create_dir_all(&second).expect("make the second project");

        store.remember_project(&first);
        store.remember_project(&second);
        store.remember_project(&first);

        let recent = store.recent_projects();
        assert_eq!(recent.len(), 2, "two folders, opened three times, got {recent:?}");
        assert!(recent[0].ends_with("one"), "the one opened last is first, got {recent:?}");
        assert!(recent[1].ends_with("two"));
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn the_recent_list_is_capped_and_drops_folders_that_have_gone() {
        let folder = temporary("unluminous-store-cap");
        let store = Store::at(&folder);
        for index in 0..RECENT_LIMIT + 5 {
            let project = folder.join(format!("project-{index}"));
            std::fs::create_dir_all(&project).expect("make a project");
            store.remember_project(&project);
        }
        assert_eq!(store.recent_projects().len(), RECENT_LIMIT);

        let newest = store.recent_projects()[0].clone();
        std::fs::remove_dir_all(&newest).expect("remove the newest project");
        assert!(
            !store.recent_projects().contains(&newest),
            "a folder that has been removed is not offered"
        );
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_remembered_project_is_written_down_as_a_plain_path() {
        // `task-1670`. `canonicalize` on Windows gives back `\\?\C:\...`, and this list is where the
        // explorer's root and the terminal's working directory come from, so a verbatim path here is a
        // terminal that starts in `C:\Windows`.
        let folder = temporary("unluminous-store-plain-path");
        let store = Store::at(&folder);
        let project = folder.join("project");
        std::fs::create_dir_all(&project).expect("make a project");
        store.remember_project(&project);

        let written = std::fs::read_to_string(store.recent_path()).expect("read the list");
        assert!(
            !written.contains(r"\\?\"),
            "the recent list should hold plain paths, and holds {written:?}"
        );
        assert_eq!(store.recent_projects().len(), 1);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_list_written_by_an_earlier_unluminous_is_read_as_plain_paths() {
        // Every line of the list on a machine that had run the earlier Unluminous was verbatim. Reading it
        // repairs it rather than leaving somebody to edit the file by hand.
        let folder = temporary("unluminous-store-old-list");
        let store = Store::at(&folder);
        let project = folder.join("project");
        std::fs::create_dir_all(&project).expect("make a project");
        let verbatim = std::fs::canonicalize(&project).expect("resolve the project");
        std::fs::create_dir_all(store.folder()).expect("make the settings folder");
        std::fs::write(store.recent_path(), format!("{}\n", verbatim.display()))
            .expect("write the old list");

        let recent = store.recent_projects();
        assert_eq!(recent.len(), 1, "the folder is still found, got {recent:?}");
        assert!(
            !recent[0].to_string_lossy().contains(r"\\?\"),
            "it comes back plain, got {:?}",
            recent[0]
        );
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn the_settings_folder_is_under_the_home_directory() {
        let folder = settings_folder();
        assert!(
            folder.ends_with("Unluminous")
                || folder.ends_with("unluminous")
                || folder.ends_with(".unluminous"),
            "the settings folder should be named after Unluminous, it was {}",
            folder.display()
        );
    }
}
