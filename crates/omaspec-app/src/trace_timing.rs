//! `omaspec trace run.jsonl --as timing`: a recorded trace as a PlantUML
//! timing diagram, a concise row for each object of the project's state
//! machines and the states it was in over time, each line where the trace
//! deviates as a band in red with what it says. Omaspec draws it itself,
//! and the writer can edit it as any timing diagram.
//!
//! The time runs as the trace gives it: seconds from the first line where
//! every line has a clock time (`20:14:02.5`), the numbers where every
//! line has one, else the lines in order. Where the trace leaves open
//! which state an object is in, the row says each (`Open or Locked`);
//! where the object is not followed (after a wrong event, until a line
//! gives its state) it says `?`.

use omaspec_core::lint::Severity;
use omaspec_model::{Spec, Specs, follow_trace, trace_objects};

/// A trace's row: an object, its machine, its steps.
struct Row<'a> {
    object: String,
    spec: &'a Spec,
    followed: omaspec_model::Followed,
}

/// The timing diagram of `trace`, titled `title`, against `specs`; `None`
/// when no object of it goes by a machine of the project.
pub fn timing(trace: &str, title: &str, specs: &Specs) -> Option<String> {
    let mut rows: Vec<Row> = Vec::new();
    for spec in specs.iter() {
        for object in trace_objects(trace, spec) {
            if rows.iter().any(|row| row.object == object) {
                continue;
            }
            let followed = follow_trace(trace, spec, &object);
            rows.push(Row {
                object,
                spec,
                followed,
            });
        }
    }
    if rows.is_empty() {
        return None;
    }
    // Every line of the trace a row takes, in order, with its time.
    let mut lines: Vec<(usize, Option<String>)> = rows
        .iter()
        .flat_map(|row| row.followed.steps.iter().map(|s| (s.line, s.time.clone())))
        .collect();
    lines.sort_by_key(|(line, _)| *line);
    lines.dedup_by_key(|(line, _)| *line);
    let clock: Option<Vec<Moment>> = lines
        .iter()
        .map(|(_, time)| time.as_deref().and_then(seconds))
        .collect();
    let plain: Option<Vec<f64>> = lines
        .iter()
        .map(|(_, time)| time.as_deref().and_then(|t| t.trim().parse().ok()))
        .collect();
    let (times, unit): (Vec<f64>, &str) = match (clock, plain) {
        (Some(moments), _) => {
            // Times that all have their dates count as they are; where one
            // has none, every line counts by its clock, so a trace that
            // mixes the two stays on one scale. A clock that goes back by
            // more than a few minutes went past midnight. A line a moment
            // out of order stays on its day, at the time before it, so the
            // lines keep the trace's order.
            let dated = moments.iter().all(|m| m.at.is_some());
            let mut day = 0.0;
            let mut before = f64::NEG_INFINITY;
            let times: Vec<f64> = moments
                .iter()
                .map(|m| {
                    let t = match m.at {
                        Some(at) if dated => at,
                        _ => m.clock,
                    };
                    if !dated && t + day < before - JITTER {
                        day += 86_400.0;
                    }
                    before = before.max(t + day);
                    before
                })
                .collect();
            let first = times.first().copied().unwrap_or(0.0);
            (
                times.iter().map(|t| t - first).collect(),
                "seconds from the first line",
            )
        }
        (None, Some(times)) => (times, "time as the trace gives it"),
        (None, None) => (
            (1..=lines.len()).map(|n| n as f64).collect(),
            "lines of the trace",
        ),
    };
    let at = |line: usize| {
        lines
            .iter()
            .position(|(l, _)| *l == line)
            .map(|i| times[i])
            .unwrap_or(0.0)
    };
    // Room before the first line for where each object begins.
    let first = times.first().copied().unwrap_or(0.0);
    let span = times.last().copied().unwrap_or(first) - first;
    let gap = if span > 0.0 {
        round_off(span / times.len().max(1) as f64)
    } else {
        1.0
    };
    let begin = first - gap;

    let mut out = format!("@startuml\ntitle {title}\ncaption {unit}\n");
    for (index, row) in rows.iter().enumerate() {
        out.push_str(&format!(
            "concise \"{}\" as R{index}\n",
            row.object.replace('"', "'")
        ));
    }
    // Each row's state at each time it changes, by time.
    let mut events: Vec<(f64, usize, String)> = Vec::new();
    let mut bands: Vec<(f64, f64, String)> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let m = &row.spec.machine;
        let names = |states: &[usize]| -> String {
            if states.is_empty() {
                return "?".to_owned();
            }
            // A final state has no name of its own; it is `[*]` as written.
            let mut said: Vec<&str> = states
                .iter()
                .map(|&s| match m.states[s].name.as_str() {
                    "" => "[*]",
                    name => name,
                })
                .collect();
            said.dedup();
            said.join(" or ")
        };
        if !row.followed.begins.is_empty() {
            events.push((begin, index, names(&row.followed.begins)));
        }
        for (n, step) in row.followed.steps.iter().enumerate() {
            let time = at(step.line);
            events.push((time, index, names(&step.at)));
            if let Some(finding) = &step.finding
                && finding.severity == Severity::Error
            {
                let next = row
                    .followed
                    .steps
                    .get(n + 1)
                    .map_or(time + gap, |s| at(s.line));
                let what = step
                    .event
                    .as_deref()
                    .or(step.state.as_deref())
                    .unwrap_or("");
                let said = format!("line {}: {} {what}?", step.line + 1, row.object);
                bands.push((time, next.max(time + gap * 0.5), said));
            }
        }
    }
    events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut last_time: Option<f64> = None;
    let mut last_state: Vec<Option<String>> = vec![None; rows.len()];
    for (time, index, state) in events {
        if last_state[index].as_deref() == Some(state.as_str()) {
            continue;
        }
        if last_time != Some(time) {
            out.push_str(&format!("@{}\n", number(time)));
            last_time = Some(time);
        }
        out.push_str(&format!("R{index} is \"{}\"\n", state.replace('"', "'")));
        last_state[index] = Some(state);
    }
    for (from, to, said) in bands {
        out.push_str(&format!(
            "highlight {} to {} #FFC9C9 : {}\n",
            number(from),
            number(to),
            said.replace('\n', " ")
        ));
    }
    out.push_str("@enduml\n");
    Some(out)
}

