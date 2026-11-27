//! Every kind of finding, by its code, and whether its findings say how
//! they are put right: always, where the fix is sure, or never, and why
//! not. An agent or a person applies a fix without reading it first
//! (`omaspec lint --fix`, *Fix all*), so a rule says one only where no
//! choice is left to the writer. The guide lists the codes; a test holds
//! every code the crates report to this list, and one applies each rule's
//! fix and finds its finding gone.

/// Whether the findings of a rule carry their fix ([`super::Finding::fix`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixes {
    /// Each finding carries one.
    Always,
    /// A finding carries one where the fix is sure: when, in words.
    Where(&'static str),
    /// None does: why no fix is sure.
    Never(&'static str),
}

/// A kind of finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// The code its findings carry ([`super::Finding::code`]).
    pub code: &'static str,
    pub fixes: Fixes,
}

impl Rule {
    /// Whether any of its findings can carry a fix.
    pub fn fixed(&self) -> bool {
        !matches!(self.fixes, Fixes::Never(_))
    }
}

const fn rule(code: &'static str, fixes: Fixes) -> Rule {
    Rule { code, fixes }
}

use Fixes::{Always, Never, Where};

/// What a trace's findings say of fixing: a trace is what happened.
const RECORD: &str = "a trace is a record of what happened; a deviation is told as a task instead";

/// Every rule, in the guide's order.
pub const RULES: &[Rule] = &[
    rule(
        "refused",
        Never("what the line should say instead is the writer's choice"),
    ),
    rule(
        "java-only",
        Never("the line is right as PlantUML reads it; Omaspec's own engine lacks it"),
    ),
    rule(
        "not-read",
        Never("Omaspec's readers lack something, not the diagram"),
    ),
    rule("missing-end", Always),
    rule(
        "unclosed-block",
        Where(
            "but a `repeat`, whose test only the writer knows, and the blocks around one: closed where the diagram ends, as it is drawn now, the innermost first",
        ),
    ),
    rule(
        "unknown-head",
        Where("its first word is one kind of diagram misspelled"),
    ),
    rule(
        "declared-twice",
        Where("the second declaration reads as the first"),
    ),
    rule(
        "undeclared-participant",
        Where("the name is written once on its line"),
    ),
    rule(
        "unused-participant",
        Never("whether it stays in the picture is the writer's choice"),
    ),
    rule("lane-spelling", Always),
    rule(
        "lone-state",
        Never("a guess: a state named once may be meant so"),
    ),
    rule(
        "namesake-state",
        Never("which of the two states a line means is the writer's choice"),
    ),
    rule(
        "no-start",
        Never("where the machine starts is the writer's choice"),
    ),
    rule(
        "entered-without-start",
        Never("where the state begins is the writer's choice"),
    ),
    rule(
        "unreachable-state",
        Never("what leads to the state, or whether it goes, is the writer's choice"),
    ),
    rule("dead-end", Where("the state's name is one word")),
    rule(
        "transition-twice",
        Where("the second line reads as the first"),
    ),
    rule(
        "ambiguous-event",
        Never("which state the event leads to is the writer's choice"),
    ),
    rule(
        "scenario-mismatch",
        Never("whether the scenario or the machine is wrong is the writer's choice"),
    ),
    rule(
        "dead-link",
        Never("which diagram the link means is the writer's choice"),
    ),
    rule("trace-line", Never(RECORD)),
    rule("trace-deviation", Never(RECORD)),
    rule("trace-unchecked", Never(RECORD)),
    rule("trace-no-start", Never(RECORD)),
    rule("trace-too-many-ways", Never(RECORD)),
    rule("stale-entry", Always),
    rule(
        "by-hand-miss",
        Never("the entry is the writer's drawing: drawn anew or taken out, the writer says which"),
    ),
    rule("unknown-icon", Where("the set has one name near it")),
    rule(
        "faint-color",
        Where(
            "the color to change is a text's on its own fill, or a line's on the page: black or white, whichever reads",
        ),
    ),
    rule(
        "unheeded-setting",
        Never("mermaid.js heeds it where Omaspec does not"),
    ),
    rule(
        "frame-shape",
        Never("the shape is the writer's, kept for when Mermaid draws it"),
    ),
    rule(
        "embedded-diagram",
        Never("the engine lacks the diagram inside, not the text"),
    ),
    rule("unseen-character", Always),
    rule("not-mermaid-js", Always),
    rule(
        "omaspec-only",
        Never("the syntax is Omaspec's own on purpose"),
    ),
    rule(
        "project-file",
        Never("what the project's file should say is the writer's choice"),
    ),
    rule(
        "not-drawn",
        Never("what the diagram should say instead is the writer's choice"),
    ),
];

/// The rule of `code`.
pub fn rule_of(code: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.code == code)
}

/// How many rules can say their fix, of all.
pub fn share() -> (usize, usize) {
    (RULES.iter().filter(|r| r.fixed()).count(), RULES.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_code_is_listed_once() {
        let mut codes: Vec<&str> = RULES.iter().map(|r| r.code).collect();
        codes.sort_unstable();
        let all = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), all);
        assert!(rule_of("dead-end").is_some_and(Rule::fixed));
        assert!(rule_of("trace-line").is_some_and(|r| !r.fixed()));
    }
}
