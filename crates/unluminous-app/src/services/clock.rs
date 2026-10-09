//! The time of day on this machine's clock, for saying when something happened.
//!
//! Asked of the operating system, because the offset from UTC depends on the time zone and its
//! daylight saving rules, which only the operating system knows. `task-2220` uses it for when a
//! notebook cell finished running.

/// A time of day, which prints as `14:03:22`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl std::fmt::Display for TimeOfDay {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

/// The time of day now, in the machine's own time zone.
pub fn time_of_day() -> TimeOfDay {
    let (hour, minute, second) = local_hour_minute_second();
    TimeOfDay { hour: hour as u8, minute: minute as u8, second: second as u8 }
}

/// The hour, minute and second now, from `GetLocalTime`.
#[cfg(windows)]
fn local_hour_minute_second() -> (u16, u16, u16) {
    let mut now = windows_sys::Win32::Foundation::SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: `GetLocalTime` only writes the structure it is given, which lives for the call.
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut now) };
    (now.wHour, now.wMinute, now.wSecond)
}

/// The hour, minute and second now, from `localtime_r`, which is safe to call from any thread.
#[cfg(not(windows))]
fn local_hour_minute_second() -> (u16, u16, u16) {
    // SAFETY: `time` with a null pointer only returns the time, and `localtime_r` only writes the
    // structure it is given, which lives for the call.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut parts: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut parts).is_null() {
            return (0, 0, 0);
        }
        (parts.tm_hour as u16, parts.tm_min as u16, parts.tm_sec as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_of_day_prints_two_digits_each_for_hours_minutes_and_seconds() {
        assert_eq!(TimeOfDay { hour: 9, minute: 5, second: 0 }.to_string(), "09:05:00");
        let now = time_of_day();
        assert!(now.hour < 24 && now.minute < 60 && now.second < 61, "{now}");
    }
}
