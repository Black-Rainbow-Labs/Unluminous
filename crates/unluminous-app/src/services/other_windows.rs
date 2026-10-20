//! The other Unluminous windows open on this machine, and bringing one of them to the front.
//!
//! `task-2317`: *"Open up Unluminous in two different folders at the same time on a mac. Hit CMD+~. Nothing
//! happens. I'd expect they'd switch between the two windows."* macOS's own `Cmd+`` moves between the windows
//! of **one process**, and every Unluminous project window is a process of its own, so the system had no other
//! window to move to. `Next Window` and `Previous Window` are menu entries on that chord instead, and an app's
//! own binding is what macOS hands the press to: Photoshop and the reference editor keep `Cmd+`` for their own
//! window switching the same way.
//!
//! The windows are the instance files every window writes for the command line (`unluminous_cli::instances`),
//! so this needs nothing a window does not already say about itself. They are cycled in the order they were
//! started, which does not change as the focus moves, so pressing the chord repeatedly visits every window.

/// One Unluminous window open on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub pid: u32,
    pub folder: std::path::PathBuf,
    /// Seconds since the epoch, which is the order the windows are cycled in.
    pub started: u64,
}

/// Every Unluminous window that is running, oldest first, this one included.
pub fn running() -> Vec<Window> {
    let mut found: Vec<Window> = unluminous_cli::instances::listed()
        .into_iter()
        .filter(|instance| unluminous_cli::instances::is_running(instance.pid))
        .map(|instance| Window {
            pid: instance.pid,
            folder: instance.folder,
            started: instance.started,
        })
        .collect();
    in_turn(&mut found);
    found
}

/// Put `windows` in the order they are visited, oldest first, with the process id settling a tie.
pub fn in_turn(windows: &mut Vec<Window>) {
    windows.sort_by(|a, b| a.started.cmp(&b.started).then(a.pid.cmp(&b.pid)));
    windows.dedup_by_key(|window| window.pid);
}

/// The window `step` places after `me` in `windows`, going round at either end. `None` when there is no other
/// window. A window that is not in the list, which is one started with `--control off`, starts from the first.
pub fn the_one_after(windows: &[Window], me: u32, step: isize) -> Option<&Window> {
    let others = windows.iter().filter(|window| window.pid != me).count();
    if others == 0 {
        return None;
    }
    let count = windows.len() as isize;
    let at = windows.iter().position(|window| window.pid == me);
    let next = match at {
        Some(at) => (at as isize + step).rem_euclid(count),
        None if step >= 0 => 0,
        None => count - 1,
    };
    windows.get(next as usize)
}

/// Bring the window of process `pid` to the front and give it the keyboard.
///
/// Asked from the window that has the keyboard, because only the foreground process may hand the foreground to
/// another one: `SetForegroundWindow` on Windows, and on macOS `NSRunningApplication`'s `activateWithOptions:`,
/// which an active app may call for another.
pub fn bring_forward(pid: u32) -> Result<(), String> {
    platform::bring_forward(pid)
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
    };

    /// What `EnumWindows` is looking for, and what it found.
    struct Search {
        pid: u32,
        found: HWND,
    }

    /// A visible window with no owner that belongs to the process, which is the project window rather than a
    /// tooltip, a native child or the hidden window the graphics driver keeps.
    unsafe extern "system" fn look(window: HWND, data: LPARAM) -> i32 {
        let search = unsafe { &mut *(data as *mut Search) };
        let mut owner_pid = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut owner_pid) };
        let top_level = unsafe { GetWindow(window, GW_OWNER) }.is_null();
        if owner_pid == search.pid && top_level && unsafe { IsWindowVisible(window) } != 0 {
            search.found = window;
            return 0;
        }
        1
    }

    pub fn bring_forward(pid: u32) -> Result<(), String> {
        let mut search = Search { pid, found: std::ptr::null_mut() };
        unsafe { EnumWindows(Some(look), &mut search as *mut Search as LPARAM) };
        if search.found.is_null() {
            return Err(format!(
                "The Unluminous window of process {pid} has no window on the screen."
            ));
        }
        unsafe {
            if IsIconic(search.found) != 0 {
                ShowWindow(search.found, SW_RESTORE);
            }
            if SetForegroundWindow(search.found) == 0 {
                return Err(
                    "Windows would not bring the other Unluminous window forward.".to_owned()
                );
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    /// `NSApplicationActivateAllWindows` and `NSApplicationActivateIgnoringOtherApps`.
    const ALL_WINDOWS_AND_NOW: usize = (1 << 0) | (1 << 1);

    pub fn bring_forward(pid: u32) -> Result<(), String> {
        let app: Option<Retained<AnyObject>> = unsafe {
            msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid as i32]
        };
        let Some(app) = app else {
            return Err(format!("There is no running Unluminous with process {pid}."));
        };
        let activated: bool = unsafe { msg_send![&*app, activateWithOptions: ALL_WINDOWS_AND_NOW] };
        match activated {
            true => Ok(()),
            false => Err("macOS would not bring the other Unluminous window forward.".to_owned()),
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    pub fn bring_forward(_pid: u32) -> Result<(), String> {
        Err("Moving to another Unluminous window is done by the window manager here.".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(pid: u32, started: u64) -> Window {
        Window { pid, folder: std::path::PathBuf::from(format!("project-{pid}")), started }
    }

    /// Pressing the chord again and again visits every window and comes back, in either direction.
    #[test]
    fn the_windows_are_visited_in_the_order_they_were_started_and_round_again() {
        let mut windows = vec![window(30, 300), window(10, 100), window(20, 200), window(10, 100)];
        in_turn(&mut windows);
        assert_eq!(windows.iter().map(|one| one.pid).collect::<Vec<_>>(), vec![10, 20, 30]);
        assert_eq!(the_one_after(&windows, 10, 1).map(|one| one.pid), Some(20));
        assert_eq!(the_one_after(&windows, 30, 1).map(|one| one.pid), Some(10));
        assert_eq!(the_one_after(&windows, 10, -1).map(|one| one.pid), Some(30));
        assert_eq!(the_one_after(&windows, 20, -1).map(|one| one.pid), Some(10));
        // A window the list does not hold starts from an end.
        assert_eq!(the_one_after(&windows, 99, 1).map(|one| one.pid), Some(10));
        assert_eq!(the_one_after(&windows, 99, -1).map(|one| one.pid), Some(30));
    }

    /// One window alone has nowhere to go.
    #[test]
    fn a_window_on_its_own_has_no_other_window() {
        let windows = vec![window(10, 100)];
        assert_eq!(the_one_after(&windows, 10, 1), None);
        assert_eq!(the_one_after(&[], 10, 1), None);
    }
}
