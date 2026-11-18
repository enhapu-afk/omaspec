//! A diagram inside a diagram (REQ-077): PlantUML draws what stands
//! between `{{` and `}}` in a note or a label as a picture of its own.
//! Omaspec's engine writes those lines as text, so lint says it on the
//! line that opens one.

use super::{Finding, Severity};

/// What may follow `{{` on its line: nothing, or the kind of the diagram.
fn opens(line: &str) -> bool {
    let Some(rest) = line.trim().strip_prefix("{{") else {
        return false;
    };
    let rest = rest.trim();
    rest.chars().all(|c| c.is_ascii_alphabetic()) && rest.len() <= 12
}

/// A hint for each line of a PlantUML diagram that opens an embedded
/// diagram, where a later line closes it.
pub(super) fn findings(lines: &[&str], findings: &mut Vec<Finding>) {
    for (row, line) in lines.iter().enumerate() {
        if opens(line) && lines[row + 1..].iter().any(|later| later.trim() == "}}") {
            findings.push(Finding::new(
                "embedded-diagram",
                row,
                Severity::Info,
                "Omaspec writes a diagram inside a diagram (`{{` to `}}`) as its text; the Java PlantUML draws it as a picture.",
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
    fn the_line_that_opens_a_diagram_inside_gets_a_hint() {
        let code = "@startuml\nAlice -> Bob\nnote right\n  {{\n  Bob -> Carol\n  }}\nend note\nnote left\n{{wbs\n* a\n}}\nend note\n@enduml";
        assert_eq!(found(code), [3, 8]);
        // Never closed, or braces that are words of a label: no diagram.
        assert!(found("@startuml\nnote right\n{{\nend note\n@enduml").is_empty());
        assert!(found("@startuml\nA -> B : {{ x }}\n}}\n@enduml").is_empty());
    }
}
