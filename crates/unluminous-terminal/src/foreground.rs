//! What program is running in a terminal right now, as opposed to what it was started with.
//!
//! **The question exists because those are different things.** A terminal node on the Base of Infinite Space
//! records the command it was *given*, and what a person does is add a plain terminal node and then type
//! `claude` into the shell — so a canvas that knew only the command came back as a shell whatever had been
//! running in it. `task-1907` reports that as *"if i just have a view with a terminal with claude-code open,
//! then quit, re-open, the terminal is there but no claude code."*
//!
//! **The answer is the pseudoterminal's foreground process group**, which is what a terminal means by "the
//! program you are talking to": `tcgetpgrp` on the master names it, and the platform names the program behind
//! the id. Measured against a real pseudoterminal with a real `bash` in it before any of this was written:
//!
//! ```text
//! foreground pgrp: 10775   proc_name -> sleep      # while `sleep 30` was running
//! after interrupt: 10770   proc_name -> bash       # after Ctrl+C
//! ```
//!
//! So a node running `claude` can be asked, and a node sitting at a prompt answers with the shell — which is
//! the distinction the feature needs.
//!
//! **Windows has no foreground process group, so it is answered by walking the process tree.** A ConPTY is a
//! pipe rather than a controlling terminal, so there is nothing to ask `tcgetpgrp` of — and for two versions
//! this module said so and answered `None`, which meant that on Windows a node where somebody typed `claude`
//! recorded nothing, came back a bare shell, and resumed no conversation. That is the second half of
//! `task-1912`'s report, and the fix is the one this module's own note named: a walk down from the child
//! process the pseudoconsole was given. [`descendant_program`] is that walk and §5 of
//! `tasks/task-1912-a-session-and-a-terminal-tdd.md` is the three rules it keeps.

/// A duplicate of a session's pseudoterminal, kept only to be asked what is running in it.
///
/// **Taken in `Session::spawn`**, in the window between `tty::new` answering and `EventLoop::new` moving the
/// `Pty` into the loop — which is the same window `reap::Reaper` takes the Windows handle in, and the last
/// moment either is reachable. A duplicate, because the loop owns and closes the original.
pub struct Master {
    #[cfg(unix)]
    fd: Option<std::os::fd::OwnedFd>,
    /// The process the pseudoconsole was given, which on Windows is where the walk down starts.
    #[cfg(windows)]
    child: Option<u32>,
}

impl Master {
    /// Duplicate the master side of `pty`, where the platform has one to duplicate.
    #[cfg_attr(not(unix), allow(unused_variables))]
    pub fn duplicating(pty: &alacritty_terminal::tty::Pty) -> Self {
        #[cfg(unix)]
        {
            use std::os::fd::AsFd;
            Self { fd: pty.file().as_fd().try_clone_to_owned().ok() }
        }
        #[cfg(windows)]
        {
            // The process id rather than the handle, because that is what a snapshot of the process table is
            // keyed on and because an id that has gone simply matches nothing — where a handle kept open would
            // hold the dead process's object alive for as long as this session lives.
            let handle = pty.child_watcher().raw_handle();
            // Safe: the handle belongs to the pseudoterminal and is open for the length of this call.
            let id = unsafe { windows_sys::Win32::System::Threading::GetProcessId(handle) };
            Self { child: (id != 0).then_some(id) }
        }
        #[cfg(not(any(unix, windows)))]
        {
            Self {}
        }
    }

    /// A session with no pseudoterminal at all, which is what `Session::detached` has.
    pub fn detached() -> Self {
        #[cfg(unix)]
        {
            Self { fd: None }
        }
        #[cfg(windows)]
        {
            Self { child: None }
        }
        #[cfg(not(any(unix, windows)))]
        {
            Self {}
        }
    }

