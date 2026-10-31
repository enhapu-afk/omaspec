//! Lines by hand as edits of the source
//! ([0016](../../../docs/adr/0016-lines-by-hand.md)): a drag in the diagram
//! gives a line its bends, a reset takes them out again. The block goes
//! before PlantUML's `@end…` line, and at the end of a Mermaid diagram,
//! inside its code fence; a blank line goes before it. It goes again with
//! its last entry. The entries' form is [`omaspec_uml::hand`].

use std::ops::Range;

use omaspec_uml::hand::{self, Line, MARK};

use crate::lang::Language;
use crate::text::line_ranges;

/// The edit that gives `line` the bends it has, in place of what the block
/// says for it, or takes its entry out when it has none: the byte range to
/// replace and its new text. `None` when nothing changes. Entries that do
/// not read stay as they are written.
pub fn edit(code: &str, language: Language, line: &Line) -> Option<(Range<usize>, String)> {
    let rows = line_ranges(code);
    let nl = newline(code);
    let Some(block) = hand::read(code) else {
        return (!line.corners.is_empty()).then(|| insert(code, language, &rows, line));
    };
    let row = |i: usize| code[rows[i].clone()].trim_end_matches(['\n', '\r']);
    let mark = row(block.first);
    let indent = &mark[..mark.len() - mark.trim_start().len()];
    let mut kept: Vec<String> = Vec::new();
    let mut put = line.corners.is_empty();
    for entry in &block.entries {
        if entry.read.as_ref().is_ok_and(|l| l.key == line.key) {
            if !put {
                kept.push(format!("{indent}{} {line}", block.comment));
                put = true;
            }
        } else {
            kept.push(row(entry.line).to_owned());
        }
    }
    if !put {
        kept.push(format!("{indent}{} {line}", block.comment));
    }
    let range = rows[block.first].start..rows[block.last].end;
    if kept.is_empty() {
        return Some(removal(code, &rows, block.first, block.last));
    }
    let mut text = format!("{mark}{nl}{}", kept.join(nl));
    if code[range.clone()].ends_with('\n') {
        text.push_str(nl);
    }
    (code[range.clone()] != text).then_some((range, text))
}

/// The edit that takes every line by hand out: the whole block.
pub fn clear(code: &str) -> Option<(Range<usize>, String)> {
    let block = hand::read(code)?;
    Some(removal(code, &line_ranges(code), block.first, block.last))
}

/// A new block for `line`.
fn insert(
    code: &str,
    language: Language,
    rows: &[Range<usize>],
    line: &Line,
) -> (Range<usize>, String) {
    let text = |i: usize| code[rows[i].clone()].trim_start_matches('\u{feff}').trim();
    let (at, comment) = match language {
        Language::PlantUml => {
            let start = (0..rows.len()).find(|&i| text(i).starts_with("@start"));
            let end =
                start.and_then(|s| (s + 1..rows.len()).find(|&i| text(i).starts_with("@end")));
            (end.map_or(code.len(), |i| rows[i].start), "'")
        }
        Language::Mermaid => {
            // As `lang::strip_mermaid_fence` finds it: blank lines may
            // come before the fence.
            let open = (0..rows.len()).find(|&i| !text(i).is_empty());
            let close = open.filter(|&i| text(i).starts_with("```")).and_then(|o| {
                (o + 1..rows.len())
                    .rev()
                    .find(|&i| text(i).starts_with("```"))
            });
            (close.map_or(code.len(), |i| rows[i].start), "%%")
        }
    };
    let nl = newline(code);
    let before = rows.iter().position(|r| r.end == at && at > 0);
    let mut out = String::new();
    if at > 0 && !code[..at].ends_with('\n') {
        out.push_str(nl);
    }
    let blank_before = before.is_some_and(|i| text(i).is_empty() || text(i).starts_with("@start"));
    if at > 0 && !blank_before {
        out.push_str(nl);
    }
    out.push_str(&format!("{comment} {MARK}{nl}{comment} {line}{nl}"));
    (at..at, out)
}

