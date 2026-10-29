//! Which diagram language a source is written in.

use std::borrow::Cow;

use regex::Regex;
use serde::Serialize;

use crate::text::{re, significant_lines, strip_bom};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[serde(rename = "plantuml")]
    PlantUml,
    Mermaid,
}

impl Language {
    /// `plantuml` or `mermaid`.
    pub fn id(self) -> &'static str {
        match self {
            Language::PlantUml => "plantuml",
            Language::Mermaid => "mermaid",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Language::PlantUml => "PlantUML",
            Language::Mermaid => "Mermaid",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id.to_ascii_lowercase().as_str() {
            "plantuml" | "puml" => Some(Language::PlantUml),
            "mermaid" | "mmd" => Some(Language::Mermaid),
            _ => None,
        }
    }

    /// The language a file name suggests: `.mmd`, `.mermaid` or `.puml`,
    /// `.plantuml`, `.wsd`, `.puml.txt`. Plain `.txt` suggests nothing.
    pub fn from_file_name(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        if [".mmd", ".mermaid"].iter().any(|ext| lower.ends_with(ext)) {
            Some(Language::Mermaid)
        } else if [".puml", ".plantuml", ".wsd", ".puml.txt"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        {
            Some(Language::PlantUml)
        } else {
            None
        }
    }

    /// The extension Omaspec saves this language with.
    pub fn extension(self) -> &'static str {
        match self {
            Language::PlantUml => "puml",
            Language::Mermaid => "mmd",
        }
    }
}

pub(crate) fn puml_start() -> &'static Regex {
    re!(
        r"(?i)^@start(uml|mindmap|json|latex|math|wbs|salt|tree|chronology|gantt|ditaa|dot|yaml|flow)\b"
    )
}

pub(crate) fn puml_end() -> &'static Regex {
    re!(
        r"(?i)^@end(uml|mindmap|json|latex|math|wbs|salt|tree|chronology|gantt|ditaa|dot|yaml|flow)\b"
    )
}

pub(crate) fn mermaid_head() -> &'static Regex {
    re!(
        r"^(flowchart|sequenceDiagram|classDiagram|stateDiagram(?:-v2)?|erDiagram|journey|gantt|pie|gitGraph|mindmap|timeline|quadrantChart|requirementDiagram|C4Context|C4Container|C4Component|C4Dynamic|C4Deployment|sankey-beta|xychart-beta|block-beta|packet-beta|kanban|architecture-beta|zenuml|radar-beta|treemap-beta)\b"
    )
}

pub(crate) fn mermaid_graph() -> &'static Regex {
    re!(r"^graph\s+(TD|TB|BT|RL|LR)\b")
}

/// Detects the language of `code`. `hint` (usually from the file name)
/// decides only when the source itself does not.
///
/// A PlantUML `@start…`/`@end…` marker anywhere in the first 80 meaningful
/// lines wins. Otherwise a Mermaid header in the first 40 lines decides;
/// `gantt` and `mindmap` exist in both languages and count as Mermaid only
/// with a second Mermaid trait (`dateFormat`, `section`, `root((…))`).
pub fn detect(code: &str, hint: Option<Language>) -> Language {
    let text = strip_bom(code);
    if text.trim().is_empty() {
        return Language::PlantUml;
    }
    if re!(r"(?i)^\s*```(?:mermaid|mmd)\b").is_match(text) {
        return Language::Mermaid;
    }
    let sig40 = significant_lines(text, 40);
    let sig80 = significant_lines(text, 80);
    if let Some(found) = scan(&sig40, &sig80) {
        return found;
    }
    if sig40
        .iter()
        .any(|l| re!(r"^(skinparam|!theme|!include)\b").is_match(l))
    {
        return Language::PlantUml;
    }
    hint.unwrap_or(Language::PlantUml)
}

fn scan(sig40: &[&str], sig80: &[&str]) -> Option<Language> {
    if sig80
        .iter()
        .any(|l| puml_start().is_match(l) || puml_end().is_match(l))
    {
        return Some(Language::PlantUml);
    }
    for (i, l) in sig40.iter().enumerate() {
        if mermaid_graph().is_match(l) || l.starts_with("%%{") {
            return Some(Language::Mermaid);
        }
        if re!(r"^(gantt|mindmap)\b").is_match(l) {
            let rest = sig40[i..sig40.len().min(i + 16)].join("\n");
            if l.starts_with("gantt")
                && re!(r"(^|\n)\s*(dateFormat|axisFormat|section|title)\b").is_match(&rest)
            {
                return Some(Language::Mermaid);
            }
            if l.starts_with("mindmap") && re!(r"root\s*\(\(").is_match(&rest) {
                return Some(Language::Mermaid);
            }
            continue;
        }
        if mermaid_head().is_match(l) {
            return Some(Language::Mermaid);
        }
    }
    None
}

