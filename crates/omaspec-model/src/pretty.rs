//! A state machine or a scenario written evenly: what `fmt` indents, and
//! then each line from one name to another with one space around its
//! arrow and its colon, the arrows and colons of lines that follow one
//! another in a column, lines that end without spaces, and one empty line
//! where there were several.
//!
//! Only spaces change. That is held to, not hoped: a result that differs
//! from the source in anything but spaces, or whose behavior reads
//! differently, is dropped for what `fmt` alone gives.

use omaspec_core::{Kind as DiagramKind, Language, classify};

use crate::assist::marks::{NO_MESSAGE, colon, declared, ends, state_declared};
use crate::assist::words::{outside_quotes, starts_with_word};

/// Names in one run of lines may differ this much in length and still
/// have their arrows in a column; past it the gaps would be wider than
/// they help.
const SPREAD: usize = 12;

/// A line from one name to another, in its parts.
struct Row<'a> {
    lead: &'a str,
    from: &'a str,
    arrow: &'a str,
    to: &'a str,
    label: Option<&'a str>,
    /// `\r` where the line had one.
    end: &'a str,
}

enum Line<'a> {
    Row(Row<'a>),
    /// Any other line, as it is but for the spaces at its end.
    Other(&'a str, &'a str),
    /// A line of a note, which is prose: untouched.
    Prose(&'a str),
}

/// `code` written evenly; a diagram that is neither a state machine nor
/// a scenario is indented as `fmt` does and no more.
pub fn pretty(code: &str, lang: Language) -> String {
    let formatted = omaspec_core::format::format(code, lang);
    let kind = classify(&formatted, lang).kind;
    if !matches!(kind, DiagramKind::State | DiagramKind::Sequence) {
        return formatted;
    }
    let even = written(&read(&formatted, lang, kind), lang, kind);
    let same_words = squeezed(&even) == squeezed(code);
    if same_words && crate::behavior(&even, lang) == crate::behavior(code, lang) {
        even
    } else {
        formatted
    }
}

/// `text` without its spaces, tabs and line ends.
fn squeezed(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn read<'a>(code: &'a str, lang: Language, kind: DiagramKind) -> Vec<Line<'a>> {
    let comment = match lang {
        Language::PlantUml => "'",
        Language::Mermaid => "%%",
    };
    let mut note = false;
    code.split('\n')
        .map(|raw| {
            let (body, end) = match raw.strip_suffix('\r') {
                Some(body) => (body, "\r"),
                None => (raw, ""),
            };
            let text = body.trim();
            let lead = &body[..body.len() - body.trim_start().len()];
            let other = Line::Other(body.trim_end(), end);
            if note {
                note = !text.to_ascii_lowercase().starts_with("end note") && text != "end";
                return if note { Line::Prose(raw) } else { other };
            }
            if starts_with_word(&text.to_ascii_lowercase(), "note") {
                note = !outside_quotes(text, ':');
                return other;
            }
            if text.is_empty() || text.starts_with(comment) || text.starts_with(['@', '!']) {
                return other;
            }
            let Some(at) = colon(text) else {
                return match line(text, kind, lang) {
                    Some((from, arrow, to)) => Line::Row(Row {
                        lead,
                        from,
                        arrow,
                        to,
                        label: None,
                        end,
                    }),
                    None => other,
                };
            };
            let label = text[at + 1..].trim();
            match line(&text[..at], kind, lang) {
                Some((from, arrow, to)) if !label.is_empty() => Line::Row(Row {
                    lead,
                    from,
                    arrow,
                    to,
                    label: Some(label),
                    end,
                }),
                _ => other,
            }
        })
        .collect()
}

/// The two names and the arrow of `head`, if it is a line from one name
/// to another.
fn line(head: &str, kind: DiagramKind, lang: Language) -> Option<(&str, &str, &str)> {
    let head = head.trim_end();
    let pseudo = kind == DiagramKind::State;
    if pseudo && state_declared(head).is_some() {
        return None;
    }
    if !pseudo
        && (declared(head, lang).is_some()
            || NO_MESSAGE.iter().any(|word| starts_with_word(head, word)))
    {
        return None;
    }
    let (from, arrow, to) = ends(head, pseudo)?;
    Some((&head[from], arrow, &head[to]))
}

fn written(lines: &[Line], lang: Language, kind: DiagramKind) -> String {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut blank = false;
    let mut at = 0;
    while at < lines.len() {
        match &lines[at] {
            Line::Prose(raw) => {
                out.push((*raw).to_owned());
                blank = false;
                at += 1;
            }
            Line::Other(body, end) => {
                // One empty line where there were several.
                if !(body.is_empty() && blank) {
                    out.push(format!("{body}{end}"));
                }
                blank = body.is_empty();
                at += 1;
            }
            Line::Row(first) => {
                let run: Vec<&Row> = lines[at..]
                    .iter()
                    .map_while(|line| match line {
                        Line::Row(row) if row.lead == first.lead => Some(row),
                        _ => None,
                    })
                    .collect();
                out.extend(rows(&run, lang, kind));
                blank = false;
                at += run.len();
            }
        }
    }
    out.join("\n")
}

