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
//! **Windows answers nothing, and that is honest rather than unfinished.** A ConPTY is a pipe, not a
//! controlling terminal, so there is no foreground process group to ask for; the control that offers to start
//! a program again simply does not appear there, which is Unluminous's own rule about a control that cannot
//! apply. The shape a later ticket would take is a process tree walk from the child handle `reap::Reaper`
//! already holds on that platform.

/// A duplicate of a session's pseudoterminal, kept only to be asked what is running in it.
///
/// **Taken in `Session::spawn`**, in the window between `tty::new` answering and `EventLoop::new` moving the
/// `Pty` into the loop — which is the same window `reap::Reaper` takes the Windows handle in, and the last
/// moment either is reachable. A duplicate, because the loop owns and closes the original.
pub struct Master {
    #[cfg(unix)]
    fd: Option<std::os::fd::OwnedFd>,
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
        #[cfg(not(unix))]
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
        #[cfg(not(unix))]
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
        #[cfg(not(unix))]
        {
            None
        }
    }
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
}