/// How far a clock may go back between two lines and still be a line out
/// of order rather than the next day.
const JITTER: f64 = 300.0;

/// When a line happened: its clock as written, in seconds of the day, and
/// for an ISO time with its date the moment in seconds from 1970 (UTC).
struct Moment {
    clock: f64,
    at: Option<f64>,
}

/// The moment of a line's time: an ISO date and time
/// (`2026-11-12T20:14:02Z`, `…+01:00`, a space for the `T`), or else a
/// clock as its last word (`20:14:02.5`, `2026/11/12 20:14:02`, `Nov 12
/// 20:14:02`), whose date, if any, is not read.
fn seconds(time: &str) -> Option<Moment> {
    let time = time.trim();
    if let Some((date, clock)) = time.split_once(['T', ' '])
        && let Some(days) = iso_date(date)
        && let Some((clock, offset)) = clock_of(clock.trim())
    {
        return Some(Moment {
            clock,
            at: Some(days as f64 * 86_400.0 + clock - offset),
        });
    }
    let last = time.rsplit([' ', 'T']).next()?;
    let (clock, _) = clock_of(last)?;
    Some(Moment { clock, at: None })
}

/// `2026-11-12` as days from 1970.
fn iso_date(date: &str) -> Option<i64> {
    let parts: Vec<i64> = date
        .split('-')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let [year, month, day] = parts[..] else {
        return None;
    };
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then(|| days_from_1970(year, month, day))
}

/// `20:14:02`, `20:14:02.5`, with `Z` or an offset after it: the seconds
/// of the day as written, and the offset east of UTC.
fn clock_of(clock: &str) -> Option<(f64, f64)> {
    let (clock, offset) = match clock.strip_suffix('Z') {
        Some(clock) => (clock, 0.0),
        None => match clock.rfind(['+', '-']) {
            Some(at) => (&clock[..at], utc_offset(&clock[at..])?),
            None => (clock, 0.0),
        },
    };
    let parts: Vec<&str> = clock.split(':').collect();
    let [h, m, s] = parts[..] else { return None };
    let (h, m): (f64, f64) = (h.parse().ok()?, m.parse().ok()?);
    let s: f64 = s.parse().ok()?;
    Some((h * 3600.0 + m * 60.0 + s, offset))
}

/// `+01:00`, `-0530`, `+02` in seconds east of UTC.
fn utc_offset(text: &str) -> Option<f64> {
    let sign = if text.starts_with('-') { -1.0 } else { 1.0 };
    let digits: String = text[1..].chars().filter(|c| *c != ':').collect();
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (h, m) = match digits.len() {
        2 => (digits.parse::<f64>().ok()?, 0.0),
        4 => (digits[..2].parse().ok()?, digits[2..].parse().ok()?),
        _ => return None,
    };
    Some(sign * (h * 3600.0 + m * 60.0))
}

