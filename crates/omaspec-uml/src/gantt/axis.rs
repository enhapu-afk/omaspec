//! Mermaid's axis: `axisFormat` names the days of the axis in d3's time
//! format (`%d.%m`, `%b %e`, `%Y-%m-%d`), and `tickInterval` sets how far
//! apart its ticks stand (`1day`, `2week`, `1month`). A chart is a column
//! a day, so a time of day is midnight, and a tick finer than a day is
//! refused by the reader.

use super::date;

/// How the days of the axis are named, and how far apart they stand.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Axis {
    /// A d3 time format; `%Y-%m-%d` where only the ticks are set.
    pub format: Option<String>,
    /// A tick every so many days, weeks or months; else as far apart as
    /// the names need.
    pub every: Option<Every>,
    /// The ISO weekday a week starts on, 1 Monday to 7 Sunday: where a
    /// weekly tick stands. Mermaid's weeks start on Sunday.
    pub week_start: u8,
}

impl Axis {
    /// Whether the chart names its days its own way.
    pub fn set(&self) -> bool {
        self.format.is_some() || self.every.is_some()
    }

    /// The format the days are named in.
    pub fn format(&self) -> &str {
        self.format.as_deref().unwrap_or("%Y-%m-%d")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Every {
    Days(u32),
    Weeks(u32),
    Months(u32),
}

/// `tickInterval`'s value: a whole number and a unit. A unit finer than a
/// day is `Err`, as is anything else.
pub fn every(text: &str) -> Result<Every, ()> {
    let text = text.trim();
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    let n: u32 = text[..digits].parse().map_err(|_| ())?;
    if n == 0 || n > 1000 {
        return Err(());
    }
    match text[digits..].trim() {
        "day" => Ok(Every::Days(n)),
        "week" => Ok(Every::Weeks(n)),
        "month" => Ok(Every::Months(n)),
        _ => Err(()),
    }
}

/// Whether d3 reads every directive of `format`, this module too.
pub fn readable(format: &str) -> bool {
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        let mut next = chars.next();
        if matches!(next, Some('-' | '_' | '0')) {
            next = chars.next();
        }
        if !next.is_some_and(|d| "aAbBdejmuwyYUWVHIMSLp%".contains(d)) {
            return false;
        }
    }
    true
}

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS: [&str; 12] = [
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

/// `day` (days since 1970-01-01) named in d3's `format`, at midnight.
pub fn name(format: &str, day: i64) -> String {
    let (y, m, d) = date::civil_from_days(day);
    // 0 is Sunday, as d3's `%w`.
    let weekday = (day + 4).rem_euclid(7) as usize;
    let yday = day - date::days_from_civil(y, 1, 1);
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut pad = None;
        let mut next = chars.next();
        if let Some(p @ ('-' | '_' | '0')) = next {
            pad = Some(p);
            next = chars.next();
        }
        let number = |value: i64, width: usize, fill: char| -> String {
            let fill = match pad {
                Some('-') => return value.to_string(),
                Some('_') => ' ',
                Some(_) => '0',
                None => fill,
            };
            let text = value.to_string();
            let short = width.saturating_sub(text.chars().count());
            format!("{}{text}", fill.to_string().repeat(short))
        };
        match next {
            Some('a') => out.push_str(&WEEKDAYS[weekday][..3]),
            Some('A') => out.push_str(WEEKDAYS[weekday]),
            Some('b') => out.push_str(&MONTHS[m as usize - 1][..3]),
            Some('B') => out.push_str(MONTHS[m as usize - 1]),
            Some('d') => out.push_str(&number(i64::from(d), 2, '0')),
            Some('e') => out.push_str(&number(i64::from(d), 2, ' ')),
            Some('j') => out.push_str(&number(yday + 1, 3, '0')),
            Some('m') => out.push_str(&number(i64::from(m), 2, '0')),
            Some('u') => out.push_str(&number(
                if weekday == 0 { 7 } else { weekday as i64 },
                1,
                '0',
            )),
            Some('w') => out.push_str(&number(weekday as i64, 1, '0')),
            Some('y') => out.push_str(&number(i64::from(y).rem_euclid(100), 2, '0')),
            Some('Y') => out.push_str(&number(i64::from(y), 4, '0')),
            // The week of the year from its first Sunday (`%U`) or
            // Monday (`%W`); days before it are week 0.
            Some('U') => out.push_str(&number((yday + 7 - weekday as i64) / 7, 2, '0')),
            Some('W') => {
                let monday = (weekday as i64 + 6) % 7;
                out.push_str(&number((yday + 7 - monday) / 7, 2, '0'));
            }
            Some('V') => out.push_str(&number(iso_week(day), 2, '0')),
            Some('H' | 'M' | 'S') => out.push_str(&number(0, 2, '0')),
            Some('I') => out.push_str(&number(12, 2, '0')),
            Some('L') => out.push_str(&number(0, 3, '0')),
            Some('p') => out.push_str("AM"),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// The ISO week of `day`: the week of its year's first Thursday is 1.
fn iso_week(day: i64) -> i64 {
    // Monday 0 to Sunday 6.
    let monday = (day + 3).rem_euclid(7);
    let thursday = day - monday + 3;
    let (y, _, _) = date::civil_from_days(thursday);
    (thursday - date::days_from_civil(y, 1, 1)) / 7 + 1
}

/// The finest a format names: by the day, the week, the month or only
/// the year, in days. Ticks closer than it would repeat a name.
fn finest(format: &str) -> u32 {
    let mut finest = 365;
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        let mut next = chars.next();
        if matches!(next, Some('-' | '_' | '0')) {
            next = chars.next();
        }
        let days = match next {
            Some('d' | 'e' | 'j' | 'a' | 'A' | 'u' | 'w' | 'H' | 'I' | 'M' | 'S' | 'L' | 'p') => 1,
            Some('U' | 'W' | 'V') => 7,
            Some('b' | 'B' | 'm') => 30,
            _ => continue,
        };
        finest = finest.min(days);
    }
    finest
}

/// The days from `t0` to `t1` (days since 1970) that carry a tick: every
/// `every` from `t0`, a week's ticks on its first day, a month's on its
/// first; or, without `every`, as far apart as names `room` days wide
/// need, a day, two, a week, two weeks or months, and never closer than
/// the format names the days.
pub fn ticks(axis: &Axis, t0: i64, t1: i64, room: f32) -> Vec<i64> {
    let every = axis.every.unwrap_or_else(|| {
        let need = (room.ceil().max(1.0) as u32).max(finest(axis.format()));
        match need {
            1 => Every::Days(1),
            2 => Every::Days(2),
            3..=7 => Every::Weeks(1),
            8..=14 => Every::Weeks(2),
            _ => Every::Months(need.div_ceil(30)),
        }
    });
    let mut out = Vec::new();
    match every {
        Every::Days(n) => {
            let mut day = t0;
            while day <= t1 {
                out.push(day);
                day += i64::from(n);
            }
        }
        Every::Weeks(n) => {
            // ISO weekday of `t0`, 1 Monday to 7 Sunday.
            let iso = (t0 + 3).rem_euclid(7) + 1;
            let mut day = t0 + (i64::from(axis.week_start) - iso).rem_euclid(7);
            while day <= t1 {
                out.push(day);
                day += 7 * i64::from(n);
            }
        }
        Every::Months(n) => {
            let (mut y, mut m, d) = date::civil_from_days(t0);
            if d != 1 {
                (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            }
            loop {
                let day = date::days_from_civil(y, m, 1);
                if day > t1 {
                    break;
                }
                out.push(day);
                let months = i64::from(m) - 1 + i64::from(n);
                y += (months / 12) as i32;
                m = (months % 12) as u32 + 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_is_named_as_d3_names_it() {
        // Monday, 5 January 2026.
        let day = date::days_from_civil(2026, 1, 5);
        assert_eq!(name("%Y-%m-%d", day), "2026-01-05");
        assert_eq!(name("%d.%m", day), "05.01");
        assert_eq!(name("%-d. %B", day), "5. January");
        assert_eq!(name("%a %e %b %y", day), "Mon  5 Jan 26");
        assert_eq!(name("%A, week %V", day), "Monday, week 02");
        assert_eq!(
            name("%j %u %w %U %W %H:%M 100%%", day),
            "005 1 1 01 01 00:00 100%"
        );
        assert!(readable("%Y-%m-%d %-d %_e"));
        assert!(!readable("%Q") && !readable("%"));
    }

    #[test]
    fn ticks_stand_every_so_often_and_on_a_week_s_or_a_month_s_first_day() {
        let t0 = date::days_from_civil(2026, 1, 5);
        let t1 = date::days_from_civil(2026, 3, 10);
        let named = |axis: &Axis| -> Vec<String> {
            ticks(axis, t0, t1, 1.0)
                .into_iter()
                .take(3)
                .map(|day| name("%Y-%m-%d", day))
                .collect()
        };
        let mut axis = Axis {
            every: Some(Every::Days(10)),
            week_start: 7,
            ..Axis::default()
        };
        assert_eq!(named(&axis), ["2026-01-05", "2026-01-15", "2026-01-25"]);
        axis.every = Some(Every::Weeks(1));
        assert_eq!(named(&axis), ["2026-01-11", "2026-01-18", "2026-01-25"]);
        axis.week_start = 1;
        assert_eq!(named(&axis), ["2026-01-05", "2026-01-12", "2026-01-19"]);
        axis.every = Some(Every::Months(1));
        assert_eq!(named(&axis), ["2026-02-01", "2026-03-01"]);
        assert_eq!(every("2week"), Ok(Every::Weeks(2)));
        // Without an interval, no closer than the format names the days.
        let months = Axis {
            format: Some("%b %Y".into()),
            ..Axis::default()
        };
        let ticked = ticks(&months, t0, t1, 1.0);
        assert_eq!(ticked.len(), 2, "{ticked:?}");
        let years = Axis {
            format: Some("%Y".into()),
            ..Axis::default()
        };
        assert!(ticks(&years, t0, t0 + 800, 1.0).len() <= 3);
        assert!(every("1hour").is_err() && every("0day").is_err() && every("day").is_err());
    }
}
