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

/// A calendar date and time of day on this machine's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl DateTime {
    /// The date and time as `YYYYMMDD-HHMMSS`, which sorts in the order the moments happened.
    pub fn stamp(&self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// The date and time now, in the machine's own time zone.
pub fn date_time() -> DateTime {
    let (year, month, day, hour, minute, second) = local_date_and_time();
    DateTime {
        year,
        month: month as u8,
        day: day as u8,
        hour: hour as u8,
        minute: minute as u8,
        second: second as u8,
    }
}

/// The year, month, day, hour, minute and second now, from `GetLocalTime`.
#[cfg(windows)]
fn local_date_and_time() -> (u16, u16, u16, u16, u16, u16) {
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
    (now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond)
}

/// The year, month, day, hour, minute and second now, from `localtime_r`.
#[cfg(not(windows))]
fn local_date_and_time() -> (u16, u16, u16, u16, u16, u16) {
    // SAFETY: `time` with a null pointer only returns the time, and `localtime_r` only writes the
    // structure it is given, which lives for the call.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut parts: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut parts).is_null() {
            return (1970, 1, 1, 0, 0, 0);
        }
        (
            (parts.tm_year + 1900) as u16,
            (parts.tm_mon + 1) as u16,
            parts.tm_mday as u16,
            parts.tm_hour as u16,
            parts.tm_min as u16,
            parts.tm_sec as u16,
        )
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

    #[test]
    fn a_stamp_sorts_in_the_order_the_moments_happened() {
        let earlier = DateTime { year: 2026, month: 9, day: 30, hour: 23, minute: 59, second: 59 };
        let later = DateTime { year: 2026, month: 10, day: 1, hour: 0, minute: 0, second: 0 };
        assert_eq!(earlier.stamp(), "20260930-235959");
        assert!(earlier.stamp() < later.stamp());
        assert!(date_time().year >= 2025);
    }
}