/// The days from 1970-01-01 to a date of the Gregorian calendar.
fn days_from_1970(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_days - 719_468
}

/// `value` to one figure (`160.17` to `200`), for a time no line gives.
fn round_off(value: f64) -> f64 {
    let scale = 10f64.powf(value.log10().floor());
    ((value / scale).round() * scale).max(f64::MIN_POSITIVE)
}

/// A time as the timing diagram writes it: to the millisecond, no needless
/// decimals (as `f64` writes itself), and no `-0` (adding `0.0` makes it `0`).
fn number(value: f64) -> String {
    format!("{}", (value * 1000.0).round() / 1000.0 + 0.0)
}

#[cfg(test)]
mod tests {
    use omaspec_core::Language;

    use super::*;

    const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";

    #[test]
    fn each_object_is_a_row_and_a_deviation_a_red_band() {
        let specs = Specs::new(vec![
            Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap(),
        ]);
        let trace = "{\"time\":\"20:00:00\",\"object\":\"Door\",\"event\":\"open\"}\n{\"time\":\"20:00:30\",\"object\":\"Door\",\"event\":\"lock\"}\n{\"time\":\"20:01:00\",\"object\":\"Door\",\"event\":\"close\"}\n";
        let text = timing(trace, "night", &specs).unwrap();
        assert!(text.contains("concise \"Door\" as R0\n"), "{text}");
        assert!(
            text.contains("caption seconds from the first line"),
            "{text}"
        );
        assert!(text.contains("@0\nR0 is \"Open\"\n"), "{text}");
        // After a wrong event without a state, it is not followed: `?`.
        assert!(text.contains("@60\nR0 is \"?\"\n"), "{text}");
        assert!(
            text.contains("@-20\nR0 is \"Closed\"\n"),
            "where it begins: {text}"
        );
        assert!(
            text.contains("highlight 30 to 60 #FFC9C9 : line 2: Door lock?"),
            "{text}"
        );
        // It draws, with Omaspec's own engine.
        let svg = omaspec_uml::render(&text).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
        assert!(svg.contains("Door") && svg.contains("Open"), "{svg}");
        // No clock: the lines in order.
        let plain =
            "{\"object\":\"Door\",\"event\":\"open\"}\n{\"object\":\"Door\",\"event\":\"close\"}\n";
        let text = timing(plain, "plain", &specs).unwrap();
        assert!(
            text.contains("caption lines of the trace") && text.contains("@2\n"),
            "{text}"
        );
        assert!(timing("{\"object\":\"Gate\",\"event\":\"x\"}\n", "none", &specs).is_none());
    }

    #[test]
    fn a_night_goes_past_midnight() {
        let specs = Specs::new(vec![
            Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap(),
        ]);
        let trace = "{\"time\":\"23:59:30\",\"object\":\"Door\",\"event\":\"open\"}\n{\"time\":\"00:00:30\",\"object\":\"Door\",\"event\":\"close\"}\n";
        let text = timing(trace, "night", &specs).unwrap();
        assert!(
            text.contains("@0\nR0 is \"Open\"\n@60\nR0 is \"Closed\"\n"),
            "{text}"
        );
    }

    #[test]
    fn a_line_a_moment_out_of_order_stays_on_its_day() {
        let specs = Specs::new(vec![
            Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap(),
        ]);
        let trace = "{\"time\":\"20:00:05\",\"object\":\"Door\",\"event\":\"open\"}\n{\"time\":\"20:00:04\",\"object\":\"Door\",\"event\":\"close\"}\n{\"time\":\"20:00:09\",\"object\":\"Door\",\"event\":\"open\"}\n";
        let text = timing(trace, "jitter", &specs).unwrap();
        assert!(
            text.contains("@0\nR0 is \"Open\"\nR0 is \"Closed\"\n@4\nR0 is \"Open\"\n"),
            "{text}"
        );
        // Five minutes back is still out of order, not the next day.
        let trace = door_line("20:05:00", "open")
            + &door_line("20:00:00", "close")
            + &door_line("20:05:10", "open");
        let text = timing(&trace, "jitter", &specs).unwrap();
        assert!(
            text.contains("@0\nR0 is \"Open\"\nR0 is \"Closed\"\n@10\nR0 is \"Open\"\n"),
            "{text}"
        );
    }

