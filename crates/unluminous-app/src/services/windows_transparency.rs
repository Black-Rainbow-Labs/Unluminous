//! Letting the desktop show through the window on Windows.
//!
//! macOS needs nothing here: `ViewportBuilder::with_transparent(true)` is enough, the compositor takes
//! the surface's alpha and that is the end of it. Windows needs two separate things, and the window
//! stays solid if either one is missing. Both were found by measurement, with a probe that painted
//! half of a window at a quarter alpha over a coloured backdrop and read the pixels back.
//!
//! **The swapchain has to be able to carry alpha.** A DXGI swapchain made from a window handle offers
//! `CompositeAlphaMode::Opaque` and nothing else, so `egui-wgpu` has no transparent mode to ask for
//! and the window is composited solid however transparent egui paints it. A swapchain made from a
//! DirectComposition visual offers `PreMultiplied`, which `egui-wgpu` picks by itself once it is
//! offered. wgpu only builds that kind on its DX12 backend, and left to itself wgpu picked Vulkan on
//! the machine this was measured on — which is why setting `WGPU_DX12_PRESENTATION_SYSTEM=visual`
//! alone had never changed anything. So the backend is named as well as the swapchain kind.
//!
//! **The window's redirection surface has to be transparent.** Every ordinary window has one: a GDI
//! bitmap the desktop window manager composites the window from. Winit asks the manager to honour its
//! alpha, with `DwmEnableBlurBehindWindow` over an empty region, but registers the window class with
//! no background brush and never paints into it, so the surface keeps whatever undefined bytes it was
//! allocated with. Those bytes read as opaque white, which is exactly what the window showed: the
//! theme faded towards white rather than towards the desktop. GDI writes zero into the alpha byte of
//! every pixel it touches, so one black fill over the whole client area is what makes the surface
//! disappear. This is the same fault GLFW has, and the fix is the one its own bug report proposes.
//!
//! **Nothing of Windows' own may be drawn round the window**, or it shows as a square edge outside the
//! window's rounded corners: a drop shadow, a black line along the top and Windows 11's one pixel
//! border all did (`task-2282`). `take_the_frame_away` says how each is turned off.
//!
//! `WS_EX_NOREDIRECTIONBITMAP` removes the surface instead of clearing it, and was measured to work
//! too. It is not used: eframe gives no way to pass a winit window attribute through, and the Win32
//! reference implementations advise against it because it breaks any presentation path that blits
//! into that surface.

use eframe::wgpu::rwh::{HasWindowHandle, RawWindowHandle};
use eframe::wgpu::{Backends, Dx12SwapchainKind};

/// Ask wgpu for the one swapchain on Windows that can be translucent.
///
/// Both choices still respect wgpu's own environment variables, so `WGPU_BACKEND` and
/// `WGPU_DX12_PRESENTATION_SYSTEM` remain a way out on a machine where DX12 is not the right answer.
/// The window is then opaque, which is what it was before this existed.
pub fn with_direct_composition(mut options: eframe::NativeOptions) -> eframe::NativeOptions {
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.backends = Backends::from_env().unwrap_or(Backends::DX12);
        setup.instance_descriptor.backend_options.dx12.presentation_system =
            Dx12SwapchainKind::DxgiFromVisual.with_env();
    }
    options
}

