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
    let clock: Option<Vec<(f64, bool)>> = lines
        .iter()
        .map(|(_, time)| time.as_deref().and_then(seconds))
        .collect();
    let plain: Option<Vec<f64>> = lines
        .iter()
        .map(|(_, time)| time.as_deref().and_then(|t| t.trim().parse().ok()))
        .collect();
    let (times, unit): (Vec<f64>, &str) = match (clock, plain) {
        (Some(times), _) => {
            // Times with their dates count as they are. A clock alone that
            // goes back by more than half a day went past midnight; a line
            // a moment out of order stays on its day, at the time before
            // it, so the lines keep the trace's order.
            let dated = times.iter().all(|(_, dated)| *dated);
            let mut day = 0.0;
            let mut before = f64::NEG_INFINITY;
            let times: Vec<f64> = times
                .iter()
                .map(|&(t, _)| {
                    if !dated && t + day < before - 43_200.0 {
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

/// A line's time in seconds, and whether it had its date: from 1970 for
/// an ISO time (`2026-11-12T20:14:02Z`, `…+01:00`, a space for the `T`),
/// of the day for a clock alone (`20:14:02`, `20:14:02.5`).
fn seconds(time: &str) -> Option<(f64, bool)> {
    let time = time.trim();
    let (date, clock) = match time.split_once(['T', ' ']) {
        Some((date, clock)) => (Some(date), clock.trim()),
        None => (None, time),
    };
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
    let of_day = h * 3600.0 + m * 60.0 + s - offset;
    let Some(date) = date else {
        return Some((of_day, false));
    };
    let parts: Vec<i64> = date
        .split('-')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let [year, month, day] = parts[..] else {
        return None;
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((
        days_from_1970(year, month, day) as f64 * 86_400.0 + of_day,
        true,
    ))
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

/// A time as the timing diagram writes it: no needless decimals.
fn number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
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
    }

    #[test]
    fn an_iso_time_counts_by_its_clock() {
        let day = 20_769.0 * 86_400.0;
        assert_eq!(
            seconds("2026-11-12T20:14:02.5Z"),
            Some((day + 72842.5, true))
        );
        // The same moment with its offset, and with a space.
        assert_eq!(
            seconds("2026-11-12T21:14:02.5+01:00"),
            Some((day + 72842.5, true))
        );
        assert_eq!(
            seconds("2026-11-12 15:14:02.5-0500"),
            Some((day + 72842.5, true))
        );
        assert_eq!(seconds("10:30:01"), Some((37801.0, false)));
        assert_eq!(seconds("soon"), None);
        assert_eq!(days_from_1970(1970, 1, 1), 0);
        assert_eq!(days_from_1970(2000, 3, 1), 11_017);
        assert_eq!(round_off(160.17), 200.0);
        assert_eq!(round_off(8031.5), 8000.0);
        assert_eq!(round_off(0.42), 0.4);
    }
}
