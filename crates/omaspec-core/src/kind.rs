//! What kind of diagram a source draws: sequence, activity, state, …
//!
//! Lint, outline and the state machines pick their rules by kind. PlantUML
//! has no header for most kinds; the engine decides the type by the lines
//! ([`omaspec_uml::uml_type`]), so nothing here reads a diagram as
//! another type than the picture shows. This names it: the structure kind
//! with the most lines, and nothing for a diagram without a line yet.

use std::borrow::Cow;

use omaspec_uml::{UmlType, element_lines, uml_type};
use regex::Regex;
use serde::Serialize;

use crate::lang::{Language, puml_start, strip_mermaid_fence};
use crate::text::{re, significant_lines};

/// The rule family of a diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Sequence,
    Activity,
    State,
    Timing,
    /// Classes, components, deployment, use cases, objects.
    Structure,
    /// Mind maps, Gantt charts, pies and everything else without rules.
    Other,
}

/// A diagram's kind and its readable name, e.g. `Structure` / "Class diagram".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagramKind {
    pub kind: Kind,
    pub name: Cow<'static, str>,
}

impl DiagramKind {
    fn new(kind: Kind, name: &'static str) -> Self {
        Self {
            kind,
            name: Cow::Borrowed(name),
        }
    }
}

pub fn classify(code: &str, lang: Language) -> DiagramKind {
    match lang {
        Language::Mermaid => {
            mermaid_kind(&significant_lines(strip_mermaid_fence(code), usize::MAX))
        }
        Language::PlantUml => plantuml_kind(code, &significant_lines(code, usize::MAX)),
    }
}

/// `@start…` blocks other than `uml` name their kind directly.
fn block_name(block: &str) -> Option<&'static str> {
    Some(match block {
        "mindmap" => "Mind map",
        "wbs" => "WBS",
        "gantt" => "Gantt chart",
        "flow" => "Signal flow",
        "chronology" => "Chronology",
        "json" => "JSON",
        "yaml" => "YAML",
        "salt" => "Salt UI",
        "latex" | "math" => "Formula",
        "ditaa" => "Ditaa",
        "dot" => "Graphviz",
        "tree" => "Tree",
        _ => return None,
    })
}

/// The structure kinds, checked on the part of a line before its label.
/// The one with the most lines wins.
const STRUCTURE_NAMES: [&str; 6] = [
    "Class diagram",
    "Use case diagram",
    "Component diagram",
    "Deployment diagram",
    "Object diagram",
    "Structure diagram",
];

fn structure_hit(i: usize, lead: &str) -> bool {
    match i {
        0 => re!(
            r"(?i)^(?:abstract|class|interface|enum|annotation|protocol|struct|exception|metaclass|dataclass|record)\s"
        )
        .is_match(lead),
        1 => {
            re!(r"(?i)^usecase\s").is_match(lead)
                || capture_is(re!(r"^\(([^)]+)\)"), lead, |c| c != "*")
                || capture_is(re!(r"[\s>-]\(([^)]+)\)\s*$"), lead, |c| c != "*")
        }
        2 => {
            re!(r"(?i)^(?:component|port|portin|portout)\s").is_match(lead)
                || capture_is(re!(r"^\[([^\]]+)\]"), lead, |c| {
                    !re!(r"(?i)^[ox]?[-<]").is_match(c)
                })
                || re!(r"\s\[[^\]#][^\]]*\]\s*$").is_match(lead)
        }
        3 => re!(r"(?i)^(?:node|cloud|frame|folder|storage|artifact|card|file|stack|hexagon|agent)\s")
            .is_match(lead),
        4 => re!(r"(?i)^(?:object|map|json)\s").is_match(lead),
        _ => re!(r"(?i)^(?:package|namespace|rectangle)\s").is_match(lead),
    }
}

/// Whether `re` matches and its first group passes `ok`.
fn capture_is(re: &Regex, s: &str, ok: impl Fn(&str) -> bool) -> bool {
    re.captures(s)
        .and_then(|c| c.get(1))
        .is_some_and(|m| ok(m.as_str()))
}

/// The part of a line before its label: `A --> B : label` → `A --> B`.
/// The actor shorthand `:Name:` is a name, not a label.
fn before_label(line: &str) -> String {
    let line = re!(r"^:[^:]+:").replace(line, "A");
    let bytes = line.as_bytes();
    let colon = (0..bytes.len()).find(|&i| bytes[i] == b':' && bytes.get(i + 1) != Some(&b':'));
    match colon {
        Some(i) => line[..i].trim_end().to_owned(),
        None => line.into_owned(),
    }
}

/// The word after `@start` in a PlantUML diagram, in lowercase: `uml`,
/// `mindmap`, `salt`, … Empty without a start line.
pub fn plantuml_block(code: &str) -> String {
    block_of(&significant_lines(code, usize::MAX))
}

fn block_of(lines: &[&str]) -> String {
    let start = lines
        .iter()
        .find(|l| puml_start().is_match(l))
        .copied()
        .unwrap_or("");
    re!(r"(?i)^@start(\w+)")
        .captures(start)
        .map(|c| c[1].to_ascii_lowercase())
        .unwrap_or_default()
}