    /// A line of the door's trace at `time`, the time as text or a number.
    fn door_line(time: &str, event: &str) -> String {
        let time = if time.parse::<f64>().is_ok() {
            time.to_owned()
        } else {
            format!("\"{time}\"")
        };
        format!("{{\"time\":{time},\"object\":\"Door\",\"event\":\"{event}\"}}\n")
    }

    fn door() -> Specs {
        Specs::new(vec![
            Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap(),
        ])
    }

    #[test]
    fn two_objects_of_one_machine_are_two_rows() {
        let trace = "{\"time\":\"20:00:00\",\"object\":\"front\",\"machine\":\"Door\",\"event\":\"open\"}\n{\"time\":\"20:00:10\",\"object\":\"back\",\"machine\":\"Door\",\"event\":\"lock\"}\n";
        let text = timing(trace, "doors", &door()).unwrap();
        assert!(
            text.contains("concise \"front\" as R0\nconcise \"back\" as R1\n"),
            "{text}"
        );
        assert!(text.contains("@10\nR1 is \"Locked\"\n"), "{text}");
    }

    /// Where every line has its date, the moments count, whatever the
    /// offset each is written in and however long the pause between.
    #[test]
    fn dated_times_count_as_the_moments_they_are() {
        let trace = door_line("2026-11-12T21:00:00+01:00", "open")
            + &door_line("2026-11-12T20:00:10Z", "close");
        let text = timing(&trace, "zones", &door()).unwrap();
        assert!(
            text.contains("@0\nR0 is \"Open\"\n@10\nR0 is \"Closed\"\n"),
            "{text}"
        );
        let trace =
            door_line("2026-11-12T20:00:00Z", "open") + &door_line("2026-11-14T20:00:05Z", "close");
        let text = timing(&trace, "weekend", &door()).unwrap();
        assert!(text.contains("@172805\n"), "two days on: {text}");
    }

    /// Where each object begins: a step before the first line, as long as
    /// the lines are apart on average; one second before a single line.
    #[test]
    fn an_object_begins_a_step_before_the_first_line() {
        let trace = door_line("100", "open") + &door_line("110", "close");
        let text = timing(&trace, "numbers", &door()).unwrap();
        assert!(
            text.contains("caption time as the trace gives it")
                && text.contains("@95\nR0 is \"Closed\"\n@100\nR0 is \"Open\"\n@110\n"),
            "{text}"
        );
        let text = timing(&door_line("20:00:00", "open"), "one", &door()).unwrap();
        assert!(
            text.contains("@-1\nR0 is \"Closed\"\n@0\nR0 is \"Open\"\n"),
            "{text}"
        );
    }

    /// A band runs to the object's next line, or a step on after its last
    /// one, and is never narrower than half a step.
    #[test]
    fn a_band_runs_to_the_next_line_and_is_never_too_narrow() {
        // The lines 30 s apart, a step of 20 s: on to 50.
        let trace = door_line("20:00:00", "open") + &door_line("20:00:30", "lock");
        let text = timing(&trace, "last", &door()).unwrap();
        assert!(
            text.contains("highlight 30 to 50 #FFC9C9 : line 2: Door lock?"),
            "{text}"
        );
        // The next line a second on, a step of 10 s: half a step, to 35.
        let trace = trace + &door_line("20:00:31", "close");
        let text = timing(&trace, "close", &door()).unwrap();
        assert!(
            text.contains("highlight 30 to 35 #FFC9C9 : line 2: Door lock?"),
            "{text}"
        );
    }

    /// A trace that mixes ISO times and clocks counts every line by its
    /// clock; a night with a long pause still goes on to the next day.
    #[test]
    fn mixed_times_and_long_nights_keep_their_spacing() {
        let specs = Specs::new(vec![
            Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap(),
        ]);
        let line = |time: &str, event: &str| {
            format!("{{\"time\":\"{time}\",\"object\":\"Door\",\"event\":\"{event}\"}}\n")
        };
        let mixed = line("2026-11-12T20:00:00Z", "open")
            + &line("20:00:05", "close")
            + &line("20:00:09", "open");
        let text = timing(&mixed, "mixed", &specs).unwrap();
        assert!(text.contains("@5\n") && text.contains("@9\n"), "{text}");
        let night = line("20:00:00", "open") + &line("09:00:00", "close");
        let text = timing(&night, "night", &specs).unwrap();
        assert!(text.contains("@46800\n"), "13 hours on: {text}");
    }