    /// The program in the foreground of this terminal, when it can be told.
    ///
    /// `None` for a detached session, for a session whose program has ended, and on Windows — see the note at
    /// the top of this module. A name rather than a command line: the arguments, any `cd` somebody did and
    /// anything typed after the program are all gone, which is why what is done with this is to *offer* the
    /// program rather than to run it.
    pub fn foreground(&self) -> Option<String> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let fd = self.fd.as_ref()?;
            // Safe: the descriptor is owned by this struct and is a pseudoterminal master. A terminal with no
            // foreground group answers -1, which is not a process id.
            let group = unsafe { libc::tcgetpgrp(fd.as_raw_fd()) };
            if group <= 0 {
                return None;
            }
            program_named(group)
        }
        #[cfg(windows)]
        {
            descendant_program(self.child?)
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    }

    /// The folder the shell in this terminal is in **now**, when the platform will say.
    ///
    /// **A different question from what the shell was started in**, and that is the whole reason it exists:
    /// `task-1945`'s *"both nodes and normal terminals should be restored to exactly where they were"*. A
    /// person types `cd`, and nothing about that reaches the pseudoterminal - the session's own
    /// `working_directory` is where it was spawned and stays that for ever.
    ///
    /// **Read off the shell process itself.** On Linux that is `/proc/<pid>/cwd`; on Windows it is the
    /// `CurrentDirectory` in the process's own parameter block. It is the **direct child** of the
    /// pseudoconsole rather than the deepest descendant, because `cd` is the shell's and a program running
    /// under it has a directory of its own that is nobody's business here.
    ///
    /// `None` for a detached session, for a session whose shell has ended, and wherever the platform
    /// refuses - a caller that gets `None` uses the folder it already had, which is what every one of these
    /// did before this existed.
    pub fn folder(&self) -> Option<std::path::PathBuf> {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let fd = self.fd.as_ref()?;
            // Safe: the descriptor is owned by this struct and is a pseudoterminal master.
            let group = unsafe { libc::tcgetpgrp(fd.as_raw_fd()) };
            if group <= 0 {
                return None;
            }
            std::fs::read_link(format!("/proc/{group}/cwd")).ok()
        }
        #[cfg(target_os = "macos")]
        {
            // Left to the caller's own folder on macOS: `proc_pidinfo` with `PROC_PIDVNODEPATHINFO` is the
            // way, it needs a `libproc` binding this crate does not have, and the platform the report came
            // from is Windows. Named here rather than silently absent.
            None
        }
        #[cfg(windows)]
        {
            folder_of(self.child?)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            None
        }
    }
}

/// The current directory of one Windows process, read out of its own parameter block.
///
/// There is no call that answers this - `GetCurrentDirectory` is about the caller - so the way every tool
/// that shows it does it is the way this does: ask `NtQueryInformationProcess` where the process's `PEB` is,
/// read the pointer to its `RTL_USER_PROCESS_PARAMETERS` out of it, and read the `CurrentDirectory` string
/// out of that.
///
/// **The offset is written down because `windows-sys` stops short of it.** Its
/// `RTL_USER_PROCESS_PARAMETERS` declares `Reserved1[16]`, `Reserved2[10]` pointers, `ImagePathName` and
/// `CommandLine` - sixteen bytes and eighty bytes of reserved space before `ImagePathName` at ninety six.
/// The real layout puts `CURDIR CurrentDirectory` at fifty six, inside that reserved run, and a `CURDIR`
/// begins with the `UNICODE_STRING` this reads. The relationship between the two is checked against the
/// struct at compile time below, so a `windows-sys` that fills the reserved space in cannot leave this
/// reading the wrong bytes.
///
/// **Every failure is `None`.** A process that has gone, one this one may not read, a pointer that reads
/// back as zero, a length that is absurd: all of them mean "the platform will not say", and the caller has
/// somewhere sensible to fall back to.
#[cfg(windows)]
fn folder_of(pid: u32) -> Option<std::path::PathBuf> {
    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_BASIC_INFORMATION, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
        RTL_USER_PROCESS_PARAMETERS,
    };

    /// Where `CurrentDirectory.DosPath` sits inside `RTL_USER_PROCESS_PARAMETERS`, in bytes.
    const CURRENT_DIRECTORY: usize = 56;
    // `ImagePathName` is two `UNICODE_STRING`s and a handle past the current directory: the directory's own
    // string, the handle that goes with it in a `CURDIR`, and `DllPath`. Checked against the declaration
    // rather than trusted, so a `windows-sys` that fills the reserved run in cannot leave this reading the
    // wrong bytes without failing the build.
    const _: () = assert!(
        std::mem::offset_of!(RTL_USER_PROCESS_PARAMETERS, ImagePathName) == CURRENT_DIRECTORY + 40,
        "the parameter block is not the layout this offset was read from"
    );
    /// A path longer than this is not a path, it is a pointer read wrong.
    const LONGEST: usize = 32 * 1024;

    // Safe: every call below is checked, every read is bounded by the size of what it reads into, and the
    // handle is closed on every path out.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut basic = PROCESS_BASIC_INFORMATION::default();
        let status = NtQueryInformationProcess(
            process,
            ProcessBasicInformation,
            (&raw mut basic).cast(),
            size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            std::ptr::null_mut(),
        );
        let answer = match status < 0 || basic.PebBaseAddress.is_null() {
            true => None,
            false => {
                read_the_directory(process, basic.PebBaseAddress.cast(), CURRENT_DIRECTORY, LONGEST)
            }
        };
        CloseHandle(process);
        answer
    }
}