fn plantuml_kind(code: &str, lines: &[&str]) -> DiagramKind {
    let block = block_of(lines);
    if !block.is_empty() && block != "uml" {
        let name = match block_name(&block) {
            Some(name) => Cow::Borrowed(name),
            None => Cow::Owned(block),
        };
        return DiagramKind {
            kind: Kind::Other,
            name,
        };
    }
    match uml_type(code) {
        Some(UmlType::Timing) => DiagramKind::new(Kind::Timing, "Timing diagram"),
        Some(UmlType::Activity) => DiagramKind::new(Kind::Activity, "Activity diagram"),
        Some(UmlType::State) => DiagramKind::new(Kind::State, "State diagram"),
        Some(UmlType::Structure) => DiagramKind::new(Kind::Structure, structure_name(code)),
        Some(UmlType::Sequence) => DiagramKind::new(Kind::Sequence, "Sequence diagram"),
        _ => DiagramKind {
            kind: Kind::Other,
            name: Cow::Borrowed(""),
        },
    }
}

/// The readable name of a structure diagram: the kind with the most lines.
fn structure_name(code: &str) -> &'static str {
    let mut structure = [0usize; STRUCTURE_NAMES.len()];
    for line in element_lines(code).unwrap_or_default() {
        let lead = before_label(line.text);
        for (i, count) in structure.iter_mut().enumerate() {
            if structure_hit(i, &lead) {
                *count += 1;
            }
        }
    }
    let mut best: Option<usize> = None;
    for (i, &n) in structure.iter().enumerate() {
        if n > 0 && best.is_none_or(|b| n > structure[b]) {
            best = Some(i);
        }
    }
    match best {
        // Nodes group components in both kinds; the rules are the same.
        Some(2 | 3) if structure[2] > 0 && structure[3] > 0 => "Component and deployment diagram",
        Some(best) => STRUCTURE_NAMES[best],
        None => "Structure diagram",
    }
}

const MERMAID_KINDS: [(&str, Kind, &str); 22] = [
    ("sequenceDiagram", Kind::Sequence, "Sequence diagram"),
    ("zenuml", Kind::Sequence, "Sequence diagram"),
    ("stateDiagram", Kind::State, "State diagram"),
    ("classDiagram", Kind::Structure, "Class diagram"),
    ("erDiagram", Kind::Structure, "ER diagram"),
    ("C4", Kind::Structure, "C4 diagram"),
    ("architecture-beta", Kind::Structure, "Architecture diagram"),
    ("block-beta", Kind::Structure, "Block diagram"),
    ("requirementDiagram", Kind::Structure, "Requirement diagram"),
    ("gantt", Kind::Other, "Gantt chart"),
    ("mindmap", Kind::Other, "Mind map"),
    ("timeline", Kind::Other, "Timeline"),
    ("journey", Kind::Other, "User journey"),
    ("pie", Kind::Other, "Pie chart"),
    ("quadrantChart", Kind::Other, "Quadrant chart"),
    ("xychart-beta", Kind::Other, "XY chart"),
    ("sankey-beta", Kind::Other, "Sankey diagram"),
    ("packet-beta", Kind::Other, "Packet diagram"),
    ("kanban", Kind::Other, "Kanban board"),
    ("radar-beta", Kind::Other, "Radar chart"),
    ("treemap-beta", Kind::Other, "Treemap"),
    ("gitGraph", Kind::Other, "Git graph"),
];

/// A head word: the keyword followed by a non-word character or the end.
/// `C4` matches any `C4…` head (`C4Context`, `C4Container`, …).
fn head_is(head: &str, keyword: &str) -> bool {
    let Some(rest) = head.strip_prefix(keyword) else {
        return false;
    };
    if keyword == "C4" {
        return rest.starts_with(|c: char| c.is_alphanumeric() || c == '_');
    }
    !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_')
}

fn mermaid_kind(lines: &[&str]) -> DiagramKind {
    let head = lines
        .iter()
        .find(|l| !l.starts_with("%%"))
        .copied()
        .unwrap_or("");
    if head_is(head, "flowchart") || head_is(head, "graph") {
        let decision = lines
            .iter()
            .filter(|l| !l.starts_with("%%"))
            .any(|l| is_decision(l));
        let kind = if decision {
            Kind::Activity
        } else {
            Kind::Structure
        };
        return DiagramKind::new(kind, "Flowchart");
    }
    if let Some(&(_, kind, name)) = MERMAID_KINDS.iter().find(|(k, ..)| head_is(head, k)) {
        return DiagramKind::new(kind, name);
    }
    let word = re!(r"^[\w-]+").find(head).map_or("", |m| m.as_str());
    DiagramKind {
        kind: Kind::Other,
        name: Cow::Owned(word.to_owned()),
    }
}