/// How wide the widest of `parts` is, or 0 when they differ too much to
/// stand in a column.
fn column<'a>(parts: impl Iterator<Item = &'a str> + Clone) -> usize {
    let width = |part: &str| part.chars().count();
    let widest = parts.clone().map(width).max().unwrap_or(0);
    let narrowest = parts.map(width).min().unwrap_or(0);
    if widest - narrowest > SPREAD {
        0
    } else {
        widest
    }
}

/// A run of lines that follow one another, each written evenly.
fn rows(run: &[&Row], lang: Language, kind: DiagramKind) -> Vec<String> {
    // Mermaid is written as its own guide writes it: nothing in a column,
    // a scenario's arrow between its names without spaces.
    if lang == Language::Mermaid {
        let gap = if kind == DiagramKind::Sequence {
            ""
        } else {
            " "
        };
        return run
            .iter()
            .map(|r| {
                let label = r.label.map(|l| format!(": {l}")).unwrap_or_default();
                format!(
                    "{}{}{gap}{}{gap}{}{label}{}",
                    r.lead, r.from, r.arrow, r.to, r.end
                )
            })
            .collect();
    }
    let from = column(run.iter().map(|r| r.from));
    let arrow = column(run.iter().map(|r| r.arrow));
    let to = column(run.iter().filter(|r| r.label.is_some()).map(|r| r.to));
    run.iter()
        .map(|r| {
            let head = format!("{}{:from$} {:arrow$} ", r.lead, r.from, r.arrow);
            match r.label {
                Some(label) => format!("{head}{:to$} : {label}{}", r.to, r.end),
                None => format!("{head}{}{}", r.to, r.end),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_machine_gets_its_arrows_and_colons_in_a_column() {
        let code = "@startuml\n[*]-->Closed\nClosed -->   Opening:open [unlocked]\nOpening->Open :  end position   \n\n\n\nstate Open {\nOpen-->Closing : close\n}\n' a   comment  \nClosing --> Closed\n@enduml\n";
        let even = pretty(code, Language::PlantUml);
        assert_eq!(
            even,
            "@startuml\n[*]     --> Closed\nClosed  --> Opening : open [unlocked]\nOpening ->  Open    : end position\n\nstate Open {\n  Open --> Closing : close\n}\n' a   comment\nClosing --> Closed\n@enduml\n"
        );
        assert_eq!(pretty(&even, Language::PlantUml), even, "once is enough");
    }

    #[test]
    fn mermaid_is_written_as_its_guide_writes_it() {
        let code = "sequenceDiagram\n  participant A as Alice\n  A ->> B :  hi there\n  B-->>A:ok\n  Note over A,B: a   note\n";
        assert_eq!(
            pretty(code, Language::Mermaid),
            "sequenceDiagram\nparticipant A as Alice\nA->>B: hi there\nB-->>A: ok\nNote over A,B: a   note\n"
        );
        let state = "stateDiagram-v2\n[*]-->Idle\nIdle   -->  Busy:start\n";
        assert_eq!(
            pretty(state, Language::Mermaid),
            "stateDiagram-v2\n[*] --> Idle\nIdle --> Busy: start\n"
        );
    }

    #[test]
    fn a_note_s_lines_stay_as_fmt_leaves_them() {
        let code = "@startuml\r\nA->B:x\r\nnote over A\r\n  A   ->  B :  y\r\n\r\n\r\nend note\r\nLongerName  ->  B : z\r\n@enduml\r\n";
        assert_eq!(
            pretty(code, Language::PlantUml),
            "@startuml\nA -> B : x\nnote over A\n  A   ->  B :  y\n\nend note\nLongerName -> B : z\n@enduml\n"
        );
    }

    #[test]
    fn names_that_differ_much_are_not_put_in_a_column() {
        let code = "@startuml\nA -> B : x\nAVeryLongParticipantName -> B : y\n@enduml\n";
        assert_eq!(pretty(code, Language::PlantUml), code);
    }

    #[test]
    fn another_diagram_is_indented_and_no_more() {
        let code = "@startuml\nclass A {\n+x  :  int\n}\nA   -->   B\n@enduml\n";
        assert_eq!(
            pretty(code, Language::PlantUml),
            omaspec_core::format::format(code, Language::PlantUml)
        );
    }
}
