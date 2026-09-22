//! On Windows the window says where its own edges are, so Windows resizes it the way it resizes any
//! other window.
//!
//! `task-2063`: *"I still have issues resizing the window from time to time. e.g. can resize from left
//! side."* The window has no operating system frame, so `components::resize_edges` drew eight invisible
//! egui grips and sent `ViewportCommand::BeginResize` when one was dragged. That goes wrong three ways,
//! and each explains *"from time to time"*:
//!
//! 1. A grip fires when egui decides a drag **started**, which is after the pointer has moved past egui's
//!    drag threshold with the button held. On the left edge that movement is away from the window, and a
//!    quick flick has already let go of the button by the time `winit` posts `WM_NCLBUTTONDOWN` on the
//!    event loop thread. A button that is up starts no size loop.
//! 2. `winit` latches a private `dragging` flag when it posts that message, and only `WM_EXITSIZEMOVE`
//!    clears it. A request that starts no size loop sends no `WM_EXITSIZEMOVE`, so every later resize
//!    **and every title bar drag** does nothing until Unluminous is restarted.
//! 3. Anything egui draws on a layer above the panes and over the edge — a popup, a context menu, a
//!    toast, a canvas node — takes the pointer away from the grip underneath it.
//!
//! Chromium, Electron, Windows Terminal and Tauri's undecorated windows all solve this the same way:
//! they answer `WM_NCHITTEST` with `HTLEFT`, `HTTOPLEFT` and the rest for the pixels near an edge.
//! Windows then does what it does for a framed window. It shows the arrow, starts the size loop on the
//! press itself, offers Aero Snap, and never involves the application's own drag state. That is
//! [`install`]: a subclass of the window's procedure that answers the hit test and passes every other
//! message on unchanged.
//!
//! **The title bar's drag still goes through `winit`**, because a double click on it maximises the
//! window and egui has to see both clicks. So fault 2 can still happen there, and [`Unlatch`] clears it:
//! when a drag was asked for and a moment later the thread is in no move or size loop and the button is
//! up, the window posts `WM_EXITSIZEMOVE` to itself, which is the one message `winit` clears the flag on.

/// Which edge or corner a point is on, when it is near one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Which edge a point `(x, y)` inside a window `width` by `height` is on, all in physical pixels.
///
/// `border` is how far in from an edge counts as the edge, and `corner` how far along an edge counts as
/// its corner, which is further so a corner is easy to hit. A maximised window has no edges: it has no
/// size to change, and Windows refuses `SC_SIZE` on one.
///
/// Pure, so every case is a test with no window.
pub fn hit_for(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    border: i32,
    corner: i32,
    maximised: bool,
) -> Option<Edge> {
    if maximised || x < 0 || y < 0 || x >= width || y >= height {
        return None;
    }
    let left = x < border;
    let right = x >= width - border;
    let top = y < border;
    let bottom = y >= height - border;
    // A corner reaches `corner` along each of the two edges that meet there.
    let near_left = x < corner;
    let near_right = x >= width - corner;
    let near_top = y < corner;
    let near_bottom = y >= height - corner;
    match () {
        _ if (top && near_left) || (left && near_top) => Some(Edge::TopLeft),
        _ if (top && near_right) || (right && near_top) => Some(Edge::TopRight),
        _ if (bottom && near_left) || (left && near_bottom) => Some(Edge::BottomLeft),
        _ if (bottom && near_right) || (right && near_bottom) => Some(Edge::BottomRight),
        _ if left => Some(Edge::Left),
        _ if right => Some(Edge::Right),
        _ if top => Some(Edge::Top),
        _ if bottom => Some(Edge::Bottom),
        _ => None,
    }
}

/// The rectangles the hit test leaves to the window, in physical pixels from the window's top left.
///
/// `task-2062` made egui's grips give up the points where a pane divider reaches the window's edge,
/// because the divider is what a person pressing there is aiming at. The hit test answers for the same
/// edges, so it gives up the same points: [`keep_clear`] writes the dividers here once a frame and the
/// window procedure reads them. A lock rather than a message, because the procedure runs on the same
/// thread between frames and needs only the last frame's list.
static KEEP_CLEAR: std::sync::Mutex<Vec<[i32; 4]>> = std::sync::Mutex::new(Vec::new());

/// Say which rectangles, in points, the hit test must leave to egui.
pub fn keep_clear(rects: &[egui::Rect], pixels_per_point: f32) {
    let scaled: Vec<[i32; 4]> = rects
        .iter()
        .map(|rect| {
            [
                (rect.left() * pixels_per_point).floor() as i32,
                (rect.top() * pixels_per_point).floor() as i32,
                (rect.right() * pixels_per_point).ceil() as i32,
                (rect.bottom() * pixels_per_point).ceil() as i32,
            ]
        })
        .collect();
    if let Ok(mut held) = KEEP_CLEAR.lock() {
        *held = scaled;
    }
}

/// Whether a point, in physical pixels from the window's top left, is one the hit test leaves alone.
pub fn is_kept_clear(x: i32, y: i32) -> bool {
    KEEP_CLEAR.lock().is_ok_and(|held| {
        held.iter()
            .any(|[left, top, right, bottom]| x >= *left && x < *right && y >= *top && y < *bottom)
    })
}

