//! A transition's label as UML writes it: `event [guard] / action`.

/// What makes a transition fire and what it does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trigger {
    /// The events that fire it (`a, b` are two); none for a transition
    /// taken when its state is done.
    pub events: Vec<String>,
    /// The condition between `[` and `]`.
    pub guard: Option<String>,
    /// What happens, after `/`.
    pub action: Option<String>,
}

impl Trigger {
    /// Reads a label. Line breaks (`\n` in PlantUML, `<br>` in Mermaid)
    /// count as spaces, and runs of spaces as one.
    pub fn parse(label: &str) -> Trigger {
        let text = squeeze(
            &label
                .replace("\\n", " ")
                .replace("<br/>", " ")
                .replace("<br>", " "),
        );
        let (head, action) = match outside(&text, '/') {
            Some(at) => (&text[..at], nonempty(&text[at + 1..])),
            None => (text.as_str(), None),
        };
        let (events, guard) = match outside(head, '[') {
            Some(open) => {
                let close = head[open..].find(']').map_or(head.len(), |c| open + c);
                let rest = format!("{} {}", &head[..open], head.get(close + 1..).unwrap_or(""));
                (rest, nonempty(&head[open + 1..close]))
            }
            None => (head.to_owned(), None),
        };
        Trigger {
            events: split_events(&events),
            guard,
            action,
        }
    }
}

/// `text` with its spaces squeezed to single ones and trimmed.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn nonempty(text: &str) -> Option<String> {
    let t = squeeze(text);
    (!t.is_empty()).then_some(t)
}

/// The first `c` outside parentheses and brackets (other than the bracket
/// itself when `c` is one).
fn outside(text: &str, c: char) -> Option<usize> {
    let mut depth = 0usize;
    for (at, ch) in text.char_indices() {
        if ch == c && depth == 0 {
            return Some(at);
        }
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// `a, b(x, y)` as `a` and `b(x, y)`.
fn split_events(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = outside(rest, ',') {
        out.extend(nonempty(&rest[..at]));
        rest = &rest[at + 1..];
    }
    out.extend(nonempty(rest));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger(events: &[&str], guard: Option<&str>, action: Option<&str>) -> Trigger {
        Trigger {
            events: events.iter().map(|e| (*e).to_owned()).collect(),
            guard: guard.map(str::to_owned),
            action: action.map(str::to_owned),
        }
    }

    #[test]
    fn reads_event_guard_and_action() {
        for (label, expected) in [
            (
                "timeout [retries < 3] / retry()",
                trigger(&["timeout"], Some("retries < 3"), Some("retry()")),
            ),
            ("door  opened", trigger(&["door opened"], None, None)),
            ("[else]", trigger(&[], Some("else"), None)),
            ("/ log", trigger(&[], None, Some("log"))),
            (
                "a, b(x, y) [ok]",
                trigger(&["a", "b(x, y)"], Some("ok"), None),
            ),
            // Line breaks are spaces; a `/` in brackets is the guard's.
            (
                "speed\\nchanged [v > 50 km/h]<br>/ warn",
                trigger(&["speed changed"], Some("v > 50 km/h"), Some("warn")),
            ),
            ("go [open", trigger(&["go"], Some("open"), None)),
            ("", trigger(&[], None, None)),
        ] {
            assert_eq!(Trigger::parse(label), expected, "{label}");
        }
    }
}