/// How many sixteen bit characters a `UNICODE_STRING` of `length` bytes holds, if it is one to read.
///
/// **An odd length is refused** (`task-1984` P1). `Length` counts bytes holding sixteen bit
/// characters, so it is even in every well formed `UNICODE_STRING` -- but this one is read out of
/// another process's memory, and an odd one made `vec![0u16; length / 2]` one byte short of what
/// `ReadProcessMemory` was then told to write into it: a heap write past the end of the allocation,
/// in `unsafe` code, from a value nothing here controls. The guard checked zero and too large and
/// not this.
///
/// A function of its own rather than a condition, because the condition is the thing worth testing
/// and everything around it needs a live process handle.
fn characters_to_read(length: usize, longest: usize) -> Option<usize> {
    match length == 0 || length > longest || length % 2 != 0 {
        true => None,
        false => Some(length / 2),
    }
}

/// The three reads that turn a `PEB` address into a folder. See [`folder_of`].
///
/// Split out so that the handle above is closed on one path rather than on five, which is the shape
/// `started_at` already uses for the same reason.
#[cfg(windows)]
unsafe fn read_the_directory(
    process: windows_sys::Win32::Foundation::HANDLE,
    peb: *const core::ffi::c_void,
    offset: usize,
    longest: usize,
) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;
    use windows_sys::Win32::Foundation::UNICODE_STRING;
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::PEB;

    // Where the pointer to the parameter block sits inside a `PEB`.
    let parameters_at = unsafe { (&raw const (*peb.cast::<PEB>()).ProcessParameters).cast() };
    let mut parameters: *mut core::ffi::c_void = std::ptr::null_mut();
    let read = unsafe {
        ReadProcessMemory(
            process,
            parameters_at,
            (&raw mut parameters).cast(),
            size_of::<*mut core::ffi::c_void>(),
            std::ptr::null_mut(),
        )
    };
    if read == 0 || parameters.is_null() {
        return None;
    }
    let mut directory =
        UNICODE_STRING { Length: 0, MaximumLength: 0, Buffer: std::ptr::null_mut() };
    let read = unsafe {
        ReadProcessMemory(
            process,
            parameters.byte_add(offset),
            (&raw mut directory).cast(),
            size_of::<UNICODE_STRING>(),
            std::ptr::null_mut(),
        )
    };
    if read == 0 || directory.Buffer.is_null() {
        return None;
    }
    let characters = characters_to_read(directory.Length as usize, longest)?;
    let length = characters * 2;
    // The letters themselves, which are sixteen bit and are not terminated.
    let mut letters = vec![0u16; characters];
    let read = unsafe {
        ReadProcessMemory(
            process,
            directory.Buffer.cast(),
            letters.as_mut_ptr().cast(),
            length,
            std::ptr::null_mut(),
        )
    };
    if read == 0 {
        return None;
    }
    let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&letters));
    // A folder that is not there any more is not a folder to reopen in.
    path.is_dir().then_some(path)
}

