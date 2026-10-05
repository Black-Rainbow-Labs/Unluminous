//! Files on the operating system's clipboard, and where the pointer is while files from another
//! program are carried over the window.
//!
//! `task-2194` asks for the folder pane to take files the way a file manager does: copied in the
//! Finder or in Explorer and pasted into a folder here, copied here and pasted there, and dragged in
//! from either. `services::file_clipboard` holds what was cut or copied inside Unluminous; this is the
//! half that reaches the system.
//!
//! **The system's file clipboard is a different interface on each platform**, and `arboard`, which
//! Unluminous uses for text, exposes neither: a list of paths in `CF_HDROP` on Windows, and file
//! addresses under `public.file-url` on the macOS pasteboard. Both are a few calls, made here directly
//! through the bindings the window already links, so no new crate is added. Elsewhere the clipboard
//! answers with nothing and Unluminous's own clipboard is the only one.
//!
//! **The pointer is asked of the system rather than of egui while something is being dropped.** A
//! file carried over a window by another program arrives through the platform's drag and drop, which
//! sends the window no pointer movement at all: Windows carries it through OLE and macOS through a
//! dragging session, and egui is left holding wherever the pointer was before the drag began. Which
//! folder a file lands in is decided by where the pointer is, so it is read here, in the window's own
//! points.

use std::path::PathBuf;

/// The files on the system clipboard, in the order they were put there. Empty when it holds no
/// files, which includes when it holds text.
pub fn clipboard_files() -> Vec<PathBuf> {
    platform::read()
}

/// Whether the system clipboard holds files, asked without reading them, which is what the folder
/// pane's menu wants to know about `Paste` on every frame it is open.
pub fn clipboard_has_files() -> bool {
    platform::has_files()
}

/// Put `paths` on the system clipboard as files, so they can be pasted in a file manager. Returns
/// whether the clipboard took them.
pub fn put_files_on_the_clipboard(paths: &[PathBuf]) -> bool {
    !paths.is_empty() && platform::write(paths)
}

/// Where the pointer is, in this window's points, asked of the system rather than of egui. `None`
/// when the platform cannot say, or when the pointer is over another program's window.
pub fn pointer(context: &egui::Context) -> Option<egui::Pos2> {
    platform::pointer(context)
}

/// Where the pointer is on the screen, in points from the top left of the main screen, which is the
/// measure egui gives a window's own position in. `None` where the platform is not asked.
///
/// Asked on macOS only, where the window moves its own edges (`task-2194`): a position inside the
/// window moves when the window does, and that is exactly what a drag of the window's own edge must
/// not be measured against. Windows hands the whole drag to the window manager and never asks.
pub fn pointer_on_screen() -> Option<egui::Pos2> {
    platform::pointer_on_screen()
}