/// Keep the window's redirection surface transparent, once a frame.
///
/// It is filled every frame rather than only when it looks as though Windows has given the window a
/// new one. Filling once is not enough and there is no event that says when it is: a fill while the
/// window is still hidden is thrown away, and eframe deliberately keeps the window hidden until it
/// has painted its first frame, so the obvious place to do it once is exactly the place that does
/// not work. Guessing at the rest — a resize, a move to a screen at another scale, a restore from
/// the taskbar — would be a list to keep up to date rather than a rule.
///
/// It costs nothing worth saving. The fill was measured at 0.06 ms for a 1100 by 720 window and
/// 0.04 ms for one filling a 4K screen, because it is a write into a surface the compositor owns
/// rather than anything the graphics card has to draw. It was measured again for `task-1666`, over
/// six hundred frames of a real window at 1100 by 720: **0.036 ms**. That is a five hundredth of a
/// frame at sixty a second, and it is now one of the larger single pieces of work in an idle frame
/// — which says how little is left in one, not that this is worth changing.
pub fn keep_transparent(window: &impl HasWindowHandle) {
    let Some(hwnd) = window_handle(window) else { return };
    take_the_frame_away(hwnd);
    let Some((width, height)) = client_size(hwnd) else { return };
    fill(hwnd, width, height);
}

/// The Win32 window handle behind an eframe window, as an integer so that nothing holds a pointer.
///
/// Absent when there is no window at all, which is the case in every screenshot test: those render
/// offscreen and have no desktop to show through.
fn window_handle(window: &impl HasWindowHandle) -> Option<isize> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

/// The window whose frame was last taken away, so it is done once and not every frame.
static FRAMELESS: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Take away everything Windows draws round the window, once for each window handle.
///
/// Two things were drawn there, and between them they made a square edge outside each of the window's
/// rounded corners (`task-2282`):
///
/// - **A drop shadow and a black line along the top.** `egui-winit` asks `winit` for its undecorated
///   shadow on every window with no title bar, and `winit` makes one by leaving a single row of the
///   window as frame in `WM_NCCALCSIZE`, which the desktop window manager answers with a shadow round
///   the whole rectangle. Its own comment says the row shows as a black line. The shadow hugs the
///   rectangle and stops at its edge, so outside a rounded corner the desktop showed through with no
///   shadow on it, framed by shadow on both sides. The macOS window asks for no shadow, and this makes
///   Windows the same: the row is given back to the window, and with no frame there is no shadow.
/// - **Windows 11's one pixel border**, which `DWMWA_COLOR_NONE` turns off. The corner preference is
///   set to square as well, so the window manager never clips the window to corners of its own.
///
/// Windows 10 has neither the border nor the corner attribute and refuses both, which changes nothing.
///
/// @param hwnd - the window
fn take_the_frame_away(hwnd: isize) {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    };
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    };
    if FRAMELESS.swap(hwnd, Ordering::SeqCst) == hwnd {
        return;
    }
    let colour: u32 = DWMWA_COLOR_NONE;
    let corners: i32 = DWMWCP_DONOTROUND;
    unsafe {
        DwmSetWindowAttribute(
            hwnd as _,
            DWMWA_BORDER_COLOR as u32,
            (&colour as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        DwmSetWindowAttribute(
            hwnd as _,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&corners as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        SetWindowSubclass(hwnd as _, Some(no_frame_row), FRAME_SUBCLASS, 0);
        // The frame is measured again only when Windows is told it has changed.
        let flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
        SetWindowPos(hwnd as _, std::ptr::null_mut(), 0, 0, 0, 0, flags);
    }
}

/// The id the frame's subclass is installed under.
const FRAME_SUBCLASS: usize = 0x2282;

/// Lets `winit` answer `WM_NCCALCSIZE` and then gives back the one row it kept as frame for its shadow.
///
/// Only that answer is changed: when `winit` has moved the client rectangle down by exactly one row,
/// which is what it does for a window that is not maximised and not full screen. A maximised window's
/// answer is the screen's work area and is left exactly as `winit` gave it.
unsafe extern "system" fn no_frame_row(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
    _id: usize,
    _data: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{NCCALCSIZE_PARAMS, WM_NCCALCSIZE};
    if message != WM_NCCALCSIZE || wparam == 0 {
        return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    }
    let params = lparam as *mut NCCALCSIZE_PARAMS;
    let proposed = unsafe { (*params).rgrc[0] };
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    let answer = unsafe { &mut (*params).rgrc[0] };
    *answer = without_the_frame_row(proposed, *answer);
    result
}

/// The client rectangle to answer with: the proposed one when `winit` only moved it down a row for its
/// shadow, and `winit`'s own answer otherwise.
///
/// @param proposed - the rectangle Windows proposed, which is the whole window
/// @param answer - the rectangle `winit` answered with
fn without_the_frame_row(
    proposed: windows_sys::Win32::Foundation::RECT,
    answer: windows_sys::Win32::Foundation::RECT,
) -> windows_sys::Win32::Foundation::RECT {
    let one_row_down = answer.left == proposed.left
        && answer.right == proposed.right
        && answer.top == proposed.top + 1
        && answer.bottom == proposed.bottom + 1;
    match one_row_down {
        true => proposed,
        false => answer,
    }
}

/// The window's client area, in pixels.
fn client_size(hwnd: isize) -> Option<(i32, i32)> {
    let mut rect = windows_sys::Win32::Foundation::RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let read =
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd as _, &mut rect) };
    if read == 0 {
        return None;
    }
    Some((rect.right - rect.left, rect.bottom - rect.top))
}

