//! Civil dates as a day count, with no clock and no dependency.
//! 1970-01-01 is day 0. Howard Hinnant's rata die.

/// Days since 1970-01-01. `m` and `d` are 1-based.
pub fn days_from_civil(mut y: i32, m: u32, d: u32) -> i64 {
    y -= i32::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u32;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::from(era) * 146_097 + i64::from(doe) - 719_468
}

/// `(year, month, day)` of a day count. Month and day are 1-based.
pub fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    (y, m, d)
}

/// The day count of a real civil date, or `None` when it is not one.
pub fn checked(y: i32, m: u32, d: u32) -> Option<i64> {
    if !(1..=12).contains(&m) || d == 0 || d > 31 {
        return None;
    }
    let n = days_from_civil(y, m, d);
    let (yy, mm, dd) = civil_from_days(n);
    (yy == y && mm == m && dd == d).then_some(n)
}

/// `YYYY-MM-DD`.
pub fn format(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// ISO weekday, Monday = 1 through Sunday = 7. 1970-01-01 is a Thursday.
pub(super) fn iso_weekday(days: i64) -> u8 {
    (days.rem_euclid(7) + 3) as u8 % 7 + 1
}

/// The ISO week number of a day: the week (from Monday) whose Thursday is
/// in the year, counted from the one with the year's first Thursday.
pub(super) fn iso_week(days: i64) -> i64 {
    let thursday = days - i64::from(iso_weekday(days)) + 4;
    let (year, _, _) = civil_from_days(thursday);
    (thursday - days_from_civil(year, 1, 1)) / 7 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unix_epoch_and_a_span_of_fifteen_days() {
        assert_eq!(checked(1970, 1, 1), Some(0));
        assert_eq!(format(0), "1970-01-01");
        let start = checked(2020, 7, 1).unwrap();
        assert_eq!(format(start + 15), "2020-07-16");
        assert!(checked(2019, 2, 29).is_none());
        assert_eq!(format(checked(2020, 2, 29).unwrap()), "2020-02-29");
        assert_eq!(iso_weekday(0), 4);
        assert_eq!(iso_weekday(checked(2026, 1, 5).unwrap()), 1);
        assert_eq!(iso_weekday(checked(2026, 1, 10).unwrap()), 6);
        assert_eq!(iso_weekday(checked(2026, 1, 11).unwrap()), 7);
    }

    #[test]
    fn iso_weeks_start_with_the_year_s_first_thursday() {
        let week = |y, m, d| iso_week(checked(y, m, d).unwrap());
        assert_eq!(week(2026, 1, 1), 1);
        assert_eq!(week(2026, 1, 5), 2);
        assert_eq!(week(2026, 12, 31), 53);
        assert_eq!(week(2027, 1, 3), 53);
        assert_eq!(week(2027, 1, 4), 1);
        assert_eq!(week(2021, 1, 3), 53);
        assert_eq!(week(2024, 12, 30), 1);
    }
}
