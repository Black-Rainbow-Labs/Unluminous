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
        // `proc_name` writes a name of at most `MAXCOMLEN * 2 + 1` bytes; 256 is comfortably above it and is
        // what Apple's own examples use.
        let mut buffer = [0_i8; 256];
        // Safe: the buffer is ours, and the length passed is its real length.
        let written = unsafe {
            proc_name(pid, buffer.as_mut_ptr() as *mut std::ffi::c_void, buffer.len() as u32)
        };
        if written <= 0 {
            return None;
        }
        let bytes: Vec<u8> = buffer[..written as usize].iter().map(|byte| *byte as u8).collect();
        let name = String::from_utf8_lossy(&bytes).trim().to_owned();
        (!name.is_empty()).then_some(name)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let text = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        let name = text.trim().to_owned();
        (!name.is_empty()).then_some(name)
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    /// libproc's own name for a process, which is what Activity Monitor shows.
    fn proc_name(pid: i32, buffer: *mut std::ffi::c_void, buffersize: u32) -> i32;
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