/// The line break the file uses.
fn newline(code: &str) -> &'static str {
    if code.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Takes out the rows `first` to `last`, with the blank row before them.
fn removal(code: &str, rows: &[Range<usize>], first: usize, last: usize) -> (Range<usize>, String) {
    let blank = first > 0 && code[rows[first - 1].clone()].trim().is_empty();
    let start = if blank { first - 1 } else { first };
    (rows[start].start..rows[last].end, String::new())
}

#[cfg(test)]
mod tests {
    use omaspec_uml::hand::{Corner, Key};

    use super::*;

    fn line(from: &str, to: &str, nth: usize, corners: &[(f32, f32)]) -> Line {
        Line {
            key: Key {
                from: from.into(),
                to: to.into(),
                nth,
            },
            corners: corners.iter().map(|&(x, y)| Corner { x, y }).collect(),
        }
    }

    fn apply(code: &str, edit: Option<(Range<usize>, String)>) -> String {
        match edit {
            Some((range, text)) => format!("{}{text}{}", &code[..range.start], &code[range.end..]),
            None => code.to_owned(),
        }
    }

    const STATES: &str = "@startuml\nIdle --> Running : start\nRunning --> Idle : stop\n@enduml\n";

    #[test]
    fn a_first_line_by_hand_opens_the_block_before_the_end() {
        let one = line("Idle", "Running", 1, &[(0.5, -60.0)]);
        let code = apply(STATES, edit(STATES, Language::PlantUml, &one));
        assert_eq!(
            code,
            "@startuml\nIdle --> Running : start\nRunning --> Idle : stop\n\n\
             ' lines by hand (Omaspec)\n' Idle -> Running at 0.5 -60\n@enduml\n"
        );
        assert_eq!(hand::lines(&code), std::slice::from_ref(&one));

        // Another line goes after it; the same line again changes nothing.
        let two = line("Running", "Idle", 1, &[(0.3, 40.0), (0.7, 40.0)]);
        let code = apply(&code, edit(&code, Language::PlantUml, &two));
        assert!(code.ends_with(
            "' Idle -> Running at 0.5 -60\n' Running -> Idle at 0.3 40, 0.7 40\n@enduml\n"
        ));
        assert!(edit(&code, Language::PlantUml, &one).is_none());

        // A moved bend replaces the entry in place.
        let moved = line("Idle", "Running", 1, &[(0.4, -20.0)]);
        let code = apply(&code, edit(&code, Language::PlantUml, &moved));
        assert!(code.contains("' Idle -> Running at 0.4 -20\n' Running -> Idle at"));

        // Without bends a line leaves; the last one takes the block along.
        let code = apply(
            &code,
            edit(&code, Language::PlantUml, &line("Idle", "Running", 1, &[])),
        );
        assert_eq!(hand::lines(&code), [two]);
        let code = apply(
            &code,
            edit(&code, Language::PlantUml, &line("Running", "Idle", 1, &[])),
        );
        assert_eq!(code, STATES);
        assert!(edit(&code, Language::PlantUml, &line("Running", "Idle", 1, &[])).is_none());
    }

    #[test]
    fn mermaid_ends_the_diagram_with_it_inside_a_fence() {
        let one = line("A", "B", 2, &[(0.5, 10.0)]);
        let flow = "flowchart LR\n  A --> B\n  A --> B";
        assert_eq!(
            apply(flow, edit(flow, Language::Mermaid, &one)),
            "flowchart LR\n  A --> B\n  A --> B\n\n%% lines by hand (Omaspec)\n%% A -> B #2 at 0.5 10\n"
        );
        let fenced = "```mermaid\nflowchart LR\n  A --> B\n```\n";
        assert_eq!(
            apply(fenced, edit(fenced, Language::Mermaid, &one)),
            "```mermaid\nflowchart LR\n  A --> B\n\n%% lines by hand (Omaspec)\n%% A -> B #2 at 0.5 10\n```\n"
        );
    }