/// Points to physical pixels at a window's DPI, rounded, and never less than one pixel.
pub fn pixels(points: f32, dpi: u32) -> i32 {
    ((points * dpi as f32 / 96.0).round() as i32).max(1)
}

/// Whether a drag the window asked `winit` for has left `winit`'s flag stuck.
///
/// Pure: the frame hands in when the drag was asked for, what time it is, and what Windows says, and
/// this answers whether to clear the flag. [`GRACE`] is how long a size or move loop is given to begin,
/// and [`WATCH`] how long after a request the window keeps asking.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Unlatch {
    /// When the last `StartDrag` or `BeginResize` was sent, in seconds of egui's clock.
    pub asked_at: Option<f64>,
}

/// How long a move or size loop is given to begin after it was asked for.
pub const GRACE: f64 = 0.3;
/// How long after a request the window goes on checking whether it was stuck.
pub const WATCH: f64 = 3.0;

impl Unlatch {
    /// Note that a drag or a resize was just asked for.
    pub fn asked(&mut self, now: f64) {
        self.asked_at = Some(now);
    }

    /// Whether to post `WM_EXITSIZEMOVE` now, and forget the request when the answer is settled.
    ///
    /// Clear it when the request is old enough for a loop to have begun, no loop is running, and the
    /// button is up. A loop that is running is the ordinary case, and ends with its own
    /// `WM_EXITSIZEMOVE`. A button still held means the person may still be pressing, so the answer
    /// waits.
    pub fn settle(&mut self, now: f64, in_a_loop: bool, button_down: bool) -> bool {
        let Some(asked) = self.asked_at else { return false };
        let age = now - asked;
        if age > WATCH || in_a_loop {
            self.asked_at = None;
            return false;
        }
        if age < GRACE || button_down {
            return false;
        }
        self.asked_at = None;
        true
    }
}

#[cfg(windows)]
mod platform {
    use super::{hit_for, pixels, Edge};
    use eframe::wgpu::rwh::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, IsZoomed, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTRIGHT,
        HTTOP, HTTOPLEFT, HTTOPRIGHT, WM_NCDESTROY, WM_NCHITTEST,
    };

    /// The number the subclass is registered under. Any value, as long as it is this one each time.
    const SUBCLASS: usize = 0x2063;

    fn window_handle(window: &impl HasWindowHandle) -> Option<isize> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        }
    }

    /// Answer `WM_NCHITTEST` for the window's edges. True once it is in place.
    pub fn install(window: &impl HasWindowHandle) -> bool {
        let Some(hwnd) = window_handle(window) else { return false };
        // SAFETY: the handle is this thread's own window, which is what `SetWindowSubclass` requires,
        // and `procedure` has the signature it asks for.
        unsafe { SetWindowSubclass(hwnd as HWND, Some(procedure), SUBCLASS, 0) != 0 }
    }

    unsafe extern "system" fn procedure(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        match message {
            WM_NCHITTEST => {
                let answer = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
                if answer != HTCLIENT as LRESULT {
                    return answer;
                }
                match unsafe { edge_at(hwnd, lparam) } {
                    Some(edge) => code(edge) as LRESULT,
                    None => answer,
                }
            }
            WM_NCDESTROY => {
                unsafe { RemoveWindowSubclass(hwnd, Some(procedure), SUBCLASS) };
                unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
            }
            _ => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
        }
    }

    /// The edge a hit test's point is on. The point arrives in screen coordinates, two signed words.
    unsafe fn edge_at(hwnd: HWND, lparam: LPARAM) -> Option<Edge> {
        let x = (lparam & 0xffff) as u16 as i16 as i32;
        let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return None;
        }
        let dpi = match unsafe { GetDpiForWindow(hwnd) } {
            0 => 96,
            dpi => dpi,
        };
        let border = pixels(crate::components::resize_edges::EDGE, dpi);
        let corner = pixels(crate::components::resize_edges::CORNER, dpi);
        let maximised = unsafe { IsZoomed(hwnd) } != 0;
        if super::is_kept_clear(x - rect.left, y - rect.top) {
            return None;
        }
        hit_for(
            x - rect.left,
            y - rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            border,
            corner,
            maximised,
        )
    }

    fn code(edge: Edge) -> u32 {
        match edge {
            Edge::Left => HTLEFT,
            Edge::Right => HTRIGHT,
            Edge::Top => HTTOP,
            Edge::Bottom => HTBOTTOM,
            Edge::TopLeft => HTTOPLEFT,
            Edge::TopRight => HTTOPRIGHT,
            Edge::BottomLeft => HTBOTTOMLEFT,
            Edge::BottomRight => HTBOTTOMRIGHT,
        }
    }

    /// Whether this thread is inside a move or size loop, and whether the left button is held.
    pub fn loop_and_button() -> (bool, bool) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetGUIThreadInfo, GUITHREADINFO, GUI_INMOVESIZE,
        };
        // SAFETY: a zeroed structure with its size filled in is what `GetGUIThreadInfo` asks for, and
        // thread 0 is the calling thread.
        let mut info: GUITHREADINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        let in_a_loop =
            unsafe { GetGUIThreadInfo(0, &mut info) } != 0 && info.flags & GUI_INMOVESIZE != 0;
        // SAFETY: takes a key code and answers a state.
        let button_down = unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } as u16 & 0x8000 != 0;
        (in_a_loop, button_down)
    }

    /// Post `WM_EXITSIZEMOVE` to the window, which is the one message `winit` clears its flag on.
    pub fn unlatch(window: &impl HasWindowHandle) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_EXITSIZEMOVE};
        let Some(hwnd) = window_handle(window) else { return };
        // SAFETY: a message with no pointer in it, posted to this thread's own window.
        unsafe { PostMessageW(hwnd as HWND, WM_EXITSIZEMOVE, 0, 0) };
    }
}

