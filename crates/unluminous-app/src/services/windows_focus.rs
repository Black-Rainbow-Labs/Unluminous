//! What Windows says about this window's keyboard focus, and telling `winit` when the two disagree.
//!
//! `egui-winit` refuses to forward `ViewportCommand::StartDrag` unless `Window::has_focus()`, and on
//! Windows `winit` answers that from a **cache** — `is_active && is_focused`, two flags it keeps up to
//! date from `WM_NCACTIVATE` and `WM_SETFOCUS`. A cache is right until it is not, and when it is not
//! there is nothing in the window that says so: the title bar simply stops moving the window, and
//! everything else about the window goes on working, because nothing else is behind that check.
//!
//! `task-2009`: *"On windows, after launch, I can't move the window around by clicking the top bar and
//! dragging, unless I first focus another window like Firefox, then focus unluminous."* That workaround
//! is the shape of the fault. Clicking the title bar of a window that is **already** the active window
//! sends no activation message at all, so nothing puts the cache right; clicking another application
//! and coming back is a real deactivate and a real activate, which is the one thing that does.
//!
//! So this asks the operating system the same question directly, and when the operating system says
//! this window is the foreground window and has the keyboard while `winit` says it has no focus, it
//! sends `winit` the two messages it is waiting for. Nothing is invented: what is sent is what Windows
//! already believes, and both answers are reported by `unluminous-cli status --section window` so the
//! next time the two disagree it is one command rather than a day of measuring.
//!
//! **It is narrower than "is this window focused".** A page in a browser tab is a native child window,
//! and while one holds the keyboard `GetFocus` answers with the child rather than with this window — so
//! nothing is sent, and `task-1945`'s rule that a page holding the keyboard really does take the title
//! bar's drag away is untouched.

/// What Windows says about this window, which is not always what `winit` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsFocus {
    /// Whether this window is the one the operating system is sending input to.
    pub foreground: bool,
    /// Whether the keyboard belongs to this window rather than to a native child inside it.
    pub keyboard: bool,
}

/// Whether `winit` has to be told what Windows already says.
///
/// The whole decision, in one place so a test can hold every case side by side: it takes three
/// answers and no window, and the act below is the only thing that needs a platform.
///
/// Both of the operating system's answers have to be yes. **Foreground**, because a window in the
/// background has no focus and saying it has would be a lie that reaches egui as a blinking caret in a
/// window nobody is typing into. **Keyboard**, because a browser tab's page is a native child window
/// and while one holds the keyboard `winit` is right to say this window does not.
pub fn winit_is_behind(os: OsFocus, winit_says_focused: bool) -> bool {
    os.foreground && os.keyboard && !winit_says_focused
}

#[cfg(windows)]
mod platform {
    use super::OsFocus;
    use eframe::wgpu::rwh::{HasWindowHandle, RawWindowHandle};

    /// The Win32 window handle behind an eframe window, as an integer so that nothing holds a pointer.
    ///
    /// Absent when there is no window at all, which is the case in every screenshot test: those render
    /// offscreen and have no operating system window to ask about.
    fn window_handle(window: &impl HasWindowHandle) -> Option<isize> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        }
    }

    /// Ask Windows where the foreground and the keyboard are.
    ///
    /// `GetFocus` answers about the **calling thread's** own queue, which is this window's thread, so
    /// it is the child-aware question: a `WebView2` child that has taken the keyboard is what it
    /// answers with, and that is exactly the case this must not resynchronise.
    pub fn ask(window: &impl HasWindowHandle) -> Option<OsFocus> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let hwnd = window_handle(window)?;
        // SAFETY: both take no arguments and answer with a window handle or null.
        let (foreground, focus) = unsafe { (GetForegroundWindow(), GetFocus()) };
        Some(OsFocus { foreground: foreground as isize == hwnd, keyboard: focus as isize == hwnd })
    }

    /// Tell `winit` what Windows already says, by sending it the two messages it keeps its cache from.
    ///
    /// `WM_NCACTIVATE` first and `WM_SETFOCUS` second, which is the order a real activation arrives in
    /// and the order in which `winit` emits its own `Focused(true)`: it compares `is_active &&
    /// is_focused` before and after each one, so a `WM_SETFOCUS` on its own moves a flag and reports
    /// nothing.
    pub fn tell_winit(window: &impl HasWindowHandle) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageW, WM_NCACTIVATE, WM_SETFOCUS,
        };
        let Some(hwnd) = window_handle(window) else { return };
        // SAFETY: the handle came from the window this call is made on, and both messages go to that
        // window's own procedure on its own thread. Neither carries a pointer.
        unsafe {
            SendMessageW(hwnd as _, WM_NCACTIVATE, 1, 0);
            SendMessageW(hwnd as _, WM_SETFOCUS, 0, 0);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::OsFocus;
    use eframe::wgpu::rwh::HasWindowHandle;

    /// No answer anywhere but Windows, which is the one platform whose `winit` keeps this cache.
    pub fn ask(_window: &impl HasWindowHandle) -> Option<OsFocus> {
        None
    }

    /// Nothing to tell, for the same reason.
    pub fn tell_winit(_window: &impl HasWindowHandle) {}
}

pub use platform::ask;

/// Ask, and put `winit` right when it is behind. Answers what Windows said, for `status` to report.
///
/// Called once a frame. It costs two calls into the window manager and nothing else, and the act
/// behind it happens on the frame the two disagree and on no other — the same bargain
/// `services::browser::the_focus` and `NativeView::clip` already make.
pub fn settle(
    window: &impl eframe::wgpu::rwh::HasWindowHandle,
    winit_says_focused: bool,
) -> Option<OsFocus> {
    let os = ask(window)?;
    if winit_is_behind(os, winit_says_focused) {
        platform::tell_winit(window);
    }
    Some(os)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The only case worth sending anything in: Windows says this window is the one being typed into
    /// and `winit` says it has no focus at all.
    #[test]
    fn winit_is_told_only_when_windows_says_this_window_has_the_keyboard() {
        let has_it = OsFocus { foreground: true, keyboard: true };
        assert!(winit_is_behind(has_it, false), "the fault this exists for");
        assert!(!winit_is_behind(has_it, true), "and nothing to do when the two agree");
    }

    /// A window in the background has no focus, and saying it has would be a lie egui would draw.
    #[test]
    fn a_window_in_the_background_is_left_alone() {
        let background = OsFocus { foreground: false, keyboard: true };
        assert!(!winit_is_behind(background, false));
    }

    /// And a page that has taken the keyboard is `task-1945`'s own case: `winit` is right there, and
    /// the title bar's drag is meant to be refused until the page gives the keyboard back.
    #[test]
    fn a_page_holding_the_keyboard_is_left_alone() {
        let page_has_it = OsFocus { foreground: true, keyboard: false };
        assert!(!winit_is_behind(page_has_it, false));
    }
}
