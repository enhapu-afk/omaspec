//! The words and blocks of PlantUML and Mermaid a line may start with, by
//! the kind of diagram, and the words a diagram has written so far.

use std::collections::HashSet;

use omaspec_core::{DiagramKind, Kind, Language};

use super::{Suggestion, What};

/// A block: the word that finds it, what it is, and its text with places
/// to fill in, indented by two spaces as `omaspec fmt` indents.
type Block = (&'static str, &'static str, &'static str);

const PUML_ALWAYS: &[&str] = &["title", "skinparam", "note", "hide", "legend", "caption"];

const PUML_SEQUENCE: &[&str] = &[
    "participant",
    "actor",
    "boundary",
    "control",
    "entity",
    "database",
    "collections",
    "queue",
    "activate",
    "deactivate",
    "destroy",
    "create",
    "return",
    "autonumber",
    "else",
    "end",
];

const PUML_SEQUENCE_BLOCKS: &[Block] = &[
    (
        "alt",
        "one way or another",
        "alt ${1:condition}\n  $0\nelse ${2:otherwise}\n  \nend",
    ),
    ("opt", "only if", "opt ${1:condition}\n  $0\nend"),
    ("loop", "again and again", "loop ${1:until done}\n  $0\nend"),
    (
        "par",
        "at the same time",
        "par ${1:first}\n  $0\nelse ${2:second}\n  \nend",
    ),
    ("group", "a frame", "group ${1:label}\n  $0\nend"),
    (
        "critical",
        "all or nothing",
        "critical ${1:label}\n  $0\nend",
    ),
    (
        "break",
        "leaves the frame",
        "break ${1:condition}\n  $0\nend",
    ),
    (
        "box",
        "around participants",
        "box \"${1:title}\"\n  $0\nend box",
    ),
    ("note over", "a note", "note over ${1:Name} : ${0:text}"),
    (
        "ref over",
        "another scenario",
        "ref over ${1:Name} : ${0:text}",
    ),
];

const PUML_STATE: &[&str] = &["state"];

const PUML_STATE_BLOCKS: &[Block] = &[
    (
        "state block",
        "a composite state",
        "state ${1:Name} {\n  $0\n}",
    ),
    ("state choice", "a choice", "state ${1:name} <<choice>>"),
    (
        "note right of",
        "a note",
        "note right of ${1:State} : ${0:text}",
    ),
];

const PUML_ACTIVITY: &[&str] = &[
    "start",
    "stop",
    "end",
    "kill",
    "detach",
    "else",
    "elseif",
    "endif",
    "endwhile",
    "fork again",
    "end fork",
    "split again",
    "end split",
    "backward",
];

const PUML_ACTIVITY_BLOCKS: &[Block] = &[
    (
        "if",
        "a decision",
        "if (${1:condition}) then (${2:yes})\n  $0\nelse (${3:no})\n  \nendif",
    ),
    (
        "while",
        "a loop",
        "while (${1:condition}) is (${2:yes})\n  $0\nendwhile (${3:no})",
    ),
    (
        "repeat",
        "a loop that runs once at least",
        "repeat\n  $0\nrepeat while (${1:condition}) is (${2:yes})",
    ),
    (
        "fork",
        "at the same time",
        "fork\n  $0\nfork again\n  \nend fork",
    ),
    (
        "switch",
        "one of several",
        "switch (${1:value})\ncase (${2:first})\n  $0\ncase (${3:second})\n  \nendswitch",
    ),
    ("partition", "a group", "partition ${1:Name} {\n  $0\n}"),
];

const PUML_STRUCTURE: &[&str] = &[
    "class",
    "interface",
    "abstract class",
    "enum",
    "object",
    "package",
    "namespace",
    "component",
    "node",
    "database",
    "cloud",
    "folder",
    "frame",
    "rectangle",
    "actor",
    "usecase",
    "artifact",
    "queue",
    "left to right direction",
    "top to bottom direction",
];

const PUML_STRUCTURE_BLOCKS: &[Block] = &[
    (
        "class block",
        "a class with members",
        "class ${1:Name} {\n  $0\n}",
    ),
    ("enum block", "an enumeration", "enum ${1:Name} {\n  $0\n}"),
    ("package block", "a package", "package ${1:Name} {\n  $0\n}"),
];

const MERMAID_HEADS: &[&str] = &[
    "flowchart TD",
    "flowchart LR",
    "sequenceDiagram",
    "stateDiagram-v2",
    "classDiagram",
    "erDiagram",
    "gantt",
    "pie",
    "mindmap",
    "timeline",
];

const MERMAID_FLOWCHART: &[&str] = &[
    "classDef",
    "class",
    "style",
    "linkStyle",
    "click",
    "direction",
    "end",
];

const MERMAID_FLOWCHART_BLOCKS: &[Block] = &[(
    "subgraph",
    "a group",
    "subgraph ${1:name} [${2:Title}]\n  $0\nend",
)];

const MERMAID_SEQUENCE: &[&str] = &[
    "participant",
    "actor",
    "activate",
    "deactivate",
    "autonumber",
    "create participant",
    "destroy",
    "else",
    "and",
    "end",
];

const MERMAID_SEQUENCE_BLOCKS: &[Block] = &[
    (
        "alt",
        "one way or another",
        "alt ${1:condition}\n  $0\nelse ${2:otherwise}\n  \nend",
    ),
    ("opt", "only if", "opt ${1:condition}\n  $0\nend"),
    ("loop", "again and again", "loop ${1:until done}\n  $0\nend"),
    (
        "par",
        "at the same time",
        "par ${1:first}\n  $0\nand ${2:second}\n  \nend",
    ),
    (
        "critical",
        "all or nothing",
        "critical ${1:label}\n  $0\nend",
    ),
    (
        "break",
        "leaves the frame",
        "break ${1:condition}\n  $0\nend",
    ),
    (
        "rect",
        "a colored band",
        "rect ${1:rgb(240, 240, 240)}\n  $0\nend",
    ),
    ("Note over", "a note", "Note over ${1:Name}: ${0:text}"),
    (
        "Note right of",
        "a note",
        "Note right of ${1:Name}: ${0:text}",
    ),
    (
        "Note left of",
        "a note",
        "Note left of ${1:Name}: ${0:text}",
    ),
];

const MERMAID_STATE: &[&str] = &["state", "direction", "classDef", "class"];

const MERMAID_STATE_BLOCKS: &[Block] = &[
    (
        "state block",
        "a composite state",
        "state ${1:Name} {\n  $0\n}",
    ),
    ("state choice", "a choice", "state ${1:name} <<choice>>"),
    (
        "note right of",
        "a note",
        "note right of ${1:State}\n  $0\nend note",
    ),
];

const MERMAID_CLASS: &[&str] = &["class", "direction", "note", "namespace"];

const MERMAID_CLASS_BLOCKS: &[Block] = &[(
    "class block",
    "a class with members",
    "class ${1:Name} {\n  $0\n}",
)];

/// Words that are neither a keyword that starts a line nor a name.
const FILLERS: &[&str] = &[
    "as", "of", "over", "left", "right", "to", "is", "then", "again", "while", "TD", "TB", "BT",
    "LR", "RL", "startuml", "enduml", "true", "false", "fill", "stroke", "color",
];

/// Words whose line says something about the diagram and names nothing
/// in it.
const SAYS: &[&str] = &[
    "title",
    "skinparam",
    "hide",
    "show",
    "legend",
    "caption",
    "header",
    "footer",
    "note",
    "classDef",
    "style",
    "linkStyle",
    "click",
    "direction",
    "autonumber",
];

/// Whether `line` starts with `word` as a word of its own.
pub(super) fn starts_with_word(line: &str, word: &str) -> bool {
    line.strip_prefix(word)
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
}

/// The words and blocks of `lang` for a diagram of `kind`.
fn tables(lang: Language, kind: &DiagramKind) -> (Vec<&'static str>, &'static [Block]) {
    match lang {
        Language::PlantUml => {
            let (words, blocks): (&[&str], &[Block]) = match kind.kind {
                Kind::Sequence => (PUML_SEQUENCE, PUML_SEQUENCE_BLOCKS),
                Kind::State => (PUML_STATE, PUML_STATE_BLOCKS),
                Kind::Activity => (PUML_ACTIVITY, PUML_ACTIVITY_BLOCKS),
                Kind::Structure => (PUML_STRUCTURE, PUML_STRUCTURE_BLOCKS),
                // Nothing says yet what it will be: a scenario's words,
                // the most written.
                Kind::Other if kind.name.is_empty() || kind.name == "Diagram" => {
                    (PUML_SEQUENCE, PUML_SEQUENCE_BLOCKS)
                }
                Kind::Timing | Kind::Other => (&[], &[]),
            };
            (words.iter().chain(PUML_ALWAYS).copied().collect(), blocks)
        }
        Language::Mermaid => {
            let (words, blocks): (&[&str], &[Block]) = match (kind.kind, &*kind.name) {
                (_, "Flowchart") => (MERMAID_FLOWCHART, MERMAID_FLOWCHART_BLOCKS),
                (Kind::Sequence, _) => (MERMAID_SEQUENCE, MERMAID_SEQUENCE_BLOCKS),
                (Kind::State, _) => (MERMAID_STATE, MERMAID_STATE_BLOCKS),
                (Kind::Structure, "Class diagram") => (MERMAID_CLASS, MERMAID_CLASS_BLOCKS),
                (Kind::Other, _) => (MERMAID_HEADS, &[]),
                _ => (&[], &[]),
            };
            (words.to_vec(), blocks)
        }
    }
}

/// Every table, for the tests and for telling a keyword from a name.
fn all() -> impl Iterator<Item = (&'static [&'static str], &'static [Block])> {
    [
        (PUML_ALWAYS, &[][..]),
        (PUML_SEQUENCE, PUML_SEQUENCE_BLOCKS),
        (PUML_STATE, PUML_STATE_BLOCKS),
        (PUML_ACTIVITY, PUML_ACTIVITY_BLOCKS),
        (PUML_STRUCTURE, PUML_STRUCTURE_BLOCKS),
        (MERMAID_HEADS, &[][..]),
        (MERMAID_FLOWCHART, MERMAID_FLOWCHART_BLOCKS),
        (MERMAID_SEQUENCE, MERMAID_SEQUENCE_BLOCKS),
        (MERMAID_STATE, MERMAID_STATE_BLOCKS),
        (MERMAID_CLASS, MERMAID_CLASS_BLOCKS),
    ]
    .into_iter()
}