/// Fill the whole client area with black through GDI, which sets every pixel's alpha to zero.
fn fill(hwnd: isize, width: i32, height: i32) {
    use windows_sys::Win32::Graphics::Gdi::{GetDC, PatBlt, ReleaseDC, BLACKNESS};
    unsafe {
        let device_context = GetDC(hwnd as _);
        if device_context.is_null() {
            return;
        }
        PatBlt(device_context, 0, 0, width, height, BLACKNESS);
        ReleaseDC(hwnd as _, device_context);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the window can be translucent at all is decided here, before a window exists, so it is
    /// worth a test: the two settings together are the difference between a window that blends and one
    /// that is solid, and neither of them is visible in anything the tests can render.
    #[test]
    fn the_swapchain_is_asked_for_on_the_one_backend_that_can_carry_alpha() {
        let options = with_direct_composition(eframe::NativeOptions::default());
        let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &options.wgpu_options.wgpu_setup
        else {
            panic!("eframe is expected to be creating its own wgpu instance");
        };
        assert_eq!(
            setup.instance_descriptor.backends,
            Backends::DX12,
            "Vulkan offers no transparent composite mode on Windows, so the backend has to be named"
        );
        assert_eq!(
            setup.instance_descriptor.backend_options.dx12.presentation_system,
            Dx12SwapchainKind::DxgiFromVisual,
            "a swapchain made from the window handle can only be opaque"
        );
    }

    /// The row `winit` keeps for its shadow is given back, and nothing else it answers is touched: a
    /// maximised window's answer is the work area, which must stay exactly as `winit` gave it.
    #[test]
    fn only_the_shadow_row_is_given_back_to_the_window() {
        use windows_sys::Win32::Foundation::RECT;
        let rect = |left, top, right, bottom| RECT { left, top, right, bottom };
        let window = rect(200, 200, 1100, 800);
        let shadowed = rect(200, 201, 1100, 801);
        let given_back = without_the_frame_row(window, shadowed);
        assert_eq!((given_back.top, given_back.bottom), (200, 800));
        let maximised = without_the_frame_row(rect(-8, -8, 3848, 2168), rect(0, 0, 3840, 2112));
        assert_eq!(
            (maximised.left, maximised.top, maximised.right, maximised.bottom),
            (0, 0, 3840, 2112)
        );
    }

    /// Nothing happens when there is no window, which is what every screenshot test has.
    #[test]
    fn a_handle_with_no_window_behind_it_is_left_alone() {
        struct Nothing;
        impl HasWindowHandle for Nothing {
            fn window_handle(
                &self,
            ) -> Result<eframe::wgpu::rwh::WindowHandle<'_>, eframe::wgpu::rwh::HandleError>
            {
                Err(eframe::wgpu::rwh::HandleError::NotSupported)
            }
        }
        keep_transparent(&Nothing);
    }
}
