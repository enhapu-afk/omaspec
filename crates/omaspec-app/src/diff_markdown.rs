//! `omaspec diff --markdown`: what changed in the behavior, written as a
//! comment for a pull request (docs/adr/0036, docs/ci/), for a person to
//! review what an agent or a colleague changed by behavior, not by lines.
//! The changes are listed by diagram, each with its file and line; a state
//! machine that changed is drawn below them as Mermaid, which GitHub draws
//! in a comment where no picture of one's own can be put: the states that
//! came in green, those that changed in amber. What only lines can say (a
//! transition that came or went) stays in the list.
//!
//! What the diagrams say is written so that it stays words: no mention of
//! a person or a team (`@name`), no reference to an issue, no fence it
//! could close.

use omaspec_core::Language;
use omaspec_model::{Change, ChangeKind, Machine};
use omaspec_uml::state::Mark;

/// One diagram's changes, as a section of the comment.
pub(crate) struct Section<'a> {
    pub(crate) old: &'a str,
    pub(crate) new: &'a str,
    pub(crate) changes: &'a [Change],
    /// The machine now, as Mermaid with its changes colored.
    pub(crate) picture: Option<String>,
}

/// The comment: a heading, a line that counts, and each section.
pub(crate) fn comment(since: &str, sections: &[Section]) -> String {
    let mut out = format!("### Behavior changes {}\n\n", words(since));
    if sections.is_empty() {
        out.push_str("No diagram changed in its behavior.\n");
        return out;
    }
    let all = sections.iter().flat_map(|s| s.changes.iter());
    let count = |kind: ChangeKind| all.clone().filter(|c| c.kind == kind).count();
    let mut parts = vec![match sections.len() {
        1 => "1 diagram".to_owned(),
        n => format!("{n} diagrams"),
    }];
    for (kind, word) in [
        (ChangeKind::Added, "came"),
        (ChangeKind::Changed, "changed"),
        (ChangeKind::Removed, "went"),
        (ChangeKind::Found, "found now"),
        (ChangeKind::Resolved, "resolved"),
    ] {
        match count(kind) {
            0 => {}
            n => parts.push(format!("{n} {word}")),
        }
    }
    out.push_str(&format!("{}.\n", parts.join(" · ")));
    for section in sections {
        out.push_str(&format!("\n#### {}\n\n", code(section.new)));
        for change in section.changes {
            let at = match (change.new_line, change.old_line) {
                (Some(line), _) => format!("{}:{}", section.new, line + 1),
                (None, Some(line)) => format!("{}:{}", section.old, line + 1),
                (None, None) => section.new.to_owned(),
            };
            let kind = match (change.kind, change.severity) {
                (ChangeKind::Found | ChangeKind::Resolved, Some(severity)) => {
                    format!("{} {}", change.kind.name(), severity_name(severity))
                }
                (kind, _) => kind.name().to_owned(),
            };
            out.push_str(&format!(
                "- **{kind}** {} ({})\n",
                words(&change.message),
                code(&at)
            ));
        }
        if let Some(picture) = &section.picture {
            // Longer than any run of backticks inside, so none ends it.
            let fence = "`".repeat(longest_run(picture, '`').max(2) + 1);
            out.push_str(&format!(
                "\n<details><summary>The machine now: what came in green, what changed in amber</summary>\n\n{fence}mermaid\n{}\n{fence}\n\n</details>\n",
                picture.trim_end()
            ));
        }
    }
    out
}

fn severity_name(severity: omaspec_core::lint::Severity) -> &'static str {
    match severity {
        omaspec_core::lint::Severity::Error => "error",
        omaspec_core::lint::Severity::Warning => "warning",
        omaspec_core::lint::Severity::Info => "note",
    }
}

/// `text` as words on one line of a Markdown list: what would start
/// markup, HTML or an entity is written as itself, and a word that GitHub
/// would take for a mention (`@team`) or a reference (`#12`) stands in
/// code, where it takes neither.
fn words(text: &str) -> String {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|word| {
            if word.contains(['@', '#']) {
                return code(word);
            }
            let mut out = String::with_capacity(word.len());
            for c in word.chars() {
                if matches!(
                    c,
                    '\\' | '`' | '*' | '_' | '<' | '>' | '[' | ']' | '|' | '&' | '!' | '~'
                ) {
                    out.push('\\');
                }
                out.push(c);
            }
            out
        })
        .collect();
    words.join(" ")
}

/// `text` as code on one line, in more backticks than any run of them in
/// it.
fn code(text: &str) -> String {
    let text = text.replace(['\n', '\r'], " ");
    let ticks = "`".repeat(longest_run(&text, '`') + 1);
    // A backtick at an edge needs a space between it and the fence.
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{ticks}{pad}{text}{pad}{ticks}")
}

