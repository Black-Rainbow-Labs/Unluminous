//! Whether Unluminous may use the microphone, asking where the system lets a program ask, and opening the
//! system's own page where it does not.
//!
//! **Windows** has no prompt a desktop program can raise. The answer is three switches in the registry under
//! `Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone`: `Value` under
//! `HKLM` is "Microphone access" for the whole device, `Value` under `HKCU` is "Let apps access your
//! microphone", and `NonPackaged\Value` under `HKCU` is "Let desktop apps access your microphone". Any of the
//! three saying `Deny` means the stream Unluminous opens is silent. A refused desktop program is handed zeros
//! rather than an error, which is why the engine also treats a stream of exact zeros as refused.
//!
//! **macOS** asks the person the first time a program opens the microphone, as long as the bundle's
//! `Info.plist` says why (`NSMicrophoneUsageDescription`) and the hardened runtime is given
//! `com.apple.security.device.audio-input`. [`request`] asks `AVCaptureDevice` directly so the prompt can
//! come at a moment of Unluminous's choosing (the first start) rather than in the middle of a dictation.

/// What the system says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    /// Unluminous may use the microphone.
    Allowed,
    /// The person has not been asked yet (macOS).
    NotAsked,
    /// The microphone is refused, and the sentence says which switch refuses it.
    Refused(String),
    /// This platform has nothing to ask.
    Unknown,
}

impl Access {
    /// The word `voice status` prints.
    pub fn name(&self) -> &'static str {
        match self {
            Access::Allowed => "allowed",
            Access::NotAsked => "not asked",
            Access::Refused(_) => "refused",
            Access::Unknown => "unknown",
        }
    }

    /// Whether opening the microphone can work.
    pub fn may_open(&self) -> bool {
        !matches!(self, Access::Refused(_))
    }
}

/// What the system says now.
pub fn status() -> Access {
    platform::status()
}

/// Asks the person, where the system has a prompt for it. Answers at once; [`status`] says the answer once
/// it is given.
pub fn request() {
    platform::request();
}

/// Opens the system's microphone privacy page.
pub fn open_settings() -> Result<(), String> {
    platform::open_settings()
}

/// The sentence for a stream that delivers nothing but zeros.
pub const SILENT_STREAM: &str = "The microphone is sending silence. Access to it may be turned off in the system's privacy settings, or it may be muted.";

#[cfg(windows)]
mod platform {
    use super::Access;
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
    };

    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

    /// A string value from the registry, when it is there.
    /// @param root - the hive
    /// @param key - the key under it
    /// @param name - the value's name
    fn read(root: HKEY, key: &str, name: &str) -> Option<String> {
        let wide = |text: &str| text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (key, name) = (wide(key), wide(name));
        let mut buffer = [0u16; 64];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: the key and value names are NUL terminated and the buffer and its size agree.
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        let length = (size as usize / 2).saturating_sub(1).min(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..length]))
    }

    /// The three switches, the first that refuses winning.
    pub fn status() -> Access {
        let refuses = |root: HKEY, key: &str| {
            read(root, key, "Value").is_some_and(|value| value.eq_ignore_ascii_case("Deny"))
        };
        if refuses(HKEY_LOCAL_MACHINE, KEY) {
            return Access::Refused("Microphone access is turned off for this device in Settings, Privacy and security, Microphone.".to_owned());
        }
        if refuses(HKEY_CURRENT_USER, KEY) {
            return Access::Refused("\"Let apps access your microphone\" is turned off in Settings, Privacy and security, Microphone.".to_owned());
        }
        if refuses(HKEY_CURRENT_USER, &format!(r"{KEY}\NonPackaged")) {
            return Access::Refused("\"Let desktop apps access your microphone\" is turned off in Settings, Privacy and security, Microphone.".to_owned());
        }
        Access::Allowed
    }

    /// Windows has no prompt for a desktop program; opening the microphone is what puts Unluminous on the list.
    pub fn request() {}

    /// `ms-settings:privacy-microphone`, through the shell.
    pub fn open_settings() -> Result<(), String> {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let wide = |text: &str| text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (verb, target) = (wide("open"), wide("ms-settings:privacy-microphone"));
        // SAFETY: both strings are NUL terminated and outlive the call.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        match result as isize > 32 {
            true => Ok(()),
            false => Err("the Settings page could not be opened".to_owned()),
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::Access;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, Bool};
    use objc2_foundation::NSString;

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    /// `AVMediaTypeAudio`, which is the four letters `soun`.
    fn audio() -> objc2::rc::Retained<NSString> {
        NSString::from_str("soun")
    }

    /// `AVCaptureDevice authorizationStatusForMediaType:`.
    pub fn status() -> Access {
        let Some(class) = AnyClass::get(c"AVCaptureDevice") else { return Access::Unknown };
        let media = audio();
        // SAFETY: a class method taking an NSString and answering an NSInteger.
        let answer: isize = unsafe { msg_send![class, authorizationStatusForMediaType: &*media] };
        match answer {
            0 => Access::NotAsked,
            1 => Access::Refused("Microphone access is restricted on this Mac by a profile or parental controls.".to_owned()),
            2 => Access::Refused("Unluminous is not allowed to use the microphone in System Settings, Privacy and Security, Microphone.".to_owned()),
            _ => Access::Allowed,
        }
    }

    /// `AVCaptureDevice requestAccessForMediaType:completionHandler:`, which shows the prompt the first time.
    pub fn request() {
        let Some(class) = AnyClass::get(c"AVCaptureDevice") else { return };
        let media = audio();
        let done = block2::RcBlock::new(|_granted: Bool| {});
        // SAFETY: a class method taking an NSString and a block with one BOOL argument.
        let () = unsafe {
            msg_send![class, requestAccessForMediaType: &*media, completionHandler: &*done]
        };
    }

    /// The Microphone page of Privacy and Security.
    pub fn open_settings() -> Result<(), String> {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .status()
            .map_err(|e| e.to_string())
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err("System Settings could not be opened".to_owned())
                }
            })
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use super::Access;

    /// Linux has no consent store; whether the device opens is the answer.
    pub fn status() -> Access {
        Access::Unknown
    }

    /// Nothing to ask.
    pub fn request() {}

    /// No page to open.
    pub fn open_settings() -> Result<(), String> {
        Err("this system has no microphone privacy page; check the sound settings".to_owned())
    }
}
