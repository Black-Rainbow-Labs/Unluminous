//! Formatted text on the operating system's clipboard: HTML and RTF beside the plain text, which is
//! what a Word tab copies and what it reads first when it pastes (task-2359, TDD 5.4 C11).
//!
//! **egui's clipboard is text only**, so a formatted copy is written here rather than through
//! `Context::copy_text`, which would replace the HTML and the RTF with the text at the end of the frame.
//! On Windows the three formats go on in one open of the clipboard: `CF_UNICODETEXT`, `HTML Format`
//! (UTF-8 with its offset header, which `unluminous_word::clipboard::cf_html` writes) and
//! `Rich Text Format`. Elsewhere `arboard` puts the HTML and the text, and RTF is left out.

/// Put the text, the HTML fragment and the RTF on the clipboard. Answers whether it took them.
pub fn write(text: &str, html: &str, rtf: &str) -> bool {
    platform::write(text, html, rtf)
}

/// What the clipboard holds: its HTML fragment, its RTF and its text, each when there is one.
pub fn read() -> (Option<String>, Option<String>, Option<String>) {
    platform::read()
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;

    fn format(name: &str) -> u32 {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: the name is a zero terminated wide string that lives for the call.
        unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
    }

    /// A movable global block holding `bytes`, or null.
    unsafe fn block(bytes: &[u8]) -> *mut core::ffi::c_void {
        // SAFETY: allocated at the size written into it, unlocked before it is handed over.
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if memory.is_null() {
                return memory;
            }
            let locked = GlobalLock(memory).cast::<u8>();
            if locked.is_null() {
                GlobalFree(memory);
                return std::ptr::null_mut();
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), locked, bytes.len());
            GlobalUnlock(memory);
            memory
        }
    }

    pub fn write(text: &str, html: &str, rtf: &str) -> bool {
        let mut wide: Vec<u16> = text.replace('\n', "\r\n").encode_utf16().collect();
        wide.push(0);
        let wide_bytes: Vec<u8> = wide.iter().flat_map(|u| u.to_le_bytes()).collect();
        let mut html_bytes = unluminous_word::clipboard::cf_html(html).into_bytes();
        html_bytes.push(0);
        let mut rtf_bytes = rtf.as_bytes().to_vec();
        rtf_bytes.push(0);
        // SAFETY: each block is filled before it is handed to the clipboard, and freed here only when
        // the clipboard did not take it. Opened for this window, for the reason `system_files` gives.
        unsafe {
            if OpenClipboard(GetActiveWindow()) == 0 {
                return false;
            }
            EmptyClipboard();
            let mut placed = true;
            for (kind, bytes) in [
                (u32::from(CF_UNICODETEXT), wide_bytes.as_slice()),
                (format("HTML Format"), html_bytes.as_slice()),
                (format("Rich Text Format"), rtf_bytes.as_slice()),
            ] {
                let memory = block(bytes);
                if memory.is_null() || SetClipboardData(kind, memory).is_null() {
                    if !memory.is_null() {
                        GlobalFree(memory);
                    }
                    placed = false;
                }
            }
            CloseClipboard();
            placed
        }
    }

    /// The bytes of one clipboard format, up to its first zero.
    unsafe fn bytes_of(kind: u32) -> Option<Vec<u8>> {
        // SAFETY: called with the clipboard open; the handle is read only while it is locked.
        unsafe {
            if IsClipboardFormatAvailable(kind) == 0 {
                return None;
            }
            let handle = GetClipboardData(kind);
            if handle.is_null() {
                return None;
            }
            let size = GlobalSize(handle);
            let locked = GlobalLock(handle).cast::<u8>();
            if locked.is_null() {
                return None;
            }
            let all = std::slice::from_raw_parts(locked, size).to_vec();
            GlobalUnlock(handle);
            Some(all)
        }
    }

    pub fn read() -> (Option<String>, Option<String>, Option<String>) {
        // SAFETY: the clipboard is opened and closed here and every read is made while it is open.
        unsafe {
            if OpenClipboard(GetActiveWindow()) == 0 {
                return (None, None, None);
            }
            let cut = |mut bytes: Vec<u8>| {
                if let Some(end) = bytes.iter().position(|b| *b == 0) {
                    bytes.truncate(end);
                }
                bytes
            };
            let html = bytes_of(format("HTML Format")).map(|b| {
                let text = String::from_utf8_lossy(&cut(b)).into_owned();
                unluminous_word::clipboard::html_from_cf(&text).to_owned()
            });
            let rtf = bytes_of(format("Rich Text Format"))
                .map(|b| cut(b).iter().map(|&c| c as char).collect::<String>());
            let text = bytes_of(u32::from(CF_UNICODETEXT)).map(|b| {
                let units: Vec<u16> = b
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take_while(|u| *u != 0)
                    .collect();
                String::from_utf16_lossy(&units).replace("\r\n", "\n")
            });
            CloseClipboard();
            (html, rtf, text)
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn write(text: &str, html: &str, _rtf: &str) -> bool {
        arboard::Clipboard::new().and_then(|mut c| c.set().html(html, Some(text))).is_ok()
    }

    pub fn read() -> (Option<String>, Option<String>, Option<String>) {
        let Ok(mut clipboard) = arboard::Clipboard::new() else { return (None, None, None) };
        let html = clipboard
            .get()
            .html()
            .ok()
            .map(|h| unluminous_word::clipboard::html_from_cf(&h).to_owned());
        let text = clipboard.get().text().ok();
        (html, None, text)
    }
}