/// Every block's text, for a test that the editor reads each of them.
pub fn block_texts() -> Vec<&'static str> {
    all()
        .flat_map(|(_, blocks)| blocks.iter().map(|(_, _, text)| *text))
        .collect()
}

/// The blocks and the words of `lang` that start a line of `kind`.
pub(super) fn of_language(lang: Language, kind: &DiagramKind, out: &mut Vec<Suggestion>) {
    let (words, blocks) = tables(lang, kind);
    for (label, detail, text) in blocks {
        let mut block = Suggestion::new(*label, What::Block, *detail, 5);
        block.snippet = Some((*text).to_owned());
        out.push(block);
    }
    for word in words {
        out.push(Suggestion::new(word, What::Keyword, "", 6));
    }
}

/// Whether `name` can be written as one word.
pub(super) fn is_word(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Whether `mark` stands in `text` outside its quotes.
pub(super) fn outside_quotes(text: &str, mark: char) -> bool {
    let mut quoted = false;
    text.chars().any(|c| {
        quoted ^= c == '"';
        !quoted && c == mark
    })
}

/// `line` without what stands between `open` and `close`.
fn without(line: &str, open: char, close: char) -> String {
    let mut out = String::with_capacity(line.len());
    let mut inside = false;
    for c in line.chars() {
        if inside {
            inside = c != close;
            if !inside {
                out.push(' ');
            }
        } else if c == open {
            inside = true;
        } else {
            out.push(c);
        }
    }
    out
}

/// The words `code` has written that may be names, each once, in the
/// order they come: no comment, no label, no text in quotes or in a
/// flowchart's box, no keyword.
pub(super) fn written(code: &str, lang: Language, kind: &DiagramKind) -> Vec<String> {
    let keywords: HashSet<&str> = all()
        .flat_map(|(words, blocks)| {
            let found = blocks.iter().map(|(label, _, _)| *label);
            words.iter().copied().chain(found)
        })
        .flat_map(|word| word.split(' '))
        .chain(FILLERS.iter().copied())
        .collect();
    let comment = match lang {
        Language::PlantUml => "'",
        Language::Mermaid => "%%",
    };
    let labelled = matches!(kind.kind, Kind::State | Kind::Sequence);
    let boxes = lang == Language::Mermaid && kind.name == "Flowchart";
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    // Mermaid's first line says what the diagram is.
    let mut head = lang == Language::Mermaid;
    for line in code.split('\n') {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with(comment)
            || line.starts_with('@')
            || line.starts_with("---")
        {
            continue;
        }
        if std::mem::take(&mut head) || SAYS.iter().any(|word| starts_with_word(line, word)) {
            continue;
        }
        let mut text = without(line, '"', '"');
        if boxes {
            for (open, close) in [('[', ']'), ('(', ')'), ('{', '}'), ('|', '|')] {
                text = without(&text, open, close);
            }
        }
        if labelled && let Some(at) = text.find(':') {
            text.truncate(at);
        }
        for word in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            let name = word.len() >= 2 && word.chars().next().is_some_and(char::is_alphabetic);
            if name && !keywords.contains(word) && seen.insert(word.to_owned()) {
                out.push(word.to_owned());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use omaspec_core::classify;

    #[test]
    fn the_words_written_are_names_not_labels_or_keywords() {
        let code = "@startuml\ntitle Shop\nparticipant \"Web app\" as web\nactor User\n' a comment here\nUser -> web : open the page\nalt ok\nweb --> User : page\nend\n@enduml\n";
        let kind = classify(code, Language::PlantUml);
        assert_eq!(
            written(code, Language::PlantUml, &kind),
            ["web", "User", "ok"]
        );
        let flow = "flowchart TD\n  A[Start here] -->|if ok| Bee{Sure?}\n  %% Not this\n  Bee --> Cee(Done)\n";
        let kind = classify(flow, Language::Mermaid);
        assert_eq!(written(flow, Language::Mermaid, &kind), ["Bee", "Cee"]);
    }

    #[test]
    fn every_block_has_a_word_and_places_that_pair_up() {
        for (_, blocks) in all() {
            for (label, detail, text) in blocks {
                assert!(!label.is_empty() && !detail.is_empty());
                assert_eq!(
                    text.matches('{').count(),
                    text.matches('}').count(),
                    "{text}"
                );
            }
        }
    }
}
