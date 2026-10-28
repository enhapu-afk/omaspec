//! A state machine or a scenario written again in the other language.
//!
//! The diagram is read into the parse the checks use and written out from
//! there, so its meaning carries over: states, regions, choices, forks,
//! transitions with their triggers; participants, messages and the blocks
//! around them. The result is read back and compared with the original
//! ([`crate::diff`]); if the two would not mean the same, nothing is
//! written. What the other language cannot say about the looks (colors,
//! directions, note places, shapes) is left out and told as a notice;
//! behavior it cannot say (a history state in Mermaid, a `ref` in a Mermaid
//! sequence) refuses the conversion.

mod sequence;
mod state;

use omagram_core::{Kind as DiagramKind, Language, classify};

/// A diagram in the other language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    pub code: String,
    /// What was left out or changed on the way, in line order.
    pub notices: Vec<Notice>,
}

/// Something the other language says otherwise or not at all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Notice {
    /// Zero-based line in the original, when one line is meant.
    pub line: Option<usize>,
    pub message: String,
}

/// Why a diagram was not converted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// Zero-based line in the original, when one line is to blame.
    pub line: Option<usize>,
    pub reason: String,
}

/// `code`, written in `from`, written again in `to`.
pub fn convert(code: &str, from: Language, to: Language) -> Result<Converted, Refused> {
    if from == to {
        return Err(refused(
            None,
            format!("The diagram is {} already", name(to)),
        ));
    }
    let kind = classify(code, from);
    let mut converted = match kind.kind {
        DiagramKind::State => state::convert(code, from, to)?,
        DiagramKind::Sequence => sequence::convert(code, from, to)?,
        _ => {
            let reason = match kind.name.is_empty() {
                true => "Omagram converts state machines and sequence diagrams only".to_owned(),
                false => format!(
                    "Omagram converts state machines and sequence diagrams, not {}",
                    crate::diff::a_kind(&kind.name)
                ),
            };
            return Err(refused(None, reason));
        }
    };
    converted.notices.sort();
    converted.notices.dedup();
    Ok(converted)
}

fn name(lang: Language) -> &'static str {
    match lang {
        Language::PlantUml => "PlantUML",
        Language::Mermaid => "Mermaid",
    }
}

fn refused(line: Option<usize>, reason: impl Into<String>) -> Refused {
    Refused {
        line,
        reason: reason.into(),
    }
}

/// The refusal when the written diagram does not mean what the original
/// does; `change` is the first difference.
fn not_the_same(to: Language, change: Option<&crate::Change>) -> Refused {
    match change {
        Some(change) => refused(
            change.old_line,
            format!(
                "In {} the diagram would not mean the same: {}",
                name(to),
                change.message
            ),
        ),
        None => refused(
            None,
            format!(
                "Omagram could not read back the {} it wrote; this is a bug in Omagram",
                name(to)
            ),
        ),
    }
}

/// `name` made into an id both languages take: letters, digits and `_`,
/// not starting with a digit or `_`, and none of `taken`.
fn plain_id(name: &str, taken: &std::collections::HashSet<String>) -> String {
    let mut id: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    id = id.trim_matches('_').to_owned();
    if id.is_empty() || id.starts_with(|c: char| c.is_ascii_digit()) {
        id.insert(0, 'S');
    }
    let base = id.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}_{n}");
        n += 1;
    }
    id
}

/// Whether `name` is a plain id already.
fn is_plain(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit() || c == '_')
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// A PlantUML text (`\n` breaks lines) as Mermaid writes it (`<br>`).
fn to_mermaid_text(text: &str) -> String {
    text.replace("\\n", "<br>")
}

/// A Mermaid text as PlantUML writes it.
fn to_plantuml_text(text: &str) -> String {
    text.replace("<br/>", "\\n")
        .replace("<br />", "\\n")
        .replace("<br>", "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_what_it_does_not_convert() {
        let classes = "@startuml\nclass A\nclass B\nA --> B\n@enduml\n";
        let err = convert(classes, Language::PlantUml, Language::Mermaid).unwrap_err();
        assert!(err.reason.contains("not a class diagram"), "{err:?}");
        let mermaid = "stateDiagram-v2\n[*] --> A\n";
        let err = convert(mermaid, Language::Mermaid, Language::Mermaid).unwrap_err();
        assert_eq!(err.reason, "The diagram is Mermaid already");
    }

    #[test]
    fn makes_plain_ids() {
        let mut taken = std::collections::HashSet::new();
        assert_eq!(plain_id("Screen saver", &taken), "Screen_saver");
        assert_eq!(plain_id("2nd", &taken), "S2nd");
        taken.insert("a_b".to_owned());
        assert_eq!(plain_id("a-b", &taken), "a_b_2");
        assert!(is_plain("Idle_2") && !is_plain("a-b") && !is_plain("_x"));
    }
}