    #[test]
    fn a_fence_after_blank_lines_or_a_bom_still_holds_the_block() {
        let one = line("A", "B", 1, &[(0.5, 10.0)]);
        for code in [
            "\n```mermaid\nflowchart LR\n  A --> B\n```\n",
            "\u{feff}```mermaid\nflowchart LR\n  A --> B\n```\n",
        ] {
            let done = apply(code, edit(code, Language::Mermaid, &one));
            assert!(
                done.ends_with(
                    "  A --> B\n\n%% lines by hand (Omaspec)\n%% A -> B at 0.5 10\n```\n"
                ),
                "{done:?}"
            );
            assert_eq!(
                crate::lang::strip_mermaid_fence(&done),
                "flowchart LR\n  A --> B\n\n%% lines by hand (Omaspec)\n%% A -> B at 0.5 10"
            );
        }
        let bom = "\u{feff}@startuml\nA --> B\n@enduml\n";
        assert_eq!(
            apply(bom, edit(bom, Language::PlantUml, &one)),
            "\u{feff}@startuml\nA --> B\n\n' lines by hand (Omaspec)\n' A -> B at 0.5 10\n@enduml\n"
        );
    }

    #[test]
    fn a_file_with_crlf_keeps_its_line_breaks() {
        let code = "@startuml\r\nA --> B\r\nB --> A\r\n@enduml\r\n";
        let one = apply(
            code,
            edit(code, Language::PlantUml, &line("A", "B", 1, &[(0.5, 1.0)])),
        );
        assert_eq!(
            one,
            "@startuml\r\nA --> B\r\nB --> A\r\n\r\n' lines by hand (Omaspec)\r\n' A -> B at 0.5 1\r\n@enduml\r\n"
        );
        let two = apply(
            &one,
            edit(&one, Language::PlantUml, &line("B", "A", 1, &[(0.2, 2.0)])),
        );
        assert_eq!(
            two,
            "@startuml\r\nA --> B\r\nB --> A\r\n\r\n' lines by hand (Omaspec)\r\n' A -> B at 0.5 1\r\n' B -> A at 0.2 2\r\n@enduml\r\n"
        );
        assert!(!two.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn the_block_keeps_its_indent_and_what_does_not_read() {
        let code = "flowchart LR\n  A --> B\n\n  %% lines by hand (Omaspec)\n  %% A -> B at oops\n  %% B -> A at 0.5 1\n";
        let one = line("B", "A", 1, &[(0.25, 3.0)]);
        assert_eq!(
            apply(code, edit(code, Language::Mermaid, &one)),
            "flowchart LR\n  A --> B\n\n  %% lines by hand (Omaspec)\n  %% A -> B at oops\n  %% B -> A at 0.25 3\n"
        );
        let other = line("C", "A", 1, &[(0.5, 0.0)]);
        assert!(
            apply(code, edit(code, Language::Mermaid, &other)).ends_with("  %% C -> A at 0.5 0\n")
        );
        assert_eq!(apply(code, clear(code)), "flowchart LR\n  A --> B\n");
        assert!(clear(STATES).is_none());
    }

    #[test]
    fn a_second_entry_for_a_line_goes_with_the_first() {
        let code = "@startuml\nA --> B\n' lines by hand (Omaspec)\n' A -> B at 0.5 1\n' A -> B at 0.2 2\n@enduml\n";
        let one = line("A", "B", 1, &[(0.9, 9.0)]);
        assert_eq!(
            apply(code, edit(code, Language::PlantUml, &one)),
            "@startuml\nA --> B\n' lines by hand (Omaspec)\n' A -> B at 0.9 9\n@enduml\n"
        );
    }
}