/// The program a process id is running.
///
/// Two platforms and two mechanisms, which is why it is a function of its own rather than a line inside
/// [`Master::foreground`]: macOS has `proc_name` in libproc and Linux has `/proc/<pid>/comm`, and neither is
/// available on the other.
#[cfg(unix)]
fn program_named(pid: i32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        // **argv[0], and neither of the two easier answers works.** The program a person typed is the word they
        // typed, and macOS has three different ideas of a process's name:
        //
        // | asked for | Claude Code answers | why |
        // |---|---|---|
        // | `proc_name` | `2.1.269` | the executable's file name, and Claude Code's executable is `~/.local/share/claude/versions/2.1.269` |
        // | `p_comm` (`ps -o ucomm=`) | `2.1.269` | the same name, truncated — set from the executable, not from the arguments |
        // | **argv[0]** (`ps -o comm=`) | `claude` | the word that was typed |
        //
        // The first two were each tried and each returned the version number, which is what the reporter's own
        // `space.conf` held after `task-1907` shipped: `running = 2.1.269`, a version offered as a program, on
        // the one program the whole section exists for. So argv[0] it is, out of `KERN_PROCARGS2`, which is
        // where `ps` reads it from.
        argv_zero(pid)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let text = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        let name = text.trim().to_owned();
        (!name.is_empty()).then_some(name)
    }
}

