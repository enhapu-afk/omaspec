//! A color for one box, kept in the source as the language writes it: on
//! a state's or a class's declaration in PlantUML (`state Idle #f7768e`),
//! as a `style` line in Mermaid (`style Idle fill:#f7768e`). The skins
//! color by kind, so every state of a machine is one color; this is for
//! the one state or class a writer points at.

use std::ops::Range;

use crate::Kind;
use crate::lang::Language;
use crate::project::key;
use crate::text::line_ranges;

/// The edit that colors the box `name` with `color` (`#rrggbb`), or takes
/// its color away with `None`: the byte range to replace and its new text.
/// `Err` says why there is none.
pub fn edit(
    code: &str,
    language: Language,
    name: &str,
    color: Option<&str>,
) -> Result<(Range<usize>, String), String> {
    let kind = crate::classify(code, language).kind;
    let word = match (language, kind) {
        (Language::Mermaid, Kind::State | Kind::Structure) => return mermaid(code, name, color),
        (Language::PlantUml, Kind::State) => "state",
        (Language::PlantUml, Kind::Structure) => "class",
        _ => return Err("a color goes on a state or a class".into()),
    };
    plantuml(code, word, name, color)
}

/// The keywords a class diagram declares a box with, the longer first.
const CLASS_WORDS: &[&str] = &[
    "abstract class",
    "class",
    "interface",
    "enum",
    "entity",
    "object",
    "abstract",
];

/// The name a PlantUML line declares with `word`, and where it ends in
/// the line: `state Idle`, `state "Long name" as Idle`, `class Idle {`.
fn declared<'a>(line: &'a str, words: &[&str]) -> Option<(&'a str, usize)> {
    let trimmed = line.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    let rest = words.iter().find_map(|word| {
        let rest = lower.strip_prefix(word)?;
        rest.starts_with(char::is_whitespace)
            .then_some(&trimmed[word.len()..])
    })?;
    let rest = rest.trim_start();
    let rest = if let Some(quoted) = rest.strip_prefix('"') {
        let close = quoted.find('"')?;
        let after = quoted[close + 1..].trim_start();
        let after = after
            .strip_prefix("as")
            .or_else(|| after.strip_prefix("AS"))?;
        if !after.starts_with(char::is_whitespace) {
            return None;
        }
        after.trim_start()
    } else {
        rest
    };
    // `rest` is the end of `line`: where it begins there.
    let at = line.len() - rest.len();
    let end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '{' | '<' | '#' | ':' | '['))
        .unwrap_or(rest.len());
    let found = &rest[..end];
    (!found.is_empty()).then_some((found, at + end))
}

fn plantuml(
    code: &str,
    word: &str,
    name: &str,
    color: Option<&str>,
) -> Result<(Range<usize>, String), String> {
    let words: &[&str] = if word == "state" {
        &["state"]
    } else {
        CLASS_WORDS
    };
    let wanted = key(name);
    for range in line_ranges(code) {
        let (line, newline) = split_newline(&code[range.clone()]);
        let Some((found, end)) = declared(line, words) else {
            continue;
        };
        if key(found) != wanted {
            continue;
        }
        // The line without its fill: `#color` after the name, not the
        // `##color` of its border.
        let (head, tail) = line.split_at(end);
        let mut kept = String::new();
        let mut rest = tail;
        while let Some(at) = rest.find('#') {
            let (before, from) = rest.split_at(at);
            if from.starts_with("##") || before.ends_with('#') {
                let past = from.find(|c: char| c.is_whitespace()).unwrap_or(from.len());
                kept.push_str(before);
                kept.push_str(&from[..past]);
                rest = &from[past..];
                continue;
            }
            let past = from
                .find(|c: char| c.is_whitespace() || matches!(c, '{' | '<' | ':'))
                .unwrap_or(from.len());
            kept.push_str(before.trim_end());
            rest = &from[past..];
        }
        kept.push_str(rest);
        let new = match color {
            Some(color) => format!("{head} {color}{}{newline}", pad(&kept)),
            None => format!("{head}{}{newline}", pad(&kept)),
        };
        if new == code[range.clone()] {
            return Err(format!("{name} has that color already"));
        }
        return Ok((range, new));
    }
    let Some(color) = color else {
        return Err(format!("{name} has no color of its own"));
    };
    // No declaration: one before the first line that names the box.
    let first = line_ranges(code)
        .into_iter()
        .find(|range| mentions(&code[range.clone()], &wanted))
        .ok_or_else(|| format!("{name} is not in the source"))?;
    let (line, _) = split_newline(&code[first.clone()]);
    let indent = &line[..line.len() - line.trim_start().len()];
    Ok((
        first.start..first.start,
        format!("{indent}{word} {name} {color}\n"),
    ))
}

