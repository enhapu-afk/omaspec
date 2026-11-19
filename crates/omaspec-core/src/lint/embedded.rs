//! A diagram inside a diagram (REQ-077): PlantUML draws what stands
//! between `{{` and `}}` in a note or a legend as a picture of its own,
//! and so does Omaspec's engine ([`omaspec_uml::embedded`]). Where the
//! engine does not draw that diagram, it writes those lines as text, and
//! lint says so on the line that opens it, with why.

use omaspec_uml::embedded::{closes, declined, opens};

use super::{Finding, Severity};

/// A hint for each line of a PlantUML diagram that opens an embedded
/// diagram the engine does not draw, where a later line closes it.
pub(super) fn findings(lines: &[&str], findings: &mut Vec<Finding>) {
    for (row, line) in lines.iter().enumerate() {
        let Some(kind) = opens(line) else { continue };
        let Some(end) = closes(&lines[row + 1..]) else {
            continue;
        };
        if let Err(why) = declined(kind, &lines[row + 1..row + 1 + end]) {
            findings.push(Finding::new(
                "embedded-diagram",
                row,
                Severity::Info,
                format!(
                    "Omaspec writes this diagram inside a diagram (`{{{{` to `}}}}`) as its text, since {}; the Java PlantUML draws it as a picture.",
                    why.reason.trim_end_matches('.')
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(code: &str) -> Vec<usize> {
        let lines: Vec<&str> = code.split('\n').collect();
        let mut out = Vec::new();
        findings(&lines, &mut out);
        out.iter().map(|f| f.line).collect()
    }

    #[test]
    fn the_line_that_opens_a_diagram_the_engine_does_not_draw_gets_a_hint() {
        // Drawn: no hint. A kind the engine does not draw: a hint.
        let code = "@startuml\nAlice -> Bob\nnote right\n  {{\n  Bob -> Carol\n  }}\nend note\nnote left\n{{wbs\n* a\n}}\nend note\nnote left\n{{dot\ndigraph { a -> b }\n}}\nend note\n@enduml";
        assert_eq!(found(code), [13]);
        // Never closed, or braces that are words of a label: no diagram.
        assert!(found("@startuml\nnote right\n{{\nend note\n@enduml").is_empty());
        assert!(found("@startuml\nA -> B : {{ x }}\n}}\n@enduml").is_empty());
    }
}
