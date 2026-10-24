//! Dates and times as Excel counts them: a serial number of days, with the time of day as the fraction.
//!
//! The 1900 system counts 1 January 1900 as day 1 and includes 29 February 1900, a day that did not
//! happen, because Lotus 1-2-3 did and Excel kept its numbers. So every date from 1 March 1900 on is one
//! more than a calendar count would give, and serial 60 is that phantom day. The 1904 system counts
//! 1 January 1904 as day 0 and has no such day.

/// Days from 0001-01-01 (day 1) in the proleptic Gregorian calendar.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    // Howard Hinnant's algorithm.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Unix day 0 (1970-01-01) as a 1900 serial.
const UNIX_EPOCH_1900: f64 = 25_569.0;

/// The serial of a calendar date. Months and days out of range roll over, the way `DATE` does.
pub fn serial(year: i64, month: i64, day: i64, date1904: bool) -> Option<f64> {
    // Excel reads a year under 1900 as 1900 plus it.
    let year = if (0..1900).contains(&year) { year + 1900 } else { year };
    let total_months = year * 12 + (month - 1);
    let (y, m) = (total_months.div_euclid(12), total_months.rem_euclid(12) + 1);
    let serial = if date1904 {
        days_from_civil(y, m as u32, 1) + (day - 1) - days_from_civil(1904, 1, 1)
    } else {
        // The first of the month, counted with the phantom 29 February from March 1900 on, and then the
        // days, so DATE(1900,2,29) is 60 the way Excel has it.
        let first = days_from_civil(y, m as u32, 1) - days_from_civil(1899, 12, 31);
        let first = if (y, m) > (1900, 2) { first + 1 } else { first };
        first + (day - 1)
    };
    (0..=2_958_465).contains(&serial).then_some(serial as f64)
}

/// The calendar date of a serial: year, month, day. Serial 60 in the 1900 system is 29 February 1900.
pub fn date(serial: f64, date1904: bool) -> Option<(i64, u32, u32)> {
    if !(0.0..2_958_466.0).contains(&serial) {
        return None;
    }
    let whole = serial.floor() as i64;
    if date1904 {
        return Some(civil_from_days(whole + days_from_civil(1904, 1, 1)));
    }
    if whole == 0 {
        return Some((1900, 1, 0));
    }
    if whole == 60 {
        return Some((1900, 2, 29));
    }
    let adjusted = if whole > 60 { whole - 1 } else { whole };
    Some(civil_from_days(adjusted + days_from_civil(1899, 12, 31)))
}

/// Hours, minutes, seconds and milliseconds of the fraction of a serial, rounded to the millisecond the
/// way Excel shows a time.
pub fn time(serial: f64) -> (u32, u32, u32, u32) {
    let fraction = serial - serial.floor();
    let ms = (fraction * 86_400_000.0).round() as u64 % 86_400_000;
    (
        (ms / 3_600_000) as u32,
        (ms / 60_000 % 60) as u32,
        (ms / 1000 % 60) as u32,
        (ms % 1000) as u32,
    )
}

/// The day of the week, 0 for Sunday.
pub fn weekday(serial: f64, date1904: bool) -> u32 {
    let whole = serial.floor() as i64 + if date1904 { 1462 } else { 0 };
    // Serial 1 (1 January 1900) is a Sunday in Excel's count, which is wrong by the same phantom day.
    ((whole + 6).rem_euclid(7)) as u32
}

/// The serial of a moment given as seconds since the Unix epoch and a local offset in seconds.
pub fn from_unix(seconds: f64, offset_seconds: i64, date1904: bool) -> f64 {
    let serial = (seconds + offset_seconds as f64) / 86_400.0 + UNIX_EPOCH_1900;
    if date1904 {
        serial - 1462.0
    } else {
        serial
    }
}

/// Whether a year has 29 February.
pub fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
    }
}

pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
pub const DAYS: [&str; 7] =
    ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serials_match_excel_on_both_sides_of_the_phantom_day() {
        assert_eq!(serial(1900, 1, 1, false), Some(1.0));
        assert_eq!(serial(1900, 2, 28, false), Some(59.0));
        assert_eq!(serial(1900, 3, 1, false), Some(61.0));
        assert_eq!(serial(2026, 10, 23, false), Some(46318.0));
        assert_eq!(date(60.0, false), Some((1900, 2, 29)));
        assert_eq!(date(61.0, false), Some((1900, 3, 1)));
        assert_eq!(date(46318.5, false), Some((2026, 10, 23)));
        assert_eq!(serial(1904, 1, 1, true), Some(0.0));
        assert_eq!(serial(2026, 10, 23, true), Some(46318.0 - 1462.0));
    }

    #[test]
    fn months_and_days_out_of_range_roll_over() {
        assert_eq!(serial(2024, 13, 1, false), serial(2025, 1, 1, false));
        assert_eq!(serial(2024, 3, 0, false), serial(2024, 2, 29, false));
        assert_eq!(serial(2024, 0, 1, false), serial(2023, 12, 1, false));
        assert_eq!(serial(126, 1, 1, false), serial(2026, 1, 1, false));
    }

    #[test]
    fn a_time_is_the_fraction_of_a_day() {
        assert_eq!(time(0.75), (18, 0, 0, 0));
        assert_eq!(time(46318.0 + 1.0 / 86_400.0 * 3661.0), (1, 1, 1, 0));
        assert_eq!(weekday(46318.0, false), 5, "23 October 2026 is a Friday");
    }
}