#[cfg(windows)]
mod platform {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{GlobalFree, POINT};
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_HDROP;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, DROPFILES};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetCursorPos, GetWindowThreadProcessId, WindowFromPoint, GA_ROOT,
    };

    pub fn has_files() -> bool {
        // SAFETY: a question about the clipboard that opens nothing.
        unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP)) != 0 }
    }

    pub fn read() -> Vec<PathBuf> {
        let mut found = Vec::new();
        // SAFETY: the clipboard is opened and closed in this function, the handle it returns is only
        // read while it is open, and every buffer handed to `DragQueryFileW` is as long as it is told.
        unsafe {
            if IsClipboardFormatAvailable(u32::from(CF_HDROP)) == 0 {
                return found;
            }
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return found;
            }
            let handle = GetClipboardData(u32::from(CF_HDROP));
            if !handle.is_null() {
                let count = DragQueryFileW(handle, u32::MAX, std::ptr::null_mut(), 0);
                for index in 0..count {
                    let length = DragQueryFileW(handle, index, std::ptr::null_mut(), 0);
                    let mut name = vec![0_u16; length as usize + 1];
                    let written =
                        DragQueryFileW(handle, index, name.as_mut_ptr(), name.len() as u32);
                    name.truncate(written as usize);
                    found.push(PathBuf::from(OsString::from_wide(&name)));
                }
            }
            CloseClipboard();
        }
        found
    }

    pub fn write(paths: &[PathBuf]) -> bool {
        // The names one after another, each ending in a zero, and a second zero after the last.
        let mut names: Vec<u16> = Vec::new();
        for path in paths {
            names.extend(path.as_os_str().encode_wide());
            names.push(0);
        }
        names.push(0);
        let header = std::mem::size_of::<DROPFILES>();
        let bytes = header + names.len() * 2;
        // SAFETY: the memory is allocated here at the size that is written into it, and handed to the
        // clipboard only once it is filled and unlocked. If the clipboard does not take it, it is
        // freed here; if it does, the clipboard owns it and it must not be freed.
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if memory.is_null() {
                return false;
            }
            let locked = GlobalLock(memory).cast::<u8>();
            if locked.is_null() {
                GlobalFree(memory);
                return false;
            }
            let files =
                DROPFILES { pFiles: header as u32, pt: POINT { x: 0, y: 0 }, fNC: 0, fWide: 1 };
            std::ptr::write_unaligned(locked.cast::<DROPFILES>(), files);
            std::ptr::copy_nonoverlapping(
                names.as_ptr().cast::<u8>(),
                locked.add(header),
                names.len() * 2,
            );
            GlobalUnlock(memory);
            // Opened for this window rather than for nobody: emptying a clipboard opened with no
            // window makes it nobody's, and then Windows refuses what is put on it.
            if OpenClipboard(GetActiveWindow()) == 0 {
                GlobalFree(memory);
                return false;
            }
            EmptyClipboard();
            let placed = !SetClipboardData(u32::from(CF_HDROP), memory).is_null();
            CloseClipboard();
            if !placed {
                GlobalFree(memory);
            }
            placed
        }
    }

    pub fn pointer_on_screen() -> Option<egui::Pos2> {
        None
    }

    pub fn pointer(context: &egui::Context) -> Option<egui::Pos2> {
        let scale = context.pixels_per_point();
        // SAFETY: every call is given a valid out pointer, and a window handle that comes back null
        // is checked before it is used.
        unsafe {
            let mut at = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut at) == 0 {
                return None;
            }
            let under = WindowFromPoint(at);
            if under.is_null() {
                return None;
            }
            let window = GetAncestor(under, GA_ROOT);
            let mut process = 0_u32;
            GetWindowThreadProcessId(window, &mut process);
            if process != GetCurrentProcessId() {
                return None;
            }
            if ScreenToClient(window, &mut at) == 0 {
                return None;
            }
            Some(egui::pos2(at.x as f32 / scale, at.y as f32 / scale))
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::PathBuf;

    use objc2::encode::{Encode, Encoding};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;

    /// `CGPoint`, declared here so the one call that returns one needs no feature of its own.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: f64,
        y: f64,
    }

    unsafe impl Encode for Point {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }

    /// `CGRect`, for the same reason.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Frame {
        origin: Point,
        size: Point,
    }

    unsafe impl Encode for Frame {
        const ENCODING: Encoding = Encoding::Struct(
            "CGRect",
            &[Point::ENCODING, Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING])],
        );
    }

    /// The pasteboard type a file's address is written under.
    const FILE_URL: &str = "public.file-url";

    pub fn has_files() -> bool {
        !read().is_empty()
    }

    pub fn read() -> Vec<PathBuf> {
        let mut found = Vec::new();
        // SAFETY: every message is sent to a class or to an object a previous message returned, and
        // each object that can be nil is checked before anything is sent to it.
        unsafe {
            let board: Option<Retained<AnyObject>> =
                msg_send![class!(NSPasteboard), generalPasteboard];
            let Some(board) = board else { return found };
            let items: Option<Retained<AnyObject>> = msg_send![&*board, pasteboardItems];
            let Some(items) = items else { return found };
            let count: usize = msg_send![&*items, count];
            let kind = NSString::from_str(FILE_URL);
            for index in 0..count {
                let item: Option<Retained<AnyObject>> = msg_send![&*items, objectAtIndex: index];
                let Some(item) = item else { continue };
                let address: Option<Retained<NSString>> = msg_send![&*item, stringForType: &*kind];
                let Some(address) = address else { continue };
                let url: Option<Retained<AnyObject>> =
                    msg_send![class!(NSURL), URLWithString: &*address];
                let Some(url) = url else { continue };
                let path: Option<Retained<NSString>> = msg_send![&*url, path];
                if let Some(path) = path {
                    found.push(PathBuf::from(path.to_string()));
                }
            }
        }
        found
    }

    pub fn write(paths: &[PathBuf]) -> bool {
        // SAFETY: as in `read`. The array is built from objects that are alive for the whole call.
        unsafe {
            let board: Option<Retained<AnyObject>> =
                msg_send![class!(NSPasteboard), generalPasteboard];
            let Some(board) = board else { return false };
            let urls: Vec<Retained<AnyObject>> = paths
                .iter()
                .filter_map(|path| {
                    let text = NSString::from_str(&path.to_string_lossy());
                    let url: Option<Retained<AnyObject>> =
                        msg_send![class!(NSURL), fileURLWithPath: &*text];
                    url
                })
                .collect();
            if urls.is_empty() {
                return false;
            }
            let pointers: Vec<*const AnyObject> =
                urls.iter().map(|url| Retained::as_ptr(url)).collect();
            let array: Option<Retained<AnyObject>> = msg_send![
                class!(NSArray),
                arrayWithObjects: pointers.as_ptr(),
                count: pointers.len()
            ];
            let Some(array) = array else { return false };
            let _: isize = msg_send![&*board, clearContents];
            let written: bool = msg_send![&*board, writeObjects: &*array];
            written
        }
    }

    pub fn pointer(context: &egui::Context) -> Option<egui::Pos2> {
        let inside = context.input(|input| input.viewport().inner_rect)?;
        let at = pointer_on_screen()?;
        Some(egui::pos2(at.x - inside.min.x, at.y - inside.min.y))
    }

    pub fn pointer_on_screen() -> Option<egui::Pos2> {
        // SAFETY: as in `read`. `mouseLocation` and `frame` return plain structures.
        unsafe {
            let at: Point = msg_send![class!(NSEvent), mouseLocation];
            let screens: Option<Retained<AnyObject>> = msg_send![class!(NSScreen), screens];
            let screens = screens?;
            let first: Option<Retained<AnyObject>> = msg_send![&*screens, firstObject];
            let frame: Frame = msg_send![&*first?, frame];
            // AppKit measures from the bottom of the first screen and egui from the top of it.
            let from_top = frame.size.y - at.y;
            Some(egui::pos2(at.x as f32, from_top as f32))
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use std::path::PathBuf;

    pub fn has_files() -> bool {
        false
    }

    pub fn read() -> Vec<PathBuf> {
        Vec::new()
    }

    pub fn write(_paths: &[PathBuf]) -> bool {
        false
    }

    pub fn pointer(_context: &egui::Context) -> Option<egui::Pos2> {
        None
    }

    pub fn pointer_on_screen() -> Option<egui::Pos2> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;

    /// Files put on the system clipboard come back off it as the same paths, which is the round trip a
    /// paste in Explorer makes.
    ///
    /// Ignored, because the clipboard is the person's own and a test must not overwrite what they
    /// copied. Run it by hand with `--ignored` from a window that is in front.
    #[cfg(windows)]
    #[test]
    #[ignore = "writes to the system clipboard"]
    fn files_put_on_the_clipboard_come_back_off_it() {
        let folder = std::env::temp_dir().join("unluminous-system-files");
        std::fs::create_dir_all(&folder).expect("make the folder");
        let first = folder.join("one.txt");
        let second = folder.join("two words.md");
        std::fs::write(&first, "one").expect("write one");
        std::fs::write(&second, "two").expect("write two");
        let paths = vec![first, second];
        // The clipboard is the machine's, and another program can hold it open for a moment.
        let mut placed = false;
        for _ in 0..20 {
            placed = put_files_on_the_clipboard(&paths);
            if placed {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(placed, "the clipboard should take the files");
        assert_eq!(clipboard_files(), paths);
    }
}