/// The longest run of `c` in `text`.
fn longest_run(text: &str, c: char) -> usize {
    let (mut longest, mut run) = (0, 0);
    for d in text.chars() {
        run = if d == c { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

/// The new version of a state machine as Mermaid, the states `changes`
/// brought or changed colored; every state where there was no machine
/// before (`old` none, or no machine). `None` for what is no state
/// machine, or what Mermaid cannot say.
pub(crate) fn picture(
    old: Option<(&str, Language)>,
    (new, new_lang): (&str, Language),
    changes: &[Change],
) -> Option<String> {
    let b = Machine::read(new, new_lang)?;
    let marks: Vec<(String, Mark)> = match old.and_then(|(text, lang)| Machine::read(text, lang)) {
        Some(a) => crate::diff_picture::marks(&a, &b, changes)
            .states
            .into_iter()
            .collect(),
        None => b
            .states
            .iter()
            .filter(|s| s.kind == omaspec_model::Kind::State && !s.name.is_empty())
            .map(|s| (s.name.clone(), Mark::Added))
            .collect(),
    };
    let mermaid = match new_lang {
        Language::Mermaid => new.to_owned(),
        Language::PlantUml => {
            omaspec_model::convert(new, Language::PlantUml, Language::Mermaid)
                .ok()?
                .code
        }
    };
    // A state renamed on the way is found by the name its box shows.
    let drawn = Machine::read(&mermaid, Language::Mermaid)?;
    let id = |name: &str| {
        drawn
            .states
            .iter()
            .find(|s| s.name == name)
            .or_else(|| {
                drawn
                    .states
                    .iter()
                    .find(|s| s.label.as_deref() == Some(name))
            })
            .map(|s| s.name.clone())
    };
    let mut marked: Vec<(String, &'static str)> = marks
        .iter()
        .filter_map(|(name, mark)| {
            let class = match mark {
                Mark::Added => "omaspecCame",
                Mark::Changed => "omaspecChanged",
                _ => return None,
            };
            Some((id(name)?, class))
        })
        .collect();
    marked.sort();
    let mut out = mermaid.trim_end().to_owned();
    if !marked.is_empty() {
        out.push_str("\n    classDef omaspecCame fill:#dafbe1,stroke:#1a7f37,color:#1f2328");
        out.push_str("\n    classDef omaspecChanged fill:#fff8c5,stroke:#9a6700,color:#1f2328");
        for (id, class) in marked {
            out.push_str(&format!("\n    class {id} {class}"));
        }
    }
    out.push('\n');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str =
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
    const NEW: &str = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";

    /// A machine that gained a state: the change listed with its file and
    /// line, and the machine drawn as Mermaid with the new state green.
    #[test]
    fn a_change_is_listed_and_the_machine_drawn_with_it_colored() {
        let changes = omaspec_model::diff(
            OLD,
            Language::PlantUml,
            NEW,
            Language::PlantUml,
            &omaspec_model::Specs::default(),
        )
        .unwrap();
        let picture = picture(
            Some((OLD, Language::PlantUml)),
            (NEW, Language::PlantUml),
            &changes,
        );
        let picture = picture.expect("a picture");
        assert!(picture.starts_with("stateDiagram-v2\n"), "{picture}");
        assert!(
            picture.contains("    class Locked omaspecCame"),
            "{picture}"
        );
        let comment = comment(
            "since main",
            &[Section {
                old: "door.puml",
                new: "door.puml",
                changes: &changes,
                picture: Some(picture),
            }],
        );
        assert!(
            comment.starts_with("### Behavior changes since main\n\n1 diagram"),
            "{comment}"
        );
        assert!(comment.contains("#### `door.puml`"), "{comment}");
        assert!(comment.contains("(`door.puml:5`)"), "{comment}");
        assert!(comment.contains("```mermaid\nstateDiagram-v2"), "{comment}");
        // Nothing changed: said so.
        let none = super::comment("since main", &[]);
        assert!(
            none.contains("No diagram changed in its behavior."),
            "{none}"
        );
        assert_eq!(words("a <b> *c*"), "a \\<b\\> \\*c\\*");
        // A machine new in the change: every state came.
        let new = super::picture(None, (OLD, Language::PlantUml), &[]).expect("a picture");
        assert!(new.contains("class Closed omaspecCame"), "{new}");
        assert!(new.contains("class Open omaspecCame"), "{new}");
    }

    /// What a diagram says stays words: no mention, no reference, no
    /// fence it closes, no name that leaves its code.
    #[test]
    fn what_a_diagram_says_cannot_write_the_comment() {
        assert_eq!(
            words("ask @security-team about #12 & <b>"),
            "ask `@security-team` about `#12` \\& \\<b\\>"
        );
        assert_eq!(code("a`b.mmd"), "``a`b.mmd``");
        assert_eq!(code("`x`"), "`` `x` ``");
        let mermaid = "stateDiagram-v2\n    %% ```\n    [*] --> A\n    A --> [*]\n";
        let changes = omaspec_model::diff(
            "stateDiagram-v2\n    [*] --> A\n",
            Language::Mermaid,
            mermaid,
            Language::Mermaid,
            &omaspec_model::Specs::default(),
        )
        .unwrap();
        let drawn = super::picture(None, (mermaid, Language::Mermaid), &changes).unwrap();
        let comment = comment(
            "since main",
            &[Section {
                old: "m.mmd",
                new: "m.mmd",
                changes: &changes,
                picture: Some(drawn),
            }],
        );
        assert!(comment.contains("\n````mermaid\n"), "{comment}");
        assert!(comment.contains("\n````\n\n</details>"), "{comment}");
    }
}