/// The first word of a process's command line, which is the name it was started by.
///
/// **`KERN_PROCARGS2`, which is where `ps -o comm=` reads it from.** The buffer it fills begins with a
/// four-byte count of the arguments, then the executable path, then a run of NUL bytes, and then argv[0]
/// followed by the rest — so the reading is: skip the count, skip the path, skip the padding, and take what is
/// left up to the next NUL. That is the shape `ps` itself walks.
///
/// A process that has gone, one owned by somebody else, or a kernel task all answer `None` rather than a
/// guess: `sysctl` refuses and there is nothing to read.
#[cfg(target_os = "macos")]
fn argv_zero(pid: i32) -> Option<String> {
    const KERN_PROCARGS2: i32 = 49;
    // **`kern.argmax`, asked for rather than guessed.** `sysctl` fills the buffer with the arguments *and the
    // whole environment*, and refuses with `ENOMEM` rather than truncating — so a buffer big enough for a shell
    // is not big enough for a program started from a shell that has a large environment. Measured: a `/bin/zsh`
    // needed 1,004 bytes and a `cargo test` binary needed more than 4,096, which is what made every process
    // answer `None` while the arithmetic was right. This is the number the kernel itself caps it at, and it is
    // what `ps` asks for.
    let mut argmax = 0_i32;
    let mut argmax_name = [libc::CTL_KERN, libc::KERN_ARGMAX];
    let mut argmax_size = std::mem::size_of::<i32>();
    // Safe: two integers as the call requires, and the size passed is the real size of `argmax`.
    let asked = unsafe {
        libc::sysctl(
            argmax_name.as_mut_ptr(),
            argmax_name.len() as u32,
            (&raw mut argmax).cast::<std::ffi::c_void>(),
            &mut argmax_size,
            std::ptr::null_mut(),
            0,
        )
    };
    // A kernel that will not say falls back to a megabyte, which is what macOS's own value has been for years.
    let room = match asked == 0 && argmax > 0 {
        true => argmax as usize,
        false => 1 << 20,
    };
    let mut name = [libc::CTL_KERN, KERN_PROCARGS2, pid];
    let mut buffer = vec![0_u8; room];
    let mut length = buffer.len();
    // Safe: `name` is three integers as the call requires, and `length` is the real length of `buffer` both
    // going in and coming back.
    let answered = unsafe {
        libc::sysctl(
            name.as_mut_ptr(),
            name.len() as u32,
            buffer.as_mut_ptr().cast::<std::ffi::c_void>(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if answered != 0 || length < std::mem::size_of::<u32>() {
        return None;
    }
    buffer.truncate(length);
    // The count of arguments comes first, and the executable path follows it.
    let after_count = &buffer[std::mem::size_of::<u32>()..];
    // The path, up to its terminator.
    let path_ends = after_count.iter().position(|byte| *byte == 0)?;
    // Then a run of NUL padding, and argv[0] begins at the first byte that is not one.
    let rest = &after_count[path_ends..];
    let starts = rest.iter().position(|byte| *byte != 0)?;
    let argv = &rest[starts..];
    let ends = argv.iter().position(|byte| *byte == 0).unwrap_or(argv.len());
    // **The last component of it**, because a command line very often names a program by its path — `/bin/zsh`
    // is what a shell is started as — and what this answers is used to offer a program by name.
    let whole = String::from_utf8_lossy(&argv[..ends]).trim().to_owned();
    let named = whole.rsplit(['/', '\\']).next().unwrap_or(&whole).to_owned();
    // A login shell is `-zsh`, which is the same shell wearing a dash.
    let named = named.strip_prefix('-').unwrap_or(&named).to_owned();
    (!named.is_empty()).then_some(named)
}

/// The name of the program a Windows terminal is talking to, found by walking down from `root`.
///
/// **Four rules, and each is the answer to a case that would otherwise be plausible and wrong.**
///
/// - **The newest child at each level**, by creation time. A shell that has run two programs has two children
///   only while the first is still exiting, and the one somebody is talking to is the one that started last.
/// - **The walk stops at the first thing that is neither a shell nor the shim**, which is the job the terminal
///   started. Walking all the way down instead was the first version of this and it is wrong in the case the
///   whole feature exists for: `claude` starts programs of its own for its tools, so a node running `claude`
///   with a `bash` open under it would have been recorded as running `bash`. This is what `tcgetpgrp` means by
///   a foreground process *group* on the other platform.
/// - **But it walks *through* a shell**, because a program is often reached by one: an `npm` installed
///   `claude.cmd` is `cmd.exe` with a program under it, and stopping at the shell would answer nothing.
/// - **The console's own child is a candidate rather than a step.** A node given a command runs that command
///   as the pseudoconsole's child with no shell at all, and a walk that moved down before looking answered
///   with whatever that program had started: measured against a real `claude`, whose node recorded
///   `running = python` for a helper it had spawned.
/// - **The shim is stepped over.** A node restoring its screen starts `unluminous-cli --replay-screen …`,
///   which prints the screen and then becomes the shell — so the pseudoconsole's own child is that program,
///   and a walk that did not know it would report every restored node as running `unluminous-cli`.
///
/// A walk that finds only shells answers with the last of them, which is a node sitting at a prompt.
/// `services::space::launch::is_a_shell` is what turns that into nothing on the other side, so the two
/// platforms are filtered by one rule rather than two.
///
/// One snapshot of the process table serves the whole walk, which matters because this is asked of every
/// terminal node on a clock.
#[cfg(windows)]
pub fn descendant_program(root: u32) -> Option<String> {
    let table = process_table()?;
    let mut at = root;
    // **The console's own child is a candidate, not a step to take.** A node given a command runs that command
    // *as* the pseudoconsole's child with no shell at all, so a walk that moved down before looking answered
    // with whatever the program had started: measured against a real `claude`, whose node recorded
    // `running = python` — a helper it had spawned — where the answer is `claude`.
    let mut name = table
        .iter()
        .find(|process| process.id == root)
        .map(|process| without_the_extension(&process.name).to_owned());
    // Bounded, because a process table read while processes are starting and stopping can in principle
    // describe a cycle, and a walk down a tree that cannot be deeper than this is not worth a visited set.
    for _ in 0..32 {
        // Only a shell or the shim is walked through: anything else is the job the terminal is talking to.
        if !name.as_deref().is_some_and(|named| is_the_shim(named) || looks_like_a_shell(named)) {
            break;
        }
        let Some(next) = newest_child(&table, at) else { break };
        at = next.id;
        name = Some(without_the_extension(&next.name).to_owned());
    }
    // A walk that got no further than the shim found no shell under it, which is a node whose program has
    // already gone: the shim is not a thing to report as running.
    let named = name.filter(|named| !is_the_shim(named))?;
    (!named.is_empty()).then_some(named)
}

/// Whether a program name is Unluminous itself, printing a node's remembered screen before becoming its shell.
///
/// Split out so that the rule has a name a test can hold: on Windows the pseudoconsole's own child is the shim
/// for the life of a restored node, and a walk that reported it would say every restored node was running
/// `unluminous`. The extension comes off first, because what the process table answers with is a file name.
#[cfg(windows)]
fn is_the_shim(named: &str) -> bool {
    without_the_extension(named).eq_ignore_ascii_case("unluminous-cli")
}

/// Whether a program name is a shell, which is a reason to keep walking rather than an answer.
///
/// **The same list `services::space::launch::is_a_shell` holds, for a different question.** That one decides
/// whether what a node is running is worth writing down; this one decides where to stop walking. It is a few
/// words rather than a dependency, because this crate is below the one that owns the other, and a name missing
/// from here costs a walk that stops one level early rather than a wrong answer.
#[cfg(windows)]
fn looks_like_a_shell(named: &str) -> bool {
    const SHELLS: [&str; 8] = ["pwsh", "powershell", "cmd", "bash", "sh", "zsh", "fish", "wsl"];
    SHELLS.iter().any(|shell| named.eq_ignore_ascii_case(shell))
}

/// A program name with `.exe` taken off it, however it is spelled.
///
/// The process table answers with the file name the file system holds, and `PING.EXE` is what it really gives
/// back — so a case-sensitive comparison takes the extension off some names and not others.
#[cfg(windows)]
fn without_the_extension(named: &str) -> &str {
    match named.len() >= 4 && named[named.len() - 4..].eq_ignore_ascii_case(".exe") {
        true => &named[..named.len() - 4],
        false => named,
    }
}

/// One row of the process table: who it is, who started it, and when.
#[cfg(windows)]
struct Process {
    id: u32,
    parent: u32,
    name: String,
}

/// Every process on the machine, read once.
#[cfg(windows)]
fn process_table() -> Option<Vec<Process>> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    // Safe: a snapshot of the process list, closed below whatever happens after it.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut out = Vec::new();
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // Safe: `entry` is sized as the call requires and lives for the length of the loop.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        let end =
            entry.szExeFile.iter().position(|unit| *unit == 0).unwrap_or(entry.szExeFile.len());
        out.push(Process {
            id: entry.th32ProcessID,
            parent: entry.th32ParentProcessID,
            name: String::from_utf16_lossy(&entry.szExeFile[..end]),
        });
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    // Safe: the snapshot was made above and is not used again.
    unsafe { CloseHandle(snapshot) };
    Some(out)
}

/// The child of `parent` that started most recently.
///
/// **Creation time asked of the process itself**, because the table's own order is the order the operating
/// system happened to walk it in and says nothing about age. A process that has gone between the snapshot and
/// this question answers with the beginning of time, so it loses to a live sibling rather than winning.
#[cfg(windows)]
fn newest_child(table: &[Process], parent: u32) -> Option<&Process> {
    table
        .iter()
        .filter(|process| process.parent == parent && process.id != parent)
        .max_by_key(|process| started_at(process.id))
}

/// When a process was created, as the number the operating system keeps.
#[cfg(windows)]
fn started_at(pid: u32) -> u64 {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // Safe: a handle asked for with the narrowest access that answers this question, closed below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return 0;
    }
    let mut created = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    // Safe: four owned structures, and a handle opened immediately above.
    let asked =
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    // Safe: the handle was opened above and is not used again.
    unsafe { CloseHandle(handle) };
    match asked {
        0 => 0,
        _ => ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session with no pseudoterminal answers with nothing rather than guessing.
    ///
    /// This is the whole of what can be tested without a real shell, and it is the case that matters most:
    /// `Session::detached` is what every screenshot test uses, so a detached session claiming a program would
    /// put one in every canvas a test wrote down.
    #[test]
    fn a_terminal_with_no_pseudoterminal_answers_with_nothing() {
        assert_eq!(Master::detached().foreground(), None);
    }

    /// A shell with a program running under it answers with the program, which is what a terminal is.
    ///
    /// **The Windows half of this module, and until `task-1912` there was none.** A ConPTY has no foreground
    /// process group, so what a node is running is read by walking down from the process the pseudoconsole was
    /// given — and a walk with no test is a walk that answers plausibly and wrongly. Real processes rather
    /// than a pseudoterminal, because what is under test is the walk and not the console.
    ///
    /// **A shell with the program under it**, which is the shape a node really has and is what says the walk
    /// goes *through* a shell rather than stopping at one. It is also what makes this test its own: walking
    /// from the test process would find whatever shell another test in this binary had just started.
    #[cfg(windows)]
    #[test]
    fn a_real_program_is_seen_running_under_a_shell() {
        // A program that stays alive long enough to be seen and ends on its own if this test does not reach
        // its kill: `ping` against the loopback address, which is on every Windows there is.
        let mut shell = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 20 127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("start a shell with a program under it");
        // The table is a snapshot, and the child is not in it until the operating system has made it.
        let mut seen = None;
        for _ in 0..60 {
            seen = descendant_program(shell.id());
            if seen.as_deref().is_some_and(|name| !looks_like_a_shell(name)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = shell.kill();
        let _ = shell.wait();
        let seen = seen.expect("a program running under the shell is seen");
        assert_eq!(
            seen.to_ascii_lowercase(),
            "ping",
            "the program under the shell, named rather than pathed"
        );
    }

    /// `task-1945`: a process's own current directory is read out of its parameter block.
    ///
    /// Against a real process started somewhere known, because the whole of this is a pointer walk through
    /// another process's memory and the only thing that proves the offsets are right is a path coming back.
    #[cfg(windows)]
    #[test]
    fn a_processs_own_folder_is_read_out_of_it() {
        let elsewhere = std::env::temp_dir();
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 20 127.0.0.1"])
            .current_dir(&elsewhere)
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("start a process somewhere known");
        // The parameter block is filled in as the process starts, so the first read can be early.
        let mut seen = None;
        for _ in 0..60 {
            seen = folder_of(child.id());
            if seen.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        let seen = seen.expect("the folder a process is in is read out of it");
        // Compared through the operating system's own canonical form, because a temporary folder is very
        // often reached through a short name or a junction and the two spellings are the same folder.
        assert_eq!(
            std::fs::canonicalize(&seen).ok(),
            std::fs::canonicalize(&elsewhere).ok(),
            "read {seen:?}, which is not {elsewhere:?}"
        );
    }

    /// A process that is not there has no folder, rather than an answer read off nothing.
    #[cfg(windows)]
    #[test]
    fn a_process_that_is_not_there_has_no_folder() {
        assert_eq!(folder_of(0xFFFF_FFF0), None);
    }

    /// A process with nothing under it answers with nothing, which is what a shell at a prompt is.
    ///
    /// `services::space::launch::is_a_shell` is what turns a shell's own name into nothing on the other side
    /// of this; what matters here is that a leaf is a leaf rather than an error.
    #[cfg(windows)]
    #[test]
    fn a_process_with_nothing_under_it_is_running_nothing() {
        // Deliberately absurd rather than merely large: no process is running under it, so nothing can be a
        // child of it either.
        assert_eq!(descendant_program(0x7fff_fff0), None);
    }

    /// The shim is not what a node is running.
    ///
    /// A node restoring its screen starts `crate::restore`'s shim, which prints the screen and then becomes
    /// the shell — so on Windows the pseudoconsole's own child is Unluminous, and a walk that did not know it
    /// would report every restored node as running `unluminous`. Asked of this process, which during a test
    /// run is `unluminous-terminal`'s test binary and not the shim, the rule is what is checked rather than
    /// the accident.
    #[cfg(windows)]
    #[test]
    fn the_shim_is_not_what_a_node_is_running() {
        assert!(!is_the_shim("pwsh.exe"));
        assert!(
            !is_the_shim("unluminous"),
            "the window is not the shim; the command line program is"
        );
        assert!(is_the_shim("unluminous-cli"));
        assert!(is_the_shim("UNLUMINOUS-CLI.EXE"), "however the file system spells it");
    }

    /// The shell this test is running under is a real process with a real name.
    ///
    /// Not about a pseudoterminal — it is the other half, [`program_named`], asked about a process that
    /// certainly exists. Without it the platform-specific half of this module would have no test at all on
    /// either platform.
    #[cfg(unix)]
    #[test]
    fn a_process_that_exists_is_named() {
        let mine = std::process::id() as i32;
        let name = program_named(mine).expect("this process has a name");
        assert!(!name.is_empty(), "and it is not empty");
    }

    /// And a process id nothing is running under answers with nothing.
    #[cfg(unix)]
    #[test]
    fn a_process_that_is_not_there_answers_with_nothing() {
        // Deliberately absurd rather than merely large: process ids are bounded well below this on both
        // platforms, so nothing can be running under it.
        assert_eq!(program_named(0x7fff_fff0), None);
    }
}

#[cfg(target_os = "macos")]
#[cfg(test)]
mod argv_tests {
    use super::*;

    /// This process's own name is read out of its arguments, and it is the name the binary was started by.
    ///
    /// **The one thing that can be tested here without a machine-specific process id.** What made this hard is
    /// that macOS has three different names for a process and two of them are wrong for this purpose — see the
    /// table in `program_named`. A test that only checked "some name came back" would have passed on all three,
    /// which is why the example beside this file exists: `cargo run -p unluminous-terminal --example name_check
    /// -- <pid>` was what showed `claude` answering `2.1.269`.
    #[test]
    fn a_process_reads_its_own_name_out_of_its_arguments() {
        let mine = std::process::id() as i32;
        let name = argv_zero(mine).expect("this process has a name");
        // A test binary is named after the crate and a hash, so the stem is what is checked.
        assert!(
            name.contains("unluminous") || name.contains("terminal"),
            "the name is this test binary's rather than something else's: {name}"
        );
        // And it is a bare name rather than a path, which is what makes it usable as an offer.
        assert!(!name.contains('/'), "no path separators survive: {name}");
    }

    /// A process id nothing is running under answers nothing rather than a guess.
    #[test]
    fn a_process_that_is_not_there_answers_with_nothing() {
        assert_eq!(argv_zero(0x7fff_fff0), None);
    }

    /// A `UNICODE_STRING` whose length is odd is not read at all.
    ///
    /// `task-1984` P1. The length is read out of another process's memory, so nothing here decides
    /// it, and `vec![0u16; length / 2]` was one byte short of what `ReadProcessMemory` was then told
    /// to write into it -- a heap write past the end of an allocation in `unsafe` code. The guard
    /// checked zero and too large and not odd.
    #[test]
    fn a_unicode_string_of_an_odd_length_is_refused() {
        let longest = 1024;
        assert_eq!(characters_to_read(9, longest), None, "odd");
        assert_eq!(characters_to_read(1, longest), None, "odd, and the smallest one");
        assert_eq!(characters_to_read(longest - 1, longest), None, "odd, and the largest one");
        assert_eq!(characters_to_read(0, longest), None, "nothing to read");
        assert_eq!(characters_to_read(longest + 1, longest), None, "past what a path may be");
        assert_eq!(characters_to_read(8, longest), Some(4), "four sixteen bit characters");
        assert_eq!(characters_to_read(longest, longest), Some(longest / 2), "the largest one");
    }
}