/// The inside of a ```` ```mermaid ```` fence, or the code unchanged.
pub fn strip_mermaid_fence(code: &str) -> &str {
    let text = strip_bom(code);
    match re!(r"(?is)^\s*```(?:mermaid|mmd)[^\n]*\n(.*?)\n?```\s*$").captures(text) {
        Some(c) => c.get(1).map_or("", |m| m.as_str()),
        None => text,
    }
}

/// Pasted text without a surrounding Markdown fence (```` ```plantuml ````,
/// ```` ```mermaid ```` or a bare ```` ``` ````) and leading blank space.
pub fn unwrap_paste(text: &str) -> Cow<'_, str> {
    let t = strip_bom(text).trim();
    let inner =
        match re!(r"(?is)^```(?:plantuml|puml|mermaid|mmd)?[^\n]*\n(.*?)\n?```$").captures(t) {
            Some(c) => c.get(1).map_or("", |m| m.as_str()),
            None => t,
        };
    Cow::Borrowed(inner.trim_start())
}

/// Whether pasted text looks like a diagram (and so deserves its own tab).
pub fn looks_like_diagram(text: &str) -> bool {
    let t = unwrap_paste(text);
    if t.chars().count() < 8 {
        return false;
    }
    let head: Vec<&str> = t.split('\n').take(12).collect();
    if puml_start().is_match(&t)
        || re!(r"(?i)@start(?:uml|mindmap|json|wbs|gantt|flow)\b").is_match(&head.join("\n"))
    {
        return true;
    }
    significant_lines(&t, 20)
        .iter()
        .any(|l| mermaid_head().is_match(l) || mermaid_graph().is_match(l))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Language::*;

    #[test]
    fn detects_by_markers() {
        assert_eq!(detect("@startuml\nA -> B\n@enduml", None), PlantUml);
        assert_eq!(detect("sequenceDiagram\n A->>B: hi", None), Mermaid);
        assert_eq!(detect("graph LR\n A-->B", None), Mermaid);
        assert_eq!(detect("```mermaid\npie\n```", None), Mermaid);
        assert_eq!(detect("%%{init: {}}%%\nflowchart TD", None), Mermaid);
        assert_eq!(detect("", Some(Mermaid)), PlantUml);
    }

    #[test]
    fn plantuml_marker_beats_mermaid_header() {
        assert_eq!(detect("flowchart TD\n@startuml\n@enduml", None), PlantUml);
    }

    #[test]
    fn gantt_and_mindmap_need_a_second_trait() {
        assert_eq!(detect("gantt\n  dateFormat YYYY", None), Mermaid);
        assert_eq!(detect("gantt\n  [Task] lasts 3 days", None), PlantUml);
        assert_eq!(detect("mindmap\n  root((x))", None), Mermaid);
        assert_eq!(detect("mindmap\n  a", Some(Mermaid)), Mermaid);
    }

    #[test]
    fn hint_decides_only_when_the_text_does_not() {
        assert_eq!(detect("A -> B", Some(Mermaid)), Mermaid);
        assert_eq!(detect("skinparam x y\nA -> B", Some(Mermaid)), PlantUml);
    }

    #[test]
    fn file_names() {
        assert_eq!(Language::from_file_name("a.MMD"), Some(Mermaid));
        assert_eq!(Language::from_file_name("a.puml.txt"), Some(PlantUml));
        assert_eq!(Language::from_file_name("a.txt"), None);
    }

    #[test]
    fn fences() {
        assert_eq!(
            strip_mermaid_fence("```mermaid\npie\n  \"a\": 1\n```\n"),
            "pie\n  \"a\": 1"
        );
        assert_eq!(strip_mermaid_fence("pie"), "pie");
        assert_eq!(
            unwrap_paste("\n```plantuml\n@startuml\n@enduml\n```\n"),
            "@startuml\n@enduml"
        );
        assert!(looks_like_diagram("```\nsequenceDiagram\nA->>B: x\n```"));
        assert!(!looks_like_diagram("hello world, nothing here"));
    }
}