/// `line` without its line break, and that break.
fn split_newline(line: &str) -> (&str, &str) {
    let text = line.trim_end_matches(['\n', '\r']);
    (text, &line[text.len()..])
}

/// `tail` with one space before it, unless it is empty.
fn pad(tail: &str) -> String {
    let tail = tail.trim_start();
    if tail.is_empty() {
        String::new()
    } else {
        format!(" {tail}")
    }
}

/// Whether `line` names the box of key `wanted` as a word of its own.
fn mentions(line: &str, wanted: &str) -> bool {
    line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|word| !word.is_empty() && key(word) == *wanted)
}

fn mermaid(code: &str, name: &str, color: Option<&str>) -> Result<(Range<usize>, String), String> {
    let wanted = key(name);
    let mut last: Option<Range<usize>> = None;
    for range in line_ranges(code) {
        let (line, newline) = split_newline(&code[range.clone()]);
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("style") else {
            if !trimmed.starts_with("%%") && !trimmed.is_empty() {
                last = Some(range.clone());
            }
            continue;
        };
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        last = Some(range.clone());
        let rest = rest.trim_start();
        let (ids, props) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let ids: Vec<&str> = ids.split(',').collect();
        if ids.len() != 1 || key(ids[0]) != wanted {
            continue;
        }
        let indent = &line[..line.len() - trimmed.len()];
        let mut kept: Vec<&str> = props
            .split(',')
            .map(str::trim)
            .filter(|prop| !prop.is_empty() && !prop.starts_with("fill:"))
            .collect();
        let fill = color.map(|color| format!("fill:{color}"));
        if let Some(fill) = &fill {
            kept.insert(0, fill);
        }
        if kept.is_empty() {
            // The line held the fill alone: it goes.
            return Ok((range.start..range.end, String::new()));
        }
        let new = format!("{indent}style {} {}{newline}", ids[0], kept.join(","));
        if new == code[range.clone()] {
            return Err(format!("{name} has that color already"));
        }
        return Ok((range, new));
    }
    let Some(color) = color else {
        return Err(format!("{name} has no color of its own"));
    };
    // After the last line of the diagram, as deep as that one.
    let last = last.ok_or_else(|| "an empty diagram".to_owned())?;
    let (line, newline) = split_newline(&code[last.clone()]);
    let indent = &line[..line.len() - line.trim_start().len()];
    let indent = if indent.is_empty() { "  " } else { indent };
    // After the last line; one without a break gets one first.
    let at = last.end;
    let lead = if newline.is_empty() { "\n" } else { "" };
    Ok((at..at, format!("{lead}{indent}style {name} fill:{color}\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(code: &str, language: Language, name: &str, color: Option<&str>) -> String {
        let (range, text) = edit(code, language, name, color).unwrap();
        let mut out = code.to_owned();
        out.replace_range(range, &text);
        out
    }

    #[test]
    fn a_state_s_declaration_takes_the_color_or_one_is_added() {
        let code = "@startuml\n[*] --> Idle\nIdle --> Busy : go\n@enduml\n";
        assert_eq!(
            applied(code, Language::PlantUml, "Busy", Some("#f7768e")),
            "@startuml\n[*] --> Idle\nstate Busy #f7768e\nIdle --> Busy : go\n@enduml\n"
        );
        let declared = "@startuml\nstate \"Long name\" as Busy ##red {\n  state In\n}\n[*] --> Busy\n@enduml\n";
        assert_eq!(
            applied(declared, Language::PlantUml, "Busy", Some("#aabbcc")),
            "@startuml\nstate \"Long name\" as Busy #aabbcc ##red {\n  state In\n}\n[*] --> Busy\n@enduml\n"
        );
        // Another color replaces it; none takes it away.
        let colored = "@startuml\n  state Busy #aabbcc <<hot>>\n[*] --> Busy\n@enduml\n";
        assert_eq!(
            applied(colored, Language::PlantUml, "busy", Some("#112233")),
            "@startuml\n  state Busy #112233 <<hot>>\n[*] --> Busy\n@enduml\n"
        );
        assert_eq!(
            applied(colored, Language::PlantUml, "Busy", None),
            "@startuml\n  state Busy <<hot>>\n[*] --> Busy\n@enduml\n"
        );
        assert!(edit(colored, Language::PlantUml, "Busy", Some("#aabbcc")).is_err());
        assert!(edit(code, Language::PlantUml, "Idle", None).is_err());
        assert!(edit(code, Language::PlantUml, "Nowhere", Some("#000000")).is_err());
    }

    #[test]
    fn a_class_takes_the_color_on_its_keyword_line() {
        let code =
            "@startuml\nclass Order {\n  id\n}\ninterface Payable\nOrder --> Payable\n@enduml\n";
        assert_eq!(
            applied(code, Language::PlantUml, "Order", Some("#ffcc00")),
            "@startuml\nclass Order #ffcc00 {\n  id\n}\ninterface Payable\nOrder --> Payable\n@enduml\n"
        );
        assert_eq!(
            applied(code, Language::PlantUml, "Payable", Some("#ffcc00")),
            "@startuml\nclass Order {\n  id\n}\ninterface Payable #ffcc00\nOrder --> Payable\n@enduml\n"
        );
        // A class that is only named gets a declaration.
        let bare = "@startuml\nclass Order\nOrder --> Line\n@enduml\n";
        assert_eq!(
            applied(bare, Language::PlantUml, "Line", Some("#ffcc00")),
            "@startuml\nclass Order\nclass Line #ffcc00\nOrder --> Line\n@enduml\n"
        );
        // Not on a scenario.
        assert!(
            edit(
                "@startuml\nA -> B : hi\n@enduml\n",
                Language::PlantUml,
                "A",
                Some("#fff")
            )
            .is_err()
        );
    }

    #[test]
    fn mermaid_keeps_the_color_in_a_style_line() {
        let code = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy\n";
        let colored = applied(code, Language::Mermaid, "Busy", Some("#f7768e"));
        assert_eq!(
            colored,
            "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy\n  style Busy fill:#f7768e\n"
        );
        // Another color replaces the fill and keeps the rest of the style.
        let styled = "stateDiagram-v2\n  Idle --> Busy\n  style Busy fill:#f7768e,stroke:#333\n";
        assert_eq!(
            applied(styled, Language::Mermaid, "Busy", Some("#00ff00")),
            "stateDiagram-v2\n  Idle --> Busy\n  style Busy fill:#00ff00,stroke:#333\n"
        );
        assert_eq!(
            applied(styled, Language::Mermaid, "Busy", None),
            "stateDiagram-v2\n  Idle --> Busy\n  style Busy stroke:#333\n"
        );
        // A line that held the fill alone goes with it.
        assert_eq!(applied(&colored, Language::Mermaid, "Busy", None), code);
        // A class diagram the same way.
        let classes = "classDiagram\n  class Order\n  Order --> Line\n";
        assert_eq!(
            applied(classes, Language::Mermaid, "Order", Some("#ffcc00")),
            "classDiagram\n  class Order\n  Order --> Line\n  style Order fill:#ffcc00\n"
        );
        assert!(edit(classes, Language::Mermaid, "Order", None).is_err());
    }
}