/// A rhombus `id{text}` (not a hexagon `{{…}}`) or a decision shape.
fn is_decision(line: &str) -> bool {
    let rhombus = re!(r"[\w\])]\{[^{}]*\}")
        .find_iter(line)
        .any(|m| !line[m.end()..].starts_with('}'));
    rhombus || re!(r"shape:\s*(?:diam|diamond|decision|question|rhombus)\b").is_match(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn puml(code: &str) -> Kind {
        classify(code, Language::PlantUml).kind
    }

    #[test]
    fn plantuml_kinds() {
        assert_eq!(puml("@startuml\nA -> B : hi\n@enduml"), Kind::Sequence);
        assert_eq!(
            puml("@startuml\nstart\n:work;\nstop\n@enduml"),
            Kind::Activity
        );
        assert_eq!(puml("@startuml\n[*] --> Idle\n@enduml"), Kind::State);
        assert_eq!(
            puml("@startuml\nconcise \"Web\" as W\n@enduml"),
            Kind::Timing
        );
        assert_eq!(
            puml("@startuml\nclock clk with period 1\n@enduml"),
            Kind::Timing
        );
        assert_eq!(
            puml("@startuml\nclock -> server : tick\n@enduml"),
            Kind::Sequence
        );
        assert_eq!(
            puml("@startuml\n[*] --> Analog\nAnalog : on\n@enduml"),
            Kind::State
        );
        assert_eq!(
            puml("@startuml\nclass A\nA --> B\n@enduml"),
            Kind::Structure
        );
        assert_eq!(puml("@startuml\n(Login) -- User\n@enduml"), Kind::Structure);
        assert_eq!(puml("@startmindmap\n* a\n@endmindmap"), Kind::Other);
        assert_eq!(puml("@startuml\n- one\n- two\n@enduml"), Kind::Activity);
        assert_eq!(puml("@startuml\n* one\n** two\n@enduml"), Kind::Activity);
        assert_eq!(
            puml("@startuml\n:Creole:\n  bold: **bold**;\n@enduml"),
            Kind::Activity
        );
        assert_eq!(
            puml("@startuml\n:User: --> (Login)\n@enduml"),
            Kind::Structure
        );
        assert_eq!(
            puml("@startuml\n[Order] --> [Billing]\n@enduml"),
            Kind::Structure
        );
        assert_eq!(puml("@startuml\ntitle Nothing yet\n@enduml"), Kind::Other);
    }

    #[test]
    fn the_engine_decides_the_type() {
        // Each of these was once read as another type than the engine draws.
        assert_eq!(
            puml("@startuml\nhide empty description\nA --> B : go\n@enduml"),
            Kind::State
        );
        assert_eq!(puml("@startuml\nX <<deephistory>>\n@enduml"), Kind::State);
        assert_eq!(
            puml("@startuml\nX <<entrypoint>>\nX --> A\n@enduml"),
            Kind::Sequence
        );
        assert_eq!(puml("@startuml\ncreate Bob\n@enduml"), Kind::Sequence);
        // Only the first diagram counts, after the preprocessor.
        assert_eq!(
            puml("@startuml\nA -> B\n@enduml\n[*] --> Idle"),
            Kind::Sequence
        );
        assert_eq!(
            puml("@startuml\n!$start = \"[*]\"\n$start --> Idle\n@enduml"),
            Kind::State
        );
    }

    #[test]
    fn plantuml_blocks() {
        assert_eq!(plantuml_block("' x\n@startSalt\n{\n}\n@endsalt"), "salt");
        assert_eq!(plantuml_block("@startuml\nA -> B\n@enduml"), "uml");
        assert_eq!(plantuml_block("A -> B"), "");
    }

    #[test]
    fn plantuml_names() {
        let k = classify("@startuml\nnode N\n[C] --> N\n@enduml", Language::PlantUml);
        assert_eq!(k.name, "Component and deployment diagram");
        let k = classify("@startgantt\n@endgantt", Language::PlantUml);
        assert_eq!(k.name, "Gantt chart");
        let k = classify("@startflow\n@endflow", Language::PlantUml);
        assert_eq!(k.name, "Signal flow");
        assert_eq!(k.kind, Kind::Other);
    }

    #[test]
    fn labels_do_not_vote() {
        // `class` after the colon is label text, the arrow makes it a message.
        assert_eq!(
            puml("@startuml\nA -> B : class Foo\n@enduml"),
            Kind::Sequence
        );
        assert_eq!(before_label("A -> B : x"), "A -> B");
        assert_eq!(before_label("a::b : c"), "a:");
        assert_eq!(before_label(":User: --> (Use)"), "A --> (Use)");
    }

    #[test]
    fn mermaid_kinds() {
        let m = |code: &str| classify(code, Language::Mermaid);
        assert_eq!(m("sequenceDiagram\nA->>B: x").kind, Kind::Sequence);
        assert_eq!(m("flowchart TD\nA{ok?} --> B").kind, Kind::Activity);
        assert_eq!(m("flowchart TD\nA{{hex}} --> B").kind, Kind::Structure);
        assert_eq!(m("C4Context\ntitle x").name, "C4 diagram");
        assert_eq!(m("stateDiagram-v2\n[*] --> A").kind, Kind::State);
        assert_eq!(m("pie\n\"a\": 1").name, "Pie chart");
        assert_eq!(m("weird-thing\nx").name, "weird-thing");
    }
}