    #[test]
    fn an_iso_time_counts_by_its_clock() {
        let day = 20_769.0 * 86_400.0;
        let at = |time: &str| seconds(time).map(|m| (m.clock, m.at));
        assert_eq!(
            at("2026-11-12T20:14:02.5Z"),
            Some((72842.5, Some(day + 72842.5)))
        );
        // The same moment with its offset, and with a space.
        assert_eq!(
            at("2026-11-12T21:14:02.5+01:00"),
            Some((76442.5, Some(day + 72842.5)))
        );
        assert_eq!(
            at("2026-11-12 15:14:02.5-0500"),
            Some((54842.5, Some(day + 72842.5)))
        );
        assert_eq!(
            at("2026-11-13T01:44:02.5+05:30"),
            Some((6242.5, Some(day + 72842.5)))
        );
        // No such month or day: the clock alone.
        assert_eq!(at("2026-13-01T10:00:00Z"), Some((36000.0, None)));
        assert_eq!(at("2026-11-32T10:00:00Z"), Some((36000.0, None)));
        assert_eq!(at("10:30:01"), Some((37801.0, None)));
        // A date not written as ISO writes it: the clock alone.
        assert_eq!(at("2026/11/12 20:14:02"), Some((72842.0, None)));
        assert_eq!(at("Nov 12 20:14:02"), Some((72842.0, None)));
        assert_eq!(at("soon"), None);
        assert_eq!(days_from_1970(1970, 1, 1), 0);
        assert_eq!(days_from_1970(2000, 3, 1), 11_017);
        assert_eq!(round_off(160.17), 200.0);
        assert_eq!(round_off(8031.5), 8000.0);
        assert_eq!(round_off(0.42), 0.4);
        assert_eq!(number(30.0), "30");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(1.23456), "1.235");
        assert_eq!(number(-0.0001), "0");
        assert_eq!(number(172_805.0), "172805");
    }

    use proptest::prelude::*;

    /// How a line writes its time.
    #[derive(Debug, Clone, Copy)]
    enum Written {
        Clock,
        Utc,
        Slashed,
    }

    /// `at` seconds after 2026-11-12 00:00 UTC, written as `how` says.
    fn written(at: u64, how: Written) -> String {
        let day = 12 + at / 86_400;
        let (h, m, s) = (at % 86_400 / 3600, at % 3600 / 60, at % 60);
        match how {
            Written::Clock => format!("{h:02}:{m:02}:{s:02}"),
            Written::Utc => format!("2026-11-{day:02}T{h:02}:{m:02}:{s:02}Z"),
            Written::Slashed => format!("2026/11/{day:02} {h:02}:{m:02}:{s:02}"),
        }
    }

    /// The times a timing diagram puts its changes at, in its order.
    fn moments(text: &str) -> Vec<f64> {
        text.lines()
            .filter_map(|l| l.strip_prefix('@')?.parse().ok())
            .collect()
    }

    proptest! {
        /// Whatever the form of its times, a trace's moments come out in
        /// its order and as far apart as they were: a night past midnight
        /// is the next day, a line a few seconds out of order stays at the
        /// time of the line before it.
        #[test]
        fn a_trace_keeps_its_time(
            start in 0u64..86_400,
            steps in prop::collection::vec((0u64..85_000, 0u8..10, 0u8..3), 1..16),
        ) {
            let specs = Specs::new(vec![Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap()]);
            let events = ["open", "close", "lock", "unlock"];
            let mut real = start;
            let mut first = None;
            let mut expected = vec![0.0];
            let mut trace = String::new();
            let mut last = 0.0f64;
            for (i, &(gap, back, how)) in steps.iter().enumerate() {
                // Now and then a line a second or three before the last.
                let at = if i > 0 && back < 2 {
                    real.saturating_sub(u64::from(back) + 1).max(first.unwrap_or(start))
                } else {
                    real += gap;
                    real
                };
                let how = [Written::Clock, Written::Utc, Written::Slashed][usize::from(how)];
                let event = events[i % events.len()];
                trace.push_str(&format!(
                    "{{\"time\":\"{}\",\"object\":\"Door\",\"event\":\"{event}\"}}\n",
                    written(at, how)
                ));
                let first = *first.get_or_insert(at);
                let since = at.saturating_sub(first) as f64;
                last = last.max(since);
                if i > 0 {
                    expected.push(last);
                }
            }
            let text = timing(&trace, "t", &specs).unwrap();
            let shown = moments(&text);
            prop_assert!(shown.windows(2).all(|w| w[0] <= w[1]), "{text}");
            for t in shown.iter().filter(|t| **t >= 0.0) {
                prop_assert!(expected.contains(t), "{t} not in {expected:?}\n{trace}\n{text}");
            }
        }
    }
}