#[cfg(not(windows))]
mod platform {
    use eframe::wgpu::rwh::HasWindowHandle;

    /// Nothing to install: macOS and Linux keep egui's grips.
    pub fn install(_window: &impl HasWindowHandle) -> bool {
        false
    }

    pub fn loop_and_button() -> (bool, bool) {
        (true, false)
    }

    pub fn unlatch(_window: &impl HasWindowHandle) {}
}

pub use platform::install;

/// Clear `winit`'s drag flag if the last drag this window asked for left it stuck. Called once a frame.
pub fn settle(window: &impl eframe::wgpu::rwh::HasWindowHandle, unlatch: &mut Unlatch, now: f64) {
    if unlatch.asked_at.is_none() {
        return;
    }
    let (in_a_loop, button_down) = platform::loop_and_button();
    if unlatch.settle(now, in_a_loop, button_down) {
        platform::unlatch(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: i32 = 1000;
    const H: i32 = 700;

    fn at(x: i32, y: i32) -> Option<Edge> {
        hit_for(x, y, W, H, 6, 16, false)
    }

    #[test]
    fn every_edge_and_corner_is_found_and_the_inside_is_not() {
        assert_eq!(at(0, 350), Some(Edge::Left));
        assert_eq!(at(5, 350), Some(Edge::Left));
        assert_eq!(at(6, 350), None, "six pixels in is the window, not its edge");
        assert_eq!(at(W - 1, 350), Some(Edge::Right));
        assert_eq!(at(500, 0), Some(Edge::Top));
        assert_eq!(at(500, H - 1), Some(Edge::Bottom));
        assert_eq!(at(0, 0), Some(Edge::TopLeft));
        assert_eq!(at(15, 2), Some(Edge::TopLeft), "a corner reaches further along the edge");
        assert_eq!(at(2, 15), Some(Edge::TopLeft));
        assert_eq!(at(W - 1, 0), Some(Edge::TopRight));
        assert_eq!(at(0, H - 1), Some(Edge::BottomLeft));
        assert_eq!(at(W - 1, H - 1), Some(Edge::BottomRight));
        assert_eq!(at(500, 350), None);
    }

    #[test]
    fn a_maximised_window_and_a_point_outside_have_no_edge() {
        assert_eq!(hit_for(0, 350, W, H, 6, 16, true), None);
        assert_eq!(at(-1, 350), None);
        assert_eq!(at(W, 350), None);
    }

    /// A divider at the window's edge keeps its points, as `task-2062` made the grips do.
    #[test]
    fn a_dividers_points_are_left_to_the_divider() {
        keep_clear(
            &[egui::Rect::from_min_max(egui::pos2(300.0, 0.0), egui::pos2(308.0, 700.0))],
            1.5,
        );
        assert!(is_kept_clear(455, 2), "inside the divider, at the top edge");
        assert!(!is_kept_clear(10, 2), "the rest of the top edge is still the window's");
        keep_clear(&[], 1.0);
    }

    #[test]
    fn points_become_pixels_at_the_windows_scale() {
        assert_eq!(pixels(6.0, 96), 6);
        assert_eq!(pixels(6.0, 144), 9);
        assert_eq!(pixels(6.0, 192), 12);
        assert_eq!(pixels(0.1, 96), 1, "never less than a pixel");
    }

    #[test]
    fn a_stuck_drag_is_cleared_once_and_only_when_nothing_is_happening() {
        let mut unlatch = Unlatch::default();
        assert!(!unlatch.settle(1.0, false, false), "nothing asked, nothing to clear");

        unlatch.asked(1.0);
        assert!(!unlatch.settle(1.1, false, false), "too soon for a loop to have begun");
        assert!(!unlatch.settle(1.5, false, true), "the button is still held");
        assert!(unlatch.settle(1.5, false, false), "no loop, button up: stuck");
        assert!(!unlatch.settle(1.6, false, false), "and it is cleared once");

        unlatch.asked(2.0);
        assert!(!unlatch.settle(2.5, true, false), "a loop running is the ordinary case");
        assert!(!unlatch.settle(2.6, false, false), "and that request is forgotten");

        unlatch.asked(3.0);
        assert!(!unlatch.settle(3.0 + WATCH + 0.1, false, false), "too old to be about this");
    }
}
